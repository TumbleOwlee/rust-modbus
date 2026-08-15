//! Byte-level transports: sockets, serial ports, and ADU boundaries
//! (`TR-R-*`).
//!
//! Role-agnostic: a client and a server use the same types, differing only in
//! which direction they send and which they receive (TR-R-002).

#[cfg(feature = "rs485")]
mod rs485;
#[cfg(feature = "rtu")]
mod rtu;
mod serial;
mod tcp;
#[cfg(feature = "tls")]
mod tls;
mod udp;

use core::future::Future;
use core::marker::PhantomData;
use core::time::Duration;

use alloc::vec::Vec;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};

use crate::error::{Error, Result};
use crate::frame::{AduBoundary, Direction, Extent, Framing, RequestPdu, ResponsePdu};

pub use serial::{DataBits, FlowControl, Parity, SerialConfig, StopBits};
#[cfg(feature = "rs485")]
pub use serial::{Rs485Config, RtsPolarity};
pub use tcp::{
    RtuOverTcpTransport, TcpConfig, TcpListener, TcpTransport, connect_tcp, connect_tcp_framed,
};
pub use udp::{
    UdpConfig, UdpTransport, UdpTransportReader, UdpTransportWriter, connect_udp,
    recv_datagram_request, send_datagram_response_into,
};

#[cfg(feature = "rtu")]
pub use rtu::{SerialStream, SerialTransport, open_serial};

#[cfg(feature = "tls")]
pub use tls::{
    ClientCertPolicy, ClientIdentity, MODBUS_TLS_PORT, RootStore, ServerCertVerification,
    TlsClientConfig, TlsClientTransport, TlsListener, TlsServerConfig, connect_tls,
    connect_tls_framed, load_pem_cert_chain, load_pem_private_key,
};

/// What boundary detection needs that the framing itself cannot supply
/// (TR-R-011, TR-R-076).
///
/// RTU consults `inter_frame_interval`, ASCII consults
/// `ascii_inter_character_timeout`; TCP and RTU-over-TCP ADUs are
/// self-delimiting and ignore both (TR-R-048).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TransportConfig {
    /// Silence that ends an RTU frame.
    pub inter_frame_interval: Duration,
    /// Silence, once an ASCII frame has begun, that abandons it (TR-R-076).
    pub ascii_inter_character_timeout: Duration,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            // The interval implied by the default serial line, 19200 8E1.
            inter_frame_interval: Duration::from_nanos(2_005_208),
            // Fixed by the Modbus spec, not derived from baud rate (TR-R-076).
            ascii_inter_character_timeout: Duration::from_secs(1),
        }
    }
}

impl TransportConfig {
    /// Derive the inter-frame interval from a serial line's parameters
    /// (TR-R-011).
    ///
    /// The ASCII inter-character timeout is not serial-derived (TR-R-076), so
    /// it comes from the default regardless of line speed.
    ///
    /// # Errors
    ///
    /// Fails if the configuration implies no character time.
    pub fn from_serial(config: &SerialConfig) -> Result<Self> {
        Ok(Self {
            inter_frame_interval: config.inter_frame_interval()?,
            ascii_inter_character_timeout: TransportConfig::default().ascii_inter_character_timeout,
        })
    }
}

/// A byte stream carrying framed Modbus ADUs (TR-R-002).
///
/// Generic over the stream so anything async and duplex serves — a socket, a
/// serial port, or an in-memory pair in a test (TR-R-001).
#[derive(Debug)]
pub struct FrameTransport<S, F> {
    /// The underlying byte stream.
    stream: S,
    /// Bytes read but not yet consumed by a caller (TR-R-004).
    buffer: Vec<u8>,
    /// The one buffer every outgoing ADU is encoded into, cleared between
    /// frames but never shrunk, so sending allocates nothing in steady state
    /// (TR-R-043).
    outgoing: Vec<u8>,
    /// Boundary parameters the framing cannot supply on its own.
    config: TransportConfig,
    /// Set while a receive is in flight; a receive that never returned left the
    /// buffer in an unknown state (TR-R-041).
    receiving: bool,
    /// Which framing this transport speaks.
    framing: PhantomData<F>,
}

impl<S, F> FrameTransport<S, F>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    F: Framing,
{
    /// Wrap a stream, with the default boundary parameters.
    pub fn new(stream: S) -> Self {
        Self::with_config(stream, TransportConfig::default())
    }

    /// Wrap a stream with explicit boundary parameters.
    pub fn with_config(stream: S, config: TransportConfig) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
            outgoing: Vec::new(),
            config,
            receiving: false,
            framing: PhantomData,
        }
    }

    /// Recover the underlying stream.
    pub fn into_inner(self) -> S {
        self.stream
    }

    /// Send a request (TR-R-003).
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode, or if the stream does.
    pub async fn send_request(&mut self, header: &F::Header, pdu: &RequestPdu) -> Result<()> {
        self.outgoing.clear();
        F::encode_request_into(header, pdu, &mut self.outgoing)?;
        shared::send(&mut self.stream, &mut self.outgoing).await
    }

    /// Send a response (TR-R-003).
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode, or if the stream does.
    pub async fn send_response(&mut self, header: &F::Header, pdu: &ResponsePdu) -> Result<()> {
        self.outgoing.clear();
        F::encode_response_into(header, pdu, &mut self.outgoing)?;
        shared::send(&mut self.stream, &mut self.outgoing).await
    }

    /// Receive one request (TR-R-004).
    ///
    /// # Errors
    ///
    /// Fails if the stream does, if the peer disappears mid-ADU, or if the ADU
    /// does not decode.
    pub async fn recv_request(&mut self) -> Result<(F::Header, RequestPdu)> {
        let adu = shared::recv_adu::<S, F>(
            &mut self.stream,
            &mut self.buffer,
            &self.config,
            &mut self.receiving,
            Direction::Request,
        )
        .await?;
        F::decode_request(&adu)
    }

    /// Receive one response (TR-R-004).
    ///
    /// # Errors
    ///
    /// Fails if the stream does, if the peer disappears mid-ADU, or if the ADU
    /// does not decode.
    pub async fn recv_response(&mut self) -> Result<(F::Header, ResponsePdu)> {
        let adu = shared::recv_adu::<S, F>(
            &mut self.stream,
            &mut self.buffer,
            &self.config,
            &mut self.receiving,
            Direction::Response,
        )
        .await?;
        F::decode_response(&adu)
    }

    /// Split into an owned, independently pollable read half and write half
    /// (TR-R-002's "anything async and duplex" now includes the two-task
    /// shape a background pipeline needs).
    ///
    /// A caller that races receiving against writing in one `select!` loop —
    /// `PipelinedClient`'s background task (`src/client/pipeline.rs`) is the
    /// one in this crate — cannot safely call `recv_response`/`send_request`
    /// on a shared `&mut FrameTransport`: `select!` polls every branch once
    /// per pass even before picking a winner, and a `recv_response` call that
    /// gets polled and then dropped (its branch lost) leaves `receiving`
    /// latched `true` forever (TR-R-041), so every later receive fails
    /// immediately rather than actually waiting — livelocking the loop. A
    /// [`FrameTransportReader`] held across loop iterations as one
    /// never-cancelled future sidesteps this; its own send-independent
    /// [`FrameTransportWriter`] lets the same loop write concurrently.
    pub fn split(self) -> (FrameTransportReader<S, F>, FrameTransportWriter<S, F>) {
        let (read_half, write_half) = tokio::io::split(self.stream);
        (
            FrameTransportReader {
                stream: read_half,
                buffer: self.buffer,
                config: self.config,
                receiving: self.receiving,
                framing: PhantomData,
            },
            FrameTransportWriter {
                stream: write_half,
                outgoing: self.outgoing,
                framing: PhantomData,
            },
        )
    }
}

