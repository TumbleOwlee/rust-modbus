//! Opening a serial port (TR-R-030, TR-R-032).
//!
//! Behind the off-by-default `rtu` feature: a TCP-only consumer should not
//! acquire a serial backend.

use tokio_serial::SerialPortBuilderExt;
/// The stream a serial port is read and written through (TR-R-034).
///
/// Re-exported rather than hidden: it already appears in [`SerialTransport`],
/// `RtuClient` and `AsciiClient`, so a consumer naming any of those in its own
/// signature needs to name this too. Unlike [`SerialConfig`]'s enums, this is
/// not a value the crate could define for itself — it is what the backend hands
/// back — so declining to export it would not keep the backend out of the
/// public API, only out of reach.
pub use tokio_serial::SerialStream;

use crate::error::{Error, Result};
use crate::frame::Framing;
#[cfg(feature = "rs485")]
use crate::transport::rs485;
use crate::transport::serial::{DataBits, FlowControl, Parity, SerialConfig, StopBits};
use crate::transport::{FrameTransport, TransportConfig};

/// A transport over a serial port, framed as the caller chooses.
///
/// Generic over the framing because one line carries RTU or ASCII at the
/// operator's choice, over identical port settings.
pub type SerialTransport<F> = FrameTransport<SerialStream, F>;

/// Open a serial port (TR-R-030).
///
/// The inter-frame interval the RTU boundary rule needs is derived from the
/// same configuration, so the port and its timing cannot disagree (TR-R-011).
///
/// # Errors
///
/// Fails if the device is absent or cannot be opened with these settings, or if
/// the settings imply no character time.
pub fn open_serial<F: Framing>(path: &str, config: SerialConfig) -> Result<SerialTransport<F>> {
    // Derived before the port is touched, so an unusable configuration is
    // rejected without a side effect (TR-R-031).
    let transport = TransportConfig::from_serial(&config)?;
    let port = tokio_serial::new(path, config.baud_rate)
        .data_bits(data_bits(config.data_bits))
        .parity(parity(config.parity))
        .stop_bits(stop_bits(config.stop_bits))
        .flow_control(flow_control(config.flow_control))
        .open_native_async()
        .map_err(convert)?;
    // TR-R-053: applied after the port opens and before the transport is
    // returned, so a caller never holds a transport whose direction control
    // silently failed to apply.
    #[cfg(feature = "rs485")]
    if let Some(rs485_config) = &config.rs485 {
        rs485::apply(&port, rs485_config)?;
    }
    Ok(FrameTransport::with_config(port, transport))
}

/// Map the backend's failure onto the crate's own (TR-R-040).
///
/// The backend distinguishes a missing device from an I/O error; both are I/O
/// as far as a caller is concerned, so the distinction is carried in the kind.
fn convert(error: tokio_serial::Error) -> Error {
    Error::Io {
        kind: match error.kind {
            tokio_serial::ErrorKind::NoDevice => std::io::ErrorKind::NotFound,
            tokio_serial::ErrorKind::InvalidInput => std::io::ErrorKind::InvalidInput,
            tokio_serial::ErrorKind::Io(kind) => kind,
            tokio_serial::ErrorKind::Unknown => std::io::ErrorKind::Other,
        },
    }
}

/// Translate this crate's data-bit count to the backend's.
fn data_bits(bits: DataBits) -> tokio_serial::DataBits {
    match bits {
        DataBits::Five => tokio_serial::DataBits::Five,
        DataBits::Six => tokio_serial::DataBits::Six,
        DataBits::Seven => tokio_serial::DataBits::Seven,
        DataBits::Eight => tokio_serial::DataBits::Eight,
    }
}

/// Translate this crate's parity to the backend's.
fn parity(parity: Parity) -> tokio_serial::Parity {
    match parity {
        Parity::None => tokio_serial::Parity::None,
        Parity::Odd => tokio_serial::Parity::Odd,
        Parity::Even => tokio_serial::Parity::Even,
    }
}

/// Translate this crate's stop bits to the backend's.
fn stop_bits(bits: StopBits) -> tokio_serial::StopBits {
    match bits {
        StopBits::One => tokio_serial::StopBits::One,
        StopBits::Two => tokio_serial::StopBits::Two,
    }
}

