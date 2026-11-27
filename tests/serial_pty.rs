//! RTU clients over a pseudo-terminal pair: the OS input queue is really
//! cleared (TR-R-108), and a response timeout leaves the client usable
//! (CL-R-116). A pty is a virtual pair, not serial hardware.
#![cfg(all(unix, feature = "rtu"))]

use std::io::{Read, Write};
use std::thread::JoinHandle;
use std::time::Duration;

use nix::fcntl::OFlag;
use nix::pty::{grantpt, posix_openpt, ptsname_r, unlockpt};
use rust_modbus::{
    Address, Client, ClientConfig, ClientState, Error, Framing, Quantity, RegisterValue,
    ResponsePdu, Rtu, SerialConfig, UnitId, open_serial,
};

const TIMEOUT: Duration = Duration::from_millis(200);

fn registers(values: [u16; 3]) -> Vec<u8> {
    Rtu::encode_response(
        &UnitId(0x11),
        &ResponsePdu::ReadHoldingRegisters {
            registers: values.into_iter().map(RegisterValue).collect(),
        },
    )
    .expect("the response encodes")
}

/// A device that answers the first request after the timeout and the second
/// in time, on the master end of a fresh pty. Returns the slave's path.
fn late_then_prompt_device() -> (String, JoinHandle<()>) {
    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
    grantpt(&master).expect("grantpt");
    unlockpt(&master).expect("unlockpt");
    let slave_path = ptsname_r(&master).expect("ptsname_r");
    let master_fd: std::os::fd::OwnedFd = master.into();
    let mut master: std::fs::File = master_fd.into();
    let device = std::thread::spawn(move || {
        let mut request = [0u8; 8];
        master.read_exact(&mut request).expect("first request");
        std::thread::sleep(TIMEOUT * 2);
        master.write_all(&registers([1, 2, 3])).expect("late reply");
        master.read_exact(&mut request).expect("second request");
        master.write_all(&registers([4, 5, 6])).expect("reply");
        std::thread::sleep(Duration::from_millis(100));
    });
    (slave_path, device)
}

#[tokio::test]
/// CL-R-116, CL-R-031, CL-R-024, CL-R-033, CL-E-009 — over a real serial
/// stream a timed-out request leaves the client usable, and the late reply
/// that reached the OS queue is discarded before the next request.
async fn it_rtu_client_over_serial_survives_a_timeout() {
    let (slave_path, device) = late_then_prompt_device();
    let transport = open_serial::<Rtu>(&slave_path, SerialConfig::default()).expect("opens");
    let mut client = Client::<_, Rtu>::with_config(
        transport,
        ClientConfig {
            response_timeout: TIMEOUT,
        },
    );

    assert_eq!(
        client
            .read_holding_registers(UnitId(0x11), Address(0x006B), Quantity(3))
            .await,
        Err(Error::Timeout { what: "response" })
    );
    assert_eq!(client.state(), ClientState::Unanswered);

    tokio::time::sleep(TIMEOUT * 3).await;
    assert_eq!(
        client
            .read_holding_registers(UnitId(0x11), Address(0x006B), Quantity(3))
            .await,
        Ok(vec![RegisterValue(4), RegisterValue(5), RegisterValue(6)])
    );
    device.join().expect("device thread");
}

#[cfg(feature = "sync")]
#[test]
/// CL-E-042, TR-R-110 — the blocking RTU client opens through `open_serial`
/// and stays usable after a response timeout.
fn it_sync_rtu_client_over_serial_survives_a_timeout() {
    let (slave_path, device) = late_then_prompt_device();
    let mut client = rust_modbus::SyncRtuClient::open(
        &slave_path,
        SerialConfig::default(),
        ClientConfig {
            response_timeout: TIMEOUT,
        },
    )
    .expect("opens");

    assert_eq!(
        client.read_holding_registers(UnitId(0x11), Address(0x006B), Quantity(3)),
        Err(Error::Timeout { what: "response" })
    );
    std::thread::sleep(TIMEOUT * 3);
    assert_eq!(
        client.read_holding_registers(UnitId(0x11), Address(0x006B), Quantity(3)),
        Ok(vec![RegisterValue(4), RegisterValue(5), RegisterValue(6)])
    );
    device.join().expect("device thread");
}
