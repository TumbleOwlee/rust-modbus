//! Pipelined clients: several requests in flight at once over one connection,
//! distinguished by MBAP transaction id (CL-R-082 … CL-R-095). Gated behind
//! the `pipeline` feature.

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::future::{Future, poll_fn};
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use std::collections::HashMap;
use std::sync::Mutex;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio_util::time::DelayQueue;
use tokio_util::time::delay_queue::Key;

use crate::client::ClientFraming;
use crate::error::{Error, Result};
use crate::frame::{MbapHeader, RequestPdu, ResponsePdu, Tcp, TransactionId, UnitId};
use crate::transport::{
    FrameTransport, FrameTransportReader, FrameTransportWriter, UdpTransport, UdpTransportReader,
    UdpTransportWriter,
};

use super::ClientConfig;

/// How a pipelined client waits and how many requests it lets outstand at
/// once (CL-R-093).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineConfig {
    /// How long a response may take before its request is abandoned.
    pub response_timeout: Duration,
    /// How many requests may be outstanding at once. `u16` already caps this
    /// at 65535, the MBAP transaction id space (CL-R-093) — no separate
    /// validation is needed for the ceiling.
    pub max_in_flight: u16,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            response_timeout: Duration::from_secs(1),
            max_in_flight: 16,
        }
    }
}

impl From<ClientConfig> for PipelineConfig {
    /// Carries `response_timeout` over; `max_in_flight` takes the default,
    /// since `ClientConfig` has no equivalent field to carry (CL-R-095).
    fn from(config: ClientConfig) -> Self {
        Self {
            response_timeout: config.response_timeout,
            max_in_flight: PipelineConfig::default().max_in_flight,
        }
    }
}

/// The transport interface the pipeline's background task needs, distinct
/// from [`crate::transport::ClientTransport`] (both are `Send`, TR-R-081):
/// the background task holds one never-cancelled receive future across every
/// loop iteration live at the same time as writes, which needs independent
/// read/write halves — `ClientTransport`'s single `&mut self` cannot express
/// that. Crate-private — nothing in CL-R-082 … CL-R-095 asks for a third
/// transport to plug in here.
///
/// Split into independent read/write halves rather than one `&mut self`:
/// `run`'s background task holds one never-cancelled receive future across
/// every loop iteration (see its doc comment for why), which needs its own
/// exclusive access to the read half for that future's whole life, live at
/// the same time as writes on the write half.
pub(crate) trait PipelineTransport: Sized + Send + 'static {
    type Reader: Send + 'static;
    type Writer: Send + 'static;

    fn split(self) -> (Self::Reader, Self::Writer);

    fn send_request(
        writer: &mut Self::Writer,
        header: &MbapHeader,
        pdu: &RequestPdu,
    ) -> impl Future<Output = Result<()>> + Send;

    fn recv_response(
        reader: &mut Self::Reader,
    ) -> impl Future<Output = Result<(MbapHeader, ResponsePdu)>> + Send;

    /// `true` on TCP-shaped transports (CL-R-090), `false` on UDP (CL-R-091).
    const TIMEOUT_DESYNCS: bool;
}

impl<S> PipelineTransport for FrameTransport<S, Tcp>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Reader = FrameTransportReader<S, Tcp>;
    type Writer = FrameTransportWriter<S, Tcp>;

    fn split(self) -> (Self::Reader, Self::Writer) {
        FrameTransport::split(self)
    }

    fn send_request(
        writer: &mut Self::Writer,
        header: &MbapHeader,
        pdu: &RequestPdu,
    ) -> impl Future<Output = Result<()>> + Send {
        FrameTransportWriter::send_request(writer, header, pdu)
    }

    fn recv_response(
        reader: &mut Self::Reader,
    ) -> impl Future<Output = Result<(MbapHeader, ResponsePdu)>> + Send {
        FrameTransportReader::recv_response(reader)
    }

    const TIMEOUT_DESYNCS: bool = true;
}

impl PipelineTransport for UdpTransport<Tcp> {
    type Reader = UdpTransportReader<Tcp>;
    type Writer = UdpTransportWriter<Tcp>;

    fn split(self) -> (Self::Reader, Self::Writer) {
        UdpTransport::split(self)
    }

    fn send_request(
        writer: &mut Self::Writer,
        header: &MbapHeader,
        pdu: &RequestPdu,
    ) -> impl Future<Output = Result<()>> + Send {
        UdpTransportWriter::send_request(writer, header, pdu)
    }

    fn recv_response(
        reader: &mut Self::Reader,
    ) -> impl Future<Output = Result<(MbapHeader, ResponsePdu)>> + Send {
        UdpTransportReader::recv_response(reader)
    }

    const TIMEOUT_DESYNCS: bool = false;
}

