//! The server over real sockets, answered by this crate's own client.
//!
//! The unit tests drive the server over an in-memory duplex pair with a
//! `FrameTransport` as the peer; these put both halves of the crate together on
//! a socket, which is the only thing the pair cannot establish.
//!
//! Every listener binds port 0 and reads the assigned port back, per the testing
//! conventions in `AGENTS.md`.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_modbus::{
    Address, Client, ClientConfig, Connection, Disconnect, Error, ExceptionCode, FunctionCode,
    Quantity, RegisterValue, RequestPdu, ResponsePdu, Server, ServerConfig, Service, TcpConfig,
    TcpListener, UnitId, connect_tcp,
};

/// An ephemeral loopback address: port 0, so the kernel assigns one.
fn ephemeral() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
}

/// A service holding its own register table behind its own lock.
///
/// The shape SV-R-003 and `docs/specs/server/data-contract.md` describe: the
/// crate ships no store, and one service answers every connection at once, so
/// the state lives here and the lock is this type's business.
///
/// Cheap to clone, and cloning shares the state — which is how a consumer keeps
/// a view of its own store after handing a copy to [`Server::new`]. It has to
/// be: the orphan rule forbids `impl Service for Arc<MyType>` outside this
/// crate, so the service *is* the handle.
#[derive(Debug, Clone, Default)]
struct Registers {
    holding: Arc<Mutex<HashMap<u16, u16>>>,
    disconnects: Arc<Mutex<Vec<Disconnect>>>,
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

    async fn on_disconnect(&self, _conn: &Connection, reason: Disconnect) {
        self.disconnects
            .lock()
            .expect("no test poisons the lock")
            .push(reason);
    }
}

/// A running server, its address, and the service it answers from.
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
        serving: tokio::spawn(server.serve(listener)),
    }
}

async fn connect(address: SocketAddr) -> Client<tokio::net::TcpStream, rust_modbus::Tcp> {
    Client::with_config(
        connect_tcp(address, TcpConfig::default())
            .await
            .expect("connects"),
        ClientConfig {
            // Shorter than the 1 s default (CL-R-030): one test deliberately
            // waits for a response that never comes.
            response_timeout: Duration::from_millis(200),
        },
    )
}

#[tokio::test]
/// SV-R-010, SV-R-011 — a request from this crate's client is answered by this
/// crate's server, over a socket, with the values the service holds.
async fn it_client_and_server_complete_an_exchange() {
    let running = start(ServerConfig::default()).await;
    let mut client = connect(running.address).await;

    client
        .write_single_register(UnitId(1), Address(4), RegisterValue(0x022B))
        .await
        .expect("writes a register");
    assert_eq!(
        client
            .read_holding_registers(UnitId(1), Address(4), Quantity(1))
            .await,
        Ok(vec![RegisterValue(0x022B)])
    );
    assert_eq!(
        running.service.locked().get(&4).copied(),
        Some(0x022B),
        "the write must have reached the service's own table"
    );

    running.handle.shutdown().await;
    assert_eq!(running.serving.await.expect("the task finishes"), Ok(()));
}

#[tokio::test]
/// SV-R-012 — a service's refusal arrives at the client as a typed exception,
/// and the connection stays usable (CL-R-042).
async fn it_service_refusal_reaches_the_client_as_an_exception() {
    let running = start(ServerConfig::default()).await;
    let mut client = connect(running.address).await;

    assert_eq!(
        client.read_coils(UnitId(1), Address(0), Quantity(1)).await,
        Err(Error::Exception {
            function: FunctionCode::ReadCoils,
            exception: ExceptionCode::IllegalFunction,
        })
    );
    assert!(!client.is_desynchronized());

    // An unmapped address is the service's judgement, not the crate's
    // (SV-R-005).
    assert_eq!(
        client
            .read_holding_registers(UnitId(1), Address(99), Quantity(1))
            .await,
        Err(Error::Exception {
            function: FunctionCode::ReadHoldingRegisters,
            exception: ExceptionCode::IllegalDataAddress,
        })
    );

    running.handle.shutdown().await;
    running
        .serving
        .await
        .expect("the task finishes")
        .expect("serving succeeds");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
/// SV-R-003, SV-R-030 — eight clients on eight connections share one service
/// across threads, each writing and reading its own register.
///
/// The service's lock is what makes this sound; the server adds none of its own.
async fn it_many_clients_share_one_service() {
    let running = start(ServerConfig::default()).await;
    let address = running.address;

    let clients = (0..8u16)
        .map(|n| {
            tokio::spawn(async move {
                let mut client = connect(address).await;
                client
                    .write_single_register(UnitId(1), Address(n), RegisterValue(n * 3))
                    .await
                    .expect("writes its own register");
                client
                    .read_holding_registers(UnitId(1), Address(n), Quantity(1))
                    .await
            })
        })
        .collect::<Vec<_>>();

    for (n, client) in clients.into_iter().enumerate() {
        let n = u16::try_from(n).expect("eight fits a u16");
        assert_eq!(
            client.await.expect("the client task finishes"),
            Ok(vec![RegisterValue(n * 3)]),
            "each connection must see its own write"
        );
    }
    assert_eq!(running.service.locked().len(), 8);

    running.handle.shutdown().await;
    running
        .serving
        .await
        .expect("the task finishes")
        .expect("serving succeeds");
}

#[tokio::test]
/// SV-R-020, SV-R-021 — a server configured for one unit leaves another unit's
/// request unanswered, and the client's own timeout is what ends the wait.
async fn it_configured_unit_answers_only_itself() {
    let running = start(ServerConfig {
        unit: Some(UnitId(1)),
    })
    .await;
    let mut client = connect(running.address).await;

    client
        .write_single_register(UnitId(1), Address(0), RegisterValue(1))
        .await
        .expect("the configured unit is answered");

    let mut other = connect(running.address).await;
    assert_eq!(
        other
            .read_holding_registers(UnitId(2), Address(0), Quantity(1))
            .await,
        Err(Error::Timeout { what: "response" }),
        "another unit's request must draw no response at all"
    );

    running.handle.shutdown().await;
    running
        .serving
        .await
        .expect("the task finishes")
        .expect("serving succeeds");
}

#[tokio::test]
/// SV-R-041, SV-R-043, SV-R-044 — shutdown ends the live connection, notifies
/// the service, and returns only once serving has finished.
async fn it_shutdown_drains_and_stops_accepting() {
    let running = start(ServerConfig::default()).await;
    let mut client = connect(running.address).await;
    client
        .write_single_register(UnitId(1), Address(0), RegisterValue(1))
        .await
        .expect("writes a register");

    running.handle.shutdown().await;
    assert!(running.serving.is_finished());
    assert_eq!(running.serving.await.expect("the task finishes"), Ok(()));
    assert_eq!(
        *running
            .service
            .disconnects
            .lock()
            .expect("no test poisons the lock"),
        vec![Disconnect::ShuttingDown]
    );

    // The client's connection is gone with the server.
    assert!(
        client
            .read_holding_registers(UnitId(1), Address(0), Quantity(1))
            .await
            .is_err()
    );
}
