//! Async Modbus server (responder). See `docs/specs/server/`.

mod framing;
mod handle;
mod service;

use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, Ordering};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::watch;
use tokio::task::JoinSet;

use crate::error::{Error, Result};
use crate::frame::{ExceptionResponse, ResponsePdu, Tcp, UnitId};
use crate::transport::FrameTransport;

pub use framing::ServerFraming;
pub use handle::ServerHandle;
pub use service::{Acceptance, Connection, ConnectionId, Disconnect, Service};

/// How a server answers (SV-R-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ServerConfig {
    /// The only unit identifier to answer, or `None` to leave the decision to
    /// the service (SV-R-020, SV-R-022).
    pub unit: Option<UnitId>,
}

/// A Modbus server (SV-R-001).
///
/// One type for every framing and for every service: what differs between RTU,
/// ASCII, and TCP is named by [`ServerFraming`], and what a request *means* is
/// named by [`Service`] — the crate supplies neither a data model nor a service
/// of its own (SV-R-005).
#[derive(Debug)]
pub struct Server<S> {
    /// The one service every connection shares (SV-R-002).
    service: Arc<S>,
    /// What this server answers (SV-R-008).
    config: ServerConfig,
    /// The identifier the next connection will carry (SV-R-031).
    next_connection: AtomicU64,
    /// Set once shutdown has been requested (SV-R-040).
    ///
    /// Receivers are subscribed per connection, so the sender's `closed()` is
    /// the drain of SV-R-044: it completes when the last of them is gone.
    shutdown: Arc<watch::Sender<bool>>,
}

impl<S> Server<S>
where
    S: Service,
{
    /// Build a server around a service, with the default configuration.
    pub fn new(service: S) -> Self {
        Self::with_config(service, ServerConfig::default())
    }

    /// Build a server around a service, configured (SV-R-002).
    pub fn with_config(service: S, config: ServerConfig) -> Self {
        Self {
            service: Arc::new(service),
            config,
            next_connection: AtomicU64::new(1),
            shutdown: Arc::new(watch::Sender::new(false)),
        }
    }

    /// A handle by which this server is shut down (SV-R-040).
    ///
    /// Taken before serving, since serving consumes the server.
    #[must_use]
    pub fn handle(&self) -> ServerHandle {
        ServerHandle::new(Arc::clone(&self.shutdown))
    }

    /// Serve one already-established transport, such as a serial link
    /// (SV-R-007).
    ///
    /// Returns when the link ends. A failure of the link itself is the
    /// connection's, not the server's, so it is reported to the service rather
    /// than returned (SV-R-051).
    ///
    /// # Errors
    ///
    /// Currently infallible; the result is part of the signature so that a
    /// future serving failure needs no API change.
    pub async fn serve_link<T, F>(self, mut transport: FrameTransport<T, F>) -> Result<()>
    where
        T: AsyncRead + AsyncWrite + Unpin + Send,
        F: ServerFraming,
    {
        let conn = Connection::new(self.next_id(), None);
        let mut signal = self.shutdown.subscribe();
        serve_connection(
            self.service.as_ref(),
            &self.config,
            &conn,
            &mut transport,
            &mut signal,
        )
        .await;
        Ok(())
    }

    /// Serve a listening socket, handling every connection it accepts
    /// concurrently (SV-R-007, SV-R-030).
    ///
    /// Returns when accepting fails. A failure confined to one connection is
    /// reported to the service and never returned (SV-R-035, SV-R-051).
    ///
    /// # Errors
    ///
    /// Fails if the listener does. Connections already running are finished
    /// before the failure is returned.
    pub async fn serve(self, listener: crate::transport::TcpListener) -> Result<()> {
        self.serve_framed::<Tcp>(listener).await
    }

    /// Serve a listening socket, for any framing (SV-R-053).
    ///
    /// `serve` is this with `F` fixed to [`Tcp`] under its existing name, so a
    /// server that only ever answered Modbus TCP needs no change. A gateway
    /// server accepting `RtuOverTcp` connections runs this instead, and gets
    /// the identical per-connection behavior (SV-R-007).
    ///
    /// # Errors
    ///
    /// Fails if the listener does. Connections already running are finished
    /// before the failure is returned.
    pub async fn serve_framed<F>(self, listener: crate::transport::TcpListener) -> Result<()>
    where
        F: ServerFraming + Send + 'static,
        F::Header: Send + Sync,
    {
        let mut connections: JoinSet<()> = JoinSet::new();
        let mut signal = self.shutdown.subscribe();
        loop {
            let accepted = tokio::select! {
                // Shutdown wins a tie: SV-R-041 forbids taking up another
                // connection once it has been requested.
                biased;
                () = shutdown_requested(&mut signal) => {
                    // The connections already accepted are owed their end
                    // (SV-R-043), and the drain of SV-R-044 waits for them.
                    while connections.join_next().await.is_some() {}
                    return Ok(());
                }
                accepted = listener.accept_framed::<F>() => accepted,
            };
            match accepted {
                Ok((mut transport, peer)) => {
                    let service = Arc::clone(&self.service);
                    let config = self.config;
                    let conn = Connection::new(self.next_id(), Some(peer));
                    let mut signal = self.shutdown.subscribe();
                    // A task each, so one connection's handler never delays
                    // another's (SV-R-030).
                    connections.spawn(async move {
                        serve_connection(
                            service.as_ref(),
                            &config,
                            &conn,
                            &mut transport,
                            &mut signal,
                        )
                        .await;
                    });
                }
                Err(error) => {
                    // The listener is gone, but the connections it accepted are
                    // still owed their end (SV-R-033).
                    while connections.join_next().await.is_some() {}
                    return Err(error);
                }
            }
        }
    }

    /// Allocate the next connection identifier (SV-R-031).
    fn next_id(&self) -> ConnectionId {
        ConnectionId(self.next_connection.fetch_add(1, Ordering::Relaxed))
    }
}