/// A cloneable handle over a TCP transport permitting several requests in
/// flight at once, distinguished by MBAP transaction id (CL-R-082). Every
/// clone shares the same background task and transport (CL-R-085).
#[derive(Debug)]
pub struct PipelinedClient<T = FrameTransport<tokio::net::TcpStream, Tcp>> {
    core: Arc<Core<T>>,
}

/// A pipelined client over UDP: each `send` is one datagram, and a response
/// timeout fails only that one request rather than desynchronizing the whole
/// handle (CL-R-091, CL-R-083) — `PipelineTransport::TIMEOUT_DESYNCS` is
/// `false` for `UdpTransport<Tcp>`, the one behavioral difference from
/// [`PipelinedClient`]'s default TCP instantiation.
pub type PipelinedUdpClient = PipelinedClient<UdpTransport<Tcp>>;

impl<T> Clone for PipelinedClient<T> {
    fn clone(&self) -> Self {
        Self {
            core: Arc::clone(&self.core),
        }
    }
}

#[derive(Debug)]
struct Core<T> {
    command_tx: mpsc::UnboundedSender<Command>,
    next_transaction: Mutex<TransactionId>,
    semaphore: Arc<Semaphore>,
    desynchronized: Arc<AtomicBool>,
    _marker: core::marker::PhantomData<T>,
}

#[derive(Debug)]
enum Command {
    Send {
        transaction: TransactionId,
        unit: UnitId,
        request: RequestPdu,
        reply: oneshot::Sender<Result<ResponsePdu>>,
    },
}

// `PipelineTransport` is deliberately sealed (`pub(crate)`): only this crate's
// two transports implement it, so an external caller can never name a `T`
// that satisfies this bound and this `impl` block is unreachable from outside
// the crate despite `PipelinedClient` itself being `pub`.
#[allow(private_bounds)]
impl<T: PipelineTransport> PipelinedClient<T> {
    /// Build a client over an established transport, with the default
    /// configuration, spawning its background task (CL-R-084).
    pub fn new(transport: T) -> Self {
        Self::with_config(transport, PipelineConfig::default())
    }

    /// Build a client over an established transport, spawning its background
    /// task (CL-R-084).
    pub fn with_config(transport: T, config: PipelineConfig) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let desynchronized = Arc::new(AtomicBool::new(false));
        let semaphore = Arc::new(Semaphore::new(usize::from(config.max_in_flight)));
        tokio::spawn(run::<T>(
            transport,
            command_rx,
            Arc::clone(&desynchronized),
            config.response_timeout,
        ));
        Self {
            core: Arc::new(Core {
                command_tx,
                // Identifier 0 is never allocated (CL-R-011), same as `Client`.
                next_transaction: Mutex::new(TransactionId(1)),
                semaphore,
                desynchronized,
                _marker: core::marker::PhantomData,
            }),
        }
    }

    /// Issue a request, awaiting a free in-flight slot first if
    /// `max_in_flight` is already reached (CL-R-086).
    ///
    /// # Errors
    ///
    /// Fails if the transport fails, if no matching response arrives within
    /// the response timeout, or if the client is already desynchronized.
    pub async fn send(&self, unit: UnitId, request: RequestPdu) -> Result<ResponsePdu> {
        let permit = Arc::clone(&self.core.semaphore)
            .acquire_owned()
            .await
            .expect("the semaphore is never closed while a handle referencing it is alive");
        self.dispatch(unit, request, permit).await
    }

    /// Whether this handle currently refuses every request (CL-R-096),
    /// mirroring [`Client::is_desynchronized`](super::Client::is_desynchronized)
    /// (CL-R-034). Answers from what the background task has already
    /// observed: touches neither the transport nor the clock, and never
    /// blocks.
    pub fn is_desynchronized(&self) -> bool {
        self.core.desynchronized.load(Ordering::Acquire)
    }

    /// Issue a request, failing immediately if `max_in_flight` is already
    /// reached rather than waiting for a slot (CL-R-086).
    ///
    /// # Errors
    ///
    /// Fails with [`Error::TooManyInFlight`] at the limit, without writing to
    /// the transport. Otherwise as [`PipelinedClient::send`].
    pub async fn try_send(&self, unit: UnitId, request: RequestPdu) -> Result<ResponsePdu> {
        let permit = Arc::clone(&self.core.semaphore)
            .try_acquire_owned()
            .map_err(|_| Error::TooManyInFlight)?;
        self.dispatch(unit, request, permit).await
    }

    /// Register a request with the background task and await its answer.
    ///
    /// `_permit` is held for the whole call: dropping the returned future
    /// before it resolves drops the permit with it, freeing the in-flight
    /// slot without any explicit cancellation (CL-R-087).
    async fn dispatch(
        &self,
        unit: UnitId,
        request: RequestPdu,
        _permit: OwnedSemaphorePermit,
    ) -> Result<ResponsePdu> {
        if self.core.desynchronized.load(Ordering::Acquire) {
            // Refused before writing (CL-R-032/CL-R-090 posture).
            return Err(Error::Desynchronized);
        }
        let transaction = {
            let mut next = self
                .core
                .next_transaction
                .lock()
                .expect("not poisoned: no panic while held");
            let id = *next;
            *next = super::next(id);
            id
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        self.core
            .command_tx
            .send(Command::Send {
                transaction,
                unit,
                request,
                reply: reply_tx,
            })
            .expect("the background task outlives every live handle, `self` included");
        reply_rx
            .await
            .expect("every registered Send is answered before its sender is dropped")
    }
}

