//! The trait a consumer implements to answer requests (SV-R-003), and the
//! connection identity every notification carries (SV-R-036).

use core::future::Future;
use core::net::SocketAddr;

use crate::error::Error;
use crate::frame::{ExceptionCode, RequestPdu, ResponsePdu, UnitId};

/// Which connection a notification concerns (SV-R-031, SV-R-036).
///
/// The identifier, not the address, is the identity: an address is reused the
/// moment a socket closes, and a serial link has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connection {
    /// Unique for the lifetime of the server that issued it.
    id: ConnectionId,
    /// The peer, where the transport has one.
    peer: Option<SocketAddr>,
}

impl Connection {
    /// Name a connection. Server-internal: identifiers are the server's to
    /// allocate (SV-R-031).
    // Only the tests call this until the serving loop exists.
    #[allow(dead_code)]
    pub(crate) fn new(id: ConnectionId, peer: Option<SocketAddr>) -> Self {
        Self { id, peer }
    }

    /// This connection's identifier.
    #[must_use]
    pub fn id(&self) -> ConnectionId {
        self.id
    }

    /// The peer's address, or `None` on a link that has no address, such as a
    /// serial port.
    #[must_use]
    pub fn peer(&self) -> Option<SocketAddr> {
        self.peer
    }
}

/// A connection's identifier, unique within one server's lifetime (SV-R-031).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId(pub u64);

impl From<u64> for ConnectionId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<ConnectionId> for u64 {
    fn from(value: ConnectionId) -> Self {
        value.0
    }
}

/// Why a connection ended (SV-R-033).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disconnect {
    /// The peer closed the connection between two ADUs (SV-R-052).
    Closed,
    /// [`Service::on_connect`] declined it (SV-R-032).
    Rejected,
    /// A failure ended it: I/O, or a request that could not be decoded
    /// (SV-R-050).
    Failed(Error),
    /// The server is shutting down (SV-R-043).
    ShuttingDown,
}

/// What a Modbus server does with the requests it receives (SV-R-003).
///
/// Methods take `&self`, and the trait is `Send + Sync + 'static`, because one
/// service answers every connection at once (SV-R-030): mutable state lives
/// behind the implementor's own lock, which is the intended shape and not a
/// workaround. Only [`on_request`](Self::on_request) must be implemented
/// (SV-R-004).
///
/// The futures are declared `impl Future + Send` rather than as `async fn`
/// because a connection is handled in its own task, and `async fn` in a trait
/// promises no `Send` future. An implementation may still write `async fn`.
///
/// The crate supplies no implementation and no register tables (SV-R-005) — see
/// `docs/specs/server/data-contract.md`.
pub trait Service: Send + Sync + 'static {
    /// Answer one request, addressed to `unit`, received on `conn`
    /// (SV-R-010).
    ///
    /// Refusing is an [`ExceptionCode`], not an [`Error`]: a refusal is a Modbus
    /// answer, and the server sends it as an exception response to the function
    /// requested (SV-R-012). Whatever is returned is sent unaltered
    /// (SV-R-013).
    fn on_request(
        &self,
        conn: &Connection,
        unit: UnitId,
        request: RequestPdu,
    ) -> impl Future<Output = core::result::Result<ResponsePdu, ExceptionCode>> + Send;

    /// A connection has been taken up; answer whether to serve it (SV-R-032).
    ///
    /// Called before any request is read. `false` closes it unread. The default
    /// accepts.
    fn on_connect(&self, conn: &Connection) -> impl Future<Output = bool> + Send {
        let _ = conn;
        async { true }
    }

    /// A connection has ended, and why (SV-R-033). Called exactly once per
    /// connection that [`on_connect`](Self::on_connect) saw. The default
    /// ignores it.
    fn on_disconnect(
        &self,
        conn: &Connection,
        reason: Disconnect,
    ) -> impl Future<Output = ()> + Send {
        let _ = (conn, reason);
        async {}
    }

    /// A request failed (SV-R-034). Separate from
    /// [`on_disconnect`](Self::on_disconnect) because most such failures leave
    /// the connection running. The default ignores it.
    fn on_error(&self, conn: &Connection, error: &Error) -> impl Future<Output = ()> + Send {
        let _ = (conn, error);
        async {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::frame::{Address, ExceptionCode, Quantity, RegisterValue, RequestPdu, ResponsePdu};

    /// The smallest service SV-R-004 permits: one method.
    struct Minimal;

    impl Service for Minimal {
        async fn on_request(
            &self,
            _conn: &Connection,
            _unit: UnitId,
            _request: RequestPdu,
        ) -> core::result::Result<ResponsePdu, ExceptionCode> {
            Ok(ResponsePdu::ReadHoldingRegisters {
                registers: alloc::vec![RegisterValue(7)],
            })
        }
    }

    fn connection() -> Connection {
        Connection::new(ConnectionId(3), None)
    }

    #[tokio::test]
    /// SV-R-004, SV-R-032 — a service that implements only request handling
    /// accepts connections by default.
    async fn ut_minimal_service_accepts_by_default() {
        assert!(Minimal.on_connect(&connection()).await);
    }

    #[tokio::test]
    /// SV-R-004 — the notifications a minimal service does not implement are
    /// inert rather than absent.
    async fn ut_default_notifications_are_inert() {
        let conn = connection();
        Minimal.on_disconnect(&conn, Disconnect::Closed).await;
        Minimal.on_error(&conn, &Error::Malformed).await;
    }

    #[tokio::test]
    /// SV-R-010 — a request reaches the service with the unit it was addressed
    /// to, and the service's answer comes back.
    async fn ut_request_reaches_the_service() {
        assert_eq!(
            Minimal
                .on_request(
                    &connection(),
                    UnitId(9),
                    RequestPdu::ReadHoldingRegisters {
                        address: Address(0),
                        quantity: Quantity(1),
                    },
                )
                .await,
            Ok(ResponsePdu::ReadHoldingRegisters {
                registers: alloc::vec![RegisterValue(7)],
            })
        );
    }

    #[test]
    /// SV-R-031, SV-R-036 — a connection carries an identifier and, where the
    /// transport has one, a peer address.
    fn ut_connection_reports_its_identity() {
        let peer = "127.0.0.1:502".parse().expect("a literal address parses");
        let conn = Connection::new(ConnectionId(1), Some(peer));
        assert_eq!(conn.id(), ConnectionId(1));
        assert_eq!(conn.peer(), Some(peer));
    }

    #[test]
    /// SV-R-031 — a link with no address, such as a serial port, has no peer.
    fn ut_serial_connection_has_no_peer() {
        assert_eq!(connection().peer(), None);
    }
}
