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
use std::time::Duration;

use rust_modbus::{
    Address, ClientConfig, DiagnosticSubFunction, Error, ExceptionCode, ExceptionResponse,
    ExceptionStatus, FileNumber, FileRecordRead, FileRecordReadResponse, FileRecordWrite,
    FunctionCode, Mask, MeiRequest, MeiResponse, Quantity, RecordLength, RecordNumber,
    RegisterValue, RequestPdu, ResponsePdu, RtuOverTcp, SyncClient, SyncRtuOverTcpClient,
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

/// Answer any request with the matching response variant.
///
/// Deliberately exhaustive rather than a catch-all: a request variant added
/// later fails to compile here, which is the reminder that CL-R-071 needs a
/// blocking method for it too.
fn echo_shaped_reply(request: &RequestPdu) -> ResponsePdu {
    match request {
        RequestPdu::ReadCoils { .. } => ResponsePdu::ReadCoils {
            coils: vec![true, false, true],
        },
        RequestPdu::ReadDiscreteInputs { .. } => ResponsePdu::ReadDiscreteInputs {
            inputs: vec![false, true],
        },
        RequestPdu::ReadHoldingRegisters { .. } => ResponsePdu::ReadHoldingRegisters {
            registers: vec![RegisterValue(1), RegisterValue(2)],
        },
        RequestPdu::ReadInputRegisters { .. } => ResponsePdu::ReadInputRegisters {
            registers: vec![RegisterValue(3)],
        },
        RequestPdu::WriteSingleCoil { address, value } => ResponsePdu::WriteSingleCoil {
            address: *address,
            value: *value,
        },
        RequestPdu::WriteSingleRegister { address, value } => ResponsePdu::WriteSingleRegister {
            address: *address,
            value: *value,
        },
        RequestPdu::WriteMultipleCoils { address, coils } => ResponsePdu::WriteMultipleCoils {
            address: *address,
            quantity: Quantity(u16::try_from(coils.len()).expect("a small fixture")),
        },
        RequestPdu::WriteMultipleRegisters { address, registers } => {
            ResponsePdu::WriteMultipleRegisters {
                address: *address,
                quantity: Quantity(u16::try_from(registers.len()).expect("a small fixture")),
            }
        }
        RequestPdu::MaskWriteRegister {
            address,
            and_mask,
            or_mask,
        } => ResponsePdu::MaskWriteRegister {
            address: *address,
            and_mask: *and_mask,
            or_mask: *or_mask,
        },
        RequestPdu::ReadWriteMultipleRegisters { .. } => ResponsePdu::ReadWriteMultipleRegisters {
            registers: vec![RegisterValue(9)],
        },
        RequestPdu::ReadExceptionStatus => ResponsePdu::ReadExceptionStatus {
            status: ExceptionStatus(0x6D),
        },
        RequestPdu::Diagnostics { sub_function, data } => ResponsePdu::Diagnostics {
            sub_function: *sub_function,
            data: data.clone(),
        },
        RequestPdu::GetCommEventCounter => ResponsePdu::GetCommEventCounter {
            status: 0,
            event_count: 264,
        },
        RequestPdu::GetCommEventLog => ResponsePdu::GetCommEventLog {
            status: 0,
            event_count: 264,
            message_count: 289,
            events: vec![0x20, 0x00],
        },
        RequestPdu::ReportServerId => ResponsePdu::ReportServerId {
            data: vec![0xFF, 0xFF],
        },
        // Three registers, not one: FR-R-041 puts a 7-byte floor under the
        // response data length, and a single-register record encodes to 4.
        RequestPdu::ReadFileRecord { .. } => ResponsePdu::ReadFileRecord {
            records: vec![FileRecordReadResponse {
                values: vec![
                    RegisterValue(0x0DFE),
                    RegisterValue(0x0000),
                    RegisterValue(0x0001),
                ],
            }],
        },
        RequestPdu::WriteFileRecord { records } => ResponsePdu::WriteFileRecord {
            records: records.clone(),
        },
        RequestPdu::ReadFifoQueue { .. } => ResponsePdu::ReadFifoQueue {
            values: vec![RegisterValue(0x01B8)],
        },
        RequestPdu::EncapsulatedInterfaceTransport(_) => {
            ResponsePdu::EncapsulatedInterfaceTransport(MeiResponse::CanOpen {
                data: vec![0x01, 0x02],
            })
        }
        RequestPdu::Custom { code, .. } => ResponsePdu::Custom {
            code: *code,
            data: vec![],
        },
    }
}

#[test]
/// CL-R-071, CL-R-072 — every request method of the async client exists on the
/// blocking one and carries its values through. Twenty exchanges over one
/// socket: the nineteen typed methods of CL-R-060 and the raw method of
/// CL-R-061.
///
/// Each result is asserted rather than merely awaited. A test that only called
/// each method would execute the delegation without proving it delegates to the
/// right thing — twenty methods that all called `read_coils` would pass it.
fn it_sync_mirrors_every_async_request_method() {
    let (address, responder) = serve_on_a_thread(20, echo_shaped_reply);
    let mut client = SyncTcpClient::connect(address, TcpConfig::default(), ClientConfig::default())
        .expect("connects");
    let unit = UnitId(0x11);

    assert_eq!(
        client.read_coils(unit, Address(0x13), Quantity(3)),
        Ok(vec![true, false, true])
    );
    assert_eq!(
        client.read_discrete_inputs(unit, Address(0xC4), Quantity(2)),
        Ok(vec![false, true])
    );
    assert_eq!(
        client.read_holding_registers(unit, Address(0x6B), Quantity(2)),
        Ok(vec![RegisterValue(1), RegisterValue(2)])
    );
    assert_eq!(
        client.read_input_registers(unit, Address(0x08), Quantity(1)),
        Ok(vec![RegisterValue(3)])
    );
    assert_eq!(client.write_single_coil(unit, Address(0xAC), true), Ok(()));
    assert_eq!(
        client.write_single_register(unit, Address(0x01), RegisterValue(3)),
        Ok(())
    );
    assert_eq!(
        client.write_multiple_coils(unit, Address(0x13), &[true, false]),
        Ok(())
    );
    assert_eq!(
        client.write_multiple_registers(unit, Address(0x01), &[RegisterValue(0x0A)]),
        Ok(())
    );
    assert_eq!(
        client.mask_write_register(unit, Address(0x04), Mask(0x00F2), Mask(0x0025)),
        Ok(())
    );
    assert_eq!(
        client.read_write_multiple_registers(
            unit,
            Address(0x03),
            Quantity(1),
            Address(0x0E),
            &[RegisterValue(0x00FF)]
        ),
        Ok(vec![RegisterValue(9)])
    );
    assert_eq!(
        client.read_exception_status(unit),
        Ok(ExceptionStatus(0x6D))
    );
    assert_eq!(
        client.diagnostics(unit, DiagnosticSubFunction::ReturnQueryData, &[0xA537]),
        Ok(vec![0xA537])
    );
    assert_eq!(
        client.get_comm_event_counter(unit).map(|c| c.event_count),
        Ok(264)
    );
    assert_eq!(
        client.get_comm_event_log(unit).map(|l| l.message_count),
        Ok(289)
    );
    assert_eq!(client.report_server_id(unit), Ok(vec![0xFF, 0xFF]));
    assert_eq!(
        client.read_file_record(
            unit,
            &[FileRecordRead {
                file_number: FileNumber(4),
                record_number: RecordNumber(1),
                record_length: RecordLength(1),
            }]
        ),
        Ok(vec![FileRecordReadResponse {
            values: vec![
                RegisterValue(0x0DFE),
                RegisterValue(0x0000),
                RegisterValue(0x0001),
            ],
        }])
    );
    assert_eq!(
        client.write_file_record(
            unit,
            &[FileRecordWrite {
                file_number: FileNumber(4),
                record_number: RecordNumber(7),
                values: vec![RegisterValue(0x06AF)],
            }]
        ),
        Ok(())
    );
    assert_eq!(
        client.read_fifo_queue(unit, Address(0x04DE)),
        Ok(vec![RegisterValue(0x01B8)])
    );
    assert_eq!(
        client.encapsulated_interface_transport(
            unit,
            MeiRequest::CanOpen {
                data: vec![0x01, 0x02],
            }
        ),
        Ok(MeiResponse::CanOpen {
            data: vec![0x01, 0x02],
        })
    );
    assert_eq!(
        client.call(unit, RequestPdu::ReportServerId),
        Ok(Some(ResponsePdu::ReportServerId {
            data: vec![0xFF, 0xFF],
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

/// Accept one connection and then never answer, holding the socket open.
///
/// A closed socket would produce `ConnectionClosed`; the point here is silence,
/// which is what the response timeout exists to bound.
fn serve_silence() -> (SocketAddr, mpsc::Sender<()>, thread::JoinHandle<()>) {
    let (address_tx, address_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let handle = thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("the responder's runtime");
        runtime.block_on(async move {
            let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
                .await
                .expect("binds");
            address_tx
                .send(listener.local_addr().expect("reports its address"))
                .expect("the test thread is waiting");
            let _connection = listener.accept().await.expect("accepts");
            // Hold the connection open, answering nothing, until released.
            let _ = tokio::task::spawn_blocking(move || done_rx.recv()).await;
        });
    });
    (
        address_rx
            .recv()
            .expect("the responder reports its address"),
        done_tx,
        handle,
    )
}

#[test]
/// CL-R-072, CL-R-073 — the response timeout of CL-R-030 fires on a blocking
/// call exactly as on an async one, and the client is desynchronized afterwards
/// (CL-R-031) so the next call is refused without writing (CL-R-032).
///
/// This is also what pins CL-R-073: the timeout is a Tokio timer, so a runtime
/// built without the timer driver would panic here rather than return.
fn it_sync_timeout_fires_and_desynchronizes() {
    let (address, release, responder) = serve_silence();
    let config = ClientConfig {
        response_timeout: Duration::from_millis(50),
    };
    let mut client =
        SyncTcpClient::connect(address, TcpConfig::default(), config).expect("connects");

    assert_eq!(
        client.read_holding_registers(UnitId(1), Address(0), Quantity(1)),
        Err(Error::Timeout { what: "response" })
    );
    assert!(client.is_desynchronized(), "CL-R-031 marks it unusable");
    assert_eq!(
        client.read_holding_registers(UnitId(1), Address(0), Quantity(1)),
        Err(Error::Desynchronized),
        "CL-R-032 refuses every later request"
    );

    drop(release);
    responder.join().expect("the responder finishes");
}

#[test]
/// CL-R-072 — an exception response reaches a blocking typed method as
/// `Error::Exception`, and leaves the client usable (CL-R-042), exactly as it
/// does on the async client.
fn it_sync_exception_surfaces_and_leaves_the_client_usable() {
    let (address, responder) = serve_on_a_thread(2, |request| match request {
        RequestPdu::ReadHoldingRegisters { .. } => ResponsePdu::Exception(ExceptionResponse {
            function: FunctionCode::ReadHoldingRegisters,
            exception: ExceptionCode::IllegalDataAddress,
        }),
        _ => ResponsePdu::ReadCoils { coils: vec![true] },
    });
    let mut client = SyncTcpClient::connect(address, TcpConfig::default(), ClientConfig::default())
        .expect("connects");

    assert_eq!(
        client.read_holding_registers(UnitId(1), Address(0), Quantity(1)),
        Err(Error::Exception {
            function: FunctionCode::ReadHoldingRegisters,
            exception: ExceptionCode::IllegalDataAddress,
        })
    );
    assert!(
        !client.is_desynchronized(),
        "CL-R-042 — the server answered, it merely refused"
    );
    assert_eq!(
        client.read_coils(UnitId(1), Address(0), Quantity(1)),
        Ok(vec![true]),
        "the next request proceeds normally"
    );

    responder.join().expect("the responder finishes");
}

#[test]
/// CL-R-072 — the broadcast rules hold identically: a write to unit 0 returns
/// without awaiting a reply (CL-R-051) and a read to unit 0 fails before
/// anything is written (CL-R-052).
///
/// Run over RTU-over-TCP, since no identifier broadcasts on Modbus TCP
/// (CL-R-050) — the rule is the framing's, and the blocking client inherits
/// whichever framing it was built with.
fn it_sync_broadcast_rules_match_the_async_client() {
    let (address_tx, address_rx) = mpsc::channel();
    let (seen_tx, seen_rx) = mpsc::channel();
    let responder = thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("the responder's runtime");
        runtime.block_on(async move {
            let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
                .await
                .expect("binds");
            address_tx
                .send(listener.local_addr().expect("reports its address"))
                .expect("the test thread is waiting");
            let (mut transport, _peer) = listener
                .accept_framed::<RtuOverTcp>()
                .await
                .expect("accepts");
            let (_header, request) = transport.recv_request().await.expect("receives");
            seen_tx.send(request).expect("the test thread is waiting");
            // Deliberately no response: CL-R-051 must not be waiting for one.
        });
    });
    let address = address_rx
        .recv()
        .expect("the responder reports its address");

    let mut client =
        SyncRtuOverTcpClient::connect(address, TcpConfig::default(), ClientConfig::default())
            .expect("connects");

    assert_eq!(
        client.write_single_register(UnitId(0), Address(1), RegisterValue(7)),
        Ok(()),
        "CL-R-051 — a broadcast write completes without a reply"
    );
    assert_eq!(
        client.read_holding_registers(UnitId(0), Address(0), Quantity(1)),
        Err(Error::IllegalValue {
            field: "broadcast read",
            value: 0,
        }),
        "CL-R-052 — a broadcast read is refused"
    );

    assert_eq!(
        seen_rx.recv().expect("the broadcast reached the wire"),
        RequestPdu::WriteSingleRegister {
            address: Address(1),
            value: RegisterValue(7),
        }
    );
    responder.join().expect("the responder finishes");
}

#[test]
/// CL-R-075 — a *request* method called from inside a runtime is refused, not
/// just a constructor. The client is built outside the runtime, so the refusal
/// comes from the method rather than from construction.
fn it_sync_request_inside_a_runtime_is_refused() {
    let (address, responder) = serve_on_a_thread(0, |_| unreachable!("no request is sent"));
    let mut client = SyncTcpClient::connect(address, TcpConfig::default(), ClientConfig::default())
        .expect("connects");

    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let refused = runtime
        .block_on(async { client.read_holding_registers(UnitId(1), Address(0), Quantity(1)) });

    assert_eq!(refused, Err(Error::BlockingInAsyncContext));
    assert!(
        !client.is_desynchronized(),
        "the refusal touched no transport, so the client is untouched"
    );

    drop(client);
    responder.join().expect("the responder finishes");
}

#[test]
/// CL-R-074 — every blocking type and every argument and return type is
/// nameable through `rust_modbus` alone. This binding compiles only if no
/// runtime type is required to spell the surface; were `SyncClient::connect` to
/// take a `tokio::runtime::Handle`, or a method to return one, this would not.
///
/// A compile-time assertion: the bindings are what is checked, not the run.
fn it_blocking_surface_names_no_runtime_type() {
    let _connect: fn(SocketAddr, TcpConfig, ClientConfig) -> rust_modbus::Result<SyncTcpClient> =
        SyncTcpClient::connect;
    let _read: fn(
        &mut SyncTcpClient,
        UnitId,
        Address,
        Quantity,
    ) -> rust_modbus::Result<Vec<RegisterValue>> = SyncTcpClient::read_holding_registers;
    let _call: fn(
        &mut SyncTcpClient,
        UnitId,
        RequestPdu,
    ) -> rust_modbus::Result<Option<ResponsePdu>> = SyncTcpClient::call;
    let _state: fn(&SyncTcpClient) -> rust_modbus::ClientState = SyncTcpClient::state;
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
