//! TCP transport over a real loopback socket (TR-R-002, TR-R-020 … TR-R-023,
//! TR-R-042).
//!
//! Every listener binds port 0 and reads the assigned port back, so nothing
//! here collides with another test run holding a fixed port.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use rust_modbus::{
    Address, Error, ExceptionCode, ExceptionResponse, FunctionCode, MbapHeader, Quantity,
    RegisterValue, RequestPdu, ResponsePdu, TcpConfig, TcpListener, TransactionId, UnitId,
    connect_tcp,
};

/// An ephemeral loopback address: port 0, so the kernel assigns one.
fn ephemeral() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
}

fn header() -> MbapHeader {
    MbapHeader {
        transaction_id: TransactionId(7),
        unit_id: UnitId(0x11),
    }
}

fn request() -> RequestPdu {
    RequestPdu::ReadHoldingRegisters {
        address: Address(0x006B),
        quantity: Quantity(3),
    }
}

fn response() -> ResponsePdu {
    ResponsePdu::ReadHoldingRegisters {
        registers: vec![
            RegisterValue(0x022B),
            RegisterValue(0x0000),
            RegisterValue(0x0064),
        ],
    }
}

#[tokio::test]
/// TR-R-002, TR-R-020, TR-R-023 — one transport type serves both roles: the
/// client sends a request and receives a response, the server does the reverse,
/// over the same type.
async fn it_client_and_server_roles_share_one_transport() {
    let listener = TcpListener::bind(ephemeral()).await.expect("binds");
    let addr = listener.local_addr().expect("reports its address");

    let server = tokio::spawn(async move {
        let (mut transport, _peer) = listener.accept().await.expect("accepts");
        let (header, pdu) = transport.recv_request().await.expect("receives a request");
        assert_eq!(pdu, request());
        transport
            .send_response(&header, &response())
            .await
            .expect("sends a response");
    });

    let mut client = connect_tcp(addr, TcpConfig::default())
        .await
        .expect("connects");
    client
        .send_request(&header(), &request())
        .await
        .expect("sends a request");
    assert_eq!(client.recv_response().await, Ok((header(), response())));
    server.await.expect("server completes");
}

#[tokio::test]
/// TR-R-021 — a refused connection is an I/O error carrying the kind the
/// platform reported, deliberately distinct from a connect timeout.
async fn it_connect_refused_is_distinct_from_a_timeout() {
    // Bind, read the port back, then drop the listener so nothing is listening
    // on a port that is nonetheless known to have been free.
    let listener = TcpListener::bind(ephemeral()).await.expect("binds");
    let addr = listener.local_addr().expect("reports its address");
    drop(listener);

    assert_eq!(
        connect_tcp(addr, TcpConfig::default()).await.err(),
        Some(Error::Io {
            kind: std::io::ErrorKind::ConnectionRefused,
        })
    );
}

#[tokio::test]
/// TR-R-022 — Nagle is disabled by default, and the default is overridable.
async fn it_nodelay_is_on_by_default_and_overridable() {
    let listener = TcpListener::bind(ephemeral()).await.expect("binds");
    let addr = listener.local_addr().expect("reports its address");
    let accepting = tokio::spawn(async move { listener.accept().await.expect("accepts") });

    let client = connect_tcp(addr, TcpConfig::default())
        .await
        .expect("connects");
    assert!(client.into_inner().nodelay().expect("reads the option"));

    let (_accepted, _peer) = accepting.await.expect("accept completes");

    let listener = TcpListener::bind(ephemeral()).await.expect("binds");
    let addr = listener.local_addr().expect("reports its address");
    let accepting = tokio::spawn(async move { listener.accept().await.expect("accepts") });

    let client = connect_tcp(
        addr,
        TcpConfig {
            nodelay: false,
            ..TcpConfig::default()
        },
    )
    .await
    .expect("connects");
    assert!(!client.into_inner().nodelay().expect("reads the option"));
    let (_accepted, _peer) = accepting.await.expect("accept completes");
}

#[tokio::test]
/// TR-R-042 — the transport imposes no response timeout of its own: a server
/// that takes its time still gets its response delivered. Per-request timing
/// belongs to the client area.
async fn it_transport_does_not_time_out_a_slow_response() {
    let listener = TcpListener::bind(ephemeral()).await.expect("binds");
    let addr = listener.local_addr().expect("reports its address");

    let server = tokio::spawn(async move {
        let (mut transport, _peer) = listener.accept().await.expect("accepts");
        let (header, _pdu) = transport.recv_request().await.expect("receives a request");
        // Far longer than any inter-frame interval, and longer than a
        // request/response exchange would normally take.
        tokio::time::sleep(Duration::from_millis(250)).await;
        transport
            .send_response(&header, &response())
            .await
            .expect("sends a response");
    });

    let mut client = connect_tcp(addr, TcpConfig::default())
        .await
        .expect("connects");
    client
        .send_request(&header(), &request())
        .await
        .expect("sends a request");
    assert_eq!(client.recv_response().await, Ok((header(), response())));
    server.await.expect("server completes");
}

#[tokio::test]
/// TR-R-002 — the exception direction crosses the same transport: a server's
/// exception response is an ordinary response as far as framing is concerned.
async fn it_exception_responses_cross_the_transport() {
    let listener = TcpListener::bind(ephemeral()).await.expect("binds");
    let addr = listener.local_addr().expect("reports its address");

    let exception = ResponsePdu::Exception(ExceptionResponse {
        function: FunctionCode::ReadHoldingRegisters,
        exception: ExceptionCode::IllegalDataAddress,
    });
    let expected = exception.clone();

    let server = tokio::spawn(async move {
        let (mut transport, _peer) = listener.accept().await.expect("accepts");
        let (header, _pdu) = transport.recv_request().await.expect("receives a request");
        transport
            .send_response(&header, &exception)
            .await
            .expect("sends an exception");
    });

    let mut client = connect_tcp(addr, TcpConfig::default())
        .await
        .expect("connects");
    client
        .send_request(&header(), &request())
        .await
        .expect("sends a request");
    assert_eq!(client.recv_response().await, Ok((header(), expected)));
    server.await.expect("server completes");
}
