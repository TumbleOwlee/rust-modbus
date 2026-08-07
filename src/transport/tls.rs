//! TLS transport over TCP, behind the `tls` feature (`TR-R-060` … `TR-R-068`).

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::net::SocketAddr;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::error::{Error, Result};
use crate::frame::{Framing, Tcp};
use crate::transport::{FrameTransport, TcpConfig};

/// The IANA-registered port for Modbus over TLS (TR-R-068).
///
/// Documentation constant only: no API in this crate applies it implicitly —
/// [`connect_tls`]/the TLS listener each take an explicit `SocketAddr`, same
/// as their plain-TCP counterparts.
pub const MODBUS_TLS_PORT: u16 = 802;

/// Recovers the `rustls::Error` a handshake failure's `io::Error` wraps —
/// tokio-rustls maps every handshake failure through
/// `io::Error::new(io::ErrorKind::InvalidData, rustls_error)`. A bare I/O
/// failure carrying no such source (e.g. an EOF before any TLS byte is sent)
/// falls back to `rustls::Error::General`, carrying the `io::Error`'s own
/// message, so `source` is never fabricated as the wrong variant, only as a
/// text fallback (TR-R-067).
fn tls_error_source(io_error: std::io::Error) -> rustls::Error {
    let message = io_error.to_string();
    io_error
        .into_inner()
        .and_then(|inner| inner.downcast::<rustls::Error>().ok())
        .map_or_else(|| rustls::Error::General(message), |boxed| *boxed)
}

/// The set of trusted root certificates a [`ServerCertVerification::Verify`]
/// or [`ClientCertPolicy::Require`] checks against (TR-R-065, TR-R-066).
#[derive(Debug, Clone)]
pub struct RootStore(pub(crate) RootCertStore);

impl RootStore {
    /// A store trusting nothing; every certificate checked against it fails.
    #[must_use]
    pub fn empty() -> Self {
        Self(RootCertStore::empty())
    }

    /// Load the platform's native trust roots (TR-R-065).
    ///
    /// Individual unreadable certs are skipped (upstream guidance); a
    /// platform with no discoverable store yields an empty store, which fails
    /// every verification closed rather than open.
    #[must_use]
    pub fn native() -> Self {
        let mut store = RootCertStore::empty();
        let found = rustls_native_certs::load_native_certs();
        let (_, _) = store.add_parsable_certificates(found.certs);
        Self(store)
    }

    /// Add every certificate found in a PEM document to the store.
    ///
    /// # Errors
    ///
    /// Fails if a certificate cannot be parsed or trusted.
    pub fn add_pem(&mut self, pem: &[u8]) -> Result<()> {
        for cert in load_pem_cert_chain(pem)? {
            self.0
                .add(cert)
                .map_err(|_error| Error::Configuration { field: "pem" })?;
        }
        Ok(())
    }
}

impl Default for RootStore {
    fn default() -> Self {
        Self::native()
    }
}

/// How a TLS client verifies the server's certificate (TR-R-065).
///
/// No boolean/`Option` spelling reaches "skip verification" silently: the
/// bypass is its own explicitly-named variant.
#[derive(Debug, Clone)]
pub enum ServerCertVerification {
    /// Verify the server's certificate against a trusted root store.
    Verify(RootStore),
    /// Accept any server certificate, verifying nothing. Named so its use is
    /// unmistakable in a diff or a config dump.
    DangerousDisableVerification,
}

impl Default for ServerCertVerification {
    fn default() -> Self {
        Self::Verify(RootStore::default())
    }
}

/// A client's own certificate and key, presented during a TLS handshake that
/// requests client authentication (TR-R-065).
///
/// Not `Clone`: [`PrivateKeyDer`] carries no `Clone` impl (it zeroizes its
/// bytes on drop), so neither does anything that owns one.
#[derive(Debug)]
pub struct ClientIdentity {
    /// The client's certificate chain, leaf first.
    pub cert_chain: Vec<CertificateDer<'static>>,
    /// The private key matching the leaf certificate.
    pub key: PrivateKeyDer<'static>,
}

/// How a TLS client connects (TR-R-062, TR-R-065).
#[derive(Debug, Default)]
pub struct TlsClientConfig {
    /// How the server's certificate is verified.
    pub server_cert: ServerCertVerification,
    /// The client's own identity, presented if the server requests one.
    pub client_identity: Option<ClientIdentity>,
}

