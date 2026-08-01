//! Serial port configuration and the timing it implies (TR-R-011, TR-R-031).
//!
//! The types here are the crate's own rather than the serial backend's, so the
//! backend is not part of the public API and the timing rule stays testable with
//! the `rtu` feature off and no port in sight.

use core::time::Duration;

use crate::error::{Error, Result};

/// Above this baud rate the inter-frame interval stops tracking the character
/// time and is fixed (TR-R-011).
const FIXED_INTERVAL_ABOVE_BAUD: u32 = 19_200;

/// The fixed interval used above [`FIXED_INTERVAL_ABOVE_BAUD`] (TR-R-011).
const FIXED_INTERVAL: Duration = Duration::from_micros(1_750);

/// Bits per character, excluding start, parity, and stop bits (TR-R-031).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DataBits {
    /// Five data bits.
    Five,
    /// Six data bits.
    Six,
    /// Seven data bits.
    Seven,
    /// Eight data bits, the Modbus default.
    #[default]
    Eight,
}

/// Parity checking (TR-R-031).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Parity {
    /// No parity bit.
    None,
    /// Odd parity.
    Odd,
    /// Even parity, the Modbus default.
    #[default]
    Even,
}

/// Stop bits following a character (TR-R-031).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StopBits {
    /// One stop bit, the Modbus default.
    #[default]
    One,
    /// Two stop bits.
    Two,
}

/// Flow control on the line (TR-R-031).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FlowControl {
    /// No flow control, the Modbus default.
    #[default]
    None,
    /// XON/XOFF.
    Software,
    /// RTS/CTS.
    Hardware,
}

/// How a serial port is opened (TR-R-031).
///
/// [`Default`] is the Modbus serial-line default: 19200 baud, 8 data bits, even
/// parity, one stop bit, no flow control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SerialConfig {
    /// Symbols per second.
    pub baud_rate: u32,
    /// Data bits per character.
    pub data_bits: DataBits,
    /// Parity checking.
    pub parity: Parity,
    /// Stop bits per character.
    pub stop_bits: StopBits,
    /// Flow control.
    pub flow_control: FlowControl,
    /// Kernel RS-485 direction control (TR-R-050, TR-R-052).
    ///
    /// `None`, the default, requests no RS-485 configuration and issues no
    /// ioctl. Present only when the `rs485` feature is enabled, so a build
    /// without it has no field whose value could be silently ignored.
    #[cfg(feature = "rs485")]
    pub rs485: Option<Rs485Config>,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            baud_rate: 19_200,
            data_bits: DataBits::default(),
            parity: Parity::default(),
            stop_bits: StopBits::default(),
            flow_control: FlowControl::default(),
            #[cfg(feature = "rs485")]
            rs485: None,
        }
    }
}

/// Kernel RS-485 direction control for a serial port (TR-R-050).
///
/// Applied by `open_serial` (TR-R-053) via the `TIOCSRS485` ioctl. There is no
/// application-driven GPIO hook: direction control is delegated entirely to the
/// kernel driver.
#[cfg(feature = "rs485")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rs485Config {
    /// The RTS level asserted while transmitting. The level asserted
    /// afterwards is always its complement (TR-R-057).
    pub rts_on_send: RtsPolarity,
    /// Delay held before a transmission begins. Kept at full resolution here;
    /// truncated to whole milliseconds only where it is written to the ioctl
    /// (TR-R-056).
    pub delay_before_send: Duration,
    /// Delay held after a transmission ends, truncated to whole milliseconds
    /// (TR-R-056).
    pub delay_after_send: Duration,
}

/// The RTS level asserted while transmitting, under RS-485 kernel direction
/// control (TR-R-050).
#[cfg(feature = "rs485")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RtsPolarity {
    /// RTS driven high while transmitting.
    High,
    /// RTS driven low while transmitting.
    Low,
}

impl SerialConfig {
    /// Silence that separates two RTU frames: 3.5 character times, or a fixed
    /// 1.75 ms above 19200 baud (TR-R-011).
    ///
    /// # Errors
    ///
    /// Fails if the baud rate is zero, which no character time can be derived
    /// from.
    pub fn inter_frame_interval(&self) -> Result<Duration> {
        if self.baud_rate == 0 {
            return Err(Error::Configuration { field: "baud_rate" });
        }
        if self.baud_rate > FIXED_INTERVAL_ABOVE_BAUD {
            return Ok(FIXED_INTERVAL);
        }
        // 3.5 character times in nanoseconds, as `bits * 3.5e9 / baud` with the
        // half carried by the denominator so the arithmetic stays integral.
        let nanos = u64::from(self.bits_per_character())
            .saturating_mul(7_000_000_000)
            .saturating_div(u64::from(self.baud_rate).saturating_mul(2));
        Ok(Duration::from_nanos(nanos))
    }

