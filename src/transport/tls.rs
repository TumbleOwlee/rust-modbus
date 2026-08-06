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
    rustls_pemfile::certs(&mut &*pem)
        .collect::<core::result::Result<Vec<_>, _>>()
        .map_err(|_error| Error::Configuration { field: "pem" })
}

/// Parse the first private key in a PEM document.
///
/// # Errors
///
/// Fails if the document cannot be read as PEM, or carries no private key.
pub fn load_pem_private_key(pem: &[u8]) -> Result<PrivateKeyDer<'static>> {
    rustls_pemfile::private_key(&mut &*pem)
        .map_err(|_error| Error::Configuration { field: "pem" })?
        .ok_or(Error::Configuration { field: "pem" })
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
        .map_err(|_error| Error::TlsHandshake)?;
    let builder = match config.server_cert {
        ServerCertVerification::Verify(store) => builder.with_root_certificates(store.0),
        ServerCertVerification::DangerousDisableVerification => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoServerCertVerification::new())),
    };
    match config.client_identity {
        Some(identity) => builder
            .with_client_auth_cert(identity.cert_chain, identity.key)
            .map_err(|_error| Error::TlsHandshake),
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
            .map_err(|_error| Error::TlsHandshake)
    };
    match tokio::time::timeout(tcp.connect_timeout, attempt).await {
        Ok(result) => Ok(FrameTransport::new(result?)),
        // Nothing failed; the wait ran out (TR-R-021).
        Err(_elapsed) => Err(Error::Timeout { what: "connect" }),
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

    /// A `rustls::ServerConfig` presenting `server.crt`/`server.key`, with the
    /// given client-cert policy.
    fn server_config(
        client_verifier: Arc<dyn rustls::server::danger::ClientCertVerifier>,
    ) -> rustls::ServerConfig {
        let cert_chain = load_pem_cert_chain(&fixture("server.crt")).expect("parses");
        let key = load_pem_private_key(&fixture("server.key")).expect("parses");
        rustls::ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("ring supports TLS 1.2/1.3")
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(cert_chain, key)
            .expect("a valid cert/key pair")
    }

    fn no_client_auth_server_config() -> rustls::ServerConfig {
        let cert_chain = load_pem_cert_chain(&fixture("server.crt")).expect("parses");
        let key = load_pem_private_key(&fixture("server.key")).expect("parses");
        rustls::ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("ring supports TLS 1.2/1.3")
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)
            .expect("a valid cert/key pair")
    }

    fn roots(pem_fixture: &str) -> RootStore {
        let mut store = RootStore::empty();
        store.add_pem(&fixture(pem_fixture)).expect("parses");
        store
    }

    #[tokio::test]
    /// TR-R-065, TR-R-067 — `Verify` rejects a server certificate issued by a
    /// CA the root store does not trust.
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
            .map_err(|_error| Error::TlsHandshake);
        assert_eq!(result.err(), Some(Error::TlsHandshake));
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
            .map_err(|_error| Error::TlsHandshake);
        assert!(result.is_ok());
        serving
            .await
            .expect("the server task finishes")
            .expect("the server side of the handshake succeeds too");
    }

    #[tokio::test]
    /// TR-R-065, SV-R-055 — a client identity is presented and reaches the
    /// server's verified peer certificates.
    async fn ut_client_identity_is_presented() {
        let (server_end, client_end) = duplex(4096);
        let client_verifier =
            rustls::server::WebPkiClientVerifier::builder(Arc::new(roots("ca.crt").0))
                .build()
                .expect("builds");
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config(client_verifier)));
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
}
