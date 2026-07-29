//! Byte-level transports: sockets, serial ports, and ADU boundaries
//! (`TR-R-*`).
//!
//! Role-agnostic: a client and a server use the same types, differing only in
//! which direction they send and which they receive (TR-R-002).

#[cfg(feature = "rtu")]
mod rtu;
mod serial;
mod tcp;

use core::marker::PhantomData;
use core::time::Duration;

use alloc::vec::Vec;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{Error, Result};
use crate::frame::{AduBoundary, Framing, RequestPdu, ResponsePdu};

pub use serial::{DataBits, FlowControl, Parity, SerialConfig, StopBits};
pub use tcp::{TcpConfig, TcpListener, TcpTransport, connect_tcp};

#[cfg(feature = "rtu")]
pub use rtu::{SerialTransport, open_serial};

/// What boundary detection needs that the framing itself cannot supply
/// (TR-R-011).
///
/// Only RTU consults it: TCP and ASCII ADUs are self-delimiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportConfig {
    /// Silence that ends an RTU frame.
    pub inter_frame_interval: Duration,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            // The interval implied by the default serial line, 19200 8E1.
            inter_frame_interval: Duration::from_nanos(2_005_208),
        }
    }
}

impl TransportConfig {
    /// Derive the inter-frame interval from a serial line's parameters
    /// (TR-R-011).
    ///
    /// # Errors
    ///
    /// Fails if the configuration implies no character time.
    pub fn from_serial(config: &SerialConfig) -> Result<Self> {
        Ok(Self {
            inter_frame_interval: config.inter_frame_interval()?,
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
        self.send(&F::encode_request(header, pdu)?).await
    }

    /// Send a response (TR-R-003).
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode, or if the stream does.
    pub async fn send_response(&mut self, header: &F::Header, pdu: &ResponsePdu) -> Result<()> {
        self.send(&F::encode_response(header, pdu)?).await
    }

    /// Receive one request (TR-R-004).
    ///
    /// # Errors
    ///
    /// Fails if the stream does, if the peer disappears mid-ADU, or if the ADU
    /// does not decode.
    pub async fn recv_request(&mut self) -> Result<(F::Header, RequestPdu)> {
        let adu = self.recv_adu().await?;
        F::decode_request(&adu)
    }

    /// Receive one response (TR-R-004).
    ///
    /// # Errors
    ///
    /// Fails if the stream does, if the peer disappears mid-ADU, or if the ADU
    /// does not decode.
    pub async fn recv_response(&mut self) -> Result<(F::Header, ResponsePdu)> {
        let adu = self.recv_adu().await?;
        F::decode_response(&adu)
    }

    /// Write every byte of an ADU (TR-R-003).
    async fn send(&mut self, adu: &[u8]) -> Result<()> {
        self.stream.write_all(adu).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Read exactly one ADU, leaving any surplus buffered (TR-R-004).
    ///
    /// The ADU's bytes leave the buffer before it is decoded, so a decode
    /// failure costs exactly that frame and no more (TR-R-005).
    async fn recv_adu(&mut self) -> Result<Vec<u8>> {
        if self.receiving {
            // A previous receive was abandoned part-way through an ADU, so the
            // buffer may hold a fragment of one (TR-R-041).
            return Err(Error::Timeout { what: "receive" });
        }
        self.receiving = true;
        let result = self.read_adu().await;
        self.receiving = false;
        result
    }

    /// Apply this framing's boundary rule until one ADU is in hand (FR-R-122).
    async fn read_adu(&mut self) -> Result<Vec<u8>> {
        match F::boundary() {
            AduBoundary::Prefixed { prefix, total } => self.read_prefixed(prefix, total).await,
            AduBoundary::Delimited { start, end } => self.read_delimited(start, end).await,
            AduBoundary::Silence => self.read_until_silence().await,
        }
    }

    /// A length-prefixed ADU: read enough to compute the length, validate it,
    /// then read the rest (TR-R-010).
    async fn read_prefixed(
        &mut self,
        prefix: usize,
        total: fn(&[u8]) -> Result<usize>,
    ) -> Result<Vec<u8>> {
        self.fill_to(prefix).await?;
        let head = self
            .buffer
            .get(..prefix)
            .expect("fill_to returned, so the buffer holds at least prefix bytes");
        // The length is validated before it sizes anything (TR-R-010).
        let len = total(head)?;
        self.fill_to(len).await?;
        Ok(self.take(len))
    }

    /// A delimited ADU: discard anything before the start byte, then read to
    /// the terminator (TR-R-012).
    async fn read_delimited(&mut self, start: u8, end: &'static [u8]) -> Result<Vec<u8>> {
        let mut searched = 0;
        loop {
            if let Some(offset) = self.buffer.iter().position(|byte| *byte == start) {
                if offset > 0 {
                    // Bytes before a start byte belong to no ADU (TR-R-012).
                    self.buffer.drain(..offset);
                    searched = 0;
                }
                if let Some(at) = find(&self.buffer, end, searched) {
                    return Ok(self.take(at.saturating_add(end.len())));
                }
                // All but a possible partial terminator has been searched.
                searched = self
                    .buffer
                    .len()
                    .saturating_sub(end.len().saturating_sub(1));
            } else {
                // No start byte in sight; none of these bytes can begin an ADU.
                self.buffer.clear();
                searched = 0;
            }
            self.check_buffer_bound()?;
            self.read_more(!self.buffer.is_empty()).await?;
        }
    }

    /// An RTU ADU: whatever arrives before the line falls silent (TR-R-011).
    async fn read_until_silence(&mut self) -> Result<Vec<u8>> {
        loop {
            if self.buffer.is_empty() {
                // Nothing yet: wait rather than call the silence before the
                // frame has begun (TR-R-042).
                self.read_more(false).await?;
                continue;
            }
            self.check_buffer_bound()?;
            match tokio::time::timeout(self.config.inter_frame_interval, self.read_more(true)).await
            {
                // The line went quiet: the frame ends here.
                Err(_elapsed) => return Ok(self.take(self.buffer.len())),
                Ok(Ok(())) => {}
                // A close after a complete frame ends it just the same.
                Ok(Err(Error::ConnectionClosed)) => return Ok(self.take(self.buffer.len())),
                Ok(Err(error)) => return Err(error),
            }
        }
    }

    /// Read until the buffer holds at least `len` bytes (TR-R-013).
    async fn fill_to(&mut self, len: usize) -> Result<()> {
        if len > F::MAX_ADU_LEN {
            return Err(Error::AduTooLarge {
                len,
                max: F::MAX_ADU_LEN,
            });
        }
        while self.buffer.len() < len {
            self.read_more(!self.buffer.is_empty()).await?;
        }
        Ok(())
    }

    /// Refuse to buffer more than one ADU's worth for one ADU (TR-R-013).
    fn check_buffer_bound(&self) -> Result<()> {
        if self.buffer.len() >= F::MAX_ADU_LEN {
            return Err(Error::AduTooLarge {
                len: self.buffer.len(),
                max: F::MAX_ADU_LEN,
            });
        }
        Ok(())
    }

    /// Read once into the buffer.
    ///
    /// `mid_adu` distinguishes the two ways a stream can end (TR-R-014): a
    /// close between ADUs ends the stream, one inside an ADU severs a frame.
    async fn read_more(&mut self, mid_adu: bool) -> Result<()> {
        let mut chunk = [0u8; READ_CHUNK];
        let read = self.stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(if mid_adu {
                Error::ConnectionClosed
            } else {
                Error::Io {
                    kind: std::io::ErrorKind::UnexpectedEof,
                }
            });
        }
        self.buffer.extend_from_slice(
            chunk
                .get(..read)
                .expect("a read never reports more bytes than the chunk holds"),
        );
        Ok(())
    }

    /// Remove and return the first `len` buffered bytes, keeping the rest for
    /// the next call (TR-R-004).
    fn take(&mut self, len: usize) -> Vec<u8> {
        self.buffer.drain(..len).collect()
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
    use crate::error::Error;
    use crate::frame::{Address, Quantity, TransactionId, UnitId};
    use crate::frame::{MbapHeader, Tcp};
    use alloc::vec;
    use tokio::io::{AsyncWriteExt, duplex};

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
