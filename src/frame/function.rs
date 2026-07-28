//! Modbus function codes (FR-R-010 … FR-R-015).

use crate::error::{Error, Result};

/// A Modbus function code.
///
/// The nineteen public codes of the Modbus Application Protocol specification
/// are named (FR-R-010). Every other code in 1–127 is carried as
/// [`FunctionCode::Custom`] with an opaque body (FR-R-011). Code 0 is invalid,
/// and 128–255 is exception-response space, never a request (FR-R-014,
/// FR-R-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FunctionCode {
    /// 1 — Read Coils.
    ReadCoils,
    /// 2 — Read Discrete Inputs.
    ReadDiscreteInputs,
    /// 3 — Read Holding Registers.
    ReadHoldingRegisters,
    /// 4 — Read Input Registers.
    ReadInputRegisters,
    /// 5 — Write Single Coil.
    WriteSingleCoil,
    /// 6 — Write Single Register.
    WriteSingleRegister,
    /// 7 — Read Exception Status.
    ReadExceptionStatus,
    /// 8 — Diagnostics.
    Diagnostics,
    /// 11 — Get Comm Event Counter.
    GetCommEventCounter,
    /// 12 — Get Comm Event Log.
    GetCommEventLog,
    /// 15 — Write Multiple Coils.
    WriteMultipleCoils,
    /// 16 — Write Multiple Registers.
    WriteMultipleRegisters,
    /// 17 — Report Server ID.
    ReportServerId,
    /// 20 — Read File Record.
    ReadFileRecord,
    /// 21 — Write File Record.
    WriteFileRecord,
    /// 22 — Mask Write Register.
    MaskWriteRegister,
    /// 23 — Read/Write Multiple Registers.
    ReadWriteMultipleRegisters,
    /// 24 — Read FIFO Queue.
    ReadFifoQueue,
    /// 43 — Encapsulated Interface Transport.
    EncapsulatedInterfaceTransport,
    /// Any other code in 1–127, carried verbatim with an opaque body.
    Custom(u8),
}

impl FunctionCode {
    /// Every named code, in wire order. The single source of truth for the
    /// byte ↔ variant mapping.
    const NAMED: [(u8, Self); 19] = [
        (1, Self::ReadCoils),
        (2, Self::ReadDiscreteInputs),
        (3, Self::ReadHoldingRegisters),
        (4, Self::ReadInputRegisters),
        (5, Self::WriteSingleCoil),
        (6, Self::WriteSingleRegister),
        (7, Self::ReadExceptionStatus),
        (8, Self::Diagnostics),
        (11, Self::GetCommEventCounter),
        (12, Self::GetCommEventLog),
        (15, Self::WriteMultipleCoils),
        (16, Self::WriteMultipleRegisters),
        (17, Self::ReportServerId),
        (20, Self::ReadFileRecord),
        (21, Self::WriteFileRecord),
        (22, Self::MaskWriteRegister),
        (23, Self::ReadWriteMultipleRegisters),
        (24, Self::ReadFifoQueue),
        (43, Self::EncapsulatedInterfaceTransport),
    ];

    /// Decode a request-direction function code byte.
    ///
    /// Fails for 0 and for 128–255, which are exception space (FR-R-014,
    /// FR-R-015). Anything else in range that is not named becomes
    /// [`FunctionCode::Custom`] (FR-R-011).
    pub fn decode(byte: u8) -> Result<Self> {
        if byte == 0 || byte >= 0x80 {
            return Err(Error::InvalidFunctionCode(byte));
        }
        Ok(Self::NAMED
            .iter()
            .find(|(candidate, _)| *candidate == byte)
            .map_or(Self::Custom(byte), |(_, code)| *code))
    }

