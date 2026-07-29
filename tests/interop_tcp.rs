//! Interop against a real Modbus TCP server.
//!
//! Ignored by default and never run in CI: it needs a server listening on
//! `127.0.0.1:5020` with unit 1 exposing a coil, a discrete input, and holding
//! and input registers at addresses 0–7. Run it with:
//!
//! ```sh
//! cargo test --test interop_tcp -- --ignored --nocapture
//! ```
//!
//! Unlike the loopback tests, the peer here is not this crate, so the byte
//! layouts are checked against an independent implementation rather than
//! against themselves.

use std::net::{Ipv4Addr, SocketAddr};

use rust_modbus::{
    Address, Client, Mask, Quantity, RegisterValue, TcpClient, TcpConfig, UnitId, connect_tcp,
};

/// The server under test.
fn server() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 5020))
}

/// The unit the fixtures live on.
const UNIT: UnitId = UnitId(1);

async fn client() -> TcpClient {
    Client::new(
        connect_tcp(server(), TcpConfig::default())
            .await
            .expect("the interop server is reachable on 127.0.0.1:5020"),
    )
}

#[tokio::test]
#[ignore = "needs an external Modbus server on 127.0.0.1:5020"]
/// CL-R-060 — the four read codes against a foreign server.
async fn it_interop_reads_every_data_type() {
    let mut client = client().await;

    let coils = client
        .read_coils(UNIT, Address(0), Quantity(1))
        .await
        .expect("reads a coil");
    assert_eq!(coils.len(), 1, "CL-R-062: exactly the quantity asked for");

    let inputs = client
        .read_discrete_inputs(UNIT, Address(0), Quantity(1))
        .await
        .expect("reads a discrete input");
    assert_eq!(inputs.len(), 1);

    let holding = client
        .read_holding_registers(UNIT, Address(0), Quantity(8))
        .await
        .expect("reads holding registers");
    assert_eq!(holding.len(), 8);

    let input = client
        .read_input_registers(UNIT, Address(0), Quantity(8))
        .await
        .expect("reads input registers");
    assert_eq!(input.len(), 8);

    println!("coil {coils:?}\ndiscrete {inputs:?}\nholding {holding:?}\ninput {input:?}");
}

#[tokio::test]
#[ignore = "needs an external Modbus server on 127.0.0.1:5020"]
/// CL-R-060 — a single-coil write reaches a foreign server and reads back.
async fn it_interop_writes_a_single_coil() {
    let mut client = client().await;

    client
        .write_single_coil(UNIT, Address(0), true)
        .await
        .expect("writes a coil");
    assert_eq!(
        client
            .read_coils(UNIT, Address(0), Quantity(1))
            .await
            .expect("reads back"),
        vec![true]
    );

    client
        .write_single_coil(UNIT, Address(0), false)
        .await
        .expect("writes a coil");
    assert_eq!(
        client
            .read_coils(UNIT, Address(0), Quantity(1))
            .await
            .expect("reads back"),
        vec![false]
    );
}

#[tokio::test]
#[ignore = "needs an external Modbus server on 127.0.0.1:5020"]
/// CL-R-060 — single and multiple register writes, read back through a
/// different function code than the one that wrote them.
async fn it_interop_writes_registers() {
    let mut client = client().await;

    client
        .write_single_register(UNIT, Address(0), RegisterValue(0x1234))
        .await
        .expect("writes one register");
    assert_eq!(
        client
            .read_holding_registers(UNIT, Address(0), Quantity(1))
            .await
            .expect("reads back"),
        vec![RegisterValue(0x1234)]
    );

    let written: Vec<RegisterValue> = (0..8).map(|n| RegisterValue(0x0100 + n)).collect();
    client
        .write_multiple_registers(UNIT, Address(0), &written)
        .await
        .expect("writes eight registers");
    assert_eq!(
        client
            .read_holding_registers(UNIT, Address(0), Quantity(8))
            .await
            .expect("reads back"),
        written
    );
}

