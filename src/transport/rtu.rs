//! Opening a serial port (TR-R-030, TR-R-032).
//!
//! Behind the off-by-default `rtu` feature: a TCP-only consumer should not
//! acquire a serial backend.

use tokio_serial::{SerialPortBuilderExt, SerialStream};

use crate::error::{Error, Result};
use crate::frame::Framing;
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
}