/// Poll `T::recv_response` to completion, then hand the reader back with the
/// result, so the caller can immediately start the next receive without ever
/// re-borrowing a reader a still-live future already holds.
///
/// This ownership hand-off — not a `&mut` reader borrowed across loop
/// iterations — is what lets `run` hold ONE receive future for the whole
/// background task's life instead of reconstructing `T::recv_response`
/// afresh every `select!` pass: reconstructing it fresh is exactly what an
/// earlier version of this function did, and it is unsound for stream
/// transports. `tokio::select!` polls every branch once per pass even before
/// choosing a winner; a `recv_response` branch that got polled (entering its
/// framing's read state) and then lost that pass had its future dropped
/// mid-poll. For `FrameTransportReader`, that leaves TR-R-041's `receiving`
/// latch stuck `true` forever — every later `recv_response` then fails
/// *synchronously* with `Error::Timeout` instead of awaiting real data,
/// which this function's `Err(_) => { desynchronized = true; fail_all(...) }`
/// arm processes instantly, forever: a busy-spin that starves the task's own
/// executor thread, so `command_rx.recv()` never gets polled again either.
/// Reproduced empirically before this fix: ~1M loop iterations/second with
/// `in_flight` never advancing past 0.
async fn recv_and_return<T: PipelineTransport>(
    mut reader: T::Reader,
) -> (T::Reader, Result<(MbapHeader, ResponsePdu)>) {
    let result = T::recv_response(&mut reader).await;
    (reader, result)
}

/// The background task CL-R-084 requires: owns the transport, writes each
/// request, and dispatches each response to the caller awaiting its
/// transaction id.
async fn run<T: PipelineTransport>(
    transport: T,
    mut command_rx: mpsc::UnboundedReceiver<Command>,
    desynchronized: Arc<AtomicBool>,
    response_timeout: Duration,
) {
    let (reader, mut writer) = transport.split();
    // Held across every loop iteration below, replaced only once it
    // resolves — see `recv_and_return`'s doc comment for why this must never
    // be reconstructed on a losing `select!` pass.
    let mut recv_fut = Box::pin(recv_and_return::<T>(reader));

    // Keyed by transaction id; pairs the caller's reply channel with the
    // `DelayQueue` key so a response that resolves the entry can cancel its
    // still-pending timer — otherwise a stale timer could fire later against
    // a *different* request that reuses the same id.
    let mut in_flight: HashMap<TransactionId, (oneshot::Sender<Result<ResponsePdu>>, Key)> =
        HashMap::new();
    // Every id this handle has ever written a request under, bounded at the
    // 65536-bit transaction id space, never cleared. Distinguishes CL-R-088
    // (id was issued, already resolved) from CL-R-089 (id never issued).
    let mut issued: Box<[bool]> = alloc::vec![false; 1 << 16].into_boxed_slice();
    let mut deadlines: DelayQueue<TransactionId> = DelayQueue::new();

    loop {
        tokio::select! {
            cmd = command_rx.recv() => {
                let Some(Command::Send { transaction, unit, request, reply }) = cmd else {
                    // Every handle dropped: `writer` (and, once `recv_fut`
                    // resolves or is dropped, `reader`) drop with this
                    // function returning, closing the transport (CL-R-085).
                    break;
                };
                if desynchronized.load(Ordering::Acquire) {
                    let _ = reply.send(Err(Error::Desynchronized));
                    continue;
                }
                *issued
                    .get_mut(usize::from(transaction.0))
                    .expect("index is a u16, `issued` holds all 65536 possible indices") = true;
                let header = <Tcp as ClientFraming>::request_header(unit, transaction);
                match T::send_request(&mut writer, &header, &request).await {
                    Ok(()) => {
                        let key = deadlines.insert(transaction, response_timeout);
                        in_flight.insert(transaction, (reply, key));
                    }
                    Err(error) => {
                        desynchronized.store(true, Ordering::Release);
                        let _ = reply.send(Err(error));
                        fail_all(&mut in_flight, &mut deadlines);
                    }
                }
            }
            (reader, received) = &mut recv_fut => {
                // The one receive in flight resolved: start the next one
                // immediately, before doing anything with this result, so a
                // later `continue`/error path can never skip re-arming it.
                recv_fut = Box::pin(recv_and_return::<T>(reader));
                match received {
                    Ok((header, response)) => {
                        let transaction = header.transaction_id;
                        if let Some((reply, key)) = in_flight.remove(&transaction) {
                            // Resolved on time: its timer must not fire later.
                            deadlines.remove(&key);
                            let _ = reply.send(Ok(response));
                        } else if *issued
                            .get(usize::from(transaction.0))
                            .expect("index is a u16, `issued` holds all 65536 possible indices")
                        {
                            // CL-R-088: late reply to a request already
                            // resolved by timeout or desync. Silent.
                        } else {
                            // CL-R-089: an id this handle never allocated —
                            // a genuine correctness violation, on both
                            // PipelinedClient and PipelinedUdpClient.
                            desynchronized.store(true, Ordering::Release);
                            fail_all(&mut in_flight, &mut deadlines);
                        }
                    }
                    Err(_error) => {
                        // Not attributable to one transaction: every
                        // in-flight request fails (CL-R-031-equivalent,
                        // applies on both types per CL-R-091's last sentence).
                        desynchronized.store(true, Ordering::Release);
                        fail_all(&mut in_flight, &mut deadlines);
                    }
                }
            }
            Some(expired) = poll_fn(|cx| deadlines.poll_expired(cx)), if !deadlines.is_empty() => {
                let transaction = expired.into_inner();
                if let Some((reply, _key)) = in_flight.remove(&transaction) {
                    let _ = reply.send(Err(Error::Timeout { what: "response" }));
                    if T::TIMEOUT_DESYNCS {
                        // CL-R-090.
                        desynchronized.store(true, Ordering::Release);
                        fail_all(&mut in_flight, &mut deadlines);
                    }
                    // else CL-R-091: only this one request failed.
                }
            }
        }
    }
}

