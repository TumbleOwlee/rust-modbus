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
