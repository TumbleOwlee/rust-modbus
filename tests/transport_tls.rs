//! TLS transport over a real loopback socket (TR-R-060 … TR-R-068).
//!
//! Every listener binds port 0 and reads the assigned port back (NF-R-023).

#![cfg(feature = "tls")]

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use rust_modbus::{
    Address, ClientCertPolicy, Error, MbapHeader, Quantity, RegisterValue, RequestPdu, ResponsePdu,
    RootStore, ServerCertVerification, TcpConfig, TlsClientConfig, TlsServerConfig, TransactionId,
    UnitId, connect_tls, load_pem_cert_chain, load_pem_private_key,
};

/// An ephemeral loopback address: port 0, so the kernel assigns one.
fn ephemeral() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/tls/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture file reads")
}

/// A bare `rustls::ServerConfig` presenting `server.crt`/`server.key`, no
/// client auth requested -- built directly (not via `TlsServerConfig`, which
/// does not exist until stage s3) to test the connector in isolation.
fn plain_server_config() -> rustls::ServerConfig {
    let cert_chain = load_pem_cert_chain(&fixture("server.crt")).expect("parses");
    let key = load_pem_private_key(&fixture("server.key")).expect("parses");
    rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("TLS 1.2/1.3 are supported by the ring provider")
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .expect("a valid cert/key pair")
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
        quantity: Quantity(1),
    }
}

fn response() -> ResponsePdu {
    ResponsePdu::ReadHoldingRegisters {
        registers: vec![RegisterValue(0x022B)],
    }
}

fn trusting_ca() -> TlsClientConfig {
    let mut roots = RootStore::empty();
    roots.add_pem(&fixture("ca.crt")).expect("parses");
    TlsClientConfig {
        server_cert: ServerCertVerification::Verify(roots),
        client_identity: None,
    }
}

#[tokio::test]
/// TR-R-062, TR-R-064 — `connect_tls` performs a TCP connect then a TLS
/// handshake and yields a `FrameTransport` that exchanges an ADU.
async fn it_connect_tls_handshakes_then_yields_a_frame_transport() {
    let listener = tokio::net::TcpListener::bind(ephemeral())
        .await
        .expect("binds");
    let addr = listener.local_addr().expect("reports its address");
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(plain_server_config()));

    let serving = tokio::spawn(async move {
        let (stream, _peer) = listener.accept().await.expect("accepts");
        let stream = acceptor.accept(stream).await.expect("handshakes");
        let mut transport = rust_modbus::FrameTransport::<_, rust_modbus::Tcp>::new(stream);
        let (received_header, received_request) =
            transport.recv_request().await.expect("reads a request");
        assert_eq!(received_header, header());
        assert_eq!(received_request, request());
        transport
            .send_response(&received_header, &response())
            .await
            .expect("writes a response");
    });

    let mut client = connect_tls(addr, TcpConfig::default(), trusting_ca())
        .await
        .expect("connects and handshakes");
    client
        .send_request(&header(), &request())
        .await
        .expect("writes a request");
    assert_eq!(client.recv_response().await, Ok((header(), response())));

    serving.await.expect("the server task finishes");
}

#[tokio::test]
/// TR-R-062, TR-R-067 -- a refused TCP connection surfaces as `Error::Io`,
/// distinct from a handshake failure.
async fn it_connect_tls_tcp_refused_is_distinct_from_handshake_failure() {
    let listener = tokio::net::TcpListener::bind(ephemeral())
        .await
        .expect("binds");
    let addr = listener.local_addr().expect("reports its address");
    drop(listener);

    assert_eq!(
        connect_tls(addr, TcpConfig::default(), trusting_ca())
            .await
            .err(),
        Some(Error::Io {
            kind: std::io::ErrorKind::ConnectionRefused,
        })
    );
}

#[tokio::test(start_paused = true)]
/// TR-R-021, TR-R-067 -- a connect timeout bounds the TCP connect and the TLS
/// handshake as one operation: a peer that completes the TCP accept but never
/// sends its `ServerHello` still times out as `Error::Timeout`, not
/// `Error::TlsHandshake`.
async fn it_connect_tls_timeout_covers_the_whole_handshake() {
    let listener = tokio::net::TcpListener::bind(ephemeral())
        .await
        .expect("binds");
    let addr = listener.local_addr().expect("reports its address");

    let stalling = tokio::spawn(async move {
        let (stream, _peer) = listener.accept().await.expect("accepts");
        // Accepted at the TCP level, then never sends a byte.
        core::future::pending::<()>().await;
        drop(stream);
    });

    let config = TcpConfig {
        connect_timeout: Duration::from_millis(50),
        ..TcpConfig::default()
    };
    assert_eq!(
        connect_tls(addr, config, trusting_ca()).await.err(),
        Some(Error::Timeout { what: "connect" })
    );
    stalling.abort();
}

fn server_config() -> TlsServerConfig {
    TlsServerConfig {
        cert_chain: load_pem_cert_chain(&fixture("server.crt")).expect("parses"),
        key: load_pem_private_key(&fixture("server.key")).expect("parses"),
        client_certs: ClientCertPolicy::None,
    }
}

#[tokio::test]
/// TR-R-063 — `TlsListener` accepts a connection and yields a
/// `FrameTransport` that exchanges an ADU.
async fn it_tls_listener_accepts_and_yields_a_frame_transport() {
    let listener = rust_modbus::TlsListener::bind(ephemeral(), server_config())
        .await
        .expect("binds");
    let addr = listener.local_addr().expect("reports its address");

    let serving = tokio::spawn(async move {
        let (mut transport, _peer, cert) = listener.accept().await.expect("accepts");
        assert_eq!(cert, None, "ClientCertPolicy::None requests no client cert");
        let (received_header, received_request) =
            transport.recv_request().await.expect("reads a request");
        transport
            .send_response(&received_header, &response())
            .await
            .expect("writes a response");
        (received_header, received_request)
    });

    let mut client = connect_tls(addr, TcpConfig::default(), trusting_ca())
        .await
        .expect("connects and handshakes");
    client
        .send_request(&header(), &request())
        .await
        .expect("writes a request");
    assert_eq!(client.recv_response().await, Ok((header(), response())));

    assert_eq!(
        serving.await.expect("the server task finishes"),
        (header(), request())
    );
}
