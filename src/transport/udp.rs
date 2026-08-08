//! A UDP transport carrying one MBAP-framed ADU per datagram (TR-R-070 …
//! TR-R-074).

use alloc::vec::Vec;
use core::marker::PhantomData;
use std::net::SocketAddr;

use tokio::net::UdpSocket;

use crate::error::Result;
use crate::frame::{Framing, RequestPdu, ResponsePdu};

/// How a UDP transport is set up (TR-R-071).
///
/// Carries no options today: unlike [`TcpConfig`](crate::transport::TcpConfig),
/// associating a UDP socket with a peer performs no handshake, so there is no
/// connect timeout to configure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UdpConfig {}

/// A transport over a UDP socket bound to one fixed peer for its lifetime,
/// carrying one ADU per datagram (TR-R-070).
///
/// Unlike [`FrameTransport`](crate::transport::FrameTransport), this performs
/// no partial-frame accumulation: the OS already delimits the datagram
/// boundary, so every send is one `send` call and every receive is one `recv`
/// call, each carrying exactly one whole ADU.
#[derive(Debug)]
pub struct UdpTransport<F> {
    socket: UdpSocket,
    /// The one buffer every outgoing ADU is encoded into, reused datagram
    /// after datagram (TR-R-043, TR-R-073) — same discipline as
    /// `FrameTransport::outgoing` (`src/transport/mod.rs`).
    outgoing: Vec<u8>,
    /// The one buffer every datagram is received into, sized to the
    /// framing's maximum once, up front.
    incoming: Vec<u8>,
    framing: PhantomData<F>,
}

impl<F: Framing> UdpTransport<F> {
    /// Wrap a UDP socket already associated with its one peer (TR-R-070).
    ///
    /// This type performs no connection of its own; the caller establishes
    /// the peer association (e.g. via [`UdpSocket::connect`]) first.
    pub fn new(socket: UdpSocket) -> Self {
        Self {
            socket,
            outgoing: Vec::new(),
            incoming: alloc::vec![0u8; F::MAX_ADU_LEN],
            framing: PhantomData,
        }
    }

    /// Recover the underlying socket.
    pub fn into_inner(self) -> UdpSocket {
        self.socket
    }

    /// Send a request as one datagram (TR-R-073).
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode, if the encoded ADU exceeds the
    /// framing's [`Framing::MAX_ADU_LEN`] (refused before any I/O), or if the
    /// socket does.
    pub async fn send_request(&mut self, header: &F::Header, pdu: &RequestPdu) -> Result<()> {
        self.outgoing.clear();
        F::encode_request_into(header, pdu, &mut self.outgoing)?;
        self.socket.send(&self.outgoing).await?;
        self.outgoing.clear();
        Ok(())
    }

    /// Send a response as one datagram (TR-R-073).
    ///
    /// # Errors
    ///
    /// Fails if the PDU does not encode, if the encoded ADU exceeds the
    /// framing's [`Framing::MAX_ADU_LEN`], or if the socket does.
    pub async fn send_response(&mut self, header: &F::Header, pdu: &ResponsePdu) -> Result<()> {
        self.outgoing.clear();
        F::encode_response_into(header, pdu, &mut self.outgoing)?;
        self.socket.send(&self.outgoing).await?;
        self.outgoing.clear();
        Ok(())
    }

    /// Receive one request from one datagram (TR-R-074).
    ///
    /// # Errors
    ///
    /// Fails if the socket does, or if the datagram does not decode. Either
    /// failure leaves this transport fully usable for the next receive —
    /// there is no shared boundary state a bad datagram could desynchronize
    /// (TR-R-074).
    pub async fn recv_request(&mut self) -> Result<(F::Header, RequestPdu)> {
        let n = self.socket.recv(&mut self.incoming).await?;
        let received = self
            .incoming
            .get(..n)
            .expect("recv never reports more bytes than the buffer holds");
        F::decode_request(received)
    }

    /// Receive one response from one datagram (TR-R-074).
    ///
    /// # Errors
    ///
    /// Fails if the socket does, or if the datagram does not decode.
    pub async fn recv_response(&mut self) -> Result<(F::Header, ResponsePdu)> {
        let n = self.socket.recv(&mut self.incoming).await?;
        let received = self
            .incoming
            .get(..n)
            .expect("recv never reports more bytes than the buffer holds");
        F::decode_response(received)
    }
}

