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

/// `TIOCSRS485`, as declared in `include/uapi/asm-generic/ioctls.h`. Defined
/// by neither `libc` nor `nix`, so this crate declares it itself. `libc::Ioctl`
/// is the request parameter's actual type for `libc::ioctl` on this target —
/// `c_ulong` on glibc, `c_int` on musl — so the constant is typed with it
/// rather than a fixed integer width, and no cast is left to hope about at the
/// call site (TR-R-050).
#[cfg(target_os = "linux")]
const TIOCSRS485: libc::Ioctl = 0x542F;

/// Issue `TIOCSRS485` against an already-open file descriptor (TR-R-050).
///
/// Split out from [`apply`] so a test can exercise the real syscall and its
/// error mapping against a file that is not a serial port at all — `/dev/null`
/// reliably answers `ENOTTY` — without needing a `SerialStream`.
///
/// # Errors
///
/// Maps `ENOTTY`, `EINVAL`, and `ENOSYS` — every errno a driver or a
/// non-serial file uses to say "this ioctl is not implemented" — to
/// [`Error::Rs485Unsupported`] (TR-R-054). Any other errno surfaces as
/// [`Error::Io`].
#[cfg(target_os = "linux")]
fn issue_ioctl(fd: std::os::unix::io::RawFd, kernel: &KernelRs485) -> Result<()> {
    // SAFETY: `fd` is a valid, open file descriptor for the duration of this
    // call (borrowed from the caller, which owns it). `kernel` is a valid,
    // fully initialized `KernelRs485` whose layout matches the kernel's
    // `struct serial_rs485` (see its doc comment), and `TIOCSRS485` is the
    // ioctl request the kernel defines for writing exactly that struct. This
    // is the crate's only unsafe code (TR-R-055).
    #[allow(unsafe_code)]
    let result = unsafe { libc::ioctl(fd, TIOCSRS485, kernel) };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ENOTTY) | Some(libc::EINVAL) | Some(libc::ENOSYS) => {
            Err(Error::Rs485Unsupported)
        }
        _ => Err(error.into()),
    }
}

/// Apply RS-485 configuration to an already-open serial port (TR-R-053).
///
/// # Errors
///
/// Fails with [`Error::Configuration`] if a delay does not fit in the
/// kernel's field (TR-R-056), or with [`Error::Rs485Unsupported`] if the
/// driver refuses the ioctl (TR-R-054).
#[cfg(target_os = "linux")]
pub(crate) fn apply(port: &tokio_serial::SerialStream, config: &Rs485Config) -> Result<()> {
    use std::os::unix::io::AsRawFd;

    let kernel = build(config)?;
    issue_ioctl(port.as_raw_fd(), &kernel)
}

/// Off Linux, RS-485 kernel direction control does not exist, so it always
/// fails (TR-R-054). Split from [`apply`] so a test can exercise it without
/// an actual open port — nothing about it depends on one.
#[cfg(not(target_os = "linux"))]
fn unsupported_off_linux() -> Result<()> {
    Err(Error::Rs485Unsupported)
}

/// Apply RS-485 configuration to an already-open serial port (TR-R-053).
///
/// Off Linux this is the whole of the story: no `libc` dependency, no unsafe
/// code, no ioctl, just [`Error::Rs485Unsupported`] (TR-R-054).
///
/// # Errors
///
/// Always fails off Linux; see [`unsupported_off_linux`].
#[cfg(not(target_os = "linux"))]
pub(crate) fn apply(_port: &tokio_serial::SerialStream, _config: &Rs485Config) -> Result<()> {
    unsupported_off_linux()
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

    #[cfg(target_os = "linux")]
    #[test]
    /// TR-R-054 — `/dev/null` really rejects `TIOCSRS485` with `ENOTTY`, and
    /// that is mapped to `Rs485Unsupported`, not a generic I/O error. Exercises
    /// the real unsafe ioctl call and the real error mapping.
    fn ut_rs485_ioctl_maps_enotty_to_unsupported() {
        use std::fs::File;
        use std::os::unix::io::AsRawFd;

        let file = File::open("/dev/null").expect("/dev/null always exists");
        let kernel = build(&Rs485Config {
            rts_on_send: RtsPolarity::High,
            delay_before_send: Duration::ZERO,
            delay_after_send: Duration::ZERO,
        })
        .expect("valid delays");
        assert_eq!(
            issue_ioctl(file.as_raw_fd(), &kernel),
            Err(Error::Rs485Unsupported)
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    /// TR-R-054 — off Linux the stub reports unsupported without touching
    /// `libc` at all, so this is the only build where an actual RS-485 device
    /// is unreachable regardless of a driver's own support.
    fn ut_rs485_unsupported_stub_on_non_linux() {
        assert_eq!(unsupported_off_linux(), Err(Error::Rs485Unsupported));
    }
}