    /// Bits on the wire per character: one start bit, the data bits, a parity
    /// bit if any, and the stop bits.
    fn bits_per_character(&self) -> u32 {
        let data = match self.data_bits {
            DataBits::Five => 5,
            DataBits::Six => 6,
            DataBits::Seven => 7,
            DataBits::Eight => 8,
        };
        let parity = match self.parity {
            Parity::None => 0,
            Parity::Odd | Parity::Even => 1,
        };
        let stop = match self.stop_bits {
            StopBits::One => 1,
            StopBits::Two => 2,
        };
        1 + data + parity + stop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// TR-R-031 — the defaults are the Modbus serial-line defaults: 19200 8E1,
    /// no flow control.
    fn ut_serial_defaults_are_19200_8e1() {
        assert_eq!(
            SerialConfig::default(),
            SerialConfig {
                baud_rate: 19_200,
                data_bits: DataBits::Eight,
                parity: Parity::Even,
                stop_bits: StopBits::One,
                flow_control: FlowControl::None,
                #[cfg(feature = "rs485")]
                rs485: None,
            }
        );
    }

    #[test]
    /// TR-R-011 — 3.5 character times, with a character counted as one start
    /// bit, the data bits, the parity bit, and the stop bits.
    ///
    /// The expected values are computed from that rule, not read back from the
    /// implementation: 8E1 is 11 bits, so at 9600 baud a character takes
    /// 11/9600 s and 3.5 of them take 4.0104 ms; 8N1 is 10 bits, giving
    /// 3.6458 ms; 8E2 is 12 bits, giving 4.375 ms.
    fn ut_interframe_interval_from_baud() {
        let at_9600 = SerialConfig {
            baud_rate: 9_600,
            ..SerialConfig::default()
        };
        assert_eq!(
            at_9600.inter_frame_interval(),
            Ok(Duration::from_nanos(4_010_416))
        );

        let no_parity = SerialConfig {
            parity: Parity::None,
            ..at_9600
        };
        assert_eq!(
            no_parity.inter_frame_interval(),
            Ok(Duration::from_nanos(3_645_833))
        );

        let two_stop = SerialConfig {
            stop_bits: StopBits::Two,
            ..at_9600
        };
        assert_eq!(
            two_stop.inter_frame_interval(),
            Ok(Duration::from_nanos(4_375_000))
        );
    }

    #[test]
    /// TR-R-011 — at 19200 baud the rule still computes (2.0052 ms), and above
    /// it the interval is fixed at 1.75 ms rather than shrinking with the baud
    /// rate.
    fn ut_interframe_interval_is_fixed_above_19200() {
        assert_eq!(
            SerialConfig::default().inter_frame_interval(),
            Ok(Duration::from_nanos(2_005_208))
        );
        for baud_rate in [19_201, 38_400, 57_600, 115_200] {
            let config = SerialConfig {
                baud_rate,
                ..SerialConfig::default()
            };
            assert_eq!(
                config.inter_frame_interval(),
                Ok(Duration::from_micros(1_750)),
                "{baud_rate} baud"
            );
        }
    }

    #[test]
    /// TR-R-031 — a baud rate of zero yields no character time; it is a
    /// configuration error, not a division by zero.
    fn ut_zero_baud_rate_is_a_configuration_error() {
        let config = SerialConfig {
            baud_rate: 0,
            ..SerialConfig::default()
        };
        assert_eq!(
            config.inter_frame_interval(),
            Err(Error::Configuration { field: "baud_rate" })
        );
    }

    #[cfg(feature = "rs485")]
    #[test]
    /// TR-R-052 — `SerialConfig::default().rs485` is `None`: requesting no
    /// RS-485 configuration issues no ioctl.
    fn ut_rs485_field_default_is_none() {
        assert_eq!(SerialConfig::default().rs485, None);
    }

    #[cfg(feature = "serde")]
    #[test]
    /// TR-R-058 — `SerialConfig` round-trips through JSON.
    fn ut_serial_config_serde_roundtrip() {
        let config = SerialConfig::default();
        let text = serde_json::to_string(&config).expect("serializes");
        #[cfg(feature = "rs485")]
        assert_eq!(
            text,
            r#"{"baud_rate":19200,"data_bits":"Eight","parity":"Even","stop_bits":"One","flow_control":"None","rs485":null}"#
        );
        #[cfg(not(feature = "rs485"))]
        assert_eq!(
            text,
            r#"{"baud_rate":19200,"data_bits":"Eight","parity":"Even","stop_bits":"One","flow_control":"None"}"#
        );
        assert_eq!(
            serde_json::from_str::<SerialConfig>(&text).expect("deserializes"),
            config
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    /// TR-R-058 — a deserialized `SerialConfig` with a zero baud rate is
    /// accepted exactly as direct construction accepts it: the configuration
    /// error fires the first time the value is used, not at deserialize time.
    fn ut_serial_config_zero_baud_deserializes_without_error() {
        let config = SerialConfig {
            baud_rate: 0,
            ..SerialConfig::default()
        };
        let text = serde_json::to_string(&config).expect("serializes");
        let deserialized: SerialConfig =
            serde_json::from_str(&text).expect("deserializing a zero baud rate does not fail");
        assert_eq!(
            deserialized.inter_frame_interval(),
            Err(Error::Configuration { field: "baud_rate" })
        );
    }

    #[cfg(all(feature = "serde", feature = "rs485"))]
    #[test]
    /// TR-R-059 — `Rs485Config` round-trips through JSON, with both delays
    /// under field names suffixed `_ns` in whole nanoseconds, so a delay the
    /// ioctl would later truncate still survives the round trip unchanged.
    fn ut_rs485_config_serde_roundtrip() {
        let config = Rs485Config {
            rts_on_send: RtsPolarity::High,
            delay_before_send: Duration::from_millis(10),
            delay_after_send: Duration::from_millis(20),
        };
        let text = serde_json::to_string(&config).expect("serializes");
        assert_eq!(
            text,
            r#"{"rts_on_send":"High","delay_before_send":{"secs":0,"nanos":10000000},"delay_after_send":{"secs":0,"nanos":20000000}}"#
        );
        assert_eq!(
            serde_json::from_str::<Rs485Config>(&text).expect("deserializes"),
            config
        );

        // TR-R-056 truncates at the ioctl, not here: the configured value is
        // preserved exactly, and only the kernel sees whole milliseconds.
        let fine = Rs485Config {
            delay_before_send: Duration::from_nanos(1_500_001),
            ..config
        };
        assert_eq!(
            serde_json::from_str::<Rs485Config>(&serde_json::to_string(&fine).expect("serializes"))
                .expect("deserializes"),
            fine
        );
    }
}