/// A read-only reader over a stream offering only `AsyncRead`: a listen-only
/// serial port, or bytes replayed from a capture (TR-R-077).
///
/// Applies the same per-framing boundary rule as [`FrameTransport`] (reusing
/// `shared::recv_adu`) and never requires a write half. Yields the raw bytes
/// of one complete ADU per call, undecoded (TR-R-079) — decode via
/// [`Framing::decode_request`]/[`Framing::decode_response`] on the result.
#[derive(Debug)]
pub struct AduReader<S, F> {
    stream: S,
    buffer: Vec<u8>,
    config: TransportConfig,
    receiving: bool,
    direction: Direction,
    framing: PhantomData<F>,
}

impl<S, F> AduReader<S, F>
where
    S: AsyncRead + Unpin + Send,
    F: Framing,
{
    /// Wrap a stream, with the default boundary parameters.
    ///
    /// `direction` is consulted only by a boundary derivation that needs it —
    /// RTU-over-stream (FR-R-146) — and ignored by TCP, RTU, and ASCII
    /// (TR-R-078).
    pub fn new(stream: S, direction: Direction) -> Self {
        Self::with_config(stream, direction, TransportConfig::default())
    }

    /// Wrap a stream with explicit boundary parameters.
    pub fn with_config(stream: S, direction: Direction, config: TransportConfig) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
            config,
            receiving: false,
            direction,
            framing: PhantomData,
        }
    }

    /// Recover the underlying stream.
    pub fn into_inner(self) -> S {
        self.stream
    }

    /// Receive one ADU's raw bytes, undecoded (TR-R-079).
    ///
    /// # Errors
    ///
    /// Fails if the stream does, if the peer disappears mid-ADU, or if a
    /// content-derived boundary cannot be resolved (TR-R-080).
    pub async fn recv_adu(&mut self) -> Result<Vec<u8>> {
        shared::recv_adu::<S, F>(
            &mut self.stream,
            &mut self.buffer,
            &self.config,
            &mut self.receiving,
            self.direction,
        )
        .await
    }
}

/// The read half of a [`FrameTransport`] produced by [`FrameTransport::split`]
/// (TR-R-004).
#[derive(Debug)]
pub struct FrameTransportReader<S, F> {
    stream: ReadHalf<S>,
    buffer: Vec<u8>,
    config: TransportConfig,
    receiving: bool,
    framing: PhantomData<F>,
}

impl<S, F> FrameTransportReader<S, F>
where
    S: AsyncRead + Send,
    F: Framing,
{
    /// Receive one request (TR-R-004). See [`FrameTransport::recv_request`].
    ///
    /// # Errors
    ///
    /// Fails if the stream does, if the peer disappears mid-ADU, or if the ADU
    /// does not decode.
    pub async fn recv_request(&mut self) -> Result<(F::Header, RequestPdu)> {
        let adu = shared::recv_adu::<ReadHalf<S>, F>(
            &mut self.stream,
            &mut self.buffer,
            &self.config,
            &mut self.receiving,
            Direction::Request,
        )
        .await?;
        F::decode_request(&adu)
    }

    /// Receive one response (TR-R-004). See [`FrameTransport::recv_response`].
    ///
    /// # Errors
    ///
    /// Fails if the stream does, if the peer disappears mid-ADU, or if the ADU
    /// does not decode.
    pub async fn recv_response(&mut self) -> Result<(F::Header, ResponsePdu)> {
        let adu = shared::recv_adu::<ReadHalf<S>, F>(
            &mut self.stream,
            &mut self.buffer,
            &self.config,
            &mut self.receiving,
            Direction::Response,
        )
        .await?;
        F::decode_response(&adu)
    }
}

/// The write half of a [`FrameTransport`] produced by [`FrameTransport::split`]
/// (TR-R-003).
#[derive(Debug)]
pub struct FrameTransportWriter<S, F> {
    stream: WriteHalf<S>,
    outgoing: Vec<u8>,
    framing: PhantomData<F>,
}

impl<S, F> FrameTransportWriter<S, F>
where
    S: AsyncWrite + Send,
    F: Framing,
{
    /// Send a request (TR-R-003). See [`FrameTransport::send_request`].
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode, or if the stream does.
    pub async fn send_request(&mut self, header: &F::Header, pdu: &RequestPdu) -> Result<()> {
        self.outgoing.clear();
        F::encode_request_into(header, pdu, &mut self.outgoing)?;
        shared::send(&mut self.stream, &mut self.outgoing).await
    }

    /// Send a response (TR-R-003). See [`FrameTransport::send_response`].
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode, or if the stream does.
    pub async fn send_response(&mut self, header: &F::Header, pdu: &ResponsePdu) -> Result<()> {
        self.outgoing.clear();
        F::encode_response_into(header, pdu, &mut self.outgoing)?;
        shared::send(&mut self.stream, &mut self.outgoing).await
    }
}

/// Boundary-detection logic shared by [`FrameTransport`] and its split
/// [`FrameTransportReader`]/[`FrameTransportWriter`] halves — TR-R-004's
/// framing rules have exactly one correct implementation regardless of which
/// type owns the stream half at the time.
mod shared {
    use super::{
        AduBoundary, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Direction, Error, Extent,
        Framing, READ_CHUNK, Result, TransportConfig, find,
    };
    use alloc::vec::Vec;

    /// Read exactly one ADU, leaving any surplus buffered (TR-R-004).
    ///
    /// The ADU's bytes leave the buffer before it is decoded, so a decode
    /// failure costs exactly that frame and no more (TR-R-005).
    pub(super) async fn recv_adu<R, F>(
        stream: &mut R,
        buffer: &mut Vec<u8>,
        config: &TransportConfig,
        receiving: &mut bool,
        direction: Direction,
    ) -> Result<Vec<u8>>
    where
        R: AsyncRead + Unpin,
        F: Framing,
    {
        if *receiving {
            // A previous receive was abandoned part-way through an ADU, so the
            // buffer may hold a fragment of one (TR-R-041).
            return Err(Error::Timeout { what: "receive" });
        }
        *receiving = true;
        let result = read_adu::<R, F>(stream, buffer, config, direction).await;
        *receiving = false;
        if result.is_err() && F::boundary().is_self_locating() {
            // No ADU was delimited, so these bytes belong to no frame anyone
            // can name. This framing finds the next boundary on the wire, so
            // dropping them is what lets the next receive start clean
            // (TR-R-044); keeping them would fail the same way forever.
            buffer.clear();
        }
        result
    }