/// Parse every certificate in a PEM document.
///
/// # Errors
///
/// Fails if the document cannot be read as PEM.
pub fn load_pem_cert_chain(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>> {
    use rustls::pki_types::pem::PemObject;

    CertificateDer::pem_slice_iter(pem)
        .collect::<core::result::Result<Vec<_>, _>>()
        .map_err(|_error| Error::Configuration { field: "pem" })
}

/// Parse the first private key in a PEM document.
///
/// # Errors
///
/// Fails if the document cannot be read as PEM, or carries no private key.
pub fn load_pem_private_key(pem: &[u8]) -> Result<PrivateKeyDer<'static>> {
    use rustls::pki_types::pem::PemObject;

    PrivateKeyDer::from_pem_slice(pem).map_err(|_error| Error::Configuration { field: "pem" })
}

/// The crypto provider every `ClientConfig`/`ServerConfig` in this module is
/// built with (TR-R-061).
///
/// Never installed as the process-global default
/// (`CryptoProvider::install_default`, a panic-on-double-call singleton) —
/// built per-config via `builder_with_provider`, so embedding this crate
/// alongside a consumer's own rustls usage cannot collide.
fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn client_config(config: TlsClientConfig) -> core::result::Result<ClientConfig, Error> {
    let builder = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|error| Error::TlsHandshake {
            source: error,
            peer_cert: None,
        })?;
    let builder = match config.server_cert {
        ServerCertVerification::Verify(store) => builder.with_root_certificates(store.0),
        ServerCertVerification::DangerousDisableVerification => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoServerCertVerification::new())),
    };
    match config.client_identity {
        Some(identity) => builder
            .with_client_auth_cert(identity.cert_chain, identity.key)
            .map_err(|error| Error::TlsHandshake {
                source: error,
                peer_cert: None,
            }),
        None => Ok(builder.with_no_client_auth()),
    }
}

/// A verifier that accepts any server certificate, verifying nothing
/// (TR-R-065's `DangerousDisableVerification`).
#[derive(Debug)]
struct NoServerCertVerification {
    provider: Arc<CryptoProvider>,
}

impl NoServerCertVerification {
    fn new() -> Self {
        Self {
            provider: provider(),
        }
    }
}

impl rustls::client::danger::ServerCertVerifier for NoServerCertVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> core::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> core::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> core::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// A TLS transport over TCP, framed as Modbus TCP.
pub type TlsClientTransport = FrameTransport<tokio_rustls::client::TlsStream<TcpStream>, Tcp>;

/// Connect to a Modbus TLS server (TR-R-062).
///
/// # Errors
///
/// Fails if the TCP connection is refused, if the network reports any other
/// error, if the TLS handshake fails, or if the connect timeout — which
/// bounds the TCP connect and the TLS handshake together as one operation —
/// expires first.
pub async fn connect_tls(
    addr: SocketAddr,
    tcp: TcpConfig,
    tls: TlsClientConfig,
) -> Result<TlsClientTransport> {
    connect_tls_framed::<Tcp>(addr, tcp, tls).await
}

/// Connect to a Modbus server over TLS, for any framing (TR-R-024, TR-R-062).
///
/// # Errors
///
/// Fails if the TCP connection is refused, if the network reports any other
/// error, if the TLS handshake fails, or if the connect timeout expires
/// first.
pub async fn connect_tls_framed<F: Framing>(
    addr: SocketAddr,
    tcp: TcpConfig,
    tls: TlsClientConfig,
) -> Result<FrameTransport<tokio_rustls::client::TlsStream<TcpStream>, F>> {
    let config = client_config(tls)?;
    let connector = TlsConnector::from(Arc::new(config));
    let server_name = ServerName::from(addr.ip());
    // The whole operation -- TCP connect *and* TLS handshake -- is inside one
    // timeout (TR-R-021): no second timeout knob. Unlike `with_connect_timeout`
    // (`transport::tcp`, typed for a plain `io::Result<TcpStream>`), the
    // attempt here already produces the crate's own `Result`, so a handshake
    // failure surfaces natively as `Error::TlsHandshake` rather than needing
    // translation out of an `io::Error`, keeping it distinct from the
    // `Error::Timeout` a timed-out `attempt` produces below (TR-R-067).
    let attempt = async {
        let tcp_stream = TcpStream::connect(addr).await?;
        tcp_stream.set_nodelay(tcp.nodelay)?;
        connector
            .connect(server_name, tcp_stream)
            .await
            .map_err(|io_error| Error::TlsHandshake {
                source: tls_error_source(io_error),
                peer_cert: None, // TR-R-069: client-side handshake failure
            })
    };
    match tokio::time::timeout(tcp.connect_timeout, attempt).await {
        Ok(result) => Ok(FrameTransport::new(result?)),
        // Nothing failed; the wait ran out (TR-R-021).
        Err(_elapsed) => Err(Error::Timeout { what: "connect" }),
    }
}

