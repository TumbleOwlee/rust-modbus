//! Robustness of the frame layer against arbitrary input (FR-R-130,
//! FR-R-132, FR-R-133).
//!
//! The requirements these tests pin are universally quantified — "for any input
//! byte sequence whatsoever" — so they are stated as properties over generated
//! input rather than as a fixture list that only samples them.

use proptest::prelude::*;
use rust_modbus::{
    Address, AduBoundary, Ascii, DeviceIdObject, DiagnosticSubFunction, Direction, Error,
    ExceptionCode, ExceptionResponse, ExceptionStatus, FileNumber, FileRecordRead,
    FileRecordReadResponse, FileRecordWrite, Framing, FunctionCode, Mask, MeiRequest, MeiResponse,
    Quantity, ReadDeviceIdCode, RecordLength, RecordNumber, RegisterValue, RequestPdu, ResponsePdu,
    Rtu, RtuOverTcp, Tcp, TransactionId, UnitId,
};

/// Byte sequences up to a little over the largest ADU any framing permits
/// (513, FR-R-113), so oversized input is generated as well as undersized.
fn arbitrary_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..600)
}

/// Byte sequences drawn from the ASCII framing's alphabet, so the generator
/// reaches past the hexadecimal check often enough to exercise what follows it.
fn hexish_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(
        prop::sample::select(b":0123456789ABCDEFabcdef\r\n".to_vec()),
        0..600,
    )
}

/// Feed one byte sequence to every decoder the frame layer exposes.
///
/// Results are deliberately discarded: the property is that none of these
/// panics, indexes out of bounds, or aborts, not that any particular one
/// succeeds.
fn decode_every_way(bytes: &[u8]) {
    let _ = RequestPdu::decode(bytes);
    let _ = ResponsePdu::decode(bytes);
    let _ = Rtu::decode_request(bytes);
    let _ = Rtu::decode_response(bytes);
    let _ = Tcp::decode_request(bytes);
    let _ = Tcp::decode_response(bytes);
    let _ = Ascii::decode_request(bytes);
    let _ = Ascii::decode_response(bytes);
}

proptest! {
    #[test]
    /// FR-R-130, NF-R-012 — no decoding operation panics, indexes out of
    /// bounds, or aborts, for any input byte sequence whatsoever.
    fn it_decoding_arbitrary_bytes_never_panics(bytes in arbitrary_bytes()) {
        decode_every_way(&bytes);
    }

    #[test]
    /// FR-R-133 — whatever a PDU decoder accepts re-encodes to the identical
    /// byte sequence; decode and encode are inverses on all valid input.
    fn it_decoded_pdus_reencode_identically(bytes in arbitrary_bytes()) {
        if let Ok(pdu) = RequestPdu::decode(&bytes) {
            prop_assert_eq!(pdu.encode(), Ok(bytes.clone()));
        }
        if let Ok(pdu) = ResponsePdu::decode(&bytes) {
            prop_assert_eq!(pdu.encode(), Ok(bytes));
        }
    }
}

proptest! {
    #[test]
    /// FR-R-130, NF-R-014 — the ASCII decoder holds to the same standard on
    /// input drawn from its own alphabet, which reaches deeper into it than
    /// random bytes: NF-R-014 requires that generated input, not a fixture list
    /// alone, is what pins the no-panic posture.
    fn it_decoding_hexadecimal_bytes_never_panics(bytes in hexish_bytes()) {
        decode_every_way(&bytes);
    }
}

/// One valid ADU per framing, alongside the framing's name for failure output.
///
/// Built by encoding rather than by literal, so the fixtures cannot drift out
/// of agreement with the encoders; the literal-derived vectors that pin the
/// wire format live in each framing's unit tests.
fn valid_adus() -> Vec<(&'static str, Vec<u8>)> {
    let request = RequestPdu::ReadHoldingRegisters {
        address: Address(0x006B),
        quantity: Quantity(3),
    };
    vec![
        (
            "RTU",
            Rtu::encode_request(&UnitId(0x11), &request).expect("RTU encodes"),
        ),
        (
            "TCP",
            Tcp::encode_request(
                &rust_modbus::MbapHeader {
                    transaction_id: TransactionId(1),
                    unit_id: UnitId(0x11),
                },
                &request,
            )
            .expect("TCP encodes"),
        ),
        (
            "ASCII",
            Ascii::encode_request(&UnitId(0x11), &request).expect("ASCII encodes"),
        ),
    ]
}