    /// Apply this framing's boundary rule until one ADU is in hand (FR-R-122).
    async fn read_adu<R, F>(
        stream: &mut R,
        buffer: &mut Vec<u8>,
        config: &TransportConfig,
        direction: Direction,
    ) -> Result<Vec<u8>>
    where
        R: AsyncRead + Unpin,
        F: Framing,
    {
        match F::boundary() {
            AduBoundary::Prefixed { prefix, total } => {
                read_prefixed::<R, F>(stream, buffer, prefix, total).await
            }
            AduBoundary::Delimited { start, end } => {
                read_delimited::<R, F>(stream, buffer, config, start, end).await
            }
            AduBoundary::Silence => read_until_silence::<R, F>(stream, buffer, config).await,
            AduBoundary::ContentLength { min, extent } => {
                read_content_length::<R, F>(stream, buffer, min, extent, direction).await
            }
        }
    }

    /// A length-prefixed ADU: read enough to compute the length, validate it,
    /// then read the rest (TR-R-010).
    async fn read_prefixed<R, F>(
        stream: &mut R,
        buffer: &mut Vec<u8>,
        prefix: usize,
        total: fn(&[u8]) -> Result<usize>,
    ) -> Result<Vec<u8>>
    where
        R: AsyncRead + Unpin,
        F: Framing,
    {
        fill_to::<R, F>(stream, buffer, prefix).await?;
        let head = buffer
            .get(..prefix)
            .expect("fill_to returned, so the buffer holds at least prefix bytes");
        // The length is validated before it sizes anything (TR-R-010).
        let len = total(head)?;
        fill_to::<R, F>(stream, buffer, len).await?;
        Ok(take(buffer, len))
    }

    /// A delimited ADU: discard anything before the start byte, then read to
    /// the terminator (TR-R-012).
    async fn read_delimited<R, F>(
        stream: &mut R,
        buffer: &mut Vec<u8>,
        config: &TransportConfig,
        start: u8,
        end: &'static [u8],
    ) -> Result<Vec<u8>>
    where
        R: AsyncRead + Unpin,
        F: Framing,
    {
        let mut searched = 0;
        loop {
            if let Some(offset) = buffer.iter().position(|byte| *byte == start) {
                if offset > 0 {
                    // Bytes before a start byte belong to no ADU (TR-R-012).
                    buffer.drain(..offset);
                    searched = 0;
                }
                if let Some(at) = find(buffer, end, searched) {
                    return Ok(take(buffer, at.saturating_add(end.len())));
                }
                // All but a possible partial terminator has been searched.
                searched = buffer.len().saturating_sub(end.len().saturating_sub(1));
            } else {
                // No start byte in sight; none of these bytes can begin an ADU.
                buffer.clear();
                searched = 0;
            }
            check_buffer_bound::<F>(buffer)?;
            if buffer.is_empty() {
                // No start byte seen yet: unbounded, same as RTU waiting for a
                // frame to begin (TR-R-042).
                read_more(stream, buffer, false).await?;
            } else {
                // Mid-frame: a stall here is abandoned, not waited out
                // (TR-R-076).
                tokio::time::timeout(
                    config.ascii_inter_character_timeout,
                    read_more(stream, buffer, true),
                )
                .await
                .map_err(|_elapsed| Error::Timeout {
                    what: "ascii inter-character",
                })??;
            }
        }
    }

    /// An RTU ADU: whatever arrives before the line falls silent (TR-R-011).
    async fn read_until_silence<R, F>(
        stream: &mut R,
        buffer: &mut Vec<u8>,
        config: &TransportConfig,
    ) -> Result<Vec<u8>>
    where
        R: AsyncRead + Unpin,
        F: Framing,
    {
        loop {
            if buffer.is_empty() {
                // Nothing yet: wait rather than call the silence before the
                // frame has begun (TR-R-042).
                read_more(stream, buffer, false).await?;
                continue;
            }
            check_buffer_bound::<F>(buffer)?;
            match tokio::time::timeout(config.inter_frame_interval, read_more(stream, buffer, true))
                .await
            {
                // The line went quiet: the frame ends here.
                Err(_elapsed) => return Ok(take(buffer, buffer.len())),
                Ok(Ok(())) => {}
                // A close after a complete frame ends it just the same.
                Ok(Err(Error::ConnectionClosed)) => return Ok(take(buffer, buffer.len())),
                Ok(Err(error)) => return Err(error),
            }
        }
    }

    /// An ADU whose length is derived from its content (TR-R-045, TR-R-046).
    ///
    /// Call `extent` repeatedly as bytes arrive to determine when a complete
    /// ADU has been received. Consume exactly the extent it yields, leaving
    /// any surplus for the next ADU.
    async fn read_content_length<R, F>(
        stream: &mut R,
        buffer: &mut Vec<u8>,
        min: usize,
        extent: fn(Direction, &[u8]) -> Result<Extent>,
        direction: Direction,
    ) -> Result<Vec<u8>>
    where
        R: AsyncRead + Unpin,
        F: Framing,
    {
        // Ensure we have at least `min` bytes to start derivation (TR-R-045).
        fill_to::<R, F>(stream, buffer, min).await?;

        loop {
            // Try to determine the extent from the bytes we have (TR-R-045).
            match extent(direction, buffer)? {
                Extent::Complete(len) => {
                    // We have the complete ADU; take exactly those bytes (TR-R-045).
                    return Ok(take(buffer, len));
                }
                Extent::NeedMore => {
                    // Need more bytes; check the buffer bound and read more (TR-R-013).
                    check_buffer_bound::<F>(buffer)?;
                    read_more(stream, buffer, true).await?;
                }
            }
        }
    }

    /// Read until the buffer holds at least `len` bytes (TR-R-013).
    async fn fill_to<R, F>(stream: &mut R, buffer: &mut Vec<u8>, len: usize) -> Result<()>
    where
        R: AsyncRead + Unpin,
        F: Framing,
    {
        if len > F::MAX_ADU_LEN {
            return Err(Error::AduTooLarge {
                len,
                max: F::MAX_ADU_LEN,
            });
        }
        while buffer.len() < len {
            read_more(stream, buffer, !buffer.is_empty()).await?;
        }
        Ok(())
    }

    /// Refuse to buffer more than one ADU's worth for one ADU (TR-R-013).
    fn check_buffer_bound<F: Framing>(buffer: &[u8]) -> Result<()> {
        if buffer.len() >= F::MAX_ADU_LEN {
            return Err(Error::AduTooLarge {
                len: buffer.len(),
                max: F::MAX_ADU_LEN,
            });
        }
        Ok(())
    }

    /// Read once into the buffer.
    ///
    /// `mid_adu` distinguishes the two ways a stream can end (TR-R-014): a
    /// close between ADUs ends the stream, one inside an ADU severs a frame.
    async fn read_more<R: AsyncRead + Unpin>(
        stream: &mut R,
        buffer: &mut Vec<u8>,
        mid_adu: bool,
    ) -> Result<()> {
        let mut chunk = [0u8; READ_CHUNK];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(if mid_adu {
                Error::ConnectionClosed
            } else {
                Error::Io {
                    kind: std::io::ErrorKind::UnexpectedEof,
                }
            });
        }
        buffer.extend_from_slice(
            chunk
                .get(..read)
                .expect("a read never reports more bytes than the chunk holds"),
        );
        Ok(())
    }

    /// Remove and return the first `len` buffered bytes, keeping the rest for
    /// the next call (TR-R-004).
    fn take(buffer: &mut Vec<u8>, len: usize) -> Vec<u8> {
        buffer.drain(..len).collect()
    }

    /// Write every byte of an ADU (TR-R-003).
    pub(super) async fn send<W: AsyncWrite + Unpin>(
        stream: &mut W,
        outgoing: &mut Vec<u8>,
    ) -> Result<()> {
        stream.write_all(outgoing).await?;
        stream.flush().await?;
        // The bytes are gone; the capacity stays (TR-R-043).
        outgoing.clear();
        Ok(())
    }
}