/// Fail every still-registered request with [`Error::Desynchronized`],
/// cancelling each one's pending timer (CL-R-090, CL-R-091's I/O case,
/// CL-R-089).
fn fail_all(
    in_flight: &mut HashMap<TransactionId, (oneshot::Sender<Result<ResponsePdu>>, Key)>,
    deadlines: &mut DelayQueue<TransactionId>,
) {
    for (_, (reply, key)) in in_flight.drain() {
        deadlines.remove(&key);
        let _ = reply.send(Err(Error::Desynchronized));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::frame::{Address, Quantity, RegisterValue};
    use alloc::vec;
    use tokio::io::{DuplexStream, duplex};

    #[test]
    /// CL-R-093 — the default response timeout is 1 second and the default
    /// in-flight ceiling is 16.
    fn ut_default_pipeline_config() {
        assert_eq!(
            PipelineConfig::default(),
            PipelineConfig {
                response_timeout: Duration::from_secs(1),
                max_in_flight: 16,
            }
        );
    }

    #[test]
    /// CL-R-095 — converting from `ClientConfig` carries the response
    /// timeout over and sets `max_in_flight` to `PipelineConfig`'s own
    /// default, so a caller moving from `Client` need not reconstruct
    /// configuration it already had.
    fn ut_pipeline_config_from_client_config() {
        let client_config = ClientConfig {
            response_timeout: Duration::from_millis(250),
        };
        assert_eq!(
            PipelineConfig::from(client_config),
            PipelineConfig {
                response_timeout: Duration::from_millis(250),
                max_in_flight: PipelineConfig::default().max_in_flight,
            }
        );
    }

    #[test]
    /// CL-R-086 — `try_send`'s failure at the in-flight limit is a distinct
    /// error from desynchronization, not folded into it.
    fn ut_too_many_in_flight_is_distinct_from_desynchronized() {
        assert_ne!(Error::TooManyInFlight, Error::Desynchronized);
    }

    /// A pipelined client and the transport a test server answers it on.
    fn pipeline_pair() -> (
        PipelinedClient<FrameTransport<DuplexStream, Tcp>>,
        FrameTransport<DuplexStream, Tcp>,
    ) {
        pipeline_pair_with_config(PipelineConfig::default())
    }

    fn pipeline_pair_with_config(
        config: PipelineConfig,
    ) -> (
        PipelinedClient<FrameTransport<DuplexStream, Tcp>>,
        FrameTransport<DuplexStream, Tcp>,
    ) {
        let (client, server) = duplex(4096);
        (
            PipelinedClient::with_config(FrameTransport::new(client), config),
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
    /// CL-R-084 — the background task already answers requests as soon as
    /// the client is constructed, with no separate start call.
    async fn ut_background_task_is_spawned_on_construction() {
        let (client, mut server) = pipeline_pair();
        let answering = tokio::spawn(async move {
            let (header, request) = server.recv_request().await.expect("receives");
            assert_eq!(request, read_holding());
            server
                .send_response(&header, &registers())
                .await
                .expect("responds");
        });
        assert_eq!(
            client.send(UnitId(0x11), read_holding()).await,
            Ok(registers())
        );
        answering.await.expect("server task finishes");
    }

    #[tokio::test]
    /// CL-R-085 — every clone shares the same background task and transport:
    /// two concurrent sends from two clones are both answered on the one
    /// connection.
    async fn ut_clone_shares_the_same_background_task() {
        let (client, mut server) = pipeline_pair();
        let clone = client.clone();
        let t1 = tokio::spawn(async move { client.send(UnitId(0x11), read_holding()).await });
        let t2 = tokio::spawn(async move { clone.send(UnitId(0x11), read_holding()).await });
        for _ in 0..2 {
            let (header, request) = server.recv_request().await.expect("receives");
            assert_eq!(request, read_holding());
            server
                .send_response(&header, &registers())
                .await
                .expect("responds");
        }
        assert_eq!(t1.await.expect("task"), Ok(registers()));
        assert_eq!(t2.await.expect("task"), Ok(registers()));
    }

    #[tokio::test]
    /// CL-R-085 — dropping the last handle closes the transport: the
    /// background task's command channel closes, it returns, and the
    /// transport it owned drops with it.
    async fn ut_dropping_the_last_handle_closes_the_transport() {
        let (client, mut server) = pipeline_pair();
        drop(client);
        assert!(
            server.recv_request().await.is_err(),
            "the transport should have closed once the only handle dropped"
        );
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-086 — `send` awaits a free in-flight slot rather than failing:
    /// with `max_in_flight` 1, a second concurrent send does not proceed
    /// until the first is answered.
    async fn ut_send_awaits_a_free_slot() {
        let (client, mut server) = pipeline_pair_with_config(PipelineConfig {
            max_in_flight: 1,
            ..PipelineConfig::default()
        });
        let c1 = client.clone();
        let t1 = tokio::spawn(async move { c1.send(UnitId(0x11), read_holding()).await });
        let (h1, _) = server.recv_request().await.expect("receives first");

        let c2 = client.clone();
        let t2 = tokio::spawn(async move { c2.send(UnitId(0x11), read_holding()).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !t2.is_finished(),
            "the second send proceeded before a slot freed"
        );

        server
            .send_response(&h1, &registers())
            .await
            .expect("answers first");
        assert_eq!(t1.await.expect("task"), Ok(registers()));

        let (h2, _) = server
            .recv_request()
            .await
            .expect("receives second now that a slot is free");
        server
            .send_response(&h2, &registers())
            .await
            .expect("answers second");
        assert_eq!(t2.await.expect("task"), Ok(registers()));
    }

    #[tokio::test]
    /// CL-R-086 — `try_send` fails immediately with `TooManyInFlight` at the
    /// limit, writing nothing: the next successful request still gets the
    /// transaction id right after the one still outstanding, proving the
    /// failed `try_send` never advanced the sequence.
    async fn ut_try_send_fails_immediately_at_the_limit() {
        let (client, mut server) = pipeline_pair_with_config(PipelineConfig {
            max_in_flight: 1,
            ..PipelineConfig::default()
        });
        let c1 = client.clone();
        let t1 = tokio::spawn(async move { c1.send(UnitId(0x11), read_holding()).await });
        let (h1, _) = server.recv_request().await.expect("receives first");
        assert_eq!(h1.transaction_id, TransactionId(1));

        assert_eq!(
            client.try_send(UnitId(0x11), read_holding()).await,
            Err(Error::TooManyInFlight)
        );

        server
            .send_response(&h1, &registers())
            .await
            .expect("answers first, freeing the slot");
        assert_eq!(t1.await.expect("task"), Ok(registers()));

        let c3 = client.clone();
        let t3 = tokio::spawn(async move { c3.send(UnitId(0x11), read_holding()).await });
        let (h3, _) = server
            .recv_request()
            .await
            .expect("receives the real next exchange");
        assert_eq!(
            h3.transaction_id,
            TransactionId(2),
            "the failed try_send must not have consumed a transaction id"
        );
        server
            .send_response(&h3, &registers())
            .await
            .expect("answers");
        assert_eq!(t3.await.expect("task"), Ok(registers()));
    }

    #[tokio::test]
    /// CL-R-087 — dropping a `send` future before it resolves frees its
    /// in-flight slot without any explicit cancellation.
    async fn ut_dropping_the_future_frees_its_slot() {
        let (client, mut server) = pipeline_pair_with_config(PipelineConfig {
            max_in_flight: 1,
            ..PipelineConfig::default()
        });
        let c1 = client.clone();
        let t1 = tokio::spawn(async move { c1.send(UnitId(0x11), read_holding()).await });
        server.recv_request().await.expect("receives first");
        t1.abort();
        let _ = t1.await;

        let c2 = client.clone();
        let t2 = tokio::spawn(async move { c2.send(UnitId(0x11), read_holding()).await });
        let (h2, _) = server
            .recv_request()
            .await
            .expect("receives second: the slot freed without the first ever answering");
        server
            .send_response(&h2, &registers())
            .await
            .expect("answers");
        assert_eq!(t2.await.expect("task"), Ok(registers()));
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-088 — a late reply for a transaction id already resolved (here,
    /// by the timeout that also desynchronizes the whole TCP connection,
    /// CL-R-090) is discarded silently: it changes nothing observable, so a
    /// following request still fails with exactly the same `Desynchronized`
    /// the timeout alone already produced.
    async fn ut_already_resolved_id_is_discarded() {
        let (client, mut server) = pipeline_pair_with_config(PipelineConfig {
            response_timeout: Duration::from_millis(10),
            ..PipelineConfig::default()
        });
        let c1 = client.clone();
        let t1 = tokio::spawn(async move { c1.send(UnitId(0x11), read_holding()).await });
        let (h1, _) = server.recv_request().await.expect("receives");
        assert_eq!(
            t1.await.expect("task"),
            Err(Error::Timeout { what: "response" })
        );

        server
            .send_response(&h1, &registers())
            .await
            .expect("sends a stale reply for the now-resolved id");

        assert_eq!(
            client.send(UnitId(0x11), read_holding()).await,
            Err(Error::Desynchronized)
        );
    }

    #[tokio::test]
    /// CL-R-089 — a response carrying a transaction id this handle never
    /// allocated desynchronizes the whole connection, failing the request
    /// still genuinely in flight too.
    async fn ut_never_issued_id_desynchronizes() {
        let (client, mut server) = pipeline_pair();
        let c1 = client.clone();
        let t1 = tokio::spawn(async move { c1.send(UnitId(0x11), read_holding()).await });
        server.recv_request().await.expect("receives");

        let bogus = MbapHeader {
            transaction_id: TransactionId(9999),
            unit_id: UnitId(0x11),
        };
        server
            .send_response(&bogus, &registers())
            .await
            .expect("sends an unsolicited reply");

        assert_eq!(t1.await.expect("task"), Err(Error::Desynchronized));
        assert_eq!(
            client.send(UnitId(0x11), read_holding()).await,
            Err(Error::Desynchronized)
        );
    }

    #[tokio::test]
    /// CL-R-096 — `is_desynchronized` reports `false` on a fresh handle and
    /// `true` once desynchronization has occurred, without blocking or
    /// touching the transport.
    async fn ut_is_desynchronized_reports_current_state() {
        let (client, mut server) = pipeline_pair();
        assert!(!client.is_desynchronized());

        let c1 = client.clone();
        let t1 = tokio::spawn(async move { c1.send(UnitId(0x11), read_holding()).await });
        server.recv_request().await.expect("receives");

        let bogus = MbapHeader {
            transaction_id: TransactionId(9999),
            unit_id: UnitId(0x11),
        };
        server
            .send_response(&bogus, &registers())
            .await
            .expect("sends an unsolicited reply");
        t1.await
            .expect("task")
            .expect_err("desynchronized by the unsolicited reply");

        assert!(client.is_desynchronized());
        // A clone observes the same state (CL-R-085's shared background task).
        assert!(client.clone().is_desynchronized());
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-090 — any timeout on `PipelinedClient` (TCP) desynchronizes the
    /// whole connection: every other in-flight request fails too, and no
    /// further request is written.
    async fn ut_timeout_desynchronizes_the_whole_connection() {
        let (client, mut server) = pipeline_pair_with_config(PipelineConfig {
            response_timeout: Duration::from_millis(10),
            ..PipelineConfig::default()
        });
        let c1 = client.clone();
        let c2 = client.clone();
        let t1 = tokio::spawn(async move { c1.send(UnitId(0x11), read_holding()).await });
        let t2 = tokio::spawn(async move { c2.send(UnitId(0x11), read_holding()).await });
        server.recv_request().await.expect("receives first");
        server.recv_request().await.expect("receives second");

        let results = [t1.await.expect("task"), t2.await.expect("task")];
        assert!(
            results
                .iter()
                .any(|r| *r == Err(Error::Timeout { what: "response" })),
            "exactly one request should have timed out: {results:?}"
        );
        assert!(
            results.iter().any(|r| *r == Err(Error::Desynchronized)),
            "the other should have been failed by the same desynchronization: {results:?}"
        );

        let watcher = tokio::spawn(async move {
            assert!(
                server.recv_request().await.is_err(),
                "a desynchronized client wrote again"
            );
        });
        assert_eq!(
            client.send(UnitId(0x11), read_holding()).await,
            Err(Error::Desynchronized)
        );
        drop(client);
        watcher.await.expect("watcher finishes");
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-092 — recovery from desynchronization is discard and reconstruct:
    /// the old handle stays refused, but a fresh client over a fresh
    /// transport works normally.
    async fn ut_recovery_is_discard_and_reconstruct() {
        let (client, mut server) = pipeline_pair_with_config(PipelineConfig {
            response_timeout: Duration::from_millis(10),
            ..PipelineConfig::default()
        });
        let c1 = client.clone();
        let t1 = tokio::spawn(async move { c1.send(UnitId(0x11), read_holding()).await });
        server.recv_request().await.expect("receives");
        assert_eq!(
            t1.await.expect("task"),
            Err(Error::Timeout { what: "response" })
        );
        assert_eq!(
            client.send(UnitId(0x11), read_holding()).await,
            Err(Error::Desynchronized)
        );
        drop(client);
        drop(server);

        let (client, mut server) = pipeline_pair();
        let answering = tokio::spawn(async move {
            let (header, request) = server.recv_request().await.expect("receives");
            assert_eq!(request, read_holding());
            server
                .send_response(&header, &registers())
                .await
                .expect("responds");
        });
        assert_eq!(
            client.send(UnitId(0x11), read_holding()).await,
            Ok(registers())
        );
        answering.await.expect("server task finishes");
    }

    /// A `PipelinedUdpClient` and the peer transport it exchanges with, two
    /// loopback sockets `connect`ed to each other — same shape as
    /// `ut_client_is_generic_over_udp_transport` (`src/client/mod.rs`).
    async fn udp_pair() -> (PipelinedUdpClient, UdpTransport<Tcp>) {
        udp_pair_with_config(PipelineConfig::default()).await
    }

    async fn udp_pair_with_config(
        config: PipelineConfig,
    ) -> (PipelinedUdpClient, UdpTransport<Tcp>) {
        let client_socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("binds");
        let peer_socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("binds");
        let client_addr = client_socket.local_addr().expect("has an address");
        let peer_addr = peer_socket.local_addr().expect("has an address");
        client_socket.connect(peer_addr).await.expect("connects");
        peer_socket.connect(client_addr).await.expect("connects");
        (
            PipelinedUdpClient::with_config(UdpTransport::new(client_socket), config),
            UdpTransport::new(peer_socket),
        )
    }

    #[tokio::test]
    /// CL-R-091 — a response timeout on one in-flight `PipelinedUdpClient`
    /// request fails only that request: the connection is not
    /// desynchronized, and a concurrent, answered request still succeeds.
    ///
    /// Not run with a paused clock (unlike the TCP timeout tests above):
    /// this test drives real UDP socket I/O, and tokio's auto-advance would
    /// fast-forward the virtual clock past `response_timeout` before the
    /// real localhost round trip below gets a chance to complete.
    async fn ut_udp_timeout_fails_only_that_request() {
        let (client, mut peer) = udp_pair_with_config(PipelineConfig {
            response_timeout: Duration::from_millis(200),
            ..PipelineConfig::default()
        })
        .await;
        let c1 = client.clone();
        let c2 = client.clone();
        let t1 = tokio::spawn(async move { c1.send(UnitId(0x11), read_holding()).await });
        let t2 = tokio::spawn(async move { c2.send(UnitId(0x11), read_holding()).await });

        // Answer only one of the two; the other times out unanswered.
        let (header, request) = peer.recv_request().await.expect("receives one of the two");
        assert_eq!(request, read_holding());
        peer.recv_request()
            .await
            .expect("receives the other, left unanswered");
        peer.send_response(&header, &registers())
            .await
            .expect("answers the first only");

        let results = [t1.await.expect("task"), t2.await.expect("task")];
        assert!(
            results.iter().any(|r| *r == Ok(registers())),
            "the answered request should have succeeded: {results:?}"
        );
        assert!(
            results
                .iter()
                .any(|r| *r == Err(Error::Timeout { what: "response" })),
            "the unanswered request should have timed out, not desynchronized: {results:?}"
        );

        // Not desynchronized: a third request still succeeds normally.
        let c3 = client.clone();
        let t3 = tokio::spawn(async move { c3.send(UnitId(0x11), read_holding()).await });
        let (header, _) = peer.recv_request().await.expect("receives a third request");
        peer.send_response(&header, &registers())
            .await
            .expect("answers");
        assert_eq!(t3.await.expect("task"), Ok(registers()));
    }

    #[tokio::test]
    /// CL-R-091 — an I/O failure (distinct from a timeout) still
    /// desynchronizes the whole `PipelinedUdpClient` connection, same
    /// posture as CL-R-031. A UDP socket connected to a peer whose port has
    /// since gone away reports this as a `recv` error once the kernel
    /// delivers the resulting ICMP port-unreachable back to us (Linux).
    async fn ut_udp_io_failure_still_desynchronizes() {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("binds");
        let dead_peer = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("binds");
        let peer_addr = dead_peer.local_addr().expect("has an address");
        socket.connect(peer_addr).await.expect("connects");
        // The port is now unbound; a datagram sent to it triggers an ICMP
        // port-unreachable, which a later `recv` on `socket` observes.
        drop(dead_peer);

        let client = PipelinedUdpClient::with_config(
            UdpTransport::new(socket),
            PipelineConfig {
                response_timeout: Duration::from_secs(2),
                ..PipelineConfig::default()
            },
        );

        // The first send may resolve as the I/O failure itself or as a
        // timeout, depending on how quickly the ICMP error is delivered —
        // either way the connection ends up desynchronized.
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            client.send(UnitId(0x11), read_holding()),
        )
        .await;
        assert_eq!(
            client.send(UnitId(0x11), read_holding()).await,
            Err(Error::Desynchronized)
        );
    }

    #[tokio::test]
    /// CL-R-089 — same rule as `PipelinedClient`'s TCP case, over real UDP
    /// sockets: a datagram carrying a transaction id this handle never
    /// allocated desynchronizes it.
    async fn ut_udp_never_issued_id_desynchronizes() {
        let (client, mut peer) = udp_pair().await;
        let c1 = client.clone();
        let t1 = tokio::spawn(async move { c1.send(UnitId(0x11), read_holding()).await });
        peer.recv_request().await.expect("receives");

        let bogus = MbapHeader {
            transaction_id: TransactionId(9999),
            unit_id: UnitId(0x11),
        };
        peer.send_response(&bogus, &registers())
            .await
            .expect("sends an unsolicited reply");

        assert_eq!(t1.await.expect("task"), Err(Error::Desynchronized));
        assert_eq!(
            client.send(UnitId(0x11), read_holding()).await,
            Err(Error::Desynchronized)
        );
    }

    #[tokio::test]
    /// CL-R-083 — `PipelinedUdpClient` is documented as
    /// `PipelinedClient<UdpTransport<Tcp>>`: the alias resolves and the
    /// constructor works unchanged for the UDP instantiation.
    async fn ut_pipelined_udp_client_is_the_documented_alias() {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("binds");
        let peer_addr = socket.local_addr().expect("has an address");
        socket.connect(peer_addr).await.expect("connects");
        let _client: PipelinedUdpClient = PipelinedClient::new(UdpTransport::new(socket));
    }

    #[tokio::test]
    /// CL-R-082 — pipelining is genuinely concurrent: four
    /// requests fire without waiting for each other, the peer answers them
    /// in reverse order, and each caller still resolves with its own
    /// matching response — proving dispatch is keyed by transaction id, not
    /// FIFO arrival order.
    async fn ut_udp_pipelining_is_genuinely_concurrent() {
        let (client, mut peer) = udp_pair().await;

        let mut tasks = Vec::new();
        for i in 0..4u16 {
            let c = client.clone();
            tasks.push(tokio::spawn(async move {
                c.send(
                    UnitId(0x11),
                    RequestPdu::ReadHoldingRegisters {
                        address: Address(i),
                        quantity: Quantity(1),
                    },
                )
                .await
            }));
        }

        let mut received = Vec::new();
        for _ in 0..4 {
            received.push(peer.recv_request().await.expect("receives"));
        }
        // Answer in reverse order: the last request received gets the first
        // response sent.
        for (header, request) in received.into_iter().rev() {
            let RequestPdu::ReadHoldingRegisters { address, .. } = request else {
                panic!("unexpected request shape");
            };
            peer.send_response(
                &header,
                &ResponsePdu::ReadHoldingRegisters {
                    registers: vec![RegisterValue(address.0)],
                },
            )
            .await
            .expect("answers");
        }

        for (i, task) in tasks.into_iter().enumerate() {
            let index = u16::try_from(i).expect("test uses fewer than u16::MAX requests");
            assert_eq!(
                task.await.expect("task"),
                Ok(ResponsePdu::ReadHoldingRegisters {
                    registers: vec![RegisterValue(index)],
                }),
                "request {i} must resolve with its own response, not another's"
            );
        }
    }
}