#[test]
/// FR-R-132 — a PDU carrying more bytes than its layout requires fails with a
/// trailing-bytes error naming the surplus, rather than silently ignoring it.
fn it_surplus_pdu_bytes_are_rejected() {
    let pdu = RequestPdu::ReadHoldingRegisters {
        address: Address(0x006B),
        quantity: Quantity(3),
    }
    .encode()
    .expect("encodes");

    for extra in 1..=3usize {
        let mut bytes = pdu.clone();
        bytes.extend(core::iter::repeat_n(0x00, extra));
        assert_eq!(
            RequestPdu::decode(&bytes),
            Err(Error::TrailingBytes { extra }),
            "{extra} surplus byte(s)"
        );
    }
}

#[test]
/// FR-R-133 — the round trip holds through the ADU layer too: every framing's
/// decode of a valid ADU re-encodes to the bytes it came from.
fn it_valid_adus_reencode_identically() {
    let request = RequestPdu::ReadHoldingRegisters {
        address: Address(0x006B),
        quantity: Quantity(3),
    };
    for (name, bytes) in valid_adus() {
        match name {
            "RTU" => {
                let (header, pdu) = Rtu::decode_request(&bytes).expect("decodes");
                assert_eq!(pdu, request, "{name}");
                assert_eq!(Rtu::encode_request(&header, &pdu), Ok(bytes), "{name}");
            }
            "TCP" => {
                let (header, pdu) = Tcp::decode_request(&bytes).expect("decodes");
                assert_eq!(pdu, request, "{name}");
                assert_eq!(Tcp::encode_request(&header, &pdu), Ok(bytes), "{name}");
            }
            _ => {
                let (header, pdu) = Ascii::decode_request(&bytes).expect("decodes");
                assert_eq!(pdu, request, "{name}");
                assert_eq!(Ascii::encode_request(&header, &pdu), Ok(bytes), "{name}");
            }
        }
    }
}

#[test]
/// FR-R-130 — truncating a valid ADU at every prefix length produces an error,
/// never a panic and never a successful decode of a partial frame.
fn it_every_truncation_of_a_valid_adu_is_rejected() {
    for (name, bytes) in valid_adus() {
        for len in 0..bytes.len() {
            let prefix = bytes.get(..len).expect("len < bytes.len()");
            let decoded = match name {
                "RTU" => Rtu::decode_request(prefix).map(|(_, pdu)| pdu),
                "TCP" => Tcp::decode_request(prefix).map(|(_, pdu)| pdu),
                _ => Ascii::decode_request(prefix).map(|(_, pdu)| pdu),
            };
            assert!(decoded.is_err(), "{name} accepted a {len}-byte prefix");
        }
    }
}

