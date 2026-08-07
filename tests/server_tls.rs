//! The TLS server over real sockets, answered by this crate's own client and
//! by a bare TLS connector (TR-R-063, TR-R-064, SV-R-030, SV-R-055, SV-R-056).
//!
//! Every listener binds port 0 and reads the assigned port back (NF-R-023).

#![cfg(feature = "tls")]

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_modbus::{
    Address, ClientCertPolicy, ClientIdentity, Connection, Error, MbapHeader, Quantity,
    RegisterValue, RequestPdu, ResponsePdu, RootStore, Server, ServerCertVerification, Service,
    TcpConfig, TlsClientConfig, TlsListener, TlsServerConfig, TransactionId, UnitId, connect_tls,
    load_pem_cert_chain, load_pem_private_key,
};

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

fn server_config(client_certs: ClientCertPolicy) -> TlsServerConfig {
    TlsServerConfig {
        cert_chain: load_pem_cert_chain(&fixture("server.crt")).expect("parses"),
        key: load_pem_private_key(&fixture("server.key")).expect("parses"),
        client_certs,
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

/// What a test service was asked, recorded in call order (SV-R-036).
#[derive(Debug, Clone, PartialEq)]
enum Event {
    Connect(SocketAddr),
    Request,
    HandshakeFailed(SocketAddr, Error),
}

#[derive(Debug, Clone, Default)]
struct Recorder {
    events: Arc<Mutex<Vec<Event>>>,
}

impl Recorder {
    fn events(&self) -> Vec<Event> {
        self.events
            .lock()
            .expect("no test poisons the lock")
            .clone()
    }
}

impl Service for Recorder {
    async fn on_request(
        &self,
        _conn: &Connection,
        _unit: UnitId,
        _request: RequestPdu,
    ) -> Result<ResponsePdu, rust_modbus::ExceptionCode> {
        self.events
            .lock()
            .expect("no test poisons the lock")
            .push(Event::Request);
        Ok(response())
    }

    async fn on_connect(&self, conn: &Connection) -> rust_modbus::Acceptance {
        self.events
            .lock()
            .expect("no test poisons the lock")
            .push(Event::Connect(conn.peer().expect("TLS always has a peer")));
        rust_modbus::Acceptance::Accept
    }

    async fn on_tls_handshake_failed(&self, peer: SocketAddr, error: &Error) {
        self.events
            .lock()
            .expect("no test poisons the lock")
            .push(Event::HandshakeFailed(peer, error.clone()));
    }
}

#[tokio::test]
/// TR-R-063 -- this crate's own TLS client against this crate's own
/// TLS server, full stack.
async fn it_tls_client_against_tls_server_read_holding_registers() {
    let listener = TlsListener::bind(ephemeral(), server_config(ClientCertPolicy::None))
        .await
        .expect("binds");
    let address = listener.local_addr().expect("reports its address");
    let service = Recorder::default();
    let server = Server::new(service.clone());
    let handle = server.handle();
    let serving = tokio::spawn(server.serve_tls::<rust_modbus::Tcp>(listener));

    let mut client = connect_tls(address, TcpConfig::default(), trusting_ca())
        .await
        .expect("connects and handshakes");
    client
        .send_request(&header(), &request())
        .await
        .expect("writes a request");
    assert_eq!(client.recv_response().await, Ok((header(), response())));

    handle.shutdown().await;
    assert_eq!(serving.await.expect("the task finishes"), Ok(()));
    assert!(matches!(service.events().first(), Some(Event::Connect(_))));
    assert!(service.events().contains(&Event::Request));
}

#[tokio::test]
/// SV-R-056, TR-R-069 -- a TLS handshake that fails (a required client cert
/// is missing) never establishes a `Connection`; the service is notified
/// through `on_tls_handshake_failed`, not `on_connect`, with
/// `peer_cert: None` (no client cert was offered).
async fn it_on_tls_handshake_failed_is_notified_with_no_connection_established() {
    let mut roots = RootStore::empty();
    roots.add_pem(&fixture("ca.crt")).expect("parses");
    let listener = TlsListener::bind(ephemeral(), server_config(ClientCertPolicy::Require(roots)))
        .await
        .expect("binds");
    let address = listener.local_addr().expect("reports its address");
    let service = Recorder::default();
    let server = Server::new(service.clone());
    let handle = server.handle();
    let serving = tokio::spawn(server.serve_tls::<rust_modbus::Tcp>(listener));

    // No client identity presented, though the server requires one. TLS 1.3
    // sends the server's Finished before it evaluates the client's certificate,
    // so the client's own handshake future may observe success even though the
    // server is about to tear the connection down -- the same either-side hedge
    // `ut_require_rejects_an_untrusted_client_cert` uses. What SV-R-056
    // guarantees is server-side: the service is notified, never on_connect.
    let _ = connect_tls(address, TcpConfig::default(), trusting_ca()).await;

    // Poll until the server side has recorded the failure -- the client's own
    // error only proves its side, not that the server notified the service.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while service.events().is_empty() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let events = service.events();
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::HandshakeFailed(
                _,
                Error::TlsHandshake {
                    peer_cert: None,
                    ..
                }
            )
        )),
        "the service must be notified of the failed handshake, with no cert offered: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Connect(_))),
        "a failed handshake must never reach on_connect: {events:?}"
    );

    handle.shutdown().await;
    let _ = serving.await.expect("the task finishes");
}