    /// Encode to its wire byte.
    ///
    /// A [`FunctionCode::Custom`] holding a code the crate names is rejected, so
    /// one wire byte keeps exactly one representation (FR-R-013).
    pub fn encode(self) -> Result<u8> {
        let byte = match self {
            Self::ReadCoils => 1,
            Self::ReadDiscreteInputs => 2,
            Self::ReadHoldingRegisters => 3,
            Self::ReadInputRegisters => 4,
            Self::WriteSingleCoil => 5,
            Self::WriteSingleRegister => 6,
            Self::ReadExceptionStatus => 7,
            Self::Diagnostics => 8,
            Self::GetCommEventCounter => 11,
            Self::GetCommEventLog => 12,
            Self::WriteMultipleCoils => 15,
            Self::WriteMultipleRegisters => 16,
            Self::ReportServerId => 17,
            Self::ReadFileRecord => 20,
            Self::WriteFileRecord => 21,
            Self::MaskWriteRegister => 22,
            Self::ReadWriteMultipleRegisters => 23,
            Self::ReadFifoQueue => 24,
            Self::EncapsulatedInterfaceTransport => 43,
            Self::Custom(byte) => {
                if byte == 0 || byte >= 0x80 {
                    return Err(Error::InvalidFunctionCode(byte));
                }
                if Self::NAMED.iter().any(|(named, _)| *named == byte) {
                    return Err(Error::ReservedCode(byte));
                }
                byte
            }
        };
        Ok(byte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// FR-R-010 — all nineteen public function codes are named, and each maps to
    /// its specified wire byte in both directions.
    fn ut_all_nineteen_public_codes_named() {
        assert_eq!(FunctionCode::NAMED.len(), 19);
        for (byte, code) in FunctionCode::NAMED {
            assert_eq!(FunctionCode::decode(byte), Ok(code), "decoding {byte}");
            assert_eq!(code.encode(), Ok(byte), "encoding {code:?}");
        }
    }

    #[test]
    /// FR-R-011 — an unnamed code in 1–127 becomes a custom code carrying the raw
    /// byte, including the user-defined ranges 65–72 and 100–110.
    fn ut_unnamed_code_becomes_custom() {
        for byte in [9, 10, 13, 14, 18, 19, 25, 42, 44, 65, 72, 100, 110, 127] {
            assert_eq!(
                FunctionCode::decode(byte),
                Ok(FunctionCode::Custom(byte)),
                "decoding {byte}"
            );
        }
    }

    #[test]
    /// FR-R-011 — every code in 1–127 decodes to exactly one representation, so
    /// no byte is both named and custom.
    fn ut_every_code_in_range_decodes_once() {
        for byte in 1..=127u8 {
            let decoded = FunctionCode::decode(byte).expect("1..=127 is always valid");
            let named = FunctionCode::NAMED.iter().any(|(b, _)| *b == byte);
            assert_eq!(
                matches!(decoded, FunctionCode::Custom(_)),
                !named,
                "byte {byte} disagrees with the named table"
            );
        }
    }

    #[test]
    /// FR-R-013 — encoding a custom code holding a named byte is a reserved-code
    /// error, so one wire code keeps exactly one representation.
    fn ut_custom_with_named_code_is_reserved_error() {
        for (byte, _) in FunctionCode::NAMED {
            assert_eq!(
                FunctionCode::Custom(byte).encode(),
                Err(Error::ReservedCode(byte)),
                "custom holding {byte}"
            );
        }
    }

    #[test]
    /// FR-R-014 — code 0 is invalid in either direction.
    fn ut_code_zero_is_invalid() {
        assert_eq!(FunctionCode::decode(0), Err(Error::InvalidFunctionCode(0)));
        assert_eq!(
            FunctionCode::Custom(0).encode(),
            Err(Error::InvalidFunctionCode(0))
        );
    }

    #[test]
    /// FR-R-015 — codes 128–255 never denote a request; they are exception
    /// space.
    fn ut_codes_above_127_invalid_as_request() {
        for byte in [128, 129, 131, 200, 255] {
            assert_eq!(
                FunctionCode::decode(byte),
                Err(Error::InvalidFunctionCode(byte)),
                "decoding {byte}"
            );
            assert_eq!(
                FunctionCode::Custom(byte).encode(),
                Err(Error::InvalidFunctionCode(byte)),
                "encoding custom {byte}"
            );
        }
    }

    #[test]
    /// FR-R-133 — decode and encode are inverse for every valid function code
    /// byte.
    fn ut_function_code_round_trips() {
        for byte in 1..=127u8 {
            let code = FunctionCode::decode(byte).expect("1..=127 is always valid");
            assert_eq!(code.encode(), Ok(byte), "round-tripping {byte}");
        }
    }
}