/// Run one connection from its first notification to its last (SV-R-032,
/// SV-R-033).
async fn serve_connection<S, T, F>(
    service: &S,
    config: &ServerConfig,
    conn: &Connection,
    transport: &mut FrameTransport<T, F>,
    signal: &mut watch::Receiver<bool>,
) where
    S: Service,
    T: AsyncRead + AsyncWrite + Unpin + Send,
    F: ServerFraming,
{
    if service.on_connect(conn).await == Acceptance::Reject {
        // Refused before a request is read (SV-R-032).
        service.on_disconnect(conn, Disconnect::Rejected).await;
        return;
    }
    let reason = exchange(service, config, conn, transport, signal).await;
    service.on_disconnect(conn, reason).await;
}

/// Answer requests until the connection ends, and say why it did (SV-R-015).
async fn exchange<S, T, F>(
    service: &S,
    config: &ServerConfig,
    conn: &Connection,
    transport: &mut FrameTransport<T, F>,
    signal: &mut watch::Receiver<bool>,
) -> Disconnect
where
    S: Service,
    T: AsyncRead + AsyncWrite + Unpin + Send,
    F: ServerFraming,
{
    loop {
        let received = tokio::select! {
            // Only the *read* is abandoned: a request already dispatched runs
            // to completion below (SV-R-041, SV-R-042).
            biased;
            () = shutdown_requested(signal) => return Disconnect::ShuttingDown,
            received = transport.recv_request() => received,
        };
        let (header, request) = match received {
            Ok(received) => received,
            // A close between two ADUs is an ordinary end, not a failure
            // (SV-R-052, TR-R-014).
            Err(Error::Io {
                kind: std::io::ErrorKind::UnexpectedEof,
            }) => return Disconnect::Closed,
            Err(error) => {
                service.on_error(conn, &error).await;
                // An I/O failure ends the connection whatever the framing. A
                // *frame* failure ends it only where the next boundary was
                // carried by the frame that failed; on a self-locating framing
                // one bad frame costs one frame, and a noise burst must not
                // take a device off the bus (SV-R-050, FR-R-144).
                if error.ends_stream() || !F::boundary().is_self_locating() {
                    return Disconnect::Failed(error);
                }
                continue;
            }
        };

        let unit = F::unit(&header);
        // A broadcast is dispatched whatever the configuration, and never
        // answered (SV-R-023). It is tested before the unit filter for exactly
        // that reason.
        let broadcast = F::is_broadcast(unit);
        if !broadcast && config.unit.is_some_and(|configured| configured != unit) {
            // Another device's request: answering it would corrupt that
            // exchange (SV-R-021).
            continue;
        }

        // Kept before the request moves into the service, for the exception
        // response it may need (SV-R-012).
        let function = request.function();

        let response = match service.on_request(conn, unit, request).await {
            Ok(response) => response,
            Err(exception) => ResponsePdu::Exception(ExceptionResponse {
                function,
                exception,
            }),
        };

        if broadcast {
            // No device answers a broadcast (SV-R-023).
            continue;
        }

        if let Err(error) = transport.send_response(&header, &response).await {
            service.on_error(conn, &error).await;
            if error.ends_stream() {
                return Disconnect::Failed(error);
            }
            // The response never reached the wire, so the stream is still
            // aligned and the next request can be answered (SV-R-014).
        }
    }
}

/// Resolve as soon as shutdown has been requested (SV-R-041).
///
/// `changed` alone would miss a request made *before* this receiver subscribed,
/// so the current value is checked first.
async fn shutdown_requested(signal: &mut watch::Receiver<bool>) {
    loop {
        if *signal.borrow_and_update() {
            return;
        }
        if signal.changed().await.is_err() {
            // The server is gone, which is as final as a shutdown.
            return;
        }
    }
}

/// Whether a failure to answer left the connection unusable.
///
#[cfg(test)]
mod tests {
    use super::*;

    use alloc::sync::Arc;
    use alloc::vec::Vec;
    use std::sync::Mutex;

    use core::net::SocketAddr;

