//! RTU over a TCP socket, end to end (FR-R-145 … FR-R-150, TR-R-024,
//! TR-R-033, TR-R-045, TR-R-046, TR-R-048, SV-R-053).
//!
//! This crate's own [`RtuOverTcpClient`] against this crate's own
//! [`Server::serve_framed`], over real loopback sockets — the only thing the
//! framing's own unit tests and the transport's duplex-pair tests cannot
//! establish. Every listener binds port 0 and reads the assigned port back,
//! per the testing conventions in `AGENTS.md`.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_modbus::{
    Address, Client, ClientConfig, Connection, Error, ExceptionCode, FrameTransport, Framing,
    FunctionCode, Quantity, RegisterValue, RequestPdu, ResponsePdu, RtuOverTcp, RtuOverTcpClient,
    Server, ServerConfig, Service, TcpConfig, TcpListener, UnitId, connect_tcp_framed,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, split};
use tokio::net::{TcpListener as TokioTcpListener, TcpStream};

/// An ephemeral loopback address: port 0, so the kernel assigns one.
fn ephemeral() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
}

/// Shorter than the 1 s default (CL-R-030), so a link that has genuinely gone
/// silent fails the test quickly rather than stalling the suite.
const TIMEOUT: Duration = Duration::from_millis(200);

/// A service holding its own register table, in the shape SV-R-003 and
/// `docs/specs/server/data-contract.md` describe: the crate ships no store.
#[derive(Debug, Clone, Default)]
struct Registers {
    holding: Arc<Mutex<HashMap<u16, u16>>>,
}

impl Registers {
    fn locked(&self) -> std::sync::MutexGuard<'_, HashMap<u16, u16>> {
        self.holding.lock().expect("no test poisons the lock")
    }
}

