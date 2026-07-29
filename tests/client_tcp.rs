//! The client over a real TCP socket (CL-R-001, CL-R-002).
//!
//! The unit tests drive the client over an in-memory duplex pair; these prove
//! the same code works over a socket, which is the only thing the pair cannot
//! establish. The server area does not exist yet, so the responder here is
//! hand-rolled from a `FrameTransport`.
//!
//! Every listener binds port 0 and reads the assigned port back, per the
//! testing conventions in `AGENTS.md`.

use std::net::{Ipv4Addr, SocketAddr};

use rust_modbus::{
    Address, Client, Error, ExceptionCode, ExceptionResponse, FrameTransport, FunctionCode,
    Quantity, RegisterValue, RequestPdu, ResponsePdu, TcpConfig, TcpListener, UnitId, connect_tcp,
};

/// An ephemeral loopback address: port 0, so the kernel assigns one.
fn ephemeral() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
}

/// Accept one connection and answer `count` requests with `reply`.
fn serve(
    listener: TcpListener,
    count: usize,
    reply: fn(&RequestPdu) -> ResponsePdu,
) -> tokio::task::JoinHandle<Vec<RequestPdu>> {
    tokio::spawn(async move {
        let (mut transport, _peer) = listener.accept().await.expect("accepts");
        let mut seen = vec![];
        for _ in 0..count {
            let (header, request) = transport.recv_request().await.expect("receives");
            let response = reply(&request);
            transport
                .send_response(&header, &response)
                .await
                .expect("responds");
            seen.push(request);
        }
        seen
    })
}

#[tokio::test]
/// CL-R-001, CL-R-060 — a typed request travels over a socket and its values
/// come back, the same code path the duplex-pair unit tests exercise.
async fn it_client_reads_registers_over_a_socket() {
    let listener = TcpListener::bind(ephemeral()).await.expect("binds");
    let address = listener.local_addr().expect("reports its address");
    let serving = serve(listener, 1, |_| ResponsePdu::ReadHoldingRegisters {
        registers: vec![RegisterValue(0x022B), RegisterValue(0x0000)],
    });

    let transport = connect_tcp(address, TcpConfig::default())
        .await
        .expect("connects");
    let mut client = Client::new(transport);

    assert_eq!(
        client
            .read_holding_registers(UnitId(0x11), Address(0x006B), Quantity(2))
            .await,
        Ok(vec![RegisterValue(0x022B), RegisterValue(0x0000)])
    );
    assert_eq!(
        serving.await.expect("the server task finishes"),
        vec![RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(2),
        }]
    );
}

#[tokio::test]
/// CL-R-040, CL-R-042 — an exception crossing a real socket surfaces as a typed
/// failure, and the connection remains usable for the next request.
async fn it_exception_over_a_socket_leaves_the_client_usable() {
    let listener = TcpListener::bind(ephemeral()).await.expect("binds");
    let address = listener.local_addr().expect("reports its address");
    let serving = serve(listener, 2, |request| match request {
        RequestPdu::ReadCoils { .. } => ResponsePdu::Exception(ExceptionResponse {
            function: FunctionCode::ReadCoils,
            exception: ExceptionCode::IllegalDataAddress,
        }),
        _ => ResponsePdu::ReadHoldingRegisters {
            registers: vec![RegisterValue(0x0064)],
        },
    });

    let mut client = Client::new(
        connect_tcp(address, TcpConfig::default())
            .await
            .expect("connects"),
    );

    assert_eq!(
        client
            .read_coils(UnitId(0x11), Address(0x9999), Quantity(1))
            .await,
        Err(Error::Exception {
            function: FunctionCode::ReadCoils,
            exception: ExceptionCode::IllegalDataAddress,
        })
    );
    assert!(!client.is_desynchronized());
    assert_eq!(
        client
            .read_holding_registers(UnitId(0x11), Address(0), Quantity(1))
            .await,
        Ok(vec![RegisterValue(0x0064)])
    );
    serving.await.expect("the server task finishes");
}

#[tokio::test]
/// CL-R-031, CL-R-032 — a peer that closes the connection desynchronizes the
/// client, which then refuses further requests instead of writing into a socket
/// whose other end is gone.
async fn it_closed_connection_desynchronizes_the_client() {
    let listener = TcpListener::bind(ephemeral()).await.expect("binds");
    let address = listener.local_addr().expect("reports its address");
    let closing = tokio::spawn(async move {
        let (transport, _peer) = listener.accept().await.expect("accepts");
        // Drops the socket without answering.
        drop(transport);
    });

    let mut client = Client::new(
        connect_tcp(address, TcpConfig::default())
            .await
            .expect("connects"),
    );
    closing.await.expect("the server task finishes");

    assert!(
        client
            .read_holding_registers(UnitId(0x11), Address(0), Quantity(1))
            .await
            .is_err()
    );
    assert!(client.is_desynchronized());
    assert_eq!(
        client
            .read_holding_registers(UnitId(0x11), Address(0), Quantity(1))
            .await,
        Err(Error::Desynchronized)
    );
}

#[tokio::test]
/// CL-R-006 — the transport comes back out, which is how a caller recovers from
/// desynchronization: discard the client, keep or replace the connection.
async fn it_client_surrenders_a_live_socket() {
    let listener = TcpListener::bind(ephemeral()).await.expect("binds");
    let address = listener.local_addr().expect("reports its address");
    let serving = serve(listener, 1, |_| ResponsePdu::ReadHoldingRegisters {
        registers: vec![RegisterValue(1)],
    });

    let mut client = Client::new(
        connect_tcp(address, TcpConfig::default())
            .await
            .expect("connects"),
    );
    client
        .read_holding_registers(UnitId(0x11), Address(0), Quantity(1))
        .await
        .expect("reads");

    let transport: FrameTransport<_, _> = client.into_inner();
    drop(transport);
    serving.await.expect("the server task finishes");
}
