//! The shutdown handle (SV-R-040).

use alloc::sync::Arc;

use tokio::sync::watch;

/// Requests a server's shutdown and waits for it (SV-R-040).
///
/// Taken from [`Server::handle`](crate::Server::handle) before serving begins,
/// and cloneable so that a signal handler and a test may each hold one.
#[derive(Debug, Clone)]
pub struct ServerHandle {
    /// Shared with the server; every connection holds a receiver.
    signal: Arc<watch::Sender<bool>>,
}

impl ServerHandle {
    /// Wrap the server's signal.
    pub(crate) fn new(signal: Arc<watch::Sender<bool>>) -> Self {
        Self { signal }
    }

    /// Ask the server to stop, and wait until it has (SV-R-041, SV-R-044).
    ///
    /// Returns once every connection has ended and serving has returned, so a
    /// caller knows no handler is still running. Requests already dispatched are
    /// answered first (SV-R-042). Idempotent.
    pub async fn shutdown(&self) {
        // Set before the wait: a connection subscribing in between sees the
        // request immediately rather than missing the change.
        //
        // `send_replace` rather than `send`, which discards the value when no
        // receiver has subscribed yet — that is exactly the case when shutdown
        // races the start of serving, and it would leave the flag false forever.
        self.signal.send_replace(true);
        // Every live connection holds a receiver, so this completes exactly
        // when the last of them is done (SV-R-044).
        self.signal.closed().await;
    }

    /// Whether shutdown has been requested, without requesting it (SV-R-045).
    #[must_use]
    pub fn is_shutting_down(&self) -> bool {
        *self.signal.borrow()
    }
}
