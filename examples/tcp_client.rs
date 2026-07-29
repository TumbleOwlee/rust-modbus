//! A Modbus TCP client: connect, read holding registers, write one, read back.
//!
//! ```sh
//! # against the server example in this directory
//! cargo run --example interop_server -- 127.0.0.1:5030 30
//! cargo run --example tcp_client -- 127.0.0.1:5030 1 0 4
//! ```
//!
//! Arguments, all optional: the device's address, the unit identifier, the first
//! register address, and how many registers to read. Modbus calls the initiating
//! side the *master* or *client*; it is the side that asks, and every exchange
//! is one request and one reply.

use std::net::SocketAddr;
use std::time::Duration;

use rust_modbus::{
    Address, Client, ClientConfig, Error, Quantity, RegisterValue, TcpConfig, UnitId, connect_tcp,
};

#[tokio::main]
async fn main() -> rust_modbus::Result<()> {
    let mut args = std::env::args().skip(1);
    let address: SocketAddr = args
        .next()
        .unwrap_or_else(|| "127.0.0.1:5030".to_owned())
        .parse()
        .expect("the first argument is a socket address");
    // The unit identifier addresses a device *behind* the socket. On a plain TCP
    // device it is often 1 and ignored; on a serial gateway it selects the slave.
    let unit = UnitId(parse(args.next(), 1));
    let start = Address(parse(args.next(), 0));
    let count = Quantity(parse(args.next(), 4));

    // Connecting is separate from constructing the client: a `Client` is built
    // from a transport that is already established, so the crate never hides a
    // reconnect you did not ask for. It also does not retry — if a request
    // fails, the policy for what to do next is yours.
    let transport = connect_tcp(
        address,
        TcpConfig {
            connect_timeout: Duration::from_secs(5),
            ..TcpConfig::default()
        },
    )
    .await?;
    println!("connected to {address}");

    // One in-flight request at a time, enforced by `&mut self` rather than by a
    // runtime check. Give each connection its own client to overlap requests.
    let mut client = Client::with_config(
        transport,
        ClientConfig {
            response_timeout: Duration::from_secs(1),
        },
    );

    // Function code 3. Values come back in wire order, one per register.
    let registers = client
        .read_holding_registers(unit, start, count)
        .await
        .map_err(describe)?;
    for (offset, RegisterValue(value)) in registers.iter().copied().enumerate() {
        println!(
            "holding[{}] = {value} (0x{value:04X})",
            start.0 as usize + offset
        );
    }

    // Function code 6. The device echoes the address and value; the client does
    // not hand the echo back, because comparing it is not the caller's job —
    // a mismatch is the device misbehaving, not a value to branch on.
    let written = RegisterValue(0x1234);
    println!("writing {} = 0x{:04X}", start.0, written.0);
    client
        .write_single_register(unit, start, written)
        .await
        .map_err(describe)?;

    let read_back = client
        .read_holding_registers(unit, start, Quantity(1))
        .await
        .map_err(describe)?;
    println!("read back: {read_back:?}");

    Ok(())
}

/// Parse one optional argument, falling back to a default.
fn parse<T>(arg: Option<String>, default: T) -> T
where
    T: std::str::FromStr,
    T::Err: std::fmt::Debug,
{
    arg.map_or(default, |value| {
        value.parse().expect("every numeric argument parses")
    })
}

/// Print an explanation for the failures a caller most often meets, then pass
/// the error on unchanged.
///
/// A device *refusing* a request is not an I/O failure: it answers with a Modbus
/// exception, which surfaces as [`Error::Exception`] and usually means the
/// address is outside the device's map rather than that anything is broken.
fn describe(error: Error) -> Error {
    match &error {
        Error::Exception {
            function,
            exception,
        } => println!("the device refused {function:?}: {exception:?}"),
        Error::Timeout { what } => println!("no {what} within the configured timeout"),
        other => println!("failed: {other}"),
    }
    error
}