    use tokio::io::{DuplexStream, duplex};

    use crate::transport::{TcpListener, connect_tcp};

    use crate::error::Error;
    use crate::frame::{
        Address, ExceptionCode, ExceptionResponse, Framing, FunctionCode, MbapHeader, Quantity,
        RegisterValue, RequestPdu, ResponsePdu, Rtu, Tcp, TransactionId, UnitId,
    };
    use crate::transport::FrameTransport;
    use core::time::Duration;

    /// What a test service was asked to do, in order (SV-R-036).
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Event {
        Connect(ConnectionId, Option<SocketAddr>),
        Request(ConnectionId, UnitId, RequestPdu),
        Failed(Error),
        Disconnect(Disconnect),
    }

    /// How a test service answers.
    type Reply =
        Box<dyn Fn(&RequestPdu) -> core::result::Result<ResponsePdu, ExceptionCode> + Send + Sync>;

    /// A service that records what it was asked and answers as told.
    ///
    /// The shape SV-R-003 expects of a real one: shared by reference, with its
    /// own lock around its own state.
    struct Recorder {
        events: Mutex<Vec<Event>>,
        reply: Reply,
        accept: Acceptance,
        /// Held open until as many requests are in flight at once (SV-R-030).
        overlap: Option<Arc<tokio::sync::Barrier>>,
        /// Holds every request until the test releases a permit (SV-R-042).
        hold: Option<Arc<tokio::sync::Semaphore>>,
    }

    impl Recorder {
        fn new(
            reply: impl Fn(&RequestPdu) -> core::result::Result<ResponsePdu, ExceptionCode>
            + Send
            + Sync
            + 'static,
        ) -> Arc<Self> {
            Arc::new(Self {
                events: Mutex::new(Vec::new()),
                reply: Box::new(reply),
                accept: Acceptance::Accept,
                overlap: None,
                hold: None,
            })
        }

        /// A service that answers no request until `at_once` of them are in
        /// flight together, so a server that serialises them deadlocks.
        fn overlapping(at_once: usize) -> Arc<Self> {
            Arc::new(Self {
                events: Mutex::new(Vec::new()),
                reply: Box::new(|_| Ok(registers())),
                accept: Acceptance::Accept,
                overlap: Some(Arc::new(tokio::sync::Barrier::new(at_once))),
                hold: None,
            })
        }

        /// A service that answers nothing until the test says so, so a request
        /// can be caught in flight (SV-R-042).
        fn holding() -> Arc<Self> {
            Arc::new(Self {
                events: Mutex::new(Vec::new()),
                reply: Box::new(|_| Ok(registers())),
                accept: Acceptance::Accept,
                overlap: None,
                hold: Some(Arc::new(tokio::sync::Semaphore::new(0))),
            })
        }

        /// Let one held request proceed.
        fn release(&self) {
            self.hold
                .as_ref()
                .expect("only a holding recorder is released")
                .add_permits(1);
        }

        /// Wait until the service has been asked something.
        async fn awaited_a_request(&self) {
            while !self
                .events()
                .iter()
                .any(|event| matches!(event, Event::Request(..)))
            {
                tokio::task::yield_now().await;
            }
        }

        /// A service that refuses every connection (SV-R-032).
        fn refusing() -> Arc<Self> {
            Arc::new(Self {
                events: Mutex::new(Vec::new()),
                reply: Box::new(|_| Err(ExceptionCode::IllegalFunction)),
                accept: Acceptance::Reject,
                overlap: None,
                hold: None,
            })
        }

        fn push(&self, event: Event) {
            self.events
                .lock()
                .expect("no test panics while holding the lock")
                .push(event);
        }

        fn events(&self) -> Vec<Event> {
            self.events
                .lock()
                .expect("no test panics while holding the lock")
                .clone()
        }
    }

    impl Service for Arc<Recorder> {
        async fn on_request(
            &self,
            conn: &Connection,
            unit: UnitId,
            request: RequestPdu,
        ) -> core::result::Result<ResponsePdu, ExceptionCode> {
            self.push(Event::Request(conn.id(), unit, request.clone()));
            if let Some(overlap) = self.overlap.as_ref() {
                overlap.wait().await;
            }
            if let Some(hold) = self.hold.as_ref() {
                let permit = hold.acquire().await.expect("the test never closes it");
                permit.forget();
            }
            (self.reply)(&request)
        }

        async fn on_connect(&self, conn: &Connection) -> Acceptance {
            self.push(Event::Connect(conn.id(), conn.peer()));
            self.accept
        }

        async fn on_disconnect(&self, conn: &Connection, reason: Disconnect) {
            let _ = conn;
            self.push(Event::Disconnect(reason));
        }

        async fn on_error(&self, conn: &Connection, error: &Error) {
            let _ = conn;
            self.push(Event::Failed(error.clone()));
        }
    }

