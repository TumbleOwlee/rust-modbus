//! A Modbus RTU client over a real serial port. **Needs hardware.**
//!
//! Unlike every test in this crate, this example opens an actual port, so it
//! needs a serial device — a USB-RS485 adapter with a Modbus slave on the other
//! end, or a virtual pair from `socat`:
//!
//! ```sh
//! socat -d -d pty,raw,echo=0 pty,raw,echo=0    # prints two /dev/pts/N paths
//! cargo run --example rtu_client --features rtu -- /dev/ttyUSB0 1 0 4
//! ```
//!
//! Arguments, all optional: the port path, the unit identifier, the first
//! register address, and how many registers to read.
//!
//! Opening a port is the only thing the `rtu` feature gates, and it is off by
//! default so a TCP-only consumer acquires no serial dependency. RTU *framing*
//! is always available — `Client<S, Rtu>` over any duplex stream works with the
//! feature off, which is how this crate's own tests exercise RTU without
//! hardware.

#[cfg(feature = "rtu")]
use rust_modbus::{
    Address, Client, ClientConfig, Quantity, RegisterValue, Rtu, SerialConfig, UnitId, open_serial,
};

#[cfg(feature = "rtu")]
#[tokio::main]
async fn main() -> rust_modbus::Result<()> {
    use std::time::Duration;

    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "/dev/ttyUSB0".to_owned());
    let unit = UnitId(parse(args.next(), 1));
    let start = Address(parse(args.next(), 0));
    let count = Quantity(parse(args.next(), 4));

    // The Modbus serial-line defaults: 19200 baud, 8 data bits, even parity, one
    // stop bit. Real installations deviate constantly — match the device, not
    // the standard, and note that every node on the bus must agree.
    let config = SerialConfig::default();

    // How long the line must stay quiet before a frame counts as finished. RTU
    // frames have no start or end delimiter, so silence *is* the delimiter: the
    // standard puts it at 3.5 character times, derived here from the port
    // settings. It is why RTU cannot simply be run over a stream that has no
    // timing, and why a wrong baud rate corrupts framing and not just bytes.
    println!(
        "inter-frame silence at {} baud: {:?}",
        config.baud_rate,
        config.inter_frame_interval()?
    );

    // Generic over the framing, because one serial port carries RTU or ASCII at
    // the operator's choice over identical port settings. `Rtu` is the binary
    // framing with a CRC; `Ascii` is the hex-and-`:`-delimited one.
    let transport = open_serial::<Rtu>(&path, config)?;
    println!("opened {path}");

    let mut client = Client::with_config(
        transport,
        ClientConfig {
            // A serial device answering a poll is slower than a socket, and one
            // slow device delays everything else on the bus.
            response_timeout: Duration::from_millis(500),
        },
    );

    let registers = client.read_holding_registers(unit, start, count).await?;
    for (offset, RegisterValue(value)) in registers.iter().copied().enumerate() {
        println!(
            "holding[{}] = {value} (0x{value:04X})",
            start.0 as usize + offset
        );
    }

    // Unit 0 is the broadcast address: every slave acts and none answers, so the
    // client returns as soon as the bytes are out and cannot tell you whether it
    // worked. Reads may not be broadcast at all — there would be no one reply to
    // return — and this crate refuses them rather than hanging until the timeout.
    client
        .write_single_register(UnitId(0), start, RegisterValue(0))
        .await?;
    println!("broadcast write sent; no reply is expected");

    Ok(())
}

/// Parse one optional argument, falling back to a default.
#[cfg(feature = "rtu")]
fn parse<T>(arg: Option<String>, default: T) -> T
where
    T: std::str::FromStr,
    T::Err: std::fmt::Debug,
{
    arg.map_or(default, |value| {
        value.parse().expect("every numeric argument parses")
    })
}

#[cfg(not(feature = "rtu"))]
fn main() {
    eprintln!("this example opens a serial port; rebuild with --features rtu");
}
