//! Async Modbus client (initiator). See `docs/specs/client/`.

mod framing;

use core::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::Instant;

use crate::error::{Error, Result};
use crate::frame::{RequestPdu, ResponsePdu, TransactionId, UnitId};
use crate::transport::FrameTransport;

pub use framing::ClientFraming;

/// How a client waits (CL-R-030).
///
/// One field: CL-R-033 rules out retry and reconnect, so there is no policy for
/// them to configure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientConfig {
    /// How long a response may take before the exchange is abandoned.
    pub response_timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            response_timeout: Duration::from_secs(1),
        }
    }
}

/// A Modbus client (CL-R-001).
///
/// One type for every framing: what differs between RTU, ASCII, and TCP is
/// named by [`ClientFraming`] and nothing else.
///
/// Every request takes `&mut self`, which is how CL-R-005 holds without a
/// run-time flag: the borrow checker permits one exchange at a time.
#[derive(Debug)]
pub struct Client<S, F> {
    /// The established transport this client speaks over (CL-R-002).
    transport: FrameTransport<S, F>,
    /// How long a response may take (CL-R-030).
    config: ClientConfig,
    /// The identifier the next request will carry (CL-R-011).
    next_transaction: TransactionId,
    /// Whether the byte stream is still accounted for (CL-R-031).
    desynchronized: bool,
}

impl<S, F> Client<S, F>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    F: ClientFraming,
{
    /// Build a client over an established transport, with the default
    /// configuration (CL-R-002).
    pub fn new(transport: FrameTransport<S, F>) -> Self {
        Self::with_config(transport, ClientConfig::default())
    }

    /// Build a client over an established transport.
    pub fn with_config(transport: FrameTransport<S, F>, config: ClientConfig) -> Self {
        Self {
            transport,
            config,
            // Identifier 0 is never allocated, so a matched response is never
            // matched against an unset field (CL-R-011).
            next_transaction: TransactionId(1),
            desynchronized: false,
        }
    }

    /// Surrender the transport (CL-R-006).
    ///
    /// The only recovery from desynchronization is to discard the client; this
    /// is how the connection underneath can be inspected or replaced.
    pub fn into_inner(self) -> FrameTransport<S, F> {
        self.transport
    }

    /// Whether this client has given up on the byte stream (CL-R-034).
    #[must_use]
    pub fn is_desynchronized(&self) -> bool {
        self.desynchronized
    }

    /// Issue a request and return the response as received (CL-R-061).
    ///
    /// An exception response is returned, not raised: `call` does not
    /// reinterpret what the server said (CL-R-041). `None` means the request
    /// was a broadcast, which no server answers (CL-R-053).
    ///
    /// # Errors
    ///
    /// Fails if the request cannot be encoded, if the transport fails, if no
    /// matching response arrives within the response timeout, or if the client
    /// is already desynchronized.
    pub async fn call(&mut self, unit: UnitId, request: RequestPdu) -> Result<Option<ResponsePdu>> {
        if self.desynchronized {
            return Err(Error::Desynchronized);
        }

        // Encoded before an identifier is spent or a byte is written, so an
        // unencodable request costs nothing (CL-R-012).
        let expected = request.function();
        request.encode()?;

        let transaction = self.next_transaction;
        let header = F::request_header(unit, transaction);
        self.transport.send_request(&header, &request).await?;
        self.next_transaction = next(transaction);

        if F::is_broadcast(unit) {
            return Ok(None);
        }

        // Absolute, and fixed once the request is on the wire: waiting is never
        // extended by the time spent writing (CL-R-014) or by discarding a
        // response that was not ours (CL-R-021).
        let deadline = Instant::now() + self.config.response_timeout;
        loop {
            let received =
                match tokio::time::timeout_at(deadline, self.transport.recv_response()).await {
                    Ok(result) => result,
                    Err(_elapsed) => {
                        // Nothing failed; the wait ran out. What the peer sends
                        // next is now unaccounted for (CL-R-031).
                        self.desynchronized = true;
                        return Err(Error::Timeout { what: "response" });
                    }
                };
            let (header_in, response) = match received {
                Ok(received) => received,
                Err(error) => {
                    // An I/O or decoding failure leaves the stream's alignment
                    // unknown (CL-R-023, CL-R-031).
                    self.desynchronized = true;
                    return Err(error);
                }
            };
            if !F::is_response_to(&header, &header_in) {
                // Another exchange's reply, or a late one. Discard it and keep
                // waiting against the same deadline (CL-R-021).
                continue;
            }
            let actual = response.function();
            if actual != expected {
                return Err(Error::UnexpectedFunction { expected, actual });
            }
            return Ok(Some(response));
        }
    }
}