    /// A serving task over one duplex link, and the initiator's end of it.
    fn link(
        service: Arc<Recorder>,
    ) -> (
        tokio::task::JoinHandle<crate::error::Result<()>>,
        FrameTransport<DuplexStream, Tcp>,
    ) {
        serving(service, ServerConfig::default())
    }

    fn serving(
        service: Arc<Recorder>,
        config: ServerConfig,
    ) -> (
        tokio::task::JoinHandle<crate::error::Result<()>>,
        FrameTransport<DuplexStream, Tcp>,
    ) {
        let (server_end, client_end) = duplex(1024);
        let server = Server::with_config(service, config);
        (
            tokio::spawn(server.serve_link(FrameTransport::<_, Tcp>::new(server_end))),
            FrameTransport::new(client_end),
        )
    }

    fn registers() -> ResponsePdu {
        ResponsePdu::ReadHoldingRegisters {
            registers: alloc::vec![RegisterValue(0x022B)],
        }
    }

    fn read_holding() -> RequestPdu {
        RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(1),
        }
    }

    fn header(transaction: u16, unit: u8) -> MbapHeader {
        MbapHeader {
            transaction_id: TransactionId(transaction),
            unit_id: UnitId(unit),
        }
    }

    #[tokio::test]
    /// SV-R-010, SV-R-011 — a request is dispatched with the unit it was
    /// addressed to, and the answer comes back under the request's own header.
    async fn ut_serve_link_answers_a_request() {
        let service = Recorder::new(|_| Ok(registers()));
        let (serving, mut client) = link(Arc::clone(&service));

        client
            .send_request(&header(7, 0x11), &read_holding())
            .await
            .expect("writes a request");
        assert_eq!(
            client.recv_response().await,
            Ok((header(7, 0x11), registers()))
        );

        drop(client);
        assert_eq!(serving.await.expect("the server task finishes"), Ok(()));
        assert_eq!(
            service.events(),
            alloc::vec![
                Event::Connect(ConnectionId(1), None),
                Event::Request(ConnectionId(1), UnitId(0x11), read_holding()),
                Event::Disconnect(Disconnect::Closed),
            ]
        );
    }

    #[tokio::test]
    /// SV-R-012 — a service's refusal reaches the wire as an exception response
    /// to the function it refused.
    async fn ut_refusal_becomes_an_exception_response() {
        let service = Recorder::new(|_| Err(ExceptionCode::IllegalDataAddress));
        let (serving, mut client) = link(service);

        client
            .send_request(&header(1, 1), &read_holding())
            .await
            .expect("writes a request");
        assert_eq!(
            client.recv_response().await,
            Ok((
                header(1, 1),
                ResponsePdu::Exception(ExceptionResponse {
                    function: FunctionCode::ReadHoldingRegisters,
                    exception: ExceptionCode::IllegalDataAddress,
                })
            ))
        );
        drop(client);
        serving
            .await
            .expect("the server task finishes")
            .expect("ok");
    }

    #[tokio::test]
    /// SV-R-013 — the server sends what the service returned, even when it
    /// answers one function with another's response.
    async fn ut_response_is_sent_unaltered() {
        let service = Recorder::new(|_| {
            Ok(ResponsePdu::ReadCoils {
                coils: alloc::vec![true],
            })
        });
        let (serving, mut client) = link(service);

        client
            .send_request(&header(1, 1), &read_holding())
            .await
            .expect("writes a request");
        // The wire carries a whole byte of bits (FR-R-024) and a decoder has no
        // quantity to truncate by, so the padding comes back too — the initiator
        // discards it (CL-R-062). What matters here is that it is a *coil*
        // response answering a register request: the server did not intervene.
        assert_eq!(
            client.recv_response().await,
            Ok((
                header(1, 1),
                ResponsePdu::ReadCoils {
                    coils: alloc::vec![true, false, false, false, false, false, false, false],
                }
            ))
        );
        drop(client);
        serving
            .await
            .expect("the server task finishes")
            .expect("ok");
    }

    #[tokio::test]
    /// SV-R-014, SV-R-034 — a response that will not encode is reported and
    /// costs only its own request: the connection answers the next one.
    async fn ut_unencodable_response_reports_and_continues() {
        let service = Recorder::new(|request| match request {
            RequestPdu::ReadHoldingRegisters { quantity, .. } if quantity.0 == 1 => {
                Ok(ResponsePdu::ReadHoldingRegisters {
                    registers: alloc::vec![RegisterValue(0); 130],
                })
            }
            _ => Ok(registers()),
        });
        let (serving, mut client) = link(Arc::clone(&service));

        client
            .send_request(&header(1, 1), &read_holding())
            .await
            .expect("writes a request");
        client
            .send_request(
                &header(2, 1),
                &RequestPdu::ReadHoldingRegisters {
                    address: Address(0),
                    quantity: Quantity(2),
                },
            )
            .await
            .expect("writes a second request");
        assert_eq!(
            client.recv_response().await,
            Ok((header(2, 1), registers())),
            "the first request draws no response, the second is answered"
        );

        drop(client);
        serving
            .await
            .expect("the server task finishes")
            .expect("ok");
        assert!(
            service
                .events()
                .iter()
                .any(|event| matches!(event, Event::Failed(Error::PduTooLarge { .. }))),
            "the encode failure must be reported: {:?}",
            service.events()
        );
    }

    #[tokio::test]
    /// SV-R-015 — one connection serves successive requests.
    async fn ut_successive_requests_on_one_link() {
        let service = Recorder::new(|_| Ok(registers()));
        let (serving, mut client) = link(Arc::clone(&service));

        for transaction in 1..=4 {
            client
                .send_request(&header(transaction, 1), &read_holding())
                .await
                .expect("writes a request");
            assert_eq!(
                client.recv_response().await,
                Ok((header(transaction, 1), registers()))
            );
        }

        drop(client);
        serving
            .await
            .expect("the server task finishes")
            .expect("ok");
        assert_eq!(
            service
                .events()
                .iter()
                .filter(|event| matches!(event, Event::Request(..)))
                .count(),
            4
        );
    }

    #[tokio::test]
    /// SV-R-032 — a service that answers `Acceptance::Reject` gets a connection
    /// closed without a request being read, ending with the refusing reason.
    async fn ut_refused_connection_reads_nothing() {
        let service = Recorder::refusing();
        let (serving, mut client) = link(Arc::clone(&service));

        client
            .send_request(&header(1, 1), &read_holding())
            .await
            .expect("writes a request into a connection that will not read it");
        serving
            .await
            .expect("the server task finishes")
            .expect("ok");

        assert_eq!(
            service.events(),
            alloc::vec![
                Event::Connect(ConnectionId(1), None),
                Event::Disconnect(Disconnect::Rejected),
            ]
        );
    }

    #[tokio::test]
    /// SV-R-033, SV-R-052 — a peer that closes between two ADUs ends the
    /// connection cleanly, not as a failure, and is notified once.
    async fn ut_clean_close_ends_the_connection_cleanly() {
        let service = Recorder::new(|_| Ok(registers()));
        let (serving, client) = link(Arc::clone(&service));

        drop(client);
        serving
            .await
            .expect("the server task finishes")
            .expect("ok");

        assert_eq!(
            service
                .events()
                .iter()
                .filter(|event| matches!(event, Event::Disconnect(_)))
                .collect::<Vec<_>>(),
            alloc::vec![&Event::Disconnect(Disconnect::Closed)]
        );
    }

    #[tokio::test]
    /// SV-R-050, SV-R-051 — over TCP an undecodable request is reported and
    /// ends the connection, but serving itself succeeds: the failure was the
    /// peer's. The MBAP length was trusted to read the ADU, so once its
    /// contents turn out to be nonsense there is no way to find the next one.
    async fn ut_undecodable_request_ends_the_connection_on_tcp() {
        let service = Recorder::new(|_| Ok(registers()));
        let (serving, client) = link(Arc::clone(&service));

        let mut stream = client.into_inner();
        // A well-formed MBAP header over a function code no request may carry
        // (FR-R-014).
        tokio::io::AsyncWriteExt::write_all(&mut stream, &[0, 1, 0, 0, 0, 2, 1, 0])
            .await
            .expect("writes a malformed request");
        serving
            .await
            .expect("the server task finishes")
            .expect("ok");

        let events = service.events();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Failed(Error::InvalidFunctionCode(0)))),
            "the decode failure must be reported: {events:?}"
        );
        assert!(
            matches!(
                events.last(),
                Some(Event::Disconnect(Disconnect::Failed(
                    Error::InvalidFunctionCode(0)
                )))
            ),
            "and must end the connection: {events:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    /// SV-R-050 — over RTU the same failure costs exactly one frame: the next
    /// boundary is the line falling silent, so the server reports the failure,
    /// answers nothing, and serves the next request. One noise burst must not
    /// take a device off the bus.
    async fn ut_undecodable_request_continues_on_rtu() {
        let service = Recorder::new(|_| Ok(registers()));
        let (server_end, mut client_end) = duplex(1024);
        let server = Server::with_config(Arc::clone(&service), ServerConfig::default());
        let serving = tokio::spawn(server.serve_link(FrameTransport::<_, Rtu>::new(server_end)));

        // A frame whose CRC is wrong: a valid request with its last byte
        // flipped.
        let good = Rtu::encode_request(&UnitId(0x11), &read_holding()).expect("encodes");
        let mut corrupt = good.clone();
        let last = corrupt.last_mut().expect("the ADU carries a CRC");
        *last ^= 0xFF;

        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        client_end
            .write_all(&corrupt)
            .await
            .expect("writes corrupt");
        tokio::time::sleep(Duration::from_millis(5)).await;
        client_end.write_all(&good).await.expect("writes good");

        // The answer to the *second* request proves the first cost one frame
        // and nothing more.
        let mut reply = [0u8; 7];
        client_end
            .read_exact(&mut reply)
            .await
            .expect("the second request is answered");
        assert_eq!(
            Rtu::decode_response(&reply),
            Ok((UnitId(0x11), registers()))
        );

        let events = service.events();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Failed(Error::Checksum { .. }))),
            "the decode failure must still be reported: {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::Disconnect(_))),
            "the connection must survive it: {events:?}"
        );
        serving.abort();
    }

    #[tokio::test]
    /// SV-R-020, SV-R-021 — a configured unit answers only itself, and a request
    /// for another unit draws no response without ending the connection.
    async fn ut_configured_unit_ignores_other_units() {
        let service = Recorder::new(|_| Ok(registers()));
        let (serving, mut client) = serving(
            Arc::clone(&service),
            ServerConfig {
                unit: Some(UnitId(1)),
            },
        );

        client
            .send_request(&header(1, 2), &read_holding())
            .await
            .expect("writes a request to another unit");
        client
            .send_request(&header(2, 1), &read_holding())
            .await
            .expect("writes a request to the configured unit");
        assert_eq!(
            client.recv_response().await,
            Ok((header(2, 1), registers())),
            "the first response must be the one to unit 1"
        );

        drop(client);
        serving
            .await
            .expect("the server task finishes")
            .expect("serving succeeds");
        assert_eq!(
            service
                .events()
                .iter()
                .filter(|event| matches!(event, Event::Request(..)))
                .collect::<Vec<_>>(),
            alloc::vec![&Event::Request(ConnectionId(1), UnitId(1), read_holding())],
            "the unit that does not match must never reach the service"
        );
    }

    #[tokio::test]
    /// SV-R-008, SV-R-022 — with no unit configured, which is the default, every
    /// identifier reaches the service.
    async fn ut_unconfigured_unit_dispatches_every_unit() {
        let service = Recorder::new(|_| Ok(registers()));
        let (serving, mut client) = link(Arc::clone(&service));

        for unit in [2u8, 7, 247] {
            client
                .send_request(&header(u16::from(unit), unit), &read_holding())
                .await
                .expect("writes a request");
            assert_eq!(
                client.recv_response().await,
                Ok((header(u16::from(unit), unit), registers()))
            );
        }

        drop(client);
        serving
            .await
            .expect("the server task finishes")
            .expect("serving succeeds");
        assert_eq!(
            service
                .events()
                .iter()
                .filter_map(|event| match event {
                    Event::Request(_, unit, _) => Some(*unit),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            alloc::vec![UnitId(2), UnitId(7), UnitId(247)]
        );
    }

    #[tokio::test]
    /// SV-R-023 — a broadcast is dispatched and never answered, even when the
    /// server is configured for another unit.
    async fn ut_broadcast_is_dispatched_but_unanswered() {
        let service = Recorder::new(|_| Ok(registers()));
        let (server_end, client_end) = duplex(1024);
        let serving = tokio::spawn(
            Server::with_config(
                Arc::clone(&service),
                ServerConfig {
                    unit: Some(UnitId(1)),
                },
            )
            .serve_link(FrameTransport::<_, Rtu>::new(server_end)),
        );
        let mut client = FrameTransport::<_, Rtu>::new(client_end);

        client
            .send_request(&UnitId(0), &read_holding())
            .await
            .expect("broadcasts a request");
        // RTU frames are separated by silence, not by a length field (TR-R-011):
        // without a gap the two requests arrive as one malformed ADU.
        tokio::time::sleep(core::time::Duration::from_millis(5)).await;
        client
            .send_request(&UnitId(1), &read_holding())
            .await
            .expect("writes a request to the configured unit");
        assert_eq!(
            client.recv_response().await,
            Ok((UnitId(1), registers())),
            "the only response may be the one to unit 1"
        );

        drop(client);
        serving
            .await
            .expect("the server task finishes")
            .expect("serving succeeds");
        assert_eq!(
            service
                .events()
                .iter()
                .filter_map(|event| match event {
                    Event::Request(_, unit, _) => Some(*unit),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            alloc::vec![UnitId(0), UnitId(1)],
            "the broadcast must reach the service despite the unit filter"
        );
    }

    /// An ephemeral loopback address: port 0, so the kernel assigns one.
    fn ephemeral() -> SocketAddr {
        SocketAddr::from((core::net::Ipv4Addr::LOCALHOST, 0))
    }

    #[tokio::test]
    /// SV-R-002, SV-R-030 — one service, shared by every connection, and the
    /// connections are served concurrently. Every request is held
    /// until all three have arrived, so a server that answered them one at a
    /// time would never finish this test.
    async fn ut_connections_are_served_concurrently() {
        let service = Recorder::overlapping(3);
        let listener = TcpListener::bind(ephemeral()).await.expect("binds");
        let address = listener.local_addr().expect("reports its address");
        let serving = tokio::spawn(Server::new(Arc::clone(&service)).serve(listener));

        let mut clients = Vec::new();
        for _ in 0..3u16 {
            clients.push(tokio::spawn(async move {
                let mut client = connect_tcp(address, crate::transport::TcpConfig::default())
                    .await
                    .expect("connects");
                client
                    .send_request(&header(1, 1), &read_holding())
                    .await
                    .expect("writes a request");
                client.recv_response().await
            }));
        }

        for client in clients {
            assert_eq!(
                tokio::time::timeout(core::time::Duration::from_secs(5), client)
                    .await
                    .expect("three overlapping requests complete")
                    .expect("the client task finishes"),
                Ok((header(1, 1), registers()))
            );
        }
        serving.abort();
    }

    #[tokio::test]
    /// SV-R-031, SV-R-036 — each accepted connection is identified in accept
    /// order and carries the peer's address.
    async fn ut_accepted_connections_are_identified_in_order() {
        let service = Recorder::new(|_| Ok(registers()));
        let listener = TcpListener::bind(ephemeral()).await.expect("binds");
        let address = listener.local_addr().expect("reports its address");
        let serving = tokio::spawn(Server::new(Arc::clone(&service)).serve(listener));

        let mut peers = Vec::new();
        for transaction in 1..=2u16 {
            let stream = tokio::net::TcpStream::connect(address)
                .await
                .expect("connects");
            peers.push(
                stream
                    .local_addr()
                    .expect("a connected socket has an address"),
            );
            let mut client = FrameTransport::<_, Tcp>::new(stream);
            client
                .send_request(&header(transaction, 1), &read_holding())
                .await
                .expect("writes a request");
            assert_eq!(
                client.recv_response().await,
                Ok((header(transaction, 1), registers()))
            );
        }

        assert_eq!(
            service
                .events()
                .iter()
                .filter_map(|event| match event {
                    Event::Connect(id, peer) => Some((*id, *peer)),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            peers
                .iter()
                .enumerate()
                .map(|(index, peer)| {
                    (
                        ConnectionId(u64::try_from(index).expect("two fits a u64") + 1),
                        Some(*peer),
                    )
                })
                .collect::<Vec<_>>()
        );
        serving.abort();
    }

    #[tokio::test]
    /// SV-R-035 — one connection failing neither disturbs another nor stops the
    /// server accepting.
    async fn ut_one_failed_connection_does_not_disturb_the_others() {
        let service = Recorder::new(|_| Ok(registers()));
        let listener = TcpListener::bind(ephemeral()).await.expect("binds");
        let address = listener.local_addr().expect("reports its address");
        let serving = tokio::spawn(Server::new(Arc::clone(&service)).serve(listener));

        let mut broken = tokio::net::TcpStream::connect(address)
            .await
            .expect("connects");
        tokio::io::AsyncWriteExt::write_all(&mut broken, &[0, 1, 0, 0, 0, 2, 1, 0])
            .await
            .expect("writes a malformed request");
        drop(broken);

        let mut sound = connect_tcp(address, crate::transport::TcpConfig::default())
            .await
            .expect("connects after another connection failed");
        sound
            .send_request(&header(1, 1), &read_holding())
            .await
            .expect("writes a request");
        assert_eq!(sound.recv_response().await, Ok((header(1, 1), registers())));
        serving.abort();
    }

    #[tokio::test]
    /// SV-R-040, SV-R-041, SV-R-043, SV-R-044 — shutdown ends an idle
    /// connection with the shutting-down reason, and returns only once serving
    /// has finished.
    async fn ut_shutdown_ends_an_idle_connection() {
        let service = Recorder::new(|_| Ok(registers()));
        let (server_end, client_end) = duplex(1024);
        let server = Server::new(Arc::clone(&service));
        let handle = server.handle();
        let serving = tokio::spawn(server.serve_link(FrameTransport::<_, Tcp>::new(server_end)));
        let mut client = FrameTransport::<_, Tcp>::new(client_end);

        client
            .send_request(&header(1, 1), &read_holding())
            .await
            .expect("writes a request");
        assert_eq!(
            client.recv_response().await,
            Ok((header(1, 1), registers()))
        );

        handle.shutdown().await;
        assert!(
            serving.is_finished(),
            "shutdown may not return while serving runs (SV-R-044)"
        );
        assert_eq!(
            serving.await.expect("the server task finishes"),
            Ok(()),
            "a shutdown is not a serving failure"
        );
        assert_eq!(
            service.events().last(),
            Some(&Event::Disconnect(Disconnect::ShuttingDown))
        );
    }

    #[tokio::test]
    /// SV-R-042, SV-R-044 — a request already dispatched is answered before its
    /// connection closes, and shutdown waits for it.
    async fn ut_shutdown_waits_for_a_request_in_flight() {
        let service = Recorder::holding();
        let (server_end, client_end) = duplex(1024);
        let server = Server::new(Arc::clone(&service));
        let handle = server.handle();
        let serving = tokio::spawn(server.serve_link(FrameTransport::<_, Tcp>::new(server_end)));
        let mut client = FrameTransport::<_, Tcp>::new(client_end);

        client
            .send_request(&header(1, 1), &read_holding())
            .await
            .expect("writes a request");
        service.awaited_a_request().await;

        let done = Arc::new(core::sync::atomic::AtomicBool::new(false));
        let shutting = {
            let done = Arc::clone(&done);
            tokio::spawn(async move {
                handle.shutdown().await;
                done.store(true, Ordering::SeqCst);
            })
        };
        tokio::task::yield_now().await;
        assert!(
            !done.load(Ordering::SeqCst),
            "shutdown must wait for the handler that is still running"
        );

        service.release();
        shutting.await.expect("the shutdown task finishes");
        assert_eq!(
            client.recv_response().await,
            Ok((header(1, 1), registers())),
            "the request in flight must still be answered"
        );
        serving
            .await
            .expect("the server task finishes")
            .expect("serving succeeds");
    }

    #[tokio::test]
    /// SV-R-041 — once shutdown is requested the listener stops accepting, so a
    /// later connection gets no service at all.
    async fn ut_shutdown_stops_accepting() {
        let service = Recorder::new(|_| Ok(registers()));
        let listener = TcpListener::bind(ephemeral()).await.expect("binds");
        let address = listener.local_addr().expect("reports its address");
        let server = Server::new(Arc::clone(&service));
        let handle = server.handle();
        let serving = tokio::spawn(server.serve(listener));

        handle.shutdown().await;
        assert_eq!(
            serving.await.expect("the server task finishes"),
            Ok(()),
            "a shutdown is not an accept failure"
        );

        let refused = match connect_tcp(address, crate::transport::TcpConfig::default()).await {
            // The listening socket is gone with the server.
            Err(_) => true,
            // Some platforms complete a queued handshake anyway; nothing answers.
            Ok(mut client) => {
                client
                    .send_request(&header(1, 1), &read_holding())
                    .await
                    .is_err()
                    || client.recv_response().await.is_err()
            }
        };
        assert!(refused, "no request may be served after shutdown");
    }

    #[tokio::test]
    /// SV-R-045 — the handle reports the state without changing it.
    async fn ut_handle_reports_whether_shutdown_was_requested() {
        let service = Recorder::new(|_| Ok(registers()));
        let (server_end, _client_end) = duplex(1024);
        let server = Server::new(Arc::clone(&service));
        let handle = server.handle();
        let watcher = handle.clone();

        assert!(!handle.is_shutting_down());
        assert!(!watcher.is_shutting_down(), "asking may not request it");

        let serving = tokio::spawn(server.serve_link(FrameTransport::<_, Tcp>::new(server_end)));
        handle.shutdown().await;
        assert!(watcher.is_shutting_down());
        serving
            .await
            .expect("the server task finishes")
            .expect("serving succeeds");
    }

    #[tokio::test]
    /// SV-R-033, SV-R-050 — a peer that vanishes part-way through an ADU ends
    /// the connection as a failure, distinct from the clean close of SV-R-052.
    async fn ut_close_mid_adu_ends_the_connection_as_a_failure() {
        let service = Recorder::new(|_| Ok(registers()));
        let (serving, client) = link(Arc::clone(&service));

        let mut stream = client.into_inner();
        // An MBAP header promising six more bytes, followed by three and a
        // close (FR-R-101).
        tokio::io::AsyncWriteExt::write_all(&mut stream, &[0, 1, 0, 0, 0, 6, 1, 3, 0])
            .await
            .expect("writes half a request");
        drop(stream);
        serving
            .await
            .expect("the server task finishes")
            .expect("serving succeeds");

        let events = service.events();
        assert_eq!(
            events.last(),
            Some(&Event::Disconnect(Disconnect::Failed(
                Error::ConnectionClosed
            ))),
            "a severed frame is a failure, not a clean close: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Failed(Error::ConnectionClosed))),
            "and must be reported to the service (SV-R-034): {events:?}"
        );
    }

    #[tokio::test]
    /// SV-R-001, SV-R-007 — one server type serves any framing over any link.
    /// The other tests cover TCP and RTU; this one is the same `Server` over
    /// ASCII, which shares no code path with either boundary rule (FR-R-116).
    async fn ut_one_server_serves_ascii_too() {
        let service = Recorder::new(|_| Ok(registers()));
        let (server_end, client_end) = duplex(1024);
        let serving = tokio::spawn(
            Server::new(Arc::clone(&service))
                .serve_link(FrameTransport::<_, crate::frame::Ascii>::new(server_end)),
        );
        let mut client = FrameTransport::<_, crate::frame::Ascii>::new(client_end);

        client
            .send_request(&UnitId(0x11), &read_holding())
            .await
            .expect("writes a request");
        assert_eq!(
            client.recv_response().await,
            Ok((UnitId(0x11), registers()))
        );

        drop(client);
        serving
            .await
            .expect("the server task finishes")
            .expect("serving succeeds");
        assert_eq!(
            service
                .events()
                .iter()
                .filter(|event| matches!(event, Event::Request(..)))
                .count(),
            1
        );
    }
}
