//! The one thing a server does differently per framing (SV-R-001).
//!
//! A responder reads a header rather than building one, so what it needs from a
//! framing is the reverse of the client's [`ClientFraming`](crate::ClientFraming):
//! which unit a received header addresses, and whether that unit is the
//! broadcast every device acts on and none answers.

use crate::frame::{Ascii, BROADCAST_UNIT, Framing, Rtu, Tcp, UnitId};

/// A framing a server can answer requests over.
///
/// Public because it bounds a public method, and unsealed because [`Framing`]
/// is: a consumer with its own framing may serve over it.
pub trait ServerFraming: Framing {
    /// The unit identifier a received header addresses (SV-R-010).
    fn unit(header: &Self::Header) -> UnitId;

    /// Whether that identifier addresses every server at once, and so must not
    /// be answered (SV-R-023).
    fn is_broadcast(unit: UnitId) -> bool;
}

impl ServerFraming for Tcp {
    fn unit(header: &Self::Header) -> UnitId {
        header.unit_id
    }

    /// Modbus TCP has no broadcast: the unit identifier is a gateway's routing
    /// field, not an address every device answers to.
    fn is_broadcast(_unit: UnitId) -> bool {
        false
    }
}

/// On a serial line the header *is* the address, and address 0 is the broadcast
/// (FR-R-096, FR-R-117).
macro_rules! serial_framing {
    ($framing:ty) => {
        impl ServerFraming for $framing {
            fn unit(header: &Self::Header) -> UnitId {
                *header
            }

            fn is_broadcast(unit: UnitId) -> bool {
                unit == BROADCAST_UNIT
            }
        }
    };
}

serial_framing!(Rtu);
serial_framing!(Ascii);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::frame::{MbapHeader, TransactionId};

    #[test]
    /// SV-R-010 — the addressed unit comes out of whichever header the framing
    /// put it in.
    fn ut_unit_comes_out_of_any_header() {
        assert_eq!(
            Tcp::unit(&MbapHeader {
                transaction_id: TransactionId(7),
                unit_id: UnitId(0x11),
            }),
            UnitId(0x11)
        );
        assert_eq!(Rtu::unit(&UnitId(0x11)), UnitId(0x11));
        assert_eq!(Ascii::unit(&UnitId(0x11)), UnitId(0x11));
    }

    #[test]
    /// SV-R-023 — broadcast is a property of the framing: unit 0 broadcasts on a
    /// serial line and nothing broadcasts on TCP.
    fn ut_broadcast_is_a_framing_property_for_the_server() {
        assert!(Rtu::is_broadcast(UnitId(0)));
        assert!(Ascii::is_broadcast(UnitId(0)));
        assert!(!Rtu::is_broadcast(UnitId(1)));
        assert!(!Tcp::is_broadcast(UnitId(0)));
    }
}
