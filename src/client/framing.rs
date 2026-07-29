//! The one thing a client does differently per framing (CL-R-003).
//!
//! Everything else about acting as an initiator is framing-agnostic, so this
//! trait names exactly the difference: how a header is built, when a received
//! header answers a sent one, and which unit identifier broadcasts.

use crate::frame::{Ascii, BROADCAST_UNIT, Framing, MbapHeader, Rtu, Tcp, TransactionId, UnitId};

/// A framing a client can issue requests over.
///
/// Public because it bounds a public type, and unsealed because [`Framing`] is:
/// a consumer with its own framing may drive the client over it.
pub trait ClientFraming: Framing {
    /// Build the header of an outgoing request (CL-R-010).
    fn request_header(unit: UnitId, transaction: TransactionId) -> Self::Header;

    /// Whether a received header answers the one sent (CL-R-020).
    fn is_response_to(sent: &Self::Header, received: &Self::Header) -> bool;

    /// Whether this unit identifier addresses every server at once (CL-R-050).
    fn is_broadcast(unit: UnitId) -> bool;
}

impl ClientFraming for Tcp {
    fn request_header(unit: UnitId, transaction: TransactionId) -> Self::Header {
        MbapHeader {
            transaction_id: transaction,
            unit_id: unit,
        }
    }

    /// Both fields must agree: the transaction identifier alone would accept a
    /// reply from another unit on a gateway (CL-R-020).
    fn is_response_to(sent: &Self::Header, received: &Self::Header) -> bool {
        sent == received
    }

    /// Modbus TCP has no broadcast: a unit identifier is a gateway's routing
    /// field, not an address every device answers to (CL-R-050).
    fn is_broadcast(_unit: UnitId) -> bool {
        false
    }
}

/// The serial framings share their whole client behavior: the header *is* the
/// address, and address 0 is the broadcast (FR-R-096, FR-R-117).
macro_rules! serial_framing {
    ($framing:ty) => {
        impl ClientFraming for $framing {
            /// The transaction identifier has nowhere to go on a serial line:
            /// the wire carries the address only.
            fn request_header(unit: UnitId, _transaction: TransactionId) -> Self::Header {
                unit
            }

            fn is_response_to(sent: &Self::Header, received: &Self::Header) -> bool {
                sent == received
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

    #[test]
    /// CL-R-010 — the header carries what its framing has room for: TCP takes
    /// both the unit and the transaction, a serial line only the unit.
    fn ut_request_header_per_framing() {
        assert_eq!(
            Tcp::request_header(UnitId(0x11), TransactionId(7)),
            MbapHeader {
                transaction_id: TransactionId(7),
                unit_id: UnitId(0x11),
            }
        );
        assert_eq!(
            Rtu::request_header(UnitId(0x11), TransactionId(7)),
            UnitId(0x11)
        );
        assert_eq!(
            Ascii::request_header(UnitId(0x11), TransactionId(7)),
            UnitId(0x11)
        );
    }

    #[test]
    /// CL-R-020 — a TCP response must agree in *both* header fields. A reply
    /// bearing the right transaction identifier but another unit is a different
    /// exchange, which a gateway can genuinely produce.
    fn ut_tcp_matches_on_both_header_fields() {
        let sent = Tcp::request_header(UnitId(0x11), TransactionId(7));
        assert!(Tcp::is_response_to(&sent, &sent));
        assert!(!Tcp::is_response_to(
            &sent,
            &Tcp::request_header(UnitId(0x12), TransactionId(7))
        ));
        assert!(!Tcp::is_response_to(
            &sent,
            &Tcp::request_header(UnitId(0x11), TransactionId(8))
        ));
    }

    #[test]
    /// CL-R-050 — broadcast is a property of the framing: unit 0 broadcasts on
    /// a serial line and nothing broadcasts on TCP.
    fn ut_broadcast_is_a_framing_property() {
        assert!(Rtu::is_broadcast(UnitId(0)));
        assert!(Ascii::is_broadcast(UnitId(0)));
        assert!(!Rtu::is_broadcast(UnitId(1)));
        assert!(!Tcp::is_broadcast(UnitId(0)));
    }
}