/// A client-side exchange over some transport, named once so a consumer can be
/// written generically over either `FrameTransport` or `UdpTransport`
/// (TR-R-075).
///
/// Declared with an explicit `-> impl Future` return rather than `async fn`
/// (`async fn` in a public trait is a clippy/rustc lint, `async_fn_in_trait`:
/// auto-trait bounds on the future cannot be named). No `+ Send` is added,
/// unlike `Service` (`src/server/service.rs`): `Service` futures are spawned
/// per connection and so must be `Send`, but nothing in this crate spawns a
/// `Client`'s own future — `Client::call` (and every typed method built on it)
/// is always awaited inline, including from `SyncClient::call`'s
/// `runtime.block_on`. A `+ Send` bound here would force `F: Send` and
/// `F::Header: Sync` onto every generic-over-`F` caller (`SyncClient<S, F>`
/// among them) for no consumer that needs it.
pub trait ClientTransport<F: Framing> {
    /// Send a request (TR-R-075).
    fn send_request(
        &mut self,
        header: &F::Header,
        pdu: &RequestPdu,
    ) -> impl Future<Output = Result<()>>;

    /// Receive a response (TR-R-075).
    fn recv_response(&mut self) -> impl Future<Output = Result<(F::Header, ResponsePdu)>>;
}

impl<S, F> ClientTransport<F> for FrameTransport<S, F>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    F: Framing,
{
    fn send_request(
        &mut self,
        header: &F::Header,
        pdu: &RequestPdu,
    ) -> impl Future<Output = Result<()>> {
        FrameTransport::send_request(self, header, pdu)
    }

    fn recv_response(&mut self) -> impl Future<Output = Result<(F::Header, ResponsePdu)>> {
        FrameTransport::recv_response(self)
    }
}

/// Bytes read from the stream at a time; one read covers the largest ADU any
/// framing permits (FR-R-113).
const READ_CHUNK: usize = 513;