#[tokio::test]
/// TR-R-069 -- a client certificate offered and rejected under
/// `ClientCertPolicy::Require` (untrusted issuer) reaches
/// `on_tls_handshake_failed` as `Error::TlsHandshake.peer_cert`, `Some`
/// with the offered certificate.
async fn it_on_tls_handshake_failed_carries_the_rejected_client_cert() {
    let mut roots = RootStore::empty();
    roots.add_pem(&fixture("ca.crt")).expect("parses");
    let listener = TlsListener::bind(ephemeral(), server_config(ClientCertPolicy::Require(roots)))
        .await
        .expect("binds");
    let address = listener.local_addr().expect("reports its address");
    let service = Recorder::default();
    let server = Server::new(service.clone());
    let handle = server.handle();
    let serving = tokio::spawn(server.serve_tls::<rust_modbus::Tcp>(listener));

    let cert_chain = load_pem_cert_chain(&fixture("unrelated-client.crt")).expect("parses");
    let key = load_pem_private_key(&fixture("unrelated-client.key")).expect("parses");
    let offered = cert_chain.first().cloned();
    let mut config = trusting_ca();
    config.client_identity = Some(ClientIdentity { cert_chain, key });
    let _ = connect_tls(address, TcpConfig::default(), config).await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while service.events().is_empty() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let events = service.events();
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::HandshakeFailed(_, Error::TlsHandshake { peer_cert, .. })
                if *peer_cert == offered
        )),
        "peer_cert must carry the rejected client certificate: {events:?}"
    );

    handle.shutdown().await;
    let _ = serving.await.expect("the task finishes");
}

#[tokio::test]
/// TR-R-063 -- one connection stalled before sending its
/// `ClientHello` does not delay a second connection's handshake and exchange.
async fn it_tls_handshakes_do_not_serialize_across_connections() {
    let listener = TlsListener::bind(ephemeral(), server_config(ClientCertPolicy::None))
        .await
        .expect("binds");
    let address = listener.local_addr().expect("reports its address");
    let service = Recorder::default();
    let server = Server::new(service);
    let serving = tokio::spawn(server.serve_tls::<rust_modbus::Tcp>(listener));

    // Opens the TCP connection, then never speaks TLS: the server's handshake
    // task for this connection never completes, so this test never asks for a
    // graceful shutdown -- draining SV-R-044-style would wait for it forever.
    let _stalled = tokio::net::TcpStream::connect(address)
        .await
        .expect("connects at the TCP level");

    // A second, well-behaved connection must still complete promptly.
    let attempt = async {
        let mut client = connect_tls(address, TcpConfig::default(), trusting_ca())
            .await
            .expect("connects and handshakes");
        client
            .send_request(&header(), &request())
            .await
            .expect("writes a request");
        client.recv_response().await
    };
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), attempt)
            .await
            .expect("the second connection is not blocked by the stalled one"),
        Ok((header(), response()))
    );

    serving.abort();
}
