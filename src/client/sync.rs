//! Blocking client (CL-R-070 … CL-R-079).
//!
//! A thin facade over [`Client`]: it owns a runtime and drives the async client
//! on it. Every guarantee lives in the async client and is reached from here by
//! delegation (CL-R-072), so the two surfaces cannot diverge — there is no
//! second implementation of a timeout, a broadcast rule, or a desynchronization
//! rule to keep in step.

use tokio::net::TcpStream;
use tokio::runtime::{Builder, Handle, Runtime};

use crate::client::{Client, ClientConfig, ClientFraming, ClientState};
use crate::error::{Error, Result};
use crate::transport::{TcpConfig, connect_tcp_framed};

/// A blocking Modbus client (CL-R-070).
///
/// Mirrors [`Client`] method for method (CL-R-071), with `async` removed. Use it
/// from a thread that has no runtime; a caller that already has one wants
/// [`Client`] instead, and CL-R-075 refuses the mistake rather than deadlocking
/// on it.
#[derive(Debug)]
pub struct SyncClient<S, F> {
    /// The async client every method delegates to (CL-R-072).
    client: Client<S, F>,
    /// The runtime that drives it (CL-R-073). Owned, so a caller never supplies
    /// one and no runtime type appears in a signature (CL-R-074).
    runtime: Runtime,
}

/// A blocking client over TCP.
pub type SyncTcpClient = SyncClient<TcpStream, crate::frame::Tcp>;
/// A blocking client speaking RTU framing over a TCP socket.
pub type SyncRtuOverTcpClient = SyncClient<TcpStream, crate::frame::RtuOverTcp>;

/// A blocking client over a serial port in RTU framing.
#[cfg(feature = "rtu")]
pub type SyncRtuClient = SyncClient<crate::transport::SerialStream, crate::frame::Rtu>;
/// A blocking client over a serial port in ASCII framing.
#[cfg(feature = "rtu")]
pub type SyncAsciiClient = SyncClient<crate::transport::SerialStream, crate::frame::Ascii>;

/// Refuse a blocking call made from a thread that already drives a runtime
/// (CL-R-075).
///
/// Checked before anything is written, and before a runtime is even built, so
/// the refusal costs nothing and leaves no transport half-established.
/// `Runtime::block_on` would panic here; a typed error is what a caller can
/// handle.
fn refuse_inside_a_runtime() -> Result<()> {
    if Handle::try_current().is_ok() {
        return Err(Error::BlockingInAsyncContext);
    }
    Ok(())
}

/// Build the runtime a blocking client owns (CL-R-073).
///
/// `enable_all` rather than a narrower set: the client below awaits both I/O and
/// timers, and a runtime built without the timer driver would panic the moment
/// the response timeout of CL-R-030 was applied. The facade owns the runtime, so
/// it owns the obligation to enable what the code beneath it uses.
fn build_runtime() -> Result<Runtime> {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| Error::Io { kind: error.kind() })
}

impl<F: ClientFraming> SyncClient<TcpStream, F> {
    /// Connect to a Modbus server over TCP and wrap it in a blocking client
    /// (CL-R-076).
    ///
    /// Takes an address rather than a transport, because a caller with no
    /// runtime cannot construct one: CL-R-002 binds the async client only.
    ///
    /// # Errors
    ///
    /// Fails with [`Error::BlockingInAsyncContext`] if called from a thread
    /// already driving a runtime (CL-R-075), if the runtime cannot be built, or
    /// if the connection is refused, times out, or the network reports an error.
    pub fn connect(
        addr: core::net::SocketAddr,
        tcp: TcpConfig,
        client: ClientConfig,
    ) -> Result<Self> {
        refuse_inside_a_runtime()?;
        let runtime = build_runtime()?;
        let transport = runtime.block_on(connect_tcp_framed::<F>(addr, tcp))?;
        Ok(Self {
            client: Client::with_config(transport, client),
            runtime,
        })
    }
}

#[cfg(feature = "rtu")]
impl<F: ClientFraming> SyncClient<crate::transport::SerialStream, F> {
    /// Open a serial port and wrap it in a blocking client (CL-R-076).
    ///
    /// # Errors
    ///
    /// Fails with [`Error::BlockingInAsyncContext`] if called from a thread
    /// already driving a runtime (CL-R-075), if the runtime cannot be built, or
    /// if the device is absent or cannot be opened with these settings.
    pub fn open(
        path: &str,
        serial: crate::transport::SerialConfig,
        client: ClientConfig,
    ) -> Result<Self> {
        refuse_inside_a_runtime()?;
        let runtime = build_runtime()?;
        // `open_serial` is already synchronous, but the backend registers the
        // opened port with the reactor, so a runtime must be *entered* rather
        // than blocked on.
        let transport = {
            let _guard = runtime.enter();
            crate::transport::open_serial::<F>(path, serial)?
        };
        Ok(Self {
            client: Client::with_config(transport, client),
            runtime,
        })
    }
}

impl<S, F> SyncClient<S, F>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    F: ClientFraming,
{
    /// Issue a request and yield the response as received (CL-R-061, CL-R-071).
    ///
    /// `None` means the request was a broadcast and no reply was awaited
    /// (CL-R-053).
    ///
    /// # Errors
    ///
    /// Fails with [`Error::BlockingInAsyncContext`] if called from a thread
    /// already driving a runtime (CL-R-075), and otherwise exactly as the async
    /// [`Client::call`] does (CL-R-072).
    pub fn call(
        &mut self,
        unit: crate::frame::UnitId,
        request: crate::frame::RequestPdu,
    ) -> Result<Option<crate::frame::ResponsePdu>> {
        refuse_inside_a_runtime()?;
        // Destructured rather than `self.runtime.block_on(self.client.call(..))`,
        // which borrows `self` twice. Every request method below repeats these
        // two lines; a helper taking the future cannot express that the future
        // borrows the client it came from.
        let Self { client, runtime } = self;
        runtime.block_on(client.call(unit, request))
    }

    /// Whether this client will refuse every further request (CL-R-034).
    ///
    /// Reads no transport and enters no runtime (CL-R-078), so it is callable
    /// from anywhere, an async context included.
    #[must_use]
    pub fn is_desynchronized(&self) -> bool {
        self.client.is_desynchronized()
    }

    /// What this client knows about its own usability (CL-R-035).
    ///
    /// Reads no transport and enters no runtime (CL-R-078).
    #[must_use]
    pub fn state(&self) -> ClientState {
        self.client.state()
    }
}