impl Service for Registers {
    async fn on_request(
        &self,
        _conn: &Connection,
        _unit: UnitId,
        request: RequestPdu,
    ) -> Result<ResponsePdu, ExceptionCode> {
        match request {
            RequestPdu::ReadHoldingRegisters { address, quantity } => {
                let table = self.locked();
                let registers = (0..quantity.0)
                    .map(|offset| {
                        let at = address
                            .0
                            .checked_add(offset)
                            .ok_or(ExceptionCode::IllegalDataAddress)?;
                        table
                            .get(&at)
                            .copied()
                            .map(RegisterValue)
                            .ok_or(ExceptionCode::IllegalDataAddress)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ResponsePdu::ReadHoldingRegisters { registers })
            }
            RequestPdu::WriteSingleRegister { address, value } => {
                self.locked().insert(address.0, value.0);
                Ok(ResponsePdu::WriteSingleRegister { address, value })
            }
            // Everything else is refused in the protocol's own vocabulary
            // (SV-R-012).
            _ => Err(ExceptionCode::IllegalFunction),
        }
    }
}

/// A running gateway server, its address, and the service it answers from.
struct Running {
    address: SocketAddr,
    service: Registers,
    handle: rust_modbus::ServerHandle,
    serving: tokio::task::JoinHandle<rust_modbus::Result<()>>,
}

async fn start(config: ServerConfig) -> Running {
    let service = Registers::default();
    let listener = TcpListener::bind(ephemeral()).await.expect("binds");
    let address = listener.local_addr().expect("reports its address");
    let server = Server::with_config(service.clone(), config);
    let handle = server.handle();
    Running {
        address,
        service,
        handle,
        serving: tokio::spawn(server.serve_framed::<RtuOverTcp>(listener)),
    }
}

async fn connect(address: SocketAddr) -> RtuOverTcpClient {
    Client::with_config(
        connect_tcp_framed::<RtuOverTcp>(address, TcpConfig::default())
            .await
            .expect("connects"),
        ClientConfig {
            response_timeout: TIMEOUT,
        },
    )
}

async fn finish(running: Running) {
    running.handle.shutdown().await;
    running
        .serving
        .await
        .expect("the task finishes")
        .expect("serving succeeds");
}

#[tokio::test]
/// FR-R-145, TR-R-024, SV-R-053 — a register read crosses a real socket
/// framed as RTU over a stream, end to end.
async fn it_register_read_round_trips_over_the_gateway() {
    let running = start(ServerConfig::default()).await;
    running.service.locked().insert(4, 0x022B);
    let mut client = connect(running.address).await;

    assert_eq!(
        client
            .read_holding_registers(UnitId(1), Address(4), Quantity(1))
            .await,
        Ok(vec![RegisterValue(0x022B)])
    );

    finish(running).await;
}

#[tokio::test]
/// FR-R-145, TR-R-024, SV-R-053 — a register write crosses a real socket the
/// same way, and lands in the service's own table.
async fn it_register_write_round_trips_over_the_gateway() {
    let running = start(ServerConfig::default()).await;
    let mut client = connect(running.address).await;

    client
        .write_single_register(UnitId(1), Address(4), RegisterValue(0x022B))
        .await
        .expect("writes a register");
    assert_eq!(running.service.locked().get(&4).copied(), Some(0x022B));

    finish(running).await;
}

#[tokio::test]
/// FR-R-147, CL-R-042 — an exception response derives its extent of 5 bytes
/// from the rule alone, arrives at the client as a typed exception, and leaves
/// the connection usable for the next request.
async fn it_exception_response_round_trips_over_the_gateway() {
    let running = start(ServerConfig::default()).await;
    let mut client = connect(running.address).await;

    assert_eq!(
        client
            .read_holding_registers(UnitId(1), Address(99), Quantity(1))
            .await,
        Err(Error::Exception {
            function: FunctionCode::ReadHoldingRegisters,
            exception: ExceptionCode::IllegalDataAddress,
        })
    );
    assert!(!client.is_desynchronized());

    running.service.locked().insert(4, 0x0064);
    assert_eq!(
        client
            .read_holding_registers(UnitId(1), Address(4), Quantity(1))
            .await,
        Ok(vec![RegisterValue(0x0064)])
    );

    finish(running).await;
}

#[tokio::test]
/// CL-R-023, FR-R-150 — RTU over a stream is not self-locating: a corrupted
/// response's extent was read out of that response's own (now wrong) bytes, so
/// the client cannot tell where the next frame begins and gives up on the
/// stream, unlike the same corruption over serial RTU (see `reframing.rs`),
/// where the silence would still be there to resynchronize on.
async fn it_corrupted_crc_desynchronizes_the_client() {
    let running = start(ServerConfig::default()).await;
    running.service.locked().insert(4, 0x022B);

    // A minimal relay standing in for the wire: passes client -> server
    // through untouched, and flips the last byte -- half the CRC -- of the
    // first response it sees on the way back.
    let gateway = TokioTcpListener::bind(ephemeral())
        .await
        .expect("binds the gateway");
    let gateway_addr = gateway.local_addr().expect("reports its address");
    let upstream = running.address;
    tokio::spawn(async move {
        let (downstream, _peer) = gateway.accept().await.expect("accepts the client");
        let upstream = TcpStream::connect(upstream)
            .await
            .expect("reaches the server");
        let (mut down_r, mut down_w) = split(downstream);
        let (mut up_r, mut up_w) = split(upstream);

        let to_server = async move {
            let mut chunk = [0u8; 512];
            loop {
                let read = match down_r.read(&mut chunk).await {
                    Ok(0) | Err(_) => return,
                    Ok(read) => read,
                };
                let sent = chunk.get(..read).expect("read <= chunk.len()");
                if up_w.write_all(sent).await.is_err() {
                    return;
                }
            }
        };
        let to_client = async move {
            let mut chunk = [0u8; 512];
            let mut corrupted_once = false;
            loop {
                let read = match up_r.read(&mut chunk).await {
                    Ok(0) | Err(_) => return,
                    Ok(read) => read,
                };
                let frame = chunk.get_mut(..read).expect("read <= chunk.len()");
                if !corrupted_once {
                    if let Some(last) = frame.last_mut() {
                        *last ^= 0xFF;
                    }
                    corrupted_once = true;
                }
                if down_w.write_all(frame).await.is_err() {
                    return;
                }
            }
        };
        tokio::join!(to_server, to_client);
    });

    let mut client = connect(gateway_addr).await;
    let failed = client
        .read_holding_registers(UnitId(1), Address(4), Quantity(1))
        .await;
    assert!(
        matches!(failed, Err(Error::Checksum { .. })),
        "expected the corrupted frame to fail its integrity check, got {failed:?}"
    );
    assert!(
        client.is_desynchronized(),
        "RTU over a stream is not self-locating: one corrupted frame must cost the whole link"
    );
    assert_eq!(
        client
            .read_holding_registers(UnitId(1), Address(4), Quantity(1))
            .await,
        Err(Error::Desynchronized),
        "a desynchronized client refuses further requests rather than write into an \
         unaccounted-for stream"
    );

    running.handle.shutdown().await;
    let _ = running.serving.await;
}

#[tokio::test]
/// SV-R-021 — a request addressed to a unit the server is not configured for
/// draws no response at all, and the connection stays open: the request after
/// it, on the very same connection, is answered normally.
async fn it_nonmatching_unit_draws_no_response_and_the_connection_stays_open() {
    let running = start(ServerConfig {
        unit: Some(UnitId(1)),
    })
    .await;
    running.service.locked().insert(4, 0x022B);

    let stream = TcpStream::connect(running.address).await.expect("connects");
    let mut transport = FrameTransport::<_, RtuOverTcp>::new(stream);

    // Both requests are written before either is read back: if the foreign
    // unit drew a response, it would be the *first* one waiting in the
    // stream, so a single `recv_response` below would return it instead of
    // the second request's. A `recv_response` cancelled by a timeout instead
    // would abandon the read mid-flight and mark the transport unusable on
    // its own terms (TR-R-041) — a fact about giving up, not about whether
    // the peer answered, so it would prove nothing about SV-R-021 here.
    transport
        .send_request(
            &UnitId(2),
            &RequestPdu::ReadHoldingRegisters {
                address: Address(4),
                quantity: Quantity(1),
            },
        )
        .await
        .expect("writes the foreign request");
    transport
        .send_request(
            &UnitId(1),
            &RequestPdu::ReadHoldingRegisters {
                address: Address(4),
                quantity: Quantity(1),
            },
        )
        .await
        .expect("writes on the same connection");

    let (header, response) = tokio::time::timeout(TIMEOUT, transport.recv_response())
        .await
        .expect("the connection is still open and answers the second request")
        .expect("the response decodes");
    assert_eq!(
        header,
        UnitId(1),
        "the foreign unit's request must have drawn no response of its own, \
         or this would be its header instead"
    );
    assert_eq!(
        response,
        ResponsePdu::ReadHoldingRegisters {
            registers: vec![RegisterValue(0x022B)],
        }
    );

    finish(running).await;
}

#[tokio::test]
/// TR-R-045, TR-R-048 — a cheap gateway's own read/write granularity has no
/// bearing on the ADUs it carries: two requests a gateway coalesces into one
/// upstream write are still delivered to the server one at a time, and a
/// request a gateway fragments byte by byte on the way out is still
/// reassembled into a single ADU, with no idle-gap heuristic in play either
/// way.
async fn it_gateway_coalescing_and_fragmenting_are_transparent() {
    let running = start(ServerConfig::default()).await;

    // The two broadcast writes this test sends, and their combined encoded
    // length -- broadcasts are written and not awaited (CL-R-051), which is
    // what lets the client fire both before either is answered, so there is
    // something for the gateway to coalesce.
    let first = RequestPdu::WriteSingleRegister {
        address: Address(4),
        value: RegisterValue(0x0AAA),
    };
    let second = RequestPdu::WriteSingleRegister {
        address: Address(5),
        value: RegisterValue(0x0BBB),
    };
    let combined_len = RtuOverTcp::encode_request(&UnitId(0), &first)
        .expect("encodes")
        .len()
        + RtuOverTcp::encode_request(&UnitId(0), &second)
            .expect("encodes")
            .len();

    let gateway = TokioTcpListener::bind(ephemeral())
        .await
        .expect("binds the gateway");
    let gateway_addr = gateway.local_addr().expect("reports its address");
    let upstream = running.address;
    tokio::spawn(async move {
        let (downstream, _peer) = gateway.accept().await.expect("accepts the client");
        let upstream = TcpStream::connect(upstream)
            .await
            .expect("reaches the server");
        let (mut down_r, mut down_w) = split(downstream);
        let (mut up_r, mut up_w) = split(upstream);

        let to_server = async move {
            // Buffer at least the two writes' combined bytes before forwarding
            // anything, so they reach the server as a single write no matter
            // how many reads it took to gather them -- exactly what a cheap
            // gateway that batches its upstream writes would do.
            let mut buffered = Vec::new();
            let mut chunk = [0u8; 512];
            while buffered.len() < combined_len {
                match down_r.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        buffered.extend_from_slice(chunk.get(..read).expect("read <= chunk.len()"))
                    }
                }
            }
            if !buffered.is_empty() && up_w.write_all(&buffered).await.is_err() {
                return;
            }
            // Whatever follows is fragmented instead: one byte, one write,
            // regardless of how many bytes a single read gathered.
            loop {
                let read = match down_r.read(&mut chunk).await {
                    Ok(0) | Err(_) => return,
                    Ok(read) => read,
                };
                for byte in chunk.get(..read).expect("read <= chunk.len()") {
                    if up_w.write_all(&[*byte]).await.is_err() {
                        return;
                    }
                }
            }
        };
        let to_client = async move {
            let mut chunk = [0u8; 512];
            loop {
                let read = match up_r.read(&mut chunk).await {
                    Ok(0) | Err(_) => return,
                    Ok(read) => read,
                };
                if down_w
                    .write_all(chunk.get(..read).expect("read <= chunk.len()"))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        };
        tokio::join!(to_server, to_client);
    });

    let mut client = connect(gateway_addr).await;

    // The two broadcasts the gateway coalesces into one upstream write.
    client
        .call(UnitId(0), first.clone())
        .await
        .expect("a broadcast is sent, not awaited");
    client
        .call(UnitId(0), second.clone())
        .await
        .expect("a broadcast is sent, not awaited");

    // A third, ordinary request, fragmented byte by byte by the same gateway
    // on its way to the server, and still answered correctly.
    let value = client
        .read_holding_registers(UnitId(1), Address(4), Quantity(1))
        .await
        .expect("the fragmented request is still answered");

    assert_eq!(
        value,
        vec![RegisterValue(0x0AAA)],
        "the coalesced write must have reached the server as two separate ADUs"
    );
    assert_eq!(
        running.service.locked().get(&5).copied(),
        Some(0x0BBB),
        "both coalesced writes landed, not just the first"
    );

    finish(running).await;
}
