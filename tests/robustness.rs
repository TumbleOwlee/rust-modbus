//! Robustness of the frame layer against arbitrary input (FR-R-130,
//! FR-R-132, FR-R-133).
//!
//! The requirements these tests pin are universally quantified — "for any input
//! byte sequence whatsoever" — so they are stated as properties over generated
//! input rather than as a fixture list that only samples them.

use proptest::prelude::*;
use rust_modbus::{
    Address, Ascii, Error, Framing, Quantity, RequestPdu, ResponsePdu, Rtu, Tcp, TransactionId,
    UnitId,
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
    /// FR-R-130 — no decoding operation panics, indexes out of bounds, or
    /// aborts, for any input byte sequence whatsoever.
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
    /// FR-R-130 — the ASCII decoder holds to the same standard on input drawn
    /// from its own alphabet, which reaches deeper into it than random bytes.
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