/// Translate this crate's flow control to the backend's.
fn flow_control(flow: FlowControl) -> tokio_serial::FlowControl {
    match flow {
        FlowControl::None => tokio_serial::FlowControl::None,
        FlowControl::Software => tokio_serial::FlowControl::Software,
        FlowControl::Hardware => tokio_serial::FlowControl::Hardware,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Rtu;

    #[test]
    /// TR-R-030 — a device that is not there is reported as the I/O failure the
    /// platform describes, not as a panic and not as a silent success.
    ///
    /// This needs no hardware, which is the point: the only serial behavior CI
    /// can exercise is the failure to open one.
    fn ut_open_serial_missing_device_is_io_error() {
        let opened = open_serial::<Rtu>("/dev/rust-modbus-no-such-device", SerialConfig::default());
        assert!(
            matches!(opened, Err(Error::Io { .. })),
            "expected an I/O error, got {opened:?}"
        );
    }

    #[test]
    /// TR-R-031 — settings that imply no character time are rejected before the
    /// port is touched, since the RTU boundary rule could not be derived.
    fn ut_open_serial_rejects_an_unusable_configuration() {
        let config = SerialConfig {
            baud_rate: 0,
            ..SerialConfig::default()
        };
        assert_eq!(
            open_serial::<Rtu>("/dev/rust-modbus-no-such-device", config)
                .expect_err("zero baud cannot be used"),
            Error::Configuration { field: "baud_rate" }
        );
    }

    #[cfg(feature = "rs485")]
    #[test]
    /// TR-R-053 — `open_serial` issues `TIOCSRS485` after the port is opened
    /// and before the transport is returned, so a caller never holds a
    /// transport whose direction control silently failed to apply. A pty
    /// rejects the ioctl with `ENOTTY`, so `rs485: Some(..)` fails at open
    /// time rather than yielding a live transport. The `None` case over the
    /// *same* pty still opens successfully, proving the ioctl is issued only
    /// when requested and not merely that ptys are unusable.
    fn ut_open_serial_applies_rs485_at_open() {
        use core::time::Duration;

        use nix::fcntl::OFlag;
        use nix::pty::{grantpt, posix_openpt, ptsname_r, unlockpt};

        use crate::transport::serial::{Rs485Config, RtsPolarity};

        let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
        grantpt(&master).expect("grantpt");
        unlockpt(&master).expect("unlockpt");
        let slave_path = ptsname_r(&master).expect("ptsname_r");

        // `open_native_async` registers the port with Tokio's reactor once it
        // actually succeeds in opening a real device, so this synchronous
        // test needs one entered even though `open_serial` itself is not
        // async.
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let _guard = runtime.enter();

        let with_rs485 = SerialConfig {
            rs485: Some(Rs485Config {
                rts_on_send: RtsPolarity::High,
                delay_before_send: Duration::ZERO,
                delay_after_send: Duration::ZERO,
            }),
            ..SerialConfig::default()
        };
        let opened = open_serial::<Rtu>(&slave_path, with_rs485);
        assert_eq!(
            opened.err(),
            Some(Error::Rs485Unsupported),
            "a pty must reject TIOCSRS485"
        );

        let opened_without = open_serial::<Rtu>(&slave_path, SerialConfig::default());
        assert!(
            opened_without.is_ok(),
            "rs485: None must still open the same pty; got {opened_without:?}"
        );
    }

    #[cfg(feature = "rs485")]
    #[ignore = "requires RS-485-capable hardware: a real serial device whose driver implements TIOCSRS485"]
    #[test]
    /// TR-R-050, TR-R-053 — end to end against a real RS-485-capable port.
    /// Named `ut_*`, never `it_*`, because it lives in a source file
    /// (`NF-R-020`); `it_*` is reserved for `tests/`. Opt-in only: no CI
    /// runner has the hardware this exercises (NF-R-024).
    fn ut_open_serial_rs485_hardware_end_to_end() {
        use core::time::Duration;

        use crate::transport::serial::{Rs485Config, RtsPolarity};

        let path =
            std::env::var("RUST_MODBUS_RS485_TEST_DEVICE").expect("set to a real RS-485 device");
        let config = SerialConfig {
            rs485: Some(Rs485Config {
                rts_on_send: RtsPolarity::High,
                delay_before_send: Duration::from_millis(1),
                delay_after_send: Duration::from_millis(1),
            }),
            ..SerialConfig::default()
        };
        let opened = open_serial::<Rtu>(&path, config);
        assert!(opened.is_ok(), "expected success, got {opened:?}");
    }
}