/// First index at or after `from` where `needle` occurs in `haystack`.
fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| offset.saturating_add(from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Error, Result};
    use crate::frame::{MbapHeader, Tcp};
    use crate::{Address, Quantity, RegisterValue, RequestPdu, ResponsePdu, TransactionId, UnitId};
    use alloc::vec;
    use tokio::io::{AsyncWriteExt, duplex};
    use tokio::net::UdpSocket;

    /// The MBAP header of every fixture below: transaction 1, unit `0x11`.
    fn header() -> MbapHeader {
        MbapHeader {
            transaction_id: TransactionId(1),
            unit_id: UnitId(0x11),
        }
    }

    pub(super) fn read_holding() -> RequestPdu {
        RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        }
    }

    /// That request as a TCP ADU: a 6-byte prefix, a length field of 6, the
    /// unit identifier, and the 5-byte PDU.
    const REQUEST_ADU: [u8; 12] = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x11, 0x03, 0x00, 0x6B, 0x00, 0x03,
    ];

    #[tokio::test]
    /// TR-R-075 — `ClientTransport<F>` is implemented by both `FrameTransport`
    /// and `UdpTransport`: one generic function, bound only on the trait,
    /// round-trips a request/response pair through either, called once per
    /// transport.
    async fn ut_client_transport_is_implemented_by_frame_and_udp_transports() {
        async fn exchange<T: ClientTransport<Tcp>>(
            client: &mut T,
        ) -> Result<(MbapHeader, ResponsePdu)> {
            client.send_request(&header(), &read_holding()).await?;
            client.recv_response().await
        }

        fn registers() -> ResponsePdu {
            ResponsePdu::ReadHoldingRegisters {
                registers: vec![RegisterValue(0x022B)],
            }
        }

        // FrameTransport, over an in-memory duplex pair.
        let (client_end, server_end) = duplex(1024);
        let mut client = FrameTransport::<_, Tcp>::new(client_end);
        let mut server = FrameTransport::<_, Tcp>::new(server_end);
        let answering = tokio::spawn(async move {
            let (received_header, request) = server.recv_request().await.expect("receives");
            assert_eq!(request, read_holding());
            server
                .send_response(&received_header, &registers())
                .await
                .expect("responds");
        });
        assert_eq!(exchange(&mut client).await, Ok((header(), registers())));
        answering.await.expect("the frame peer finishes");

        // UdpTransport, over two loopback sockets bound to an ephemeral port.
        let a = UdpSocket::bind("127.0.0.1:0").await.expect("binds");
        let b = UdpSocket::bind("127.0.0.1:0").await.expect("binds");
        a.connect(b.local_addr().expect("has an address"))
            .await
            .expect("connects");
        b.connect(a.local_addr().expect("has an address"))
            .await
            .expect("connects");
        let mut client = UdpTransport::<Tcp>::new(a);
        let mut peer = UdpTransport::<Tcp>::new(b);
        let answering = tokio::spawn(async move {
            let (received_header, request) = peer.recv_request().await.expect("receives");
            assert_eq!(request, read_holding());
            peer.send_response(&received_header, &registers())
                .await
                .expect("responds");
        });
        assert_eq!(exchange(&mut client).await, Ok((header(), registers())));
        answering.await.expect("the udp peer finishes");
    }

    #[tokio::test]
    /// TR-R-043 — the transport owns one outgoing buffer, reused frame after
    /// frame: its contents are cleared between sends but its capacity, once
    /// grown to the framing maximum, is retained rather than reallocated.
    async fn ut_write_buffer_capacity_is_retained() {
        let (client, server) = duplex(1024);
        let mut client = FrameTransport::<_, Tcp>::new(client);
        let mut server = FrameTransport::<_, Tcp>::new(server);

        client
            .send_request(&header(), &read_holding())
            .await
            .expect("sends");
        let capacity = client.outgoing.capacity();
        assert!(
            capacity >= Tcp::MAX_ADU_LEN,
            "reserved {capacity} of {}",
            Tcp::MAX_ADU_LEN
        );
        // The frame is out of the door; nothing of it is held back.
        assert_eq!(client.outgoing.len(), 0);
        server.recv_request().await.expect("receives");

        client
            .send_request(&header(), &read_holding())
            .await
            .expect("sends");
        assert_eq!(
            client.outgoing.capacity(),
            capacity,
            "the second frame reallocated the buffer"
        );
        assert_eq!(client.outgoing.len(), 0);
        server.recv_request().await.expect("receives");
    }

    #[tokio::test]
    /// TR-R-001 — the transport is generic over the stream: an in-memory duplex
    /// pair serves exactly as a socket does, which is what makes every rule
    /// below testable without a network.
    async fn ut_transport_works_over_a_duplex_pair() {
        let (client, server) = duplex(64);
        let mut client = FrameTransport::<_, Tcp>::new(client);
        let mut server = FrameTransport::<_, Tcp>::new(server);

        client
            .send_request(&header(), &read_holding())
            .await
            .expect("sends");
        assert_eq!(server.recv_request().await, Ok((header(), read_holding())));
    }

    #[tokio::test]
    /// TR-R-003 — every byte of an ADU is written before the send returns. The
    /// one-byte pipe forces the write to be split, so a send that returned
    /// after a partial write would lose bytes.
    async fn ut_send_writes_every_byte() {
        let (client, server) = duplex(1);
        let mut client = FrameTransport::<_, Tcp>::new(client);
        let mut server = FrameTransport::<_, Tcp>::new(server);

        let sender = tokio::spawn(async move {
            client
                .send_request(&header(), &read_holding())
                .await
                .expect("sends");
        });
        assert_eq!(server.recv_request().await, Ok((header(), read_holding())));
        sender.await.expect("sender completes");
    }

    #[tokio::test]
    /// TR-R-004 — two ADUs arriving in one read are delivered one per call; the
    /// surplus is retained rather than discarded.
    async fn ut_two_adus_in_one_read_are_delivered_separately() {
        let (mut peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Tcp>::new(server);

        let mut both = REQUEST_ADU.to_vec();
        both.extend_from_slice(&REQUEST_ADU);
        peer.write_all(&both).await.expect("writes both");

        for _ in 0..2 {
            assert_eq!(server.recv_request().await, Ok((header(), read_holding())));
        }
    }

    #[tokio::test]
    /// TR-R-010 — a read that splits an ADU anywhere is resumed until the
    /// length field, and then the ADU, are complete.
    async fn ut_tcp_boundary_from_mbap_length() {
        let (mut peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Tcp>::new(server);

        // Split inside the MBAP header, before the length field is complete.
        peer.write_all(&REQUEST_ADU[..5])
            .await
            .expect("writes head");
        peer.write_all(&REQUEST_ADU[5..])
            .await
            .expect("writes tail");

        assert_eq!(server.recv_request().await, Ok((header(), read_holding())));
    }

    #[tokio::test]
    /// TR-R-005 — a frame that does not decode consumes exactly its own bytes:
    /// the next ADU behind it still arrives intact.
    async fn ut_decode_failure_leaves_transport_usable() {
        let (mut peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Tcp>::new(server);

        // A well-framed ADU carrying function code 0, which is not a request
        // (FR-R-014), followed by a good one.
        let mut bytes = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x11, 0x00];
        bytes.extend_from_slice(&REQUEST_ADU);
        peer.write_all(&bytes).await.expect("writes both");

        assert_eq!(
            server.recv_request().await,
            Err(Error::InvalidFunctionCode(0))
        );
        assert_eq!(server.recv_request().await, Ok((header(), read_holding())));
    }

    #[tokio::test]
    /// TR-R-010, FR-R-105 — an MBAP length field outside its permitted range is
    /// rejected before it sizes a read, so a hostile length cannot make the
    /// transport wait for bytes that will never come.
    async fn ut_tcp_invalid_length_is_rejected_before_reading() {
        let (mut peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Tcp>::new(server);

        peer.write_all(&[0x00, 0x01, 0x00, 0x00, 0xFF, 0xFF])
            .await
            .expect("writes header");

        assert_eq!(
            server.recv_request().await,
            Err(Error::OutOfRange {
                field: "MBAP length",
                value: 65535,
                min: 1,
                max: 254,
            })
        );
    }

    #[tokio::test]
    /// TR-R-014 — a stream that ends between two ADUs is an end of stream; one
    /// that ends inside an ADU severed a frame, and says so differently.
    async fn ut_eof_between_adus_vs_mid_adu() {
        let (peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Tcp>::new(server);
        drop(peer);
        assert_eq!(
            server.recv_request().await,
            Err(Error::Io {
                kind: std::io::ErrorKind::UnexpectedEof,
            })
        );

        let (mut peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Tcp>::new(server);
        peer.write_all(&REQUEST_ADU[..7])
            .await
            .expect("writes head");
        drop(peer);
        assert_eq!(server.recv_request().await, Err(Error::ConnectionClosed));
    }

    #[cfg(feature = "serde")]
    #[test]
    /// TR-R-058 — `TransportConfig` round-trips through JSON, `Duration`
    /// fields keeping `Duration`'s own serde representation. The
    /// `inter_frame_interval` default (2,005,208 ns, derived from 19200 8E1)
    /// must survive exactly: a millisecond representation would round it to
    /// 2 ms and silently change RTU framing timing. `ascii_inter_character_timeout`
    /// (TR-R-076) round-trips the same way.
    fn ut_transport_config_serde_roundtrip() {
        let config = TransportConfig::default();
        let text = serde_json::to_string(&config).expect("serializes");
        assert_eq!(
            text,
            r#"{"inter_frame_interval":{"secs":0,"nanos":2005208},"ascii_inter_character_timeout":{"secs":1,"nanos":0}}"#
        );
        assert_eq!(
            serde_json::from_str::<TransportConfig>(&text).expect("deserializes"),
            config
        );
    }
}

#[cfg(test)]
mod ascii_tests {
    use super::tests::read_holding;
    use super::*;
    use crate::error::Error;
    use crate::frame::Ascii;
    use crate::frame::UnitId;
    use alloc::vec;
    use tokio::io::{AsyncWriteExt, duplex};

    /// The specification's Read Holding Registers request to server `0x11`, as
    /// an ASCII ADU (FR-R-110).
    const REQUEST_ADU: &[u8] = b":1103006B00037E\r\n";

    #[tokio::test]
    /// TR-R-012 — an ASCII ADU opens on `:` and closes on the first CR LF after
    /// it; bytes arriving before the `:` belong to no ADU and are discarded.
    async fn ut_ascii_boundary_and_leading_garbage() {
        let (mut peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Ascii>::new(server);

        let mut bytes = b"line noise\r\n".to_vec();
        bytes.extend_from_slice(REQUEST_ADU);
        peer.write_all(&bytes).await.expect("writes");

        assert_eq!(
            server.recv_request().await,
            Ok((UnitId(0x11), read_holding()))
        );
    }

    #[tokio::test(start_paused = true)]
    /// TR-R-076 — once the ASCII start byte has begun a frame, a sender that
    /// falls silent past the inter-character timeout is abandoned rather than
    /// held forever; the gathered bytes are discarded (TR-R-044) so the next
    /// receive starts clean.
    async fn ut_ascii_inter_character_timeout_abandons_stalled_frame() {
        let (mut peer, server) = duplex(64);
        let config = TransportConfig {
            ascii_inter_character_timeout: Duration::from_millis(50),
            ..TransportConfig::default()
        };
        let mut server = FrameTransport::<_, Ascii>::with_config(server, config);

        peer.write_all(b":110300")
            .await
            .expect("writes partial frame");

        assert_eq!(
            server.recv_request().await,
            Err(Error::Timeout {
                what: "ascii inter-character"
            })
        );

        // The stalled attempt's bytes are gone; a fresh frame is received as if
        // nothing had happened (TR-R-044).
        peer.write_all(REQUEST_ADU).await.expect("writes a frame");
        assert_eq!(
            server.recv_request().await,
            Ok((UnitId(0x11), read_holding()))
        );
    }

    #[tokio::test]
    /// TR-R-004 — the terminator ends the ADU exactly, so a second frame in the
    /// same read is still there for the next call.
    async fn ut_ascii_two_frames_in_one_read() {
        let (mut peer, server) = duplex(128);
        let mut server = FrameTransport::<_, Ascii>::new(server);

        let mut bytes = REQUEST_ADU.to_vec();
        bytes.extend_from_slice(REQUEST_ADU);
        peer.write_all(&bytes).await.expect("writes");

        for _ in 0..2 {
            assert_eq!(
                server.recv_request().await,
                Ok((UnitId(0x11), read_holding()))
            );
        }
    }

    #[tokio::test(start_paused = true)]
    /// TR-R-012 — a terminator split across two reads still terminates: the CR
    /// arriving alone is not mistaken for the end, nor missed once the LF
    /// follows.
    ///
    /// The receive runs concurrently and the writes are separated in time, so
    /// the CR really does arrive in a read of its own -- writing both halves
    /// back to back would let the pipe coalesce them and test nothing.
    async fn ut_ascii_split_terminator_still_terminates() {
        let (mut peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Ascii>::new(server);

        let receiver = tokio::spawn(async move { server.recv_request().await });
        let (head, tail) = REQUEST_ADU.split_at(REQUEST_ADU.len() - 1);
        peer.write_all(head).await.expect("writes head");
        tokio::time::sleep(Duration::from_millis(10)).await;
        peer.write_all(tail).await.expect("writes tail");

        assert_eq!(
            receiver.await.expect("receiver completes"),
            Ok((UnitId(0x11), read_holding()))
        );
    }

    #[tokio::test]
    /// TR-R-013 — an ADU that never terminates does not grow the buffer without
    /// bound: at the framing's maximum it is an oversized ADU.
    async fn ut_oversized_adu_does_not_grow_buffer() {
        let (mut peer, server) = duplex(1024);
        let mut server = FrameTransport::<_, Ascii>::new(server);

        let mut bytes = vec![b':'];
        bytes.extend(core::iter::repeat_n(b'0', Ascii::MAX_ADU_LEN - 1));
        peer.write_all(&bytes).await.expect("writes");

        assert_eq!(
            server.recv_request().await,
            Err(Error::AduTooLarge {
                len: Ascii::MAX_ADU_LEN,
                max: Ascii::MAX_ADU_LEN,
            })
        );
    }

    #[tokio::test]
    /// TR-R-045 — ContentLength boundary reads one byte at a time until extent
    /// says the ADU is complete, then consumes exactly those bytes.
    async fn ut_rtu_over_tcp_boundary_from_content() {
        use crate::{Address, Quantity, RequestPdu, RtuOverTcp, UnitId};
        let (mut peer, server) = duplex(1024);
        let mut server = FrameTransport::<_, RtuOverTcp>::new(server);

        // Create a simple FC 3 request ADU for RtuOverTcp:
        // FC 3: FC(1) + addr(2) + qty(2) = 5 bytes PDU
        // RTU ADU: address(1) + PDU(5) + CRC(2) = 8 bytes
        let req = RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        };
        let header = UnitId(0x11);
        let encoded = RtuOverTcp::encode_request(&header, &req).expect("encodes");

        // Write one byte at a time
        let writer = tokio::spawn(async move {
            for &byte in encoded.iter() {
                peer.write_all(&[byte]).await.expect("writes byte");
            }
        });

        // Receive should assemble from byte-by-byte delivery
        let (recv_header, recv_req) = server.recv_request().await.expect("receives request");
        assert_eq!(recv_header, header);
        assert_eq!(recv_req, req);
        writer.await.expect("writer completes");
    }

    #[tokio::test]
    /// TR-R-004 — two complete ADUs arriving in one write are delivered one
    /// per call to recv_request, with the surplus retained.
    async fn ut_rtu_over_tcp_two_adus_in_one_read() {
        use crate::{Address, Quantity, RequestPdu, RtuOverTcp, UnitId};
        let (mut peer, server) = duplex(1024);
        let mut server = FrameTransport::<_, RtuOverTcp>::new(server);

        let req = RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        };
        let header = UnitId(0x11);
        let adu = RtuOverTcp::encode_request(&header, &req).expect("encodes");

        // Write two ADUs in one write
        let mut both = adu.clone();
        both.extend_from_slice(&adu);
        peer.write_all(&both).await.expect("writes both");

        // First receive
        let (h1, r1) = server.recv_request().await.expect("receives first");
        assert_eq!(h1, header);
        assert_eq!(r1, req);

        // Second receive should get the retained bytes
        let (h2, r2) = server.recv_request().await.expect("receives second");
        assert_eq!(h2, header);
        assert_eq!(r2, req);
    }

    #[tokio::test]
    /// TR-R-046 — when extent derivation fails, the attempted bytes are retained
    /// so the stream does not desynchronize.
    async fn ut_rtu_over_tcp_failed_derivation_retains_the_attempt() {
        use crate::{Address, Quantity, RequestPdu, RtuOverTcp, UnitId};
        let (mut peer, server) = duplex(1024);
        let mut server = FrameTransport::<_, RtuOverTcp>::new(server);

        // Write a valid ADU
        let req = RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        };
        let header = UnitId(0x11);
        let adu = RtuOverTcp::encode_request(&header, &req).expect("encodes");

        // Write it and corrupt the CRC
        let mut corrupted = adu.clone();
        let len = corrupted.len();
        #[allow(clippy::indexing_slicing)]
        {
            corrupted[len - 1] ^= 0xFF;
        }
        peer.write_all(&corrupted).await.expect("writes corrupted");

        // The decode should fail (CRC mismatch)
        let result = server.recv_request().await;
        assert!(result.is_err(), "corrupted CRC should fail decode");

        // Now write a good ADU
        peer.write_all(&adu).await.expect("writes good");

        // The stream is still usable: the good ADU is received
        let (h, r) = server.recv_request().await.expect("receives good");
        assert_eq!(h, header);
        assert_eq!(r, req);
    }

    #[tokio::test(start_paused = true)]
    /// TR-R-048 — inter-frame silence has no effect on RtuOverTcp framing.
    /// ContentLength boundaries are derived from bytes, not timing.
    async fn ut_rtu_over_tcp_ignores_the_inter_frame_interval() {
        use crate::{Address, Quantity, RequestPdu, RtuOverTcp, UnitId};
        let (mut peer, server) = duplex(1024);
        let mut server = FrameTransport::<_, RtuOverTcp>::new(server);

        let req = RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        };
        let header = UnitId(0x11);
        let adu = RtuOverTcp::encode_request(&header, &req).expect("encodes");

        // Write one byte
        #[allow(clippy::indexing_slicing)]
        {
            peer.write_all(&adu[..1]).await.expect("writes first byte");
        }

        // Sleep for a long time (longer than any inter-frame interval)
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Write the rest, still one at a time
        for &byte in adu.iter().skip(1) {
            peer.write_all(&[byte])
                .await
                .expect("writes byte after long delay");
        }

        // The receive should complete regardless of the delay
        let (recv_header, recv_req) = server.recv_request().await.expect("receives despite delay");
        assert_eq!(recv_header, header);
        assert_eq!(recv_req, req);
    }
}

#[cfg(test)]
mod rtu_tests {
    use super::tests::read_holding;
    use super::*;
    use crate::error::Error;
    use crate::frame::UnitId;
    use crate::frame::{Rtu, Tcp};
    use tokio::io::{AsyncWriteExt, duplex};

    /// The specification's Read Holding Registers request to server `0x11`, as
    /// an RTU ADU with the CRC of FR-R-092.
    const REQUEST_ADU: [u8; 8] = [0x11, 0x03, 0x00, 0x6B, 0x00, 0x03, 0x76, 0x87];

    /// Shorter than the default 19200 8E1 interval of 2.005 ms, so it is a gap
    /// *within* a frame rather than between two.
    const SHORT_GAP: Duration = Duration::from_micros(500);

    /// Longer than that interval: the line has gone quiet.
    const LONG_GAP: Duration = Duration::from_millis(5);

    #[tokio::test(start_paused = true)]
    /// TR-R-044 — a receive that fails before an ADU was delimited discards the
    /// bytes it had gathered, because RTU finds the next boundary in the
    /// silence rather than in the frame that failed. Were they kept, every
    /// later receive would re-read the same rubbish.
    async fn ut_failed_read_discards_the_attempt_on_rtu() {
        let (mut peer, server) = duplex(1024);
        let mut server = FrameTransport::<_, Rtu>::new(server);

        // Exactly the framing maximum, so the bound is reached with nothing
        // left over in the stream to contaminate the next frame.
        let rubbish = alloc::vec![0xFFu8; Rtu::MAX_ADU_LEN];
        peer.write_all(&rubbish).await.expect("writes rubbish");
        assert_eq!(
            server.recv_request().await,
            Err(Error::AduTooLarge {
                len: Rtu::MAX_ADU_LEN,
                max: Rtu::MAX_ADU_LEN,
            })
        );
        assert!(
            server.buffer.is_empty(),
            "the failed attempt was left in the buffer"
        );

        // The line is quiet, then a good frame: it is received as if nothing
        // had happened.
        tokio::time::sleep(LONG_GAP).await;
        peer.write_all(&REQUEST_ADU).await.expect("writes a frame");
        assert_eq!(
            server.recv_request().await,
            Ok((UnitId(0x11), read_holding()))
        );
    }

    #[tokio::test]
    /// TR-R-044 — over TCP the gathered bytes are kept instead: the length that
    /// would delimit the next frame was carried by the one that failed, so
    /// there is no later boundary to resume from and discarding would only
    /// hide that.
    async fn ut_failed_read_retains_the_attempt_on_tcp() {
        let (mut peer, client) = duplex(64);
        let mut client = FrameTransport::<_, Tcp>::new(client);

        // A well-formed MBAP prefix whose length field is illegal: the failure
        // happens after six bytes are buffered and before any ADU is delimited.
        peer.write_all(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x00])
            .await
            .expect("writes");

        assert!(client.recv_response().await.is_err());
        assert!(
            !client.buffer.is_empty(),
            "the attempt was discarded on a framing that cannot resynchronize"
        );
    }

    #[tokio::test(start_paused = true)]
    /// TR-R-011 — an RTU ADU carries no length and no terminator, so the frame
    /// ends when the line falls silent for 3.5 character times.
    async fn ut_rtu_boundary_on_idle_gap() {
        let (mut peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Rtu>::new(server);

        let receiver = tokio::spawn(async move {
            let first = server.recv_request().await;
            let second = server.recv_request().await;
            (first, second)
        });

        peer.write_all(&REQUEST_ADU).await.expect("writes first");
        tokio::time::sleep(LONG_GAP).await;
        peer.write_all(&REQUEST_ADU).await.expect("writes second");

        let (first, second) = receiver.await.expect("receiver completes");
        assert_eq!(first, Ok((UnitId(0x11), read_holding())));
        assert_eq!(second, Ok((UnitId(0x11), read_holding())));
    }

    #[tokio::test(start_paused = true)]
    /// TR-R-011 — a gap shorter than the inter-frame interval is inside a
    /// frame, not between two: the halves are delivered as one ADU. Were they
    /// split, neither half would carry a valid CRC.
    async fn ut_rtu_short_gap_does_not_end_a_frame() {
        let (mut peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Rtu>::new(server);

        let receiver = tokio::spawn(async move { server.recv_request().await });

        // Three chunks, so a frame ended at the first lull in traffic would be
        // short by the third: only waiting out the silence yields the whole ADU.
        for chunk in REQUEST_ADU.chunks(3) {
            peer.write_all(chunk).await.expect("writes chunk");
            tokio::time::sleep(SHORT_GAP).await;
        }

        assert_eq!(
            receiver.await.expect("receiver completes"),
            Ok((UnitId(0x11), read_holding()))
        );
    }

    #[tokio::test(start_paused = true)]
    /// TR-R-011 — the interval comes from the configuration, so a slower line
    /// tolerates a longer gap inside one frame.
    async fn ut_rtu_interval_follows_the_configuration() {
        let config = TransportConfig::from_serial(&SerialConfig {
            baud_rate: 1_200,
            ..SerialConfig::default()
        })
        .expect("1200 baud has a character time");
        // 11 bits at 1200 baud: 3.5 characters take 32.08 ms.
        assert_eq!(
            config.inter_frame_interval,
            Duration::from_nanos(32_083_333)
        );

        let (mut peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Rtu>::with_config(server, config);

        let receiver = tokio::spawn(async move { server.recv_request().await });

        // Gaps that would each end a frame at 19200 baud, but not at 1200.
        for chunk in REQUEST_ADU.chunks(3) {
            peer.write_all(chunk).await.expect("writes chunk");
            tokio::time::sleep(LONG_GAP).await;
        }

        assert_eq!(
            receiver.await.expect("receiver completes"),
            Ok((UnitId(0x11), read_holding()))
        );
    }

    #[tokio::test(start_paused = true)]
    /// TR-R-041 — a receive abandoned part-way through an ADU leaves the
    /// transport desynchronized: the buffer holds a fragment whose extent is
    /// unknown, so the next receive refuses rather than decode a splice.
    async fn ut_timeout_mid_adu_marks_desynchronized() {
        let (mut peer, server) = duplex(64);
        let mut server = FrameTransport::<_, Tcp>::new(server);

        // Half of a TCP ADU: enough to start one, not enough to finish it.
        peer.write_all(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x11])
            .await
            .expect("writes head");

        let abandoned = tokio::time::timeout(LONG_GAP, server.recv_request()).await;
        assert!(abandoned.is_err(), "the receive should not have completed");

        assert_eq!(
            server.recv_request().await,
            Err(Error::Timeout { what: "receive" })
        );
    }
}

#[cfg(test)]
mod reader_tests {
    use super::*;

    #[tokio::test]
    /// TR-R-077 — `AduReader` never requires a write half: this stream type
    /// implements only `AsyncRead`, and the reader still receives a complete
    /// ADU from it.
    async fn ut_reader_never_requires_a_write_half() {
        use core::pin::Pin;
        use core::task::{Context, Poll};
        use tokio::io::ReadBuf;

        struct ReadOnlyBytes(std::io::Cursor<Vec<u8>>);
        impl AsyncRead for ReadOnlyBytes {
            fn poll_read(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
                buf: &mut ReadBuf<'_>,
            ) -> Poll<std::io::Result<()>> {
                let before = buf.filled().len();
                let n = std::io::Read::read(&mut self.0, buf.initialize_unfilled())?;
                buf.set_filled(before + n);
                Poll::Ready(Ok(()))
            }
        }

        // TCP framing's REQUEST_ADU fixture (from `tests::REQUEST_ADU` above).
        let bytes = vec![
            0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x11, 0x03, 0x00, 0x6B, 0x00, 0x03,
        ];
        let stream = ReadOnlyBytes(std::io::Cursor::new(bytes.clone()));
        let mut reader = AduReader::<_, crate::frame::Tcp>::new(stream, Direction::Request);
        assert_eq!(reader.recv_adu().await, Ok(bytes));
    }

    #[tokio::test]
    /// TR-R-079 — `recv_adu` yields the ADU's raw bytes undecoded; the caller
    /// decodes them through `Framing::decode_request`.
    async fn ut_reader_yields_raw_undecoded_bytes() {
        use crate::frame::Tcp;
        let bytes: [u8; 12] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x11, 0x03, 0x00, 0x6B, 0x00, 0x03,
        ];
        let (mut peer, server) = tokio::io::duplex(64);
        let mut reader = AduReader::<_, Tcp>::new(server, Direction::Request);
        tokio::io::AsyncWriteExt::write_all(&mut peer, &bytes)
            .await
            .expect("writes");

        let adu = reader.recv_adu().await.expect("receives");
        assert_eq!(adu, bytes);
        let (header, pdu) = Tcp::decode_request(&adu).expect("decodes");
        assert_eq!(header.unit_id, crate::frame::UnitId(0x11));
        assert_eq!(
            pdu,
            crate::frame::RequestPdu::ReadHoldingRegisters {
                address: crate::frame::Address(0x006B),
                quantity: crate::frame::Quantity(3),
            }
        );
    }

    #[tokio::test]
    /// TR-R-078 — `direction` is consulted only where the boundary needs it.
    /// `Tcp` (`Prefixed`) ignores it: readers built with either `Direction`
    /// receive the same ADU whole. `RtuOverTcp` (`ContentLength`) does not:
    /// an FC 3 request is fixed-length in the request direction but carries a
    /// byte count at a different offset in the response direction, so a
    /// reader constructed with the wrong `Direction` derives a different
    /// (wrong) boundary from the identical bytes.
    async fn ut_reader_direction_governs_content_length_boundary_only() {
        use crate::frame::{Address, Quantity, RequestPdu, RtuOverTcp, Tcp, UnitId};

        let tcp_bytes: [u8; 12] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x11, 0x03, 0x00, 0x6B, 0x00, 0x03,
        ];
        for direction in [Direction::Request, Direction::Response] {
            let (mut peer, server) = tokio::io::duplex(64);
            let mut reader = AduReader::<_, Tcp>::new(server, direction);
            tokio::io::AsyncWriteExt::write_all(&mut peer, &tcp_bytes)
                .await
                .expect("writes");
            assert_eq!(reader.recv_adu().await, Ok(tcp_bytes.to_vec()));
        }

        let req = RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        };
        let adu = RtuOverTcp::encode_request(&UnitId(0x11), &req).expect("encodes");
        assert_eq!(adu.len(), 8, "address(1) + FC/addr/qty(5) + CRC(2)");

        let (mut peer, server) = tokio::io::duplex(64);
        let mut correct = AduReader::<_, RtuOverTcp>::new(server, Direction::Request);
        tokio::io::AsyncWriteExt::write_all(&mut peer, &adu)
            .await
            .expect("writes");
        assert_eq!(correct.recv_adu().await, Ok(adu.clone()));

        let (mut peer, server) = tokio::io::duplex(64);
        let mut wrong = AduReader::<_, RtuOverTcp>::new(server, Direction::Response);
        tokio::io::AsyncWriteExt::write_all(&mut peer, &adu)
            .await
            .expect("writes");
        // FC 3 read as a response: byte count is read from offset 2 (0x00,
        // the high byte of address 0x006B), so the wrong direction derives a
        // 5-byte ADU instead of the correct 8 -- a different boundary from
        // the same bytes.
        let expected = adu.get(..5).expect("adu is 8 bytes").to_vec();
        assert_eq!(wrong.recv_adu().await, Ok(expected));
    }

    #[tokio::test]
    /// TR-R-080 — after a boundary failure on a self-locating framing (ASCII,
    /// `Delimited`: an ADU that never finds its terminator, the same fixture
    /// as `ascii_tests::ut_oversized_adu_does_not_grow_buffer` above), the
    /// reader recovers on the next call rather than staying desynchronized
    /// (TR-R-044). Note this is a boundary failure, not a decode failure:
    /// `recv_adu` never decodes (TR-R-079), so a merely-malformed-but-
    /// delimited frame (e.g. odd hex count) would be returned successfully
    /// as raw bytes, not exercise this path — the failure has to come from
    /// the boundary rule itself.
    async fn ut_reader_recovers_after_self_locating_boundary_failure() {
        use crate::frame::{Ascii, UnitId};

        let (mut peer, server) = tokio::io::duplex(1024);
        let mut reader = AduReader::<_, Ascii>::new(server, Direction::Request);

        // ':' followed by MAX_ADU_LEN - 1 bytes that never contain CR LF: the
        // terminator is never found, so the boundary itself fails.
        let mut bytes = vec![b':'];
        bytes.extend(core::iter::repeat_n(b'0', Ascii::MAX_ADU_LEN - 1));
        tokio::io::AsyncWriteExt::write_all(&mut peer, &bytes)
            .await
            .expect("writes");
        assert_eq!(
            reader.recv_adu().await,
            Err(Error::AduTooLarge {
                len: Ascii::MAX_ADU_LEN,
                max: Ascii::MAX_ADU_LEN,
            })
        );

        let good = b":1103006B00037E\r\n";
        tokio::io::AsyncWriteExt::write_all(&mut peer, good)
            .await
            .expect("writes a good frame");
        let adu = reader.recv_adu().await.expect("recovers");
        assert_eq!(adu, good);
        let (header, _pdu) = Ascii::decode_request(&adu).expect("decodes");
        assert_eq!(header, UnitId(0x11));
    }

    #[tokio::test]
    /// TR-R-080 — after a boundary failure on a non-self-locating framing
    /// (RtuOverTcp, `ContentLength`), the failure is terminal: the next call
    /// fails too, matching `FrameTransport`'s
    /// `ut_rtu_over_tcp_failed_derivation_retains_the_attempt` counterpart
    /// (TR-R-046).
    async fn ut_reader_is_terminal_after_content_length_boundary_failure() {
        use crate::frame::RtuOverTcp;
        let (mut peer, server) = tokio::io::duplex(64);
        let mut reader = AduReader::<_, RtuOverTcp>::new(server, Direction::Request);

        // FC 8 (Diagnostics) has indeterminate length under RTU-over-stream
        // (FR-R-148): the derivation itself fails, not just the decode.
        tokio::io::AsyncWriteExt::write_all(&mut peer, &[0x11, 0x08, 0x00, 0x00])
            .await
            .expect("writes");
        assert!(
            reader.recv_adu().await.is_err(),
            "indeterminate length errors"
        );

        // A good frame afterward still fails: the earlier bytes were
        // retained, not discarded, so they still lead every later read.
        let good = crate::frame::RtuOverTcp::encode_request(
            &crate::frame::UnitId(0x11),
            &crate::frame::RequestPdu::ReadHoldingRegisters {
                address: crate::frame::Address(0x006B),
                quantity: crate::frame::Quantity(3),
            },
        )
        .expect("encodes");
        tokio::io::AsyncWriteExt::write_all(&mut peer, &good)
            .await
            .expect("writes");
        assert!(
            reader.recv_adu().await.is_err(),
            "terminal: the retained bytes from the failed attempt lead every later read"
        );
    }
}