/// Connect to a Modbus server over UDP (TR-R-071).
///
/// Binds an ephemeral local socket and associates it with `peer` via
/// [`UdpSocket::connect`]; this is a local operation with no network
/// handshake, so [`TcpConfig::connect_timeout`](crate::transport::TcpConfig)'s
/// counterpart does not exist here — `_config` is accepted for symmetry with
/// [`connect_tcp`](crate::transport::connect_tcp) and forward compatibility.
///
/// # Errors
///
/// Fails if the local socket cannot be bound or associated with `peer`.
pub async fn connect_udp(
    peer: SocketAddr,
    _config: UdpConfig,
) -> Result<UdpTransport<crate::frame::Tcp>> {
    let local: SocketAddr = if peer.is_ipv4() {
        (std::net::Ipv4Addr::UNSPECIFIED, 0).into()
    } else {
        (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let socket = UdpSocket::bind(local).await?;
    socket.connect(peer).await?;
    Ok(UdpTransport::new(socket))
}

/// Receive one request from any peer on an already-bound socket, for any
/// framing (TR-R-072).
///
/// The framing-generic counterpart to [`UdpTransport::recv_request`], for a
/// socket serving many peers rather than one — mirrors
/// [`connect_tcp_framed`](crate::transport::connect_tcp_framed)'s
/// framing-agnostic stance at the transport layer. `buf` is caller-owned and
/// reused across calls (TR-R-043); it must hold at least `F::MAX_ADU_LEN`
/// bytes for a maximum-size datagram to decode rather than truncate.
///
/// # Errors
///
/// Fails if the socket does, or if the datagram does not decode. Either
/// failure leaves the socket fully usable for the next receive (TR-R-074).
/// The source address is not reported on failure — only on a successful
/// decode.
pub async fn recv_datagram_request<F: Framing>(
    socket: &UdpSocket,
    buf: &mut [u8],
) -> Result<(F::Header, RequestPdu, SocketAddr)> {
    let (n, peer) = socket.recv_from(buf).await?;
    let received = buf
        .get(..n)
        .expect("recv_from never reports more bytes than the buffer holds");
    let (header, pdu) = F::decode_request(received)?;
    Ok((header, pdu, peer))
}

/// Send one response to one peer on an already-bound socket, for any framing
/// (TR-R-072).
///
/// The framing-generic counterpart to [`UdpTransport::send_response`]. `out`
/// is caller-owned and reused across calls (TR-R-043, TR-R-073): cleared here
/// before encoding, so its capacity survives the call.
///
/// # Errors
///
/// Fails if the PDU does not encode, if the encoded ADU exceeds
/// `F::MAX_ADU_LEN` (refused before `send_to` — see Shared in the plan), or if
/// the socket does.
pub async fn send_datagram_response_into<F: Framing>(
    socket: &UdpSocket,
    peer: SocketAddr,
    header: &F::Header,
    pdu: &ResponsePdu,
    out: &mut Vec<u8>,
) -> Result<()> {
    out.clear();
    F::encode_response_into(header, pdu, out)?;
    socket.send_to(out, peer).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    /// TR-R-071 — connecting to a peer with nothing listening still succeeds:
    /// unlike TCP (`it_connect_refused_is_distinct_from_a_timeout`,
    /// tests/transport_tcp.rs), UDP performs no handshake, so there is no
    /// refusal to observe at connect time.
    async fn ut_connect_udp_succeeds_with_nothing_listening() {
        let unused = SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0));
        // Bind and immediately drop, so the port is known free (per this
        // repo's ephemeral-port convention) yet nothing answers it.
        let holder = UdpSocket::bind(unused).await.expect("binds");
        let addr = holder.local_addr().expect("reports its address");
        drop(holder);
        connect_udp(addr, UdpConfig::default())
            .await
            .expect("connects with no peer listening");
    }

    #[tokio::test]
    /// TR-R-073 — the transport owns one outgoing buffer, reused
    /// datagram after datagram: cleared between sends, capacity retained once
    /// grown to the framing maximum.
    async fn ut_write_buffer_capacity_is_retained() {
        use crate::frame::{Address, Quantity, Tcp};
        use crate::{MbapHeader, RequestPdu, TransactionId, UnitId};

        let peer = UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("binds");
        let peer_addr = peer.local_addr().expect("reports its address");
        let mut client = connect_udp(peer_addr, UdpConfig::default())
            .await
            .expect("connects");

        let header = MbapHeader {
            transaction_id: TransactionId(1),
            unit_id: UnitId(0x11),
        };
        let request = RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        };

        client.send_request(&header, &request).await.expect("sends");
        let capacity = client.outgoing.capacity();
        assert!(
            capacity >= Tcp::MAX_ADU_LEN,
            "reserved {capacity} of {}",
            Tcp::MAX_ADU_LEN
        );
        assert_eq!(client.outgoing.len(), 0);

        client
            .send_request(&header, &request)
            .await
            .expect("sends again");
        assert_eq!(
            client.outgoing.capacity(),
            capacity,
            "the second send reallocated the buffer"
        );
    }

    #[tokio::test]
    /// TR-R-074 — a datagram that fails to decode surfaces as a typed error and
    /// costs nothing beyond itself: the next datagram, however malformed the
    /// first one was, still decodes normally.
    async fn ut_decode_failure_leaves_udp_transport_usable() {
        use crate::error::Error;
        use crate::frame::{Address, Quantity, Tcp};
        use crate::{MbapHeader, RequestPdu, TransactionId, UnitId};

        let server_socket = UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("binds");
        let server_addr = server_socket.local_addr().expect("reports its address");
        let peer_socket = UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("binds");
        peer_socket.connect(server_addr).await.expect("connects");
        let mut peer = UdpTransport::<Tcp>::new(peer_socket);
        let mut server_side = UdpTransport::<Tcp>::new(server_socket);

        // A well-formed MBAP prefix carrying function code 0, which is not a
        // request (FR-R-014).
        let garbage = [0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x11, 0x00];
        let socket = peer.into_inner();
        socket.send(&garbage).await.expect("sends garbage");
        peer = UdpTransport::new(socket);

        assert_eq!(
            server_side.recv_request().await,
            Err(Error::InvalidFunctionCode(0))
        );

        let header = MbapHeader {
            transaction_id: TransactionId(2),
            unit_id: UnitId(0x11),
        };
        let request = RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        };
        peer.send_request(&header, &request)
            .await
            .expect("sends a good request");
        assert_eq!(server_side.recv_request().await, Ok((header, request)));
    }
}
