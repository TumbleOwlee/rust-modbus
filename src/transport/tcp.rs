//! TCP sockets: connecting, listening, and the options that go with them
//! (TR-R-020 … TR-R-023).

use core::future::Future;
use core::time::Duration;
use std::net::SocketAddr;

use tokio::net::TcpStream;

use crate::error::{Error, Result};
use crate::frame::{Framing, RtuOverTcp, Tcp};
use crate::transport::FrameTransport;

/// A transport over a TCP socket, framed as Modbus TCP.
pub type TcpTransport = FrameTransport<TcpStream, Tcp>;

/// A transport over a TCP socket carrying RTU-over-stream framing, for a
/// transparent serial gateway (TR-R-024, TR-R-033).
pub type RtuOverTcpTransport = FrameTransport<TcpStream, RtuOverTcp>;

/// How a TCP connection is made (TR-R-021, TR-R-022).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TcpConfig {
    /// How long to wait for the connection to be established.
    pub connect_timeout: Duration,
    /// Whether to disable Nagle's algorithm.
    pub nodelay: bool,
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            nodelay: true,
        }
    }
}

/// Connect to a Modbus TCP server (TR-R-020).
///
/// # Errors
///
/// Fails if the connection is refused, if the network reports any other error,
/// or if the connect timeout expires first.
pub async fn connect_tcp(addr: SocketAddr, config: TcpConfig) -> Result<TcpTransport> {
    connect_tcp_framed::<Tcp>(addr, config).await
}

/// Connect to a Modbus server over TCP, for any framing (TR-R-024).
///
/// Establishing the socket does not differ by framing; only what is read off
/// it does. `connect_tcp` is this with `F` fixed to [`Tcp`] under its existing
/// name, so no call site that only ever spoke Modbus TCP needs to change.
///
/// # Errors
///
/// Fails if the connection is refused, if the network reports any other error,
/// or if the connect timeout expires first.
pub async fn connect_tcp_framed<F: Framing>(
    addr: SocketAddr,
    config: TcpConfig,
) -> Result<FrameTransport<TcpStream, F>> {
    let stream = with_connect_timeout(config.connect_timeout, TcpStream::connect(addr)).await?;
    stream.set_nodelay(config.nodelay)?;
    Ok(FrameTransport::new(stream))
}

/// A listening Modbus TCP socket (TR-R-023).
#[derive(Debug)]
pub struct TcpListener {
    /// The bound socket.
    inner: tokio::net::TcpListener,
}

impl TcpListener {
    /// Bind a listening socket.
    ///
    /// # Errors
    ///
    /// Fails if the address cannot be bound.
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        Ok(Self {
            inner: tokio::net::TcpListener::bind(addr).await?,
        })
    }

    /// The address actually bound, which is how a caller learns the port after
    /// binding port 0.
    ///
    /// # Errors
    ///
    /// Fails if the socket cannot report its address.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.inner.local_addr()?)
    }

    /// Accept one connection (TR-R-023).
    ///
    /// # Errors
    ///
    /// Fails if the accept does.
    pub async fn accept(&self) -> Result<(TcpTransport, SocketAddr)> {
        self.accept_framed::<Tcp>().await
    }

    /// Accept one connection, for any framing (TR-R-024).
    ///
    /// `accept` is this with `F` fixed to [`Tcp`] under its existing name, so a
    /// listener that only ever accepted Modbus TCP connections needs no change.
    /// A gateway listener accepts `RtuOverTcp` connections through this instead.
    ///
    /// # Errors
    ///
    /// Fails if the accept does.
    pub async fn accept_framed<F: Framing>(
        &self,
    ) -> Result<(FrameTransport<TcpStream, F>, SocketAddr)> {
        let (stream, peer) = self.inner.accept().await?;
        Ok((FrameTransport::new(stream), peer))
    }
}

/// Apply the connect timeout to a connection attempt (TR-R-021).
///
/// Separated from the socket so the rule is testable without a peer that can be
/// relied on to hang: expiry is a timeout error, never an I/O one.
async fn with_connect_timeout<F>(timeout: Duration, attempt: F) -> Result<TcpStream>
where
    F: Future<Output = std::io::Result<TcpStream>>,
{
    match tokio::time::timeout(timeout, attempt).await {
        Ok(result) => Ok(result?),
        // Nothing failed; the wait ran out (TR-R-021).
        Err(_elapsed) => Err(Error::Timeout { what: "connect" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    /// TR-R-021 — a connect that does not complete in time is a timeout naming
    /// what timed out, not an I/O error: nothing failed, the wait ran out.
    async fn ut_connect_timeout_is_a_timeout_error() {
        let never = core::future::pending::<std::io::Result<TcpStream>>();
        assert_eq!(
            with_connect_timeout(Duration::from_secs(5), never)
                .await
                .map(|_| ()),
            Err(Error::Timeout { what: "connect" })
        );
    }

    #[test]
    /// TR-R-021, TR-R-022 — the defaults are a 5-second connect timeout and
    /// Nagle disabled, since Modbus is request/response and Nagle delay buys
    /// nothing.
    fn ut_tcp_config_defaults() {
        assert_eq!(
            TcpConfig::default(),
            TcpConfig {
                connect_timeout: Duration::from_secs(5),
                nodelay: true,
            }
        );
    }
}