/// One instance of every `RequestPdu` variant this crate can encode, each with
/// field values inside the ranges encoding accepts (FR-R-021, FR-R-022,
/// FR-R-031, FR-R-033, FR-R-038, FR-R-056), so every arm of FR-R-147's table
/// — and the custom and opaque-MEI arms FR-R-148 refuses — is reachable.
fn every_request_pdu() -> Vec<RequestPdu> {
    vec![
        RequestPdu::ReadCoils {
            address: Address(0x0013),
            quantity: Quantity(2000),
        },
        RequestPdu::ReadDiscreteInputs {
            address: Address(0x00C4),
            quantity: Quantity(2000),
        },
        RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(125),
        },
        RequestPdu::ReadInputRegisters {
            address: Address(0x0008),
            quantity: Quantity(125),
        },
        RequestPdu::WriteSingleCoil {
            address: Address(0x00AC),
            value: true,
        },
        RequestPdu::WriteSingleRegister {
            address: Address(0x0001),
            value: RegisterValue(0x0003),
        },
        RequestPdu::WriteMultipleCoils {
            address: Address(0x0013),
            coils: vec![true; 1968],
        },
        RequestPdu::WriteMultipleRegisters {
            address: Address(0x0001),
            registers: vec![RegisterValue(0x000A); 123],
        },
        RequestPdu::MaskWriteRegister {
            address: Address(0x0004),
            and_mask: Mask(0x00F2),
            or_mask: Mask(0x0025),
        },
        RequestPdu::ReadWriteMultipleRegisters {
            read_address: Address(0x0003),
            read_quantity: Quantity(125),
            write_address: Address(0x000E),
            registers: vec![RegisterValue(0x00FF); 121],
        },
        RequestPdu::ReadExceptionStatus,
        RequestPdu::Diagnostics {
            sub_function: DiagnosticSubFunction::ReturnQueryData,
            data: vec![0xA537, 0x0102],
        },
        RequestPdu::GetCommEventCounter,
        RequestPdu::GetCommEventLog,
        RequestPdu::ReportServerId,
        RequestPdu::ReadFileRecord {
            records: vec![FileRecordRead {
                file_number: FileNumber(4),
                record_number: RecordNumber(1),
                record_length: RecordLength(1),
            }],
        },
        RequestPdu::WriteFileRecord {
            records: vec![FileRecordWrite {
                file_number: FileNumber(4),
                record_number: RecordNumber(7),
                values: vec![RegisterValue(0x06AF)],
            }],
        },
        RequestPdu::ReadFifoQueue {
            address: Address(0x04DE),
        },
        RequestPdu::EncapsulatedInterfaceTransport(MeiRequest::ReadDeviceIdentification {
            read_device_id_code: ReadDeviceIdCode::Basic,
            object_id: 0x00,
        }),
        RequestPdu::EncapsulatedInterfaceTransport(MeiRequest::CanOpen {
            data: vec![0x01, 0x02, 0x03],
        }),
        RequestPdu::EncapsulatedInterfaceTransport(MeiRequest::Other {
            mei_type: 0x0D,
            data: vec![0xAA],
        }),
        RequestPdu::Custom {
            code: 0x41,
            data: vec![0x01, 0x02, 0x03],
        },
    ]
}

