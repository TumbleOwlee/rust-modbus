//! The accept-error hook, reached through the crate root as an external
//! implementor sees it.

use rust_modbus::{
    AcceptErrorAction, Connection, Error, ExceptionCode, ListenerFailure, RequestPdu, ResponsePdu,
    Service, UnitId,
};

/// Implements only the required method: the default hook applies.
struct Plain;

impl Service for Plain {
    async fn on_request(
        &self,
        _conn: &Connection,
        _unit: UnitId,
        _request: RequestPdu,
    ) -> Result<Option<ResponsePdu>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }
}

/// Overrides the hook to keep serving.
struct Keeper;

impl Service for Keeper {
    async fn on_request(
        &self,
        _conn: &Connection,
        _unit: UnitId,
        _request: RequestPdu,
    ) -> Result<Option<ResponsePdu>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    async fn on_accept_error(&self, _error: &Error) -> AcceptErrorAction {
        AcceptErrorAction::Continue
    }
}

fn error() -> Error {
    Error::Io {
        kind: std::io::ErrorKind::OutOfMemory,
        raw_os_error: None,
    }
}

#[tokio::test]
/// SV-R-059 — an external implementor inherits `Stop` and may override with
/// `Continue`, naming `AcceptErrorAction` from the crate root.
async fn it_external_service_answers_accept_error_actions() {
    assert_eq!(
        Plain.on_accept_error(&error()).await,
        AcceptErrorAction::Stop
    );
    assert_eq!(
        Keeper.on_accept_error(&error()).await,
        AcceptErrorAction::Continue
    );
}

#[test]
/// TR-R-106 — `ListenerFailure` has exactly `Transient` and `Fatal`; the
/// wildcard-free `match` fails to compile outside the crate if that changes.
fn it_listener_failure_variants_are_exactly_two() {
    fn name(f: ListenerFailure) -> &'static str {
        match f {
            ListenerFailure::Transient => "transient",
            ListenerFailure::Fatal => "fatal",
        }
    }
    fn assert_traits<T: Copy + Eq + core::fmt::Debug>() {}
    assert_traits::<ListenerFailure>();
    assert_eq!(name(ListenerFailure::Transient), "transient");
    assert_eq!(name(ListenerFailure::Fatal), "fatal");
}

#[test]
/// SV-R-073, SV-R-074 — an external service inherits the `on_receive_error` default.
fn it_external_service_inherits_receive_error_default() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("builds");
    rt.block_on(async {
        assert_eq!(
            Plain.on_receive_error(&Error::Malformed).await,
            AcceptErrorAction::Stop
        );
        assert_eq!(
            Keeper.on_receive_error(&Error::Malformed).await,
            AcceptErrorAction::Stop
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            Plain
                .on_receive_error(&Error::from(std::io::Error::from_raw_os_error(105)))
                .await,
            AcceptErrorAction::Continue
        );
    });
}

/// Classifies in `on_accept_error`.
struct Classifier;

impl Service for Classifier {
    async fn on_request(
        &self,
        _conn: &Connection,
        _unit: UnitId,
        _request: RequestPdu,
    ) -> Result<Option<ResponsePdu>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    async fn on_accept_error(&self, error: &Error) -> AcceptErrorAction {
        match error.listener_failure() {
            Some(ListenerFailure::Transient) => AcceptErrorAction::Continue,
            _ => AcceptErrorAction::Stop,
        }
    }
}

#[test]
/// SV-E-036, TR-R-101 — a service backs off by reading `listener_failure()`.
fn it_external_service_backs_off_on_listener_failure() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("builds");
    rt.block_on(async {
        #[cfg(target_os = "linux")]
        {
            let enobufs = Error::from(std::io::Error::from_raw_os_error(105));
            assert_eq!(
                Classifier.on_accept_error(&enobufs).await,
                AcceptErrorAction::Continue
            );
            let ebadf = Error::from(std::io::Error::from_raw_os_error(9));
            assert_eq!(
                Classifier.on_accept_error(&ebadf).await,
                AcceptErrorAction::Stop
            );
        }
        assert_eq!(Error::Malformed.listener_failure(), None);
        assert_eq!(
            Classifier.on_accept_error(&Error::Malformed).await,
            AcceptErrorAction::Stop
        );
    });
}