/// Whether a TLS server requests a client certificate (TR-R-066).
#[derive(Debug, Clone)]
pub enum ClientCertPolicy {
    /// Require a client certificate, verified against a trusted root store.
    /// A handshake presenting none, or one the store does not trust, fails.
    Require(RootStore),
    /// Encryption only; no client certificate is requested.
    None,
}

/// How a TLS server accepts connections (TR-R-063, TR-R-066).
#[derive(Debug)]
pub struct TlsServerConfig {
    /// The server's certificate chain, leaf first.
    pub cert_chain: Vec<CertificateDer<'static>>,
    /// The private key matching the leaf certificate.
    pub key: PrivateKeyDer<'static>,
    /// Whether a client certificate is requested, and how it is verified.
    pub client_certs: ClientCertPolicy,
}

fn server_config(config: TlsServerConfig) -> core::result::Result<rustls::ServerConfig, Error> {
    let builder = rustls::ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|error| Error::TlsHandshake {
            source: error,
            peer_cert: None,
        })?;
    let builder = match config.client_certs {
        ClientCertPolicy::Require(store) => {
            let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
                Arc::new(store.0),
                provider(),
            )
            .build()
            .map_err(|error| Error::TlsHandshake {
                source: rustls::Error::General(error.to_string()),
                peer_cert: None,
            })?;
            builder.with_client_cert_verifier(verifier)
        }
        ClientCertPolicy::None => builder.with_no_client_auth(),
    };
    builder
        .with_single_cert(config.cert_chain, config.key)
        .map_err(|error| Error::TlsHandshake {
            source: error,
            peer_cert: None,
        })
}

/// A TLS listener over TCP, wrapping a bound [`TcpListener`](crate::transport::TcpListener)
/// (TR-R-063).
pub struct TlsListener {
    inner: crate::transport::TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
}

impl TlsListener {
    /// Bind a listening socket and load the TLS identity it will present
    /// (TR-R-063).
    ///
    /// # Errors
    ///
    /// Fails if the address cannot be bound, or if the certificate/key/root
    /// store do not build into a valid server configuration.
    pub async fn bind(addr: SocketAddr, config: TlsServerConfig) -> Result<Self> {
        let config = server_config(config)?;
        Ok(Self {
            inner: crate::transport::TcpListener::bind(addr).await?,
            acceptor: tokio_rustls::TlsAcceptor::from(Arc::new(config)),
        })
    }