/// The identifier following `current`, wrapping to 1 rather than to 0
/// (CL-R-011).
fn next(current: TransactionId) -> TransactionId {
    match current.0.checked_add(1) {
        Some(next) => TransactionId(next),
        None => TransactionId(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::frame::{
        Address, ExceptionCode, ExceptionResponse, FunctionCode, MbapHeader, Quantity,
        RegisterValue, RequestPdu, ResponsePdu, Tcp, TransactionId, UnitId,
    };
    use crate::transport::FrameTransport;
    use alloc::vec;
    use core::time::Duration;
    use tokio::io::{DuplexStream, duplex};

    /// A client and the transport a test server answers it on.
    fn pair() -> (Client<DuplexStream, Tcp>, FrameTransport<DuplexStream, Tcp>) {
        let (client, server) = duplex(1024);
        (
            Client::new(FrameTransport::new(client)),
            FrameTransport::new(server),
        )
    }

    fn read_holding() -> RequestPdu {
        RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        }
    }

    fn registers() -> ResponsePdu {
        ResponsePdu::ReadHoldingRegisters {
            registers: vec![RegisterValue(0x022B)],
        }
    }

    #[tokio::test]
    /// CL-R-010, CL-R-061 — a raw call writes the request and yields the
    /// response as received.
    async fn ut_call_round_trips_a_request() {
        let (mut client, mut server) = pair();
        let answering = tokio::spawn(async move {
            let (header, request) = server.recv_request().await.expect("receives");
            assert_eq!(request, read_holding());
            server
                .send_response(&header, &registers())
                .await
                .expect("responds");
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Ok(Some(registers()))
        );
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-011 — transaction identifiers start at 1 and advance by one per
    /// request, so two requests are distinguishable on the wire.
    async fn ut_transaction_ids_start_at_one_and_advance() {
        let (mut client, mut server) = pair();
        let answering = tokio::spawn(async move {
            let mut seen = vec![];
            for _ in 0..2 {
                let (header, _) = server.recv_request().await.expect("receives");
                seen.push(header.transaction_id);
                server
                    .send_response(&header, &registers())
                    .await
                    .expect("responds");
            }
            seen
        });

        for _ in 0..2 {
            client
                .call(UnitId(0x11), read_holding())
                .await
                .expect("calls");
        }
        assert_eq!(
            answering.await.expect("the server task finishes"),
            vec![TransactionId(1), TransactionId(2)]
        );
    }

    #[tokio::test]
    /// CL-R-011 — the sequence wraps to 1, never to 0: an unallocated
    /// identifier must not be matchable.
    async fn ut_transaction_ids_wrap_past_zero() {
        let (mut client, mut server) = pair();
        client.next_transaction = TransactionId(u16::MAX);
        let answering = tokio::spawn(async move {
            let mut seen = vec![];
            for _ in 0..2 {
                let (header, _) = server.recv_request().await.expect("receives");
                seen.push(header.transaction_id);
                server
                    .send_response(&header, &registers())
                    .await
                    .expect("responds");
            }
            seen
        });

        for _ in 0..2 {
            client
                .call(UnitId(0x11), read_holding())
                .await
                .expect("calls");
        }
        assert_eq!(
            answering.await.expect("the server task finishes"),
            vec![TransactionId(u16::MAX), TransactionId(1)]
        );
    }

    #[tokio::test]
    /// CL-R-021 — a response whose header does not answer the request is
    /// discarded and the wait continues, rather than being handed back as if it
    /// did.
    async fn ut_unmatched_response_is_discarded() {
        let (mut client, mut server) = pair();
        let answering = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            let stale = MbapHeader {
                transaction_id: TransactionId(999),
                unit_id: header.unit_id,
            };
            server
                .send_response(
                    &stale,
                    &ResponsePdu::ReadHoldingRegisters { registers: vec![] },
                )
                .await
                .expect("sends a stale response");
            server
                .send_response(&header, &registers())
                .await
                .expect("responds");
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Ok(Some(registers()))
        );
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-022 — a matching header carrying another function's response is a
    /// protocol error naming both codes, not a silent mismatch.
    async fn ut_wrong_function_code_is_an_error() {
        let (mut client, mut server) = pair();
        let answering = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            server
                .send_response(&header, &ResponsePdu::ReadCoils { coils: vec![true] })
                .await
                .expect("responds");
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::UnexpectedFunction {
                expected: FunctionCode::ReadHoldingRegisters,
                actual: FunctionCode::ReadCoils,
            })
        );
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-040, CL-R-042 — `call` hands an exception response back verbatim
    /// rather than reinterpreting it, and the client stays usable.
    async fn ut_call_returns_an_exception_response_verbatim() {
        let (mut client, mut server) = pair();
        let exception = ResponsePdu::Exception(ExceptionResponse {
            function: FunctionCode::ReadHoldingRegisters,
            exception: ExceptionCode::IllegalDataAddress,
        });
        let expected = exception.clone();
        let answering = tokio::spawn(async move {
            for _ in 0..2 {
                let (header, _) = server.recv_request().await.expect("receives");
                server
                    .send_response(&header, &exception)
                    .await
                    .expect("responds");
            }
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Ok(Some(expected))
        );
        assert!(!client.is_desynchronized());
        assert!(client.call(UnitId(0x11), read_holding()).await.is_ok());
        answering.await.expect("the server task finishes");
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-030, CL-R-031 — a silent server costs the response timeout, then
    /// fails as a timeout naming the response, and leaves the client
    /// desynchronized.
    async fn ut_silence_times_out_and_desynchronizes() {
        let (mut client, mut server) = pair();
        let silent = tokio::spawn(async move {
            let _ = server.recv_request().await;
            // Never answers, and holds the transport open so the wait is
            // silence rather than a closed connection.
            core::future::pending::<()>().await;
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Timeout { what: "response" })
        );
        assert!(client.is_desynchronized());
        silent.abort();
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-014 — the deadline is absolute: a stream of unmatched responses
    /// cannot hold a request open past the timeout by restarting it.
    async fn ut_discarding_does_not_extend_the_deadline() {
        let (mut client, mut server) = pair();
        let chatty = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            loop {
                let stale = MbapHeader {
                    transaction_id: TransactionId(999),
                    unit_id: header.unit_id,
                };
                if server.send_response(&stale, &registers()).await.is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(600)).await;
            }
        });

        // Each unmatched response arrives well inside the 1-second timeout; a
        // deadline that restarted on each would never expire.
        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Timeout { what: "response" })
        );
        chatty.abort();
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-032 — a desynchronized client refuses the next request outright,
    /// without writing: the peer's next bytes are no longer accounted for.
    async fn ut_desynchronized_client_writes_nothing() {
        let (mut client, mut server) = pair();
        let silent = tokio::spawn(async move {
            let first = server.recv_request().await;
            assert!(first.is_ok());
            // A second request must never arrive.
            let second = server.recv_request().await;
            assert!(second.is_err(), "a desynchronized client wrote again");
        });

        assert!(client.call(UnitId(0x11), read_holding()).await.is_err());
        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Desynchronized)
        );
        drop(client);
        silent.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-012 — a request that cannot be encoded fails without writing, and
    /// without spending a transaction identifier on it.
    async fn ut_unencodable_request_writes_nothing() {
        let (mut client, mut server) = pair();
        let unencodable = RequestPdu::ReadHoldingRegisters {
            address: Address(0),
            // Beyond the 125 of FR-R-022, so encoding rejects it.
            quantity: Quantity(0xFFFF),
        };

        assert!(matches!(
            client.call(UnitId(0x11), unencodable).await,
            Err(Error::OutOfRange { .. })
        ));
        assert!(!client.is_desynchronized());

        let answering = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            server
                .send_response(&header, &registers())
                .await
                .expect("responds");
            header.transaction_id
        });
        client
            .call(UnitId(0x11), read_holding())
            .await
            .expect("the client is still usable");
        assert_eq!(
            answering.await.expect("the server task finishes"),
            TransactionId(1),
            "the failed request must not consume an identifier"
        );
    }

    #[tokio::test]
    /// CL-R-002, CL-R-006 — a client is built from a transport and gives it
    /// back, which is what makes recovery from desynchronization possible.
    async fn ut_client_surrenders_its_transport() {
        let (client, _server) = pair();
        drop(client.into_inner());
    }

    #[test]
    /// CL-R-030 — the default response timeout is 1 second.
    fn ut_default_response_timeout() {
        assert_eq!(
            ClientConfig::default(),
            ClientConfig {
                response_timeout: Duration::from_secs(1),
            }
        );
    }
}