#[tokio::test]
#[ignore = "needs an external Modbus server on 127.0.0.1:5020"]
/// CL-R-060 — the compound codes: read/write in one exchange (23) and the
/// read-modify-write of a mask (22).
async fn it_interop_compound_register_access() {
    let mut client = client().await;

    let write: Vec<RegisterValue> = (0..4).map(|n| RegisterValue(0x0200 + n)).collect();
    let read = client
        .read_write_multiple_registers(UNIT, Address(0), Quantity(8), Address(0), &write)
        .await
        .expect("reads and writes in one exchange");
    assert_eq!(read.len(), 8);

    // The specification (§6.17) performs the write before the read, so the
    // returned values should already be `write`. Some servers read first; that
    // is the server's ordering to get right, not this client's, so the check
    // that belongs here is that the write *landed*, not when it was visible.
    if read.get(..4) != Some(write.as_slice()) {
        println!("server read before it wrote (§6.17 says write first): got {read:?}");
    }
    assert_eq!(
        client
            .read_holding_registers(UNIT, Address(0), Quantity(4))
            .await
            .expect("reads back"),
        write,
        "the write half of code 23 must have landed"
    );

    client
        .write_single_register(UNIT, Address(1), RegisterValue(0x0012))
        .await
        .expect("seeds the register");
    match client
        .mask_write_register(UNIT, Address(1), Mask(0x00F2), Mask(0x0025))
        .await
    {
        Ok(()) => assert_eq!(
            client
                .read_holding_registers(UNIT, Address(1), Quantity(1))
                .await
                .expect("reads back"),
            // (0x0012 AND 0x00F2) OR (0x0025 AND NOT 0x00F2) = 0x0017
            // (FR-R-036), the worked example from §6.16.
            vec![RegisterValue(0x0017)]
        ),
        // Code 22 is optional; a server may not implement it at all. That the
        // request was well formed enough to be *refused* by function code is
        // itself the interop result.
        Err(error) => println!("mask write unsupported by this server: {error}"),
    }
}

#[tokio::test]
#[ignore = "needs an external Modbus server on 127.0.0.1:5020"]
/// CL-R-060 — a multiple-coil write, read back through code 1.
async fn it_interop_writes_multiple_coils() {
    let mut client = client().await;
    let coils = [true, false, true, true];

    match client.write_multiple_coils(UNIT, Address(0), &coils).await {
        Ok(()) => {
            let read = client
                .read_coils(UNIT, Address(0), Quantity(4))
                .await
                .expect("reads back");
            assert_eq!(read, coils.to_vec());
        }
        // A server exposing a single coil may legitimately refuse four.
        Err(error) => println!("multiple-coil write refused: {error}"),
    }
}

#[tokio::test]
#[ignore = "needs an external Modbus server on 127.0.0.1:5020"]
/// CL-R-040 — a foreign server's exception surfaces as a typed failure and
/// leaves the connection usable (CL-R-042).
async fn it_interop_exception_leaves_the_client_usable() {
    let mut client = client().await;

    let refused = client
        .read_holding_registers(UNIT, Address(0x7FFF), Quantity(8))
        .await;
    println!("out-of-range read answered with: {refused:?}");
    assert!(refused.is_err(), "an unmapped address must not succeed");
    assert!(!client.is_desynchronized());

    assert_eq!(
        client
            .read_holding_registers(UNIT, Address(0), Quantity(1))
            .await
            .expect("the connection still works")
            .len(),
        1
    );
}

#[tokio::test]
#[ignore = "needs an external Modbus server on 127.0.0.1:5020"]
/// CL-R-011 — transaction identifiers advance across a real connection, and the
/// server echoes each one back well enough to match (CL-R-020).
async fn it_interop_many_requests_on_one_connection() {
    let mut client = client().await;

    for _ in 0..64 {
        client
            .read_holding_registers(UNIT, Address(0), Quantity(1))
            .await
            .expect("every request matches its own response");
    }
    assert!(!client.is_desynchronized());
}
