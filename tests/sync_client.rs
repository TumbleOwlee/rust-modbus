//! The blocking client (CL-R-070 … CL-R-079).
//!
//! These are plain `#[test]` functions, not `#[tokio::test]`: the whole point of
//! the blocking client is that the calling thread has no runtime, so a test that
//! provided one would not be testing it. Where a runtime *is* needed — to run a
//! responder, or to prove CL-R-075 refuses a nested call — it is created
//! explicitly and confined to its own thread.
//!
//! Every listener binds port 0 and reads the assigned port back, per the testing
//! conventions in `AGENTS.md`.

#![cfg(feature = "sync")]

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::mpsc;
use std::thread;

use rust_modbus::{
    Address, ClientConfig, Error, Quantity, RegisterValue, RequestPdu, ResponsePdu, SyncClient,
    SyncTcpClient, TcpConfig, TcpListener, UnitId,
};

/// Run a responder on its own thread, with its own runtime, and hand back the
/// address it bound.
///
/// This is what makes the test thread runtime-free: the server needs a runtime,
/// the blocking client must not have one, so the two cannot share a thread.
/// Answers `count` requests with `reply`, then drops the connection.
fn serve_on_a_thread(
    count: usize,
    reply: fn(&RequestPdu) -> ResponsePdu,
) -> (SocketAddr, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("the responder's runtime");
        runtime.block_on(async move {
            let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
                .await
                .expect("binds");
            tx.send(listener.local_addr().expect("reports its address"))
                .expect("the test thread is waiting");
            let (mut transport, _peer) = listener.accept().await.expect("accepts");
            for _ in 0..count {
                let (header, request) = transport.recv_request().await.expect("receives");
                let response = reply(&request);
                transport
                    .send_response(&header, &response)
                    .await
                    .expect("responds");
            }
        });
    });
    (
        rx.recv().expect("the responder reports its address"),
        handle,
    )
}

#[test]
/// CL-R-073, CL-R-076 — a blocking client is constructed from an address by a
/// thread that owns no runtime, and owns the one it needs itself. A caller with
/// a runtime would have used `Client`; this test would not compile against a
/// constructor that demanded one.
fn it_sync_connect_needs_no_runtime() {
    let (address, responder) = serve_on_a_thread(0, |_| unreachable!("no request is sent"));

    let client = SyncTcpClient::connect(address, TcpConfig::default(), ClientConfig::default());

    assert!(
        client.is_ok(),
        "expected a connected client, got {client:?}"
    );
    drop(client);
    responder.join().expect("the responder finishes");
}

#[test]
/// CL-R-076 — the serial constructor reports a device that is not there as the
/// platform's I/O failure, the same way `open_serial` does. No hardware, which
/// is the only serial behavior CI can exercise (NF-R-024).
#[cfg(feature = "rtu")]
fn it_sync_open_reports_a_missing_device() {
    use rust_modbus::{Rtu, SerialConfig, SerialStream};

    let opened = SyncClient::<SerialStream, Rtu>::open(
        "/dev/rust-modbus-no-such-device",
        SerialConfig::default(),
        ClientConfig::default(),
    );

    assert!(
        matches!(opened, Err(Error::Io { .. })),
        "expected an I/O error, got {opened:?}"
    );
}

#[test]
/// CL-R-071, CL-R-072 — a raw request completes over a real socket from a thread
/// with no runtime, and the response arrives as received. This is the whole
/// bridge working end to end: the guard, the owned runtime, and the delegation.
fn it_sync_call_completes_an_exchange() {
    let (address, responder) = serve_on_a_thread(1, |_| ResponsePdu::ReadHoldingRegisters {
        registers: vec![RegisterValue(0x022B), RegisterValue(0x0000)],
    });

    let mut client = SyncTcpClient::connect(address, TcpConfig::default(), ClientConfig::default())
        .expect("connects");

    assert_eq!(
        client.call(
            UnitId(0x11),
            RequestPdu::ReadHoldingRegisters {
                address: Address(0x006B),
                quantity: Quantity(2),
            }
        ),
        Ok(Some(ResponsePdu::ReadHoldingRegisters {
            registers: vec![RegisterValue(0x022B), RegisterValue(0x0000)],
        }))
    );
    responder.join().expect("the responder finishes");
}

#[test]
/// CL-R-077 — two calls back to back with no sleep between them both succeed.
/// A facade that did not settle the exchange before returning would need the
/// caller to pause; this pins that it does not.
fn it_sync_back_to_back_calls_need_no_sleep() {
    let (address, responder) = serve_on_a_thread(5, |_| ResponsePdu::ReadHoldingRegisters {
        registers: vec![RegisterValue(7)],
    });

    let mut client = SyncTcpClient::connect(address, TcpConfig::default(), ClientConfig::default())
        .expect("connects");

    for attempt in 0..5 {
        assert_eq!(
            client.call(
                UnitId(1),
                RequestPdu::ReadHoldingRegisters {
                    address: Address(0),
                    quantity: Quantity(1),
                }
            ),
            Ok(Some(ResponsePdu::ReadHoldingRegisters {
                registers: vec![RegisterValue(7)],
            })),
            "call {attempt} needed no pause before it"
        );
    }
    responder.join().expect("the responder finishes");
}

#[test]
/// CL-R-078 — the state projection enters no runtime, so it answers from inside
/// an async context where every request method would be refused (CL-R-075).
fn it_sync_state_is_reported_without_entering_the_runtime() {
    let (address, responder) = serve_on_a_thread(0, |_| unreachable!("no request is sent"));
    let client = SyncTcpClient::connect(address, TcpConfig::default(), ClientConfig::default())
        .expect("connects");

    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    runtime.block_on(async {
        assert!(!client.is_desynchronized());
        assert_eq!(client.state(), rust_modbus::ClientState::Untried);
    });

    drop(client);
    responder.join().expect("the responder finishes");
}

#[test]
/// CL-R-075 — constructing a blocking client from a thread that already drives a
/// runtime is refused with the typed error, rather than panicking inside
/// `block_on` or deadlocking. The check happens before the address is touched,
/// so the address here is never connected to.
fn it_sync_connect_inside_a_runtime_is_refused() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");

    let refused = runtime.block_on(async {
        SyncTcpClient::connect(
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
            TcpConfig::default(),
            ClientConfig::default(),
        )
    });

    assert_eq!(refused.err(), Some(Error::BlockingInAsyncContext));
}
