//! The response length a request determines (CL-R-118, CL-R-119).

use crate::frame::{RequestPdu, ResponsePdu};

/// The response PDU length in bytes, function code included, that `request`
/// determines (CL-R-119); `None` where none is determined.
pub(super) fn expected_response_len(request: &RequestPdu) -> Option<usize> {
    match request {
        RequestPdu::ReadCoils { quantity, .. }
        | RequestPdu::ReadDiscreteInputs { quantity, .. } => {
            Some(2 + usize::from(quantity.0).div_ceil(8))
        }
        RequestPdu::ReadHoldingRegisters { quantity, .. }
        | RequestPdu::ReadInputRegisters { quantity, .. } => Some(2 + 2 * usize::from(quantity.0)),
        RequestPdu::WriteSingleCoil { .. }
        | RequestPdu::WriteSingleRegister { .. }
        | RequestPdu::WriteMultipleCoils { .. }
        | RequestPdu::WriteMultipleRegisters { .. }
        | RequestPdu::GetCommEventCounter => Some(5),
        RequestPdu::ReadExceptionStatus => Some(2),
        RequestPdu::MaskWriteRegister { .. } => Some(7),
        RequestPdu::ReadWriteMultipleRegisters { read_quantity, .. } => {
            Some(2 + 2 * usize::from(read_quantity.0))
        }
        RequestPdu::Diagnostics { .. }
        | RequestPdu::GetCommEventLog
        | RequestPdu::ReportServerId
        | RequestPdu::ReadFileRecord { .. }
        | RequestPdu::WriteFileRecord { .. }
        | RequestPdu::ReadFifoQueue { .. }
        | RequestPdu::EncapsulatedInterfaceTransport(_)
        | RequestPdu::Custom { .. } => None,
    }
}

/// The wire length of a decoded response for the codes CL-R-119 checks.
fn received_len(response: &ResponsePdu) -> Option<usize> {
    match response {
        ResponsePdu::ReadCoils { coils: bits }
        | ResponsePdu::ReadDiscreteInputs { inputs: bits } => Some(2 + bits.len() / 8),
        ResponsePdu::ReadHoldingRegisters { registers }
        | ResponsePdu::ReadInputRegisters { registers }
        | ResponsePdu::ReadWriteMultipleRegisters { registers } => Some(2 + 2 * registers.len()),
        ResponsePdu::WriteSingleCoil { .. }
        | ResponsePdu::WriteSingleRegister { .. }
        | ResponsePdu::WriteMultipleCoils { .. }
        | ResponsePdu::WriteMultipleRegisters { .. }
        | ResponsePdu::GetCommEventCounter { .. } => Some(5),
        ResponsePdu::ReadExceptionStatus { .. } => Some(2),
        ResponsePdu::MaskWriteRegister { .. } => Some(7),
        ResponsePdu::Diagnostics { .. }
        | ResponsePdu::GetCommEventLog { .. }
        | ResponsePdu::ReportServerId { .. }
        | ResponsePdu::ReadFileRecord { .. }
        | ResponsePdu::WriteFileRecord { .. }
        | ResponsePdu::ReadFifoQueue { .. }
        | ResponsePdu::EncapsulatedInterfaceTransport(_)
        | ResponsePdu::Custom { .. }
        | ResponsePdu::Exception(_) => None,
    }
}

/// Whether `response` has the length `request` determines. An exception and a
/// code with no determined length always fit.
pub(super) fn fits(request: &RequestPdu, response: &ResponsePdu) -> bool {
    if matches!(response, ResponsePdu::Exception(_)) {
        return true;
    }
    match expected_response_len(request) {
        None => true,
        Some(n) => received_len(response) == Some(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{
        Address, ExceptionCode, ExceptionResponse, FunctionCode, Mask, Quantity, RegisterValue,
    };
    use alloc::vec;

    fn len(request: RequestPdu) -> Option<usize> {
        expected_response_len(&request)
    }

    fn coils(q: u16) -> Option<usize> {
        len(RequestPdu::ReadCoils {
            address: Address(0),
            quantity: Quantity(q),
        })
    }

    fn registers(q: u16) -> Option<usize> {
        len(RequestPdu::ReadHoldingRegisters {
            address: Address(0),
            quantity: Quantity(q),
        })
    }

    #[test]
    /// CL-R-119 — the response length each request determines.
    fn ut_expected_response_len_per_function_code() {
        assert_eq!(coils(1), Some(3));
        assert_eq!(coils(8), Some(3));
        assert_eq!(coils(9), Some(4));
        assert_eq!(coils(2000), Some(252));
        assert_eq!(registers(3), Some(8));
        assert_eq!(registers(125), Some(252));
        assert_eq!(
            len(RequestPdu::WriteSingleCoil {
                address: Address(0),
                value: true
            }),
            Some(5)
        );
        assert_eq!(
            len(RequestPdu::WriteSingleRegister {
                address: Address(0),
                value: RegisterValue(0)
            }),
            Some(5)
        );
        assert_eq!(
            len(RequestPdu::WriteMultipleCoils {
                address: Address(0),
                coils: vec![true]
            }),
            Some(5)
        );
        assert_eq!(
            len(RequestPdu::WriteMultipleRegisters {
                address: Address(0),
                registers: vec![RegisterValue(0)]
            }),
            Some(5)
        );
        assert_eq!(len(RequestPdu::ReadExceptionStatus), Some(2));
        assert_eq!(len(RequestPdu::GetCommEventCounter), Some(5));
        assert_eq!(
            len(RequestPdu::MaskWriteRegister {
                address: Address(0),
                and_mask: Mask(0),
                or_mask: Mask(0)
            }),
            Some(7)
        );
        assert_eq!(
            len(RequestPdu::ReadWriteMultipleRegisters {
                read_address: Address(0),
                read_quantity: Quantity(2),
                write_address: Address(0),
                registers: vec![RegisterValue(0)]
            }),
            Some(6)
        );
        assert_eq!(len(RequestPdu::GetCommEventLog), None);
        assert_eq!(len(RequestPdu::ReportServerId), None);
        assert_eq!(
            len(RequestPdu::Custom {
                code: 0x41,
                data: vec![]
            }),
            None
        );
    }

    #[test]
    /// CL-R-118 — a response fits when its decoded length is the determined
    /// one; an exception and an unchecked code always fit.
    fn ut_fits_compares_decoded_length() {
        let request = RequestPdu::ReadHoldingRegisters {
            address: Address(0),
            quantity: Quantity(2),
        };
        let regs = |n: usize| ResponsePdu::ReadHoldingRegisters {
            registers: vec![RegisterValue(0); n],
        };
        assert!(fits(&request, &regs(2)));
        assert!(!fits(&request, &regs(1)));
        assert!(!fits(
            &request,
            &ResponsePdu::ReadExceptionStatus {
                status: crate::frame::ExceptionStatus(0)
            }
        ));
        assert!(fits(
            &request,
            &ResponsePdu::Exception(ExceptionResponse {
                function: FunctionCode::ReadHoldingRegisters,
                exception: ExceptionCode::IllegalDataAddress
            })
        ));
        assert!(fits(
            &RequestPdu::ReportServerId,
            &ResponsePdu::ReportServerId { data: vec![1; 9] }
        ));
    }
}
