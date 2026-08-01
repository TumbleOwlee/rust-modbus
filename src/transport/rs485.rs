//! RS-485 kernel direction control: struct assembly and the `TIOCSRS485`
//! ioctl (TR-R-050).
//!
//! Its own file because it isolates every FFI concern — the kernel struct
//! layout, the ioctl number, the unsafe call itself — into one small,
//! independently auditable module. Everything except the ioctl call in the
//! next stage is pure computation, testable on any host.

use core::time::Duration;

use crate::error::{Error, Result};
use crate::transport::serial::{Rs485Config, RtsPolarity};

/// RS-485 mode enabled (`SER_RS485_ENABLED`).
const SER_RS485_ENABLED: u32 = 1 << 0;
/// RTS driven high, rather than low, while transmitting (`SER_RS485_RTS_ON_SEND`).
const SER_RS485_RTS_ON_SEND: u32 = 1 << 1;
/// RTS driven high, rather than low, after transmitting (`SER_RS485_RTS_AFTER_SEND`).
const SER_RS485_RTS_AFTER_SEND: u32 = 1 << 2;

/// The kernel's `struct serial_rs485`, as declared in
/// `include/uapi/linux/serial.h`:
///
/// ```c
/// struct serial_rs485 {
///     __u32   flags;
///     __u32   delay_rts_before_send;
///     __u32   delay_rts_after_send;
///     union {
///         __u32   padding[5];
///         struct {
///             __u8    addr_recv;
///             __u8    addr_dest;
///             __u8    addr_flags;
///             __u8    padding1;
///             __u32   padding2[4];
///         };
///     };
/// };
/// ```
///
/// Three `u32` fields (12 bytes) followed by a 20-byte union, for 32 bytes
/// total. This crate never reads the address-extension fields the union's
/// second arm names, so the union is represented as the plain `[u32; 5]`
/// padding arm — same layout (both arms of the union are 20 bytes; `repr(C)`
/// gives the struct itself no padding between three `u32`s and a `[u32; 5]`,
/// since every field shares the same 4-byte alignment), no field it would
/// otherwise need to zero individually (TR-R-050).
// `build` and `KernelRs485` are wired into `open_serial` by the next stage,
// which issues the ioctl this struct is assembled for; until then only the
// tests below construct them.
#[allow(dead_code)]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct KernelRs485 {
    pub(crate) flags: u32,
    pub(crate) delay_rts_before_send: u32,
    pub(crate) delay_rts_after_send: u32,
    padding: [u32; 5],
}

/// Assemble the kernel struct from this crate's own configuration (TR-R-050).
///
/// Pure: no syscall, so it is testable on any host regardless of `target_os`.
///
/// # Errors
///
/// Fails with [`Error::Configuration`] if either delay's millisecond count
/// does not fit in a `u32` (TR-R-056).
#[allow(dead_code)]
pub(crate) fn build(config: &Rs485Config) -> Result<KernelRs485> {
    let mut flags = SER_RS485_ENABLED;
    // TR-R-057: the after-send level is always the complement of the
    // on-send level, so the two flag bits are set from one polarity.
    match config.rts_on_send {
        RtsPolarity::High => flags |= SER_RS485_RTS_ON_SEND,
        RtsPolarity::Low => flags |= SER_RS485_RTS_AFTER_SEND,
    }
    Ok(KernelRs485 {
        flags,
        delay_rts_before_send: truncate_to_millis(config.delay_before_send, "delay_before_send")?,
        delay_rts_after_send: truncate_to_millis(config.delay_after_send, "delay_after_send")?,
        padding: [0; 5],
    })
}

/// Truncate a [`Duration`] to whole milliseconds, since the kernel field's own
/// resolution is one millisecond (TR-R-056).
///
/// # Errors
///
/// Fails with [`Error::Configuration`] if the millisecond count does not fit
/// in a `u32`, rather than wrapping.
fn truncate_to_millis(delay: Duration, field: &'static str) -> Result<u32> {
    u32::try_from(delay.as_millis()).map_err(|_| Error::Configuration { field })
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::*;
    use crate::error::Error;
    use crate::transport::serial::{Rs485Config, RtsPolarity};

    #[test]
    /// TR-R-050 — the assembled struct enables RS-485 mode and carries the
    /// requested delays, in milliseconds, in the kernel's own fields.
    fn ut_rs485_flags_and_delays_assembled() {
        let config = Rs485Config {
            rts_on_send: RtsPolarity::High,
            delay_before_send: Duration::from_millis(3),
            delay_after_send: Duration::from_millis(7),
        };
        let kernel = build(&config).expect("valid delays");
        assert_ne!(kernel.flags & SER_RS485_ENABLED, 0, "mode must be enabled");
        assert_eq!(kernel.delay_rts_before_send, 3);
        assert_eq!(kernel.delay_rts_after_send, 7);
    }

    #[test]
    /// TR-R-057 — the after-send RTS level is always the on-send level's
    /// complement, for both polarities, since the crate offers no
    /// independently configurable after-send flag.
    fn ut_rs485_after_send_is_complement_of_on_send() {
        let high = build(&Rs485Config {
            rts_on_send: RtsPolarity::High,
            delay_before_send: Duration::ZERO,
            delay_after_send: Duration::ZERO,
        })
        .expect("valid delays");
        assert_ne!(high.flags & SER_RS485_RTS_ON_SEND, 0);
        assert_eq!(high.flags & SER_RS485_RTS_AFTER_SEND, 0);

        let low = build(&Rs485Config {
            rts_on_send: RtsPolarity::Low,
            delay_before_send: Duration::ZERO,
            delay_after_send: Duration::ZERO,
        })
        .expect("valid delays");
        assert_eq!(low.flags & SER_RS485_RTS_ON_SEND, 0);
        assert_ne!(low.flags & SER_RS485_RTS_AFTER_SEND, 0);
    }

    #[test]
    /// TR-R-056 — a delay finer than a millisecond truncates to whole
    /// milliseconds at the ioctl boundary, since the kernel field has no finer
    /// resolution.
    fn ut_rs485_delay_truncated_to_milliseconds() {
        let config = Rs485Config {
            rts_on_send: RtsPolarity::High,
            delay_before_send: Duration::from_micros(2_999),
            delay_after_send: Duration::from_micros(1_001),
        };
        let kernel = build(&config).expect("valid delays");
        assert_eq!(kernel.delay_rts_before_send, 2);
        assert_eq!(kernel.delay_rts_after_send, 1);
    }

    #[test]
    /// TR-R-056 — a `Duration` whose millisecond count does not fit in a
    /// `u32` is a configuration error, not a silent wraparound.
    fn ut_rs485_delay_overflow_is_configuration_error() {
        let too_long = Duration::from_millis(u64::from(u32::MAX) + 1);
        let config = Rs485Config {
            rts_on_send: RtsPolarity::High,
            delay_before_send: too_long,
            delay_after_send: Duration::ZERO,
        };
        assert_eq!(
            build(&config),
            Err(Error::Configuration {
                field: "delay_before_send"
            })
        );
    }
}