    /// The address actually bound.
    ///
    /// # Errors
    ///
    /// Fails if the socket cannot report its address.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.inner.local_addr()
    }

    /// Accept one connection (TR-R-063).
    ///
    /// # Errors
    ///
    /// Fails if the accept or the TLS handshake does.
    pub async fn accept(
        &self,
    ) -> Result<(
        FrameTransport<tokio_rustls::server::TlsStream<TcpStream>, Tcp>,
        SocketAddr,
        Option<CertificateDer<'static>>,
    )> {
        self.accept_framed::<Tcp>().await
    }

    /// Accept one connection, for any framing (TR-R-024, TR-R-063).
    ///
    /// # Errors
    ///
    /// Fails if the accept or the TLS handshake does.
    pub async fn accept_framed<F: Framing>(
        &self,
    ) -> Result<(
        FrameTransport<tokio_rustls::server::TlsStream<TcpStream>, F>,
        SocketAddr,
        Option<CertificateDer<'static>>,
    )> {
        let (stream, peer) = self.inner.accept_tcp_only().await?;
        let (transport, cert) = self.handshake_framed::<F>(stream).await?;
        Ok((transport, peer, cert))
    }

    /// Accept a TCP connection with no TLS handshake performed yet (SV-R-030,
    /// via `Server::serve_tls`, which needs to accept before spawning the
    /// per-connection task the handshake itself runs in -- a stalled or
    /// hostile `ClientHello` then blocks only that task, not the next accept).
    pub(crate) async fn accept_tcp_only(&self) -> Result<(TcpStream, SocketAddr)> {
        self.inner.accept_tcp_only().await
    }

    /// Run the TLS handshake on an already-accepted TCP stream (TR-R-064): the
    /// handshake happens entirely here, before `FrameTransport` construction.
    ///
    /// # Errors
    ///
    /// Fails if the handshake does.
    pub(crate) async fn handshake_framed<F: Framing>(
        &self,
        stream: TcpStream,
    ) -> Result<(
        FrameTransport<tokio_rustls::server::TlsStream<TcpStream>, F>,
        Option<CertificateDer<'static>>,
    )> {
        let tls_stream = self.acceptor.accept(stream).await.map_err(|io_error| {
            Error::TlsHandshake {
                source: tls_error_source(io_error),
                peer_cert: None, // TR-R-069: wired to the offered cert in stage s2
            }
        })?;
        let cert = tls_stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certs| certs.first().cloned());
        Ok((FrameTransport::new(tls_stream), cert))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::io::duplex;

    #[test]
    /// TR-R-068 — the documented port for Modbus over TLS.
    fn ut_modbus_tls_port_is_802() {
        assert_eq!(MODBUS_TLS_PORT, 802);
    }

    #[test]
    /// TR-R-067 — recovers the exact `rustls::Error` tokio-rustls boxed into
    /// the `io::Error`.
    fn ut_tls_error_source_recovers_the_boxed_rustls_error() {
        let io_error =
            std::io::Error::new(std::io::ErrorKind::InvalidData, rustls::Error::DecryptError);
        assert_eq!(tls_error_source(io_error), rustls::Error::DecryptError);
    }

    #[test]
    /// TR-R-067 — a bare I/O failure with no boxed `rustls::Error` (e.g. an
    /// EOF mid-handshake) falls back to `General`, not fabricated as some
    /// other variant.
    fn ut_tls_error_source_falls_back_to_general_when_no_source_is_boxed() {
        let io_error = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "tls handshake eof");
        assert!(matches!(
            tls_error_source(io_error),
            rustls::Error::General(_)
        ));
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/tests/fixtures/tls/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("fixture file reads")
    }

    fn server_name() -> ServerName<'static> {
        ServerName::from(std::net::IpAddr::from([127, 0, 0, 1]))
    }

    /// This module's own `server_config` (TR-R-066), presenting
    /// `server.crt`/`server.key` under the given client-cert policy — tests
    /// dogfood the same builder `TlsListener::bind` uses.
    fn tls_server_config(client_certs: ClientCertPolicy) -> rustls::ServerConfig {
        let cert_chain = load_pem_cert_chain(&fixture("server.crt")).expect("parses");
        let key = load_pem_private_key(&fixture("server.key")).expect("parses");
        server_config(TlsServerConfig {
            cert_chain,
            key,
            client_certs,
        })
        .expect("a valid cert/key/policy combination")
    }

    fn no_client_auth_server_config() -> rustls::ServerConfig {
        tls_server_config(ClientCertPolicy::None)
    }

    fn roots(pem_fixture: &str) -> RootStore {
        let mut store = RootStore::empty();
        store.add_pem(&fixture(pem_fixture)).expect("parses");
        store
    }

    #[tokio::test]
    /// TR-R-067, TR-R-069 — `Verify` rejects a server certificate issued by
    /// a CA the root store does not trust, surfacing `Error::TlsHandshake`
    /// distinctly (not merely `is_err()`), with `peer_cert: None` (a
    /// client-side handshake failure).
    async fn ut_verify_rejects_a_cert_from_an_untrusted_issuer() {
        let (server_end, client_end) = duplex(4096);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(no_client_auth_server_config()));
        tokio::spawn(async move {
            let _ = acceptor.accept(server_end).await;
        });

        let config = client_config(TlsClientConfig {
            server_cert: ServerCertVerification::Verify(roots("other-ca.crt")),
            client_identity: None,
        })
        .expect("builds");
        let connector = TlsConnector::from(Arc::new(config));
        let result = connector
            .connect(server_name(), client_end)
            .await
            .map_err(|io_error| Error::TlsHandshake {
                source: tls_error_source(io_error),
                peer_cert: None,
            });
        assert!(matches!(
            result,
            Err(Error::TlsHandshake {
                peer_cert: None,
                ..
            })
        ));
    }

    #[tokio::test]
    /// TR-R-065 — `DangerousDisableVerification` accepts a server certificate
    /// no root store trusts.
    async fn ut_dangerous_disable_verification_accepts_an_untrusted_cert() {
        let (server_end, client_end) = duplex(4096);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(no_client_auth_server_config()));
        let serving = tokio::spawn(async move { acceptor.accept(server_end).await.map(|_| ()) });

        let config = client_config(TlsClientConfig {
            server_cert: ServerCertVerification::DangerousDisableVerification,
            client_identity: None,
        })
        .expect("builds");
        let connector = TlsConnector::from(Arc::new(config));
        let result = connector
            .connect(server_name(), client_end)
            .await
            .map_err(|io_error| Error::TlsHandshake {
                source: tls_error_source(io_error),
                peer_cert: None,
            });
        assert!(result.is_ok());
        serving
            .await
            .expect("the server task finishes")
            .expect("the server side of the handshake succeeds too");
    }

    #[tokio::test]
    /// TR-R-065 — a client identity is presented and reaches the server's
    /// verified peer certificates.
    async fn ut_client_identity_is_presented() {
        let (server_end, client_end) = duplex(4096);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_server_config(
            ClientCertPolicy::Require(roots("ca.crt")),
        )));
        let serving = tokio::spawn(async move { acceptor.accept(server_end).await });

        let cert_chain = load_pem_cert_chain(&fixture("client.crt")).expect("parses");
        let key = load_pem_private_key(&fixture("client.key")).expect("parses");
        let config = client_config(TlsClientConfig {
            server_cert: ServerCertVerification::Verify(roots("ca.crt")),
            client_identity: Some(ClientIdentity { cert_chain, key }),
        })
        .expect("builds");
        let connector = TlsConnector::from(Arc::new(config));
        let _client_stream = connector
            .connect(server_name(), client_end)
            .await
            .expect("handshakes");

        let server_stream = serving
            .await
            .expect("the server task finishes")
            .expect("the server accepts the handshake");
        let peer_certs = server_stream
            .get_ref()
            .1
            .peer_certificates()
            .expect("a client certificate was presented");
        assert_eq!(
            peer_certs.first(),
            load_pem_cert_chain(&fixture("client.crt"))
                .expect("parses")
                .first()
        );
    }

    #[tokio::test]
    /// TR-R-066 — `Require` rejects a client certificate issued by a CA the
    /// root store does not trust.
    async fn ut_require_rejects_an_untrusted_client_cert() {
        let (server_end, client_end) = duplex(4096);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_server_config(
            ClientCertPolicy::Require(roots("ca.crt")),
        )));
        let serving = tokio::spawn(async move { acceptor.accept(server_end).await });

        let cert_chain = load_pem_cert_chain(&fixture("unrelated-client.crt")).expect("parses");
        let key = load_pem_private_key(&fixture("unrelated-client.key")).expect("parses");
        let config = client_config(TlsClientConfig {
            server_cert: ServerCertVerification::Verify(roots("ca.crt")),
            client_identity: Some(ClientIdentity { cert_chain, key }),
        })
        .expect("builds");
        let connector = TlsConnector::from(Arc::new(config));
        let client_result = connector.connect(server_name(), client_end).await;
        let server_result = serving.await.expect("the server task finishes");

        assert!(
            client_result.is_err() || server_result.is_err(),
            "an untrusted client cert must fail the handshake on one side or the other"
        );
    }

    #[tokio::test]
    /// TR-R-066 — `Require` accepts a client certificate issued by a trusted
    /// CA.
    async fn ut_require_accepts_a_trusted_client_cert() {
        let (server_end, client_end) = duplex(4096);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_server_config(
            ClientCertPolicy::Require(roots("ca.crt")),
        )));
        let serving = tokio::spawn(async move { acceptor.accept(server_end).await });

        let cert_chain = load_pem_cert_chain(&fixture("client.crt")).expect("parses");
        let key = load_pem_private_key(&fixture("client.key")).expect("parses");
        let config = client_config(TlsClientConfig {
            server_cert: ServerCertVerification::Verify(roots("ca.crt")),
            client_identity: Some(ClientIdentity { cert_chain, key }),
        })
        .expect("builds");
        let connector = TlsConnector::from(Arc::new(config));
        let _client_stream = connector
            .connect(server_name(), client_end)
            .await
            .expect("handshakes");
        serving
            .await
            .expect("the server task finishes")
            .expect("the server accepts a trusted client cert");
    }
}
