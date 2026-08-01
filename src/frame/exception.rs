//! Modbus exception responses (FR-R-080 … FR-R-086).

#[cfg(test)]
use alloc::format;
#[cfg(test)]
use alloc::vec;
use alloc::vec::Vec;

use crate::error::{Error, Result};
use crate::frame::function::FunctionCode;

/// The bit an exception response sets on the echoed function code (FR-R-080).
const EXCEPTION_FLAG: u8 = 0x80;

/// A Modbus exception code.
///
/// The nine codes defined by the specification are named (FR-R-082). Every
/// other byte, including 0, is carried as [`ExceptionCode::Other`] (FR-R-083) —
/// a server's exception codes are its own to choose, and a client that cannot
/// name one must still be able to report it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExceptionCode {
    /// 1 — Illegal Function.
    IllegalFunction,
    /// 2 — Illegal Data Address.
    IllegalDataAddress,
    /// 3 — Illegal Data Value.
    IllegalDataValue,
    /// 4 — Server Device Failure.
    ServerDeviceFailure,
    /// 5 — Acknowledge.
    Acknowledge,
    /// 6 — Server Device Busy.
    ServerDeviceBusy,
    /// 8 — Memory Parity Error.
    MemoryParityError,
    /// 10 — Gateway Path Unavailable.
    GatewayPathUnavailable,
    /// 11 — Gateway Target Device Failed To Respond.
    GatewayTargetDeviceFailedToRespond,
    /// Any other code, carried verbatim.
    Other(u8),
}

impl ExceptionCode {
    /// Every named code, in wire order. The single source of truth for the
    /// byte ↔ variant mapping.
    const NAMED: [(u8, Self); 9] = [
        (1, Self::IllegalFunction),
        (2, Self::IllegalDataAddress),
        (3, Self::IllegalDataValue),
        (4, Self::ServerDeviceFailure),
        (5, Self::Acknowledge),
        (6, Self::ServerDeviceBusy),
        (8, Self::MemoryParityError),
        (10, Self::GatewayPathUnavailable),
        (11, Self::GatewayTargetDeviceFailedToRespond),
    ];

    /// Decode an exception code byte. Infallible: every byte is a valid
    /// exception code (FR-R-083).
    pub fn decode(byte: u8) -> Self {
        Self::NAMED
            .iter()
            .find(|(candidate, _)| *candidate == byte)
            .map_or(Self::Other(byte), |(_, code)| *code)
    }

    /// Encode to its wire byte.
    ///
    /// An [`ExceptionCode::Other`] holding a named code is rejected, so one wire
    /// byte keeps exactly one representation (FR-R-084).
    pub fn encode(self) -> Result<u8> {
        let byte = match self {
            Self::IllegalFunction => 1,
            Self::IllegalDataAddress => 2,
            Self::IllegalDataValue => 3,
            Self::ServerDeviceFailure => 4,
            Self::Acknowledge => 5,
            Self::ServerDeviceBusy => 6,
            Self::MemoryParityError => 8,
            Self::GatewayPathUnavailable => 10,
            Self::GatewayTargetDeviceFailedToRespond => 11,
            Self::Other(byte) => {
                if Self::NAMED.iter().any(|(named, _)| *named == byte) {
                    return Err(Error::ReservedCode(byte));
                }
                byte
            }
        };
        Ok(byte)
    }
}

impl core::fmt::Display for ExceptionCode {
    /// FR-R-154 — a named code renders as its English name (FR-R-082); an
    /// unnamed one as `"Other exception "` followed by its decimal byte value.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            Self::IllegalFunction => "Illegal Function",
            Self::IllegalDataAddress => "Illegal Data Address",
            Self::IllegalDataValue => "Illegal Data Value",
            Self::ServerDeviceFailure => "Server Device Failure",
            Self::Acknowledge => "Acknowledge",
            Self::ServerDeviceBusy => "Server Device Busy",
            Self::MemoryParityError => "Memory Parity Error",
            Self::GatewayPathUnavailable => "Gateway Path Unavailable",
            Self::GatewayTargetDeviceFailedToRespond => "Gateway Target Device Failed To Respond",
            Self::Other(byte) => return write!(f, "Other exception {byte}"),
        };
        f.write_str(name)
    }
}

/// An exception response: the echoed function code and the exception raised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExceptionResponse {
    /// The function code of the request that failed, high bit cleared
    /// (FR-R-081).
    pub function: FunctionCode,
    /// The exception the server reported.
    pub exception: ExceptionCode,
}

