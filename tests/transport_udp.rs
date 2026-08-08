//! The UDP transport and its framing-generic primitives over real loopback
//! sockets (TR-R-070 … TR-R-074).
//!
//! Every socket binds port 0 and reads the assigned port back, per this
//! repo's testing convention.

use std::net::{Ipv4Addr, SocketAddr};

use rust_modbus::{
    Address, Framing, MbapHeader, Quantity, RegisterValue, RequestPdu, ResponsePdu, RtuOverTcp,
    Tcp, TransactionId, UdpConfig, UnitId, connect_udp, recv_datagram_request,
    send_datagram_response_into,
};
use tokio::net::UdpSocket;

/// An ephemeral loopback address: port 0, so the kernel assigns one.
fn ephemeral() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
}

#[tokio::test]
/// TR-R-072 — the framing-generic receive/send primitives work over `Tcp`
/// (MBAP) framing, the one `Server::serve_udp` uses.
async fn it_datagram_primitives_work_with_tcp_framing() {
    let socket = UdpSocket::bind(ephemeral()).await.expect("binds");
    let addr = socket.local_addr().expect("reports its address");
    let sender = UdpSocket::bind(ephemeral()).await.expect("binds");
    let sender_addr = sender.local_addr().expect("reports its address");

    let header = MbapHeader {
        transaction_id: TransactionId(3),
        unit_id: UnitId(9),
    };
    let request = RequestPdu::ReadHoldingRegisters {
        address: Address(1),
        quantity: Quantity(2),
    };
    let encoded = Tcp::encode_request(&header, &request).expect("encodes");
    sender.send_to(&encoded, addr).await.expect("sends");

    let mut buf = [0u8; 512];
    let (recv_header, recv_request, peer) = recv_datagram_request::<Tcp>(&socket, &mut buf)
        .await
        .expect("receives");
    assert_eq!(
        (recv_header, recv_request, peer),
        (header, request, sender_addr)
    );

    let response = ResponsePdu::ReadHoldingRegisters {
        registers: vec![RegisterValue(7), RegisterValue(8)],
    };
    let mut out = Vec::new();
    send_datagram_response_into::<Tcp>(&socket, peer, &header, &response, &mut out)
        .await
        .expect("sends");
    let mut reply = [0u8; 512];
    let n = sender.recv(&mut reply).await.expect("receives the reply");
    assert_eq!(
        Tcp::decode_response(reply.get(..n).expect("recv never exceeds the buffer")),
        Ok((header, response))
    );
}

#[tokio::test]
/// TR-R-072 — and over `RtuOverTcp` framing too, proving the primitives are
/// not secretly Tcp-specific: a whole datagram decodes the same way
/// regardless of which framing's boundary rule would apply to a stream.
async fn it_datagram_primitives_work_with_any_framing() {
    let socket = UdpSocket::bind(ephemeral()).await.expect("binds");
    let addr = socket.local_addr().expect("reports its address");
    let sender = UdpSocket::bind(ephemeral()).await.expect("binds");

    let header = UnitId(0x11);
    let request = RequestPdu::ReadHoldingRegisters {
        address: Address(0x006B),
        quantity: Quantity(3),
    };
    let encoded = RtuOverTcp::encode_request(&header, &request).expect("encodes");
    sender.send_to(&encoded, addr).await.expect("sends");

    let mut buf = [0u8; 512];
    let (recv_header, recv_request, _peer) = recv_datagram_request::<RtuOverTcp>(&socket, &mut buf)
        .await
        .expect("receives");
    assert_eq!((recv_header, recv_request), (header, request));
}

#[tokio::test]
/// TR-R-070 — one transport type serves both directions over UDP, the same
/// way `FrameTransport` does over a stream: a request crosses one datagram,
/// the response crosses the next.
async fn it_udp_transport_roundtrips_request_and_response() {
    let server_socket = UdpSocket::bind(ephemeral()).await.expect("binds");
    let server_addr = server_socket.local_addr().expect("reports its address");
    let mut client = connect_udp(server_addr, UdpConfig::default())
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

    let mut buf = [0u8; 512];
    let (recv_header, recv_request, peer) = recv_datagram_request::<Tcp>(&server_socket, &mut buf)
        .await
        .expect("receives");
    assert_eq!((recv_header, recv_request), (header, request));

    let response = ResponsePdu::ReadHoldingRegisters {
        registers: vec![RegisterValue(0x022B)],
    };
    let mut out = Vec::new();
    send_datagram_response_into::<Tcp>(&server_socket, peer, &recv_header, &response, &mut out)
        .await
        .expect("sends");
    assert_eq!(client.recv_response().await, Ok((header, response)));
}
