//! `RootStore::native()` against a controlled "platform" store (TR-E-047).
//!
//! The native loader reads `SSL_CERT_FILE`/`SSL_CERT_DIR` in place of the
//! platform store when either is set. Setting it is process-wide, so this file holds a single test and
//! runs as its own process: nothing else observes the variable.

#![cfg(feature = "tls")]

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rust_modbus::{
    Error, RootStore, ServerCertVerification, TcpConfig, TlsClientConfig, connect_tls,
    load_pem_cert_chain, load_pem_private_key,
};

fn fixture_path(name: &str) -> String {
    format!("{}/tests/fixtures/tls/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).expect("fixture file reads")
}

/// Accept one TLS connection presenting `server.crt` (issued by `ca.crt`) and
/// return the address to dial.
async fn serve_once() -> SocketAddr {
    let cert_chain = load_pem_cert_chain(&fixture("server.crt")).expect("parses");
    let key = load_pem_private_key(&fixture("server.key")).expect("parses");
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("TLS 1.2/1.3 are supported by the ring provider")
    .with_no_client_auth()
    .with_single_cert(cert_chain, key)
    .expect("a valid cert/key pair");
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .expect("binds");
    let addr = listener.local_addr().expect("reports its address");
    tokio::spawn(async move {
        let (stream, _peer) = listener.accept().await.expect("accepts");
        let _ = acceptor.accept(stream).await;
    });
    addr
}

async fn connect_with_native_roots() -> Result<(), Error> {
    let addr = serve_once().await;
    let config = TlsClientConfig {
        server_cert: ServerCertVerification::Verify(RootStore::native()),
        client_identity: None,
    };
    connect_tls(addr, TcpConfig::default(), config)
        .await
        .map(|_transport| ())
}

#[tokio::test]
/// TR-E-047 — native trust loading fails closed: a platform store that cannot
/// be found yields a store that rejects the handshake, and an unreadable
/// certificate among readable ones is skipped while the rest are trusted.
async fn it_native_root_store_fails_closed() {
    let scratch = std::env::temp_dir().join(format!("rust-modbus-tr-e-047-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("creates scratch dir");

    // No discoverable store: the configured file does not exist, and no
    // certificate directory is configured beside it.
    // SAFETY: this file's only test, so no other thread reads the environment.
    unsafe {
        std::env::remove_var("SSL_CERT_DIR");
        std::env::set_var("SSL_CERT_FILE", scratch.join("absent.pem"));
    }
    assert!(matches!(
        connect_with_native_roots().await,
        Err(Error::TlsHandshake { .. })
    ));

    // A store holding one certificate whose DER does not parse, then the CA.
    let mut bundle = b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n".to_vec();
    bundle.extend_from_slice(&fixture("ca.crt"));
    let bundle_path = scratch.join("bundle.pem");
    std::fs::write(&bundle_path, bundle).expect("writes bundle");
    // SAFETY: as above.
    unsafe {
        std::env::set_var("SSL_CERT_FILE", &bundle_path);
    }
    assert_eq!(connect_with_native_roots().await, Ok(()));

    std::fs::remove_dir_all(&scratch).expect("removes scratch dir");
}