impl ExceptionResponse {
    /// The exact PDU length of an exception response (FR-R-085).
    const PDU_LEN: usize = 2;

    /// Decode an exception response PDU.
    ///
    /// The length is fixed at two bytes, so it is checked here rather than left
    /// to the generic truncation and trailing-byte rules: FR-R-085 specifies a
    /// length error for this PDU, and it is the more specific requirement.
    pub fn decode(pdu: &[u8]) -> Result<Self> {
        let (&raw, &code) = match pdu {
            [raw, code] => (raw, code),
            _ => {
                return Err(Error::InvalidLength {
                    expected: Self::PDU_LEN,
                    actual: pdu.len(),
                });
            }
        };
        if raw & EXCEPTION_FLAG == 0 {
            return Err(Error::InvalidFunctionCode(raw));
        }
        Ok(Self {
            function: FunctionCode::decode(raw & !EXCEPTION_FLAG)?,
            exception: ExceptionCode::decode(code),
        })
    }

    /// Encode to an exception response PDU.
    ///
    /// # Errors
    ///
    /// Fails if the function code has no encoding.
    pub fn encode(self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        self.encode_into(&mut out)?;
        Ok(out)
    }

    /// Encode to an exception response PDU, appending to `out` (FR-R-140).
    ///
    /// Crate-internal: the appending form is public API at the PDU and ADU
    /// level, and an exception response reaches a caller as a `ResponsePdu`
    /// variant, which already offers it.
    ///
    /// # Errors
    ///
    /// Fails if the function code has no encoding.
    pub(crate) fn encode_into(self, out: &mut Vec<u8>) -> Result<()> {
        // Both fields are validated before either byte is written, so a failure
        // leaves `out` untouched (FR-R-142).
        let function = self.function.encode()? | EXCEPTION_FLAG;
        let exception = self.exception.encode()?;
        out.reserve(Self::PDU_LEN);
        out.push(function);
        out.push(exception);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(function: FunctionCode, exception: ExceptionCode) -> ExceptionResponse {
        ExceptionResponse {
            function,
            exception,
        }
    }

    #[test]
    /// FR-R-080 — an exception response is the request's function code with the
    /// most significant bit set, followed by one exception code byte.
    fn ut_exception_sets_high_bit() {
        let pdu = response(
            FunctionCode::ReadHoldingRegisters,
            ExceptionCode::IllegalDataAddress,
        )
        .encode()
        .expect("named codes encode");
        assert_eq!(pdu, vec![0x83, 0x02]);
    }

    #[test]
    /// FR-R-081 — decoding reports the original function code with the high bit
    /// cleared, not the raw byte.
    fn ut_decode_reports_original_function_code() {
        assert_eq!(
            ExceptionResponse::decode(&[0x83, 0x02]),
            Ok(response(
                FunctionCode::ReadHoldingRegisters,
                ExceptionCode::IllegalDataAddress
            ))
        );
    }

    #[test]
    /// FR-R-082 — all nine specified exception codes are named, each mapping to
    /// its wire byte in both directions.
    fn ut_all_nine_exception_codes_named() {
        assert_eq!(ExceptionCode::NAMED.len(), 9);
        for (byte, code) in ExceptionCode::NAMED {
            assert_eq!(ExceptionCode::decode(byte), code, "decoding {byte}");
            assert_eq!(code.encode(), Ok(byte), "encoding {code:?}");
        }
    }

    #[test]
    /// FR-R-083 — an unnamed exception code, including 0, decodes successfully
    /// into a general value rather than failing.
    fn ut_unknown_exception_code_wrapped() {
        for byte in [0, 7, 9, 12, 13, 100, 255] {
            assert_eq!(
                ExceptionCode::decode(byte),
                ExceptionCode::Other(byte),
                "decoding {byte}"
            );
        }
    }

    #[test]
    /// FR-R-083 — every one of the 256 bytes decodes, so no server's choice of
    /// exception code can make a response undecodable.
    fn ut_every_exception_byte_decodes() {
        for byte in 0..=255u8 {
            let decoded = ExceptionCode::decode(byte);
            let named = ExceptionCode::NAMED.iter().any(|(b, _)| *b == byte);
            assert_eq!(
                matches!(decoded, ExceptionCode::Other(_)),
                !named,
                "byte {byte} disagrees with the named table"
            );
        }
    }

    #[test]
    /// FR-R-084 — encoding a general value holding a named code is a
    /// reserved-code error, so one wire byte keeps one representation.
    fn ut_other_holding_named_code_is_reserved_error() {
        for (byte, _) in ExceptionCode::NAMED {
            assert_eq!(
                ExceptionCode::Other(byte).encode(),
                Err(Error::ReservedCode(byte)),
                "other holding {byte}"
            );
        }
    }

    #[test]
    /// FR-R-085 — an exception response PDU of any length other than two fails
    /// with a length error. This is more specific than the generic truncation
    /// and trailing-byte rules, and takes precedence over them.
    fn ut_exception_length_not_two_errors() {
        for pdu in [
            [].as_slice(),
            &[0x83],
            &[0x83, 0x02, 0x00],
            &[0x83, 0x02, 0x00, 0x00],
        ] {
            assert_eq!(
                ExceptionResponse::decode(pdu),
                Err(Error::InvalidLength {
                    expected: 2,
                    actual: pdu.len(),
                }),
                "pdu {pdu:?}"
            );
        }
    }

    #[test]
    /// FR-R-086 — the exception path works for a custom function code, not only
    /// for the named ones.
    fn ut_exception_for_custom_function_code() {
        let decoded = ExceptionResponse::decode(&[0x80 | 100, 0x0B]);
        assert_eq!(
            decoded,
            Ok(response(
                FunctionCode::Custom(100),
                ExceptionCode::GatewayTargetDeviceFailedToRespond
            ))
        );
        assert_eq!(
            decoded.and_then(ExceptionResponse::encode),
            Ok(vec![0xE4, 0x0B])
        );
    }

    #[test]
    /// FR-R-014 — an exception response echoing function code 0 is invalid, so
    /// the byte 0x80 does not decode.
    fn ut_exception_on_function_zero_is_invalid() {
        assert_eq!(
            ExceptionResponse::decode(&[0x80, 0x01]),
            Err(Error::InvalidFunctionCode(0))
        );
    }

    #[test]
    /// FR-R-081 — a first byte without the high bit is not an exception
    /// response, and is rejected rather than silently accepted.
    fn ut_exception_requires_high_bit() {
        assert_eq!(
            ExceptionResponse::decode(&[0x03, 0x02]),
            Err(Error::InvalidFunctionCode(0x03))
        );
    }

    #[test]
    /// FR-R-133 — decode and encode are inverse for every function code and
    /// every exception code.
    fn ut_exception_round_trips() {
        for function in 1..=127u8 {
            for exception in [0u8, 1, 6, 11, 200, 255] {
                let pdu = [function | EXCEPTION_FLAG, exception];
                let decoded = ExceptionResponse::decode(&pdu).expect("valid exception pdu");
                assert_eq!(decoded.encode(), Ok(pdu.to_vec()), "round-tripping {pdu:?}");
            }
        }
    }

    #[test]
    /// FR-R-154 — every named exception code Displays as the exact English name
    /// FR-R-082 gives it. Transcribed by hand from the spec, not read from
    /// `NAMED`, so a table typo cannot pass by construction.
    fn ut_exception_code_display_names_every_named_code() {
        let expected = [
            (ExceptionCode::IllegalFunction, "Illegal Function"),
            (ExceptionCode::IllegalDataAddress, "Illegal Data Address"),
            (ExceptionCode::IllegalDataValue, "Illegal Data Value"),
            (ExceptionCode::ServerDeviceFailure, "Server Device Failure"),
            (ExceptionCode::Acknowledge, "Acknowledge"),
            (ExceptionCode::ServerDeviceBusy, "Server Device Busy"),
            (ExceptionCode::MemoryParityError, "Memory Parity Error"),
            (
                ExceptionCode::GatewayPathUnavailable,
                "Gateway Path Unavailable",
            ),
            (
                ExceptionCode::GatewayTargetDeviceFailedToRespond,
                "Gateway Target Device Failed To Respond",
            ),
        ];
        assert_eq!(expected.len(), 9, "all nine named codes are covered");
        for (code, name) in expected {
            assert_eq!(format!("{code}"), name);
        }
    }

    #[test]
    /// FR-R-154 — an unnamed exception code Displays with its decimal byte
    /// value, since there is no name to substitute.
    fn ut_exception_code_display_other() {
        assert_eq!(
            format!("{}", ExceptionCode::Other(0x7F)),
            "Other exception 127"
        );
    }
}