/// One instance of every `ResponsePdu` variant this crate can encode,
/// including the exception path FR-R-147 always derives (FR-R-086).
fn every_response_pdu() -> Vec<ResponsePdu> {
    vec![
        ResponsePdu::ReadCoils {
            coils: vec![true; 16],
        },
        ResponsePdu::ReadDiscreteInputs {
            inputs: vec![false; 16],
        },
        ResponsePdu::ReadHoldingRegisters {
            registers: vec![RegisterValue(0x022B); 3],
        },
        ResponsePdu::ReadInputRegisters {
            registers: vec![RegisterValue(0x000A); 3],
        },
        ResponsePdu::WriteSingleCoil {
            address: Address(0x00AC),
            value: true,
        },
        ResponsePdu::WriteSingleRegister {
            address: Address(0x0001),
            value: RegisterValue(0x0003),
        },
        ResponsePdu::WriteMultipleCoils {
            address: Address(0x0013),
            quantity: Quantity(2),
        },
        ResponsePdu::WriteMultipleRegisters {
            address: Address(0x0001),
            quantity: Quantity(2),
        },
        ResponsePdu::MaskWriteRegister {
            address: Address(0x0004),
            and_mask: Mask(0x00F2),
            or_mask: Mask(0x0025),
        },
        ResponsePdu::ReadWriteMultipleRegisters {
            registers: vec![RegisterValue(0x00FF); 3],
        },
        ResponsePdu::ReadExceptionStatus {
            status: ExceptionStatus(0x6D),
        },
        ResponsePdu::Diagnostics {
            sub_function: DiagnosticSubFunction::ReturnQueryData,
            data: vec![0xA537],
        },
        ResponsePdu::GetCommEventCounter {
            status: 0xFFFF,
            event_count: 0x0108,
        },
        ResponsePdu::GetCommEventLog {
            status: 0x0000,
            event_count: 0x0108,
            message_count: 0x0121,
            events: vec![0x20, 0x00],
        },
        ResponsePdu::ReportServerId {
            data: vec![0x11, 0xFF],
        },
        ResponsePdu::ReadFileRecord {
            records: vec![FileRecordReadResponse {
                values: vec![
                    RegisterValue(0x0DFE),
                    RegisterValue(0x0000),
                    RegisterValue(0x0001),
                ],
            }],
        },
        ResponsePdu::WriteFileRecord {
            records: vec![FileRecordWrite {
                file_number: FileNumber(4),
                record_number: RecordNumber(7),
                values: vec![RegisterValue(0x06AF)],
            }],
        },
        ResponsePdu::ReadFifoQueue {
            values: vec![RegisterValue(0x01B8)],
        },
        ResponsePdu::EncapsulatedInterfaceTransport(MeiResponse::ReadDeviceIdentification {
            read_device_id_code: ReadDeviceIdCode::Basic,
            conformity_level: 0x01,
            more_follows: false,
            next_object_id: 0x00,
            objects: vec![DeviceIdObject {
                id: 0,
                value: vec![0x41, 0x42],
            }],
        }),
        ResponsePdu::EncapsulatedInterfaceTransport(MeiResponse::CanOpen { data: vec![0x01] }),
        ResponsePdu::EncapsulatedInterfaceTransport(MeiResponse::Other {
            mei_type: 0x0D,
            data: vec![0xAA],
        }),
        ResponsePdu::Custom {
            code: 0x41,
            data: vec![0x01, 0x02],
        },
        ResponsePdu::Exception(ExceptionResponse {
            function: FunctionCode::ReadHoldingRegisters,
            exception: ExceptionCode::IllegalDataAddress,
        }),
    ]
}

#[test]
/// FR-R-146, FR-R-147, FR-R-148, FR-R-149 — the extent derivation never
/// panics: not on any prefix of any ADU this crate can encode, in either
/// direction, and not on any function-code byte at any short length, including
/// codes no PDU above happens to produce. This is the permanent floor under
/// the length table, run on every `cargo test`, not a spot check performed
/// once by hand.
fn it_derive_extent_never_panics_on_any_prefix_or_short_input() {
    let AduBoundary::ContentLength { extent, .. } = RtuOverTcp::boundary() else {
        panic!("RtuOverTcp is a ContentLength framing");
    };
    let unit = UnitId(0x11);

    for request in every_request_pdu() {
        if let Ok(encoded) = RtuOverTcp::encode_request(&unit, &request) {
            for len in 0..=encoded.len() {
                let prefix = encoded.get(..len).expect("len <= encoded.len()");
                let _ = extent(Direction::Request, prefix);
            }
        }
    }
    for response in every_response_pdu() {
        if let Ok(encoded) = RtuOverTcp::encode_response(&unit, &response) {
            for len in 0..=encoded.len() {
                let prefix = encoded.get(..len).expect("len <= encoded.len()");
                let _ = extent(Direction::Response, prefix);
            }
        }
    }

    // Every function-code byte, at every length up to 12, in both directions:
    // this reaches codes no PDU above produces (128–255 outside an exception,
    // and every unnamed custom code) and every length too short for any field
    // the table reads to exist yet.
    for direction in [Direction::Request, Direction::Response] {
        for function in 0u8..=255 {
            for len in 0..=12usize {
                let mut bytes = vec![0u8; len];
                if let Some(second) = bytes.get_mut(1) {
                    *second = function;
                }
                let _ = extent(direction, &bytes);
            }
        }
    }
}
