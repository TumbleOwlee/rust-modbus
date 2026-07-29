//! A Modbus TCP server backed by four in-memory tables, for interop testing.
//!
//! The crate ships no data model (SV-R-005), so this is what a consumer writes:
//! one type holding its own state behind its own lock, `impl Service` for it,
//! and `Server::new`. It is also how a foreign Modbus *master* is pointed at this
//! crate's server — see `tests/interop/README.md`.
//!
//! ```sh
//! cargo run --example interop_server -- 127.0.0.1:5030
//! ```
//!
//! Every request is printed, so the exchange is visible from this side too.
//! Coils and discrete inputs cover addresses 0–15, holding and input registers
//! 0–15, on unit 1; holding registers start at their own address as their value
//! and input registers at 100 + address.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use rust_modbus::{
    Address, Connection, Disconnect, ExceptionCode, Quantity, RegisterValue, RequestPdu,
    ResponsePdu, Server, ServerConfig, Service, TcpListener, UnitId,
};

/// How many addresses each table exposes.
const SIZE: u16 = 16;

/// The four Modbus tables, shared by every connection.
#[derive(Debug, Clone)]
struct Device {
    coils: Arc<Mutex<HashMap<u16, bool>>>,
    discrete: Arc<Mutex<HashMap<u16, bool>>>,
    holding: Arc<Mutex<HashMap<u16, u16>>>,
    input: Arc<Mutex<HashMap<u16, u16>>>,
}

impl Default for Device {
    fn default() -> Self {
        Self {
            coils: Arc::new(Mutex::new((0..SIZE).map(|at| (at, false)).collect())),
            discrete: Arc::new(Mutex::new((0..SIZE).map(|at| (at, at % 2 == 0)).collect())),
            holding: Arc::new(Mutex::new((0..SIZE).map(|at| (at, at)).collect())),
            input: Arc::new(Mutex::new((0..SIZE).map(|at| (at, 100 + at)).collect())),
        }
    }
}

/// Read `quantity` values from one table, starting at `address`.
fn read<T: Copy>(
    table: &Mutex<HashMap<u16, T>>,
    address: Address,
    quantity: Quantity,
) -> Result<Vec<T>, ExceptionCode> {
    let table = table.lock().expect("the example never panics under lock");
    (0..quantity.0)
        .map(|offset| {
            let at = address
                .0
                .checked_add(offset)
                .ok_or(ExceptionCode::IllegalDataAddress)?;
            table
                .get(&at)
                .copied()
                .ok_or(ExceptionCode::IllegalDataAddress)
        })
        .collect()
}

/// Write consecutive values into one table, refusing any address it lacks.
fn write<T: Copy>(
    table: &Mutex<HashMap<u16, T>>,
    address: Address,
    values: &[T],
) -> Result<(), ExceptionCode> {
    let mut table = table.lock().expect("the example never panics under lock");
    for (offset, value) in values.iter().enumerate() {
        let offset = u16::try_from(offset).map_err(|_| ExceptionCode::IllegalDataValue)?;
        let at = address
            .0
            .checked_add(offset)
            .ok_or(ExceptionCode::IllegalDataAddress)?;
        if !table.contains_key(&at) {
            return Err(ExceptionCode::IllegalDataAddress);
        }
        table.insert(at, *value);
    }
    Ok(())
}

impl Service for Device {
    async fn on_request(
        &self,
        conn: &Connection,
        unit: UnitId,
        request: RequestPdu,
    ) -> Result<ResponsePdu, ExceptionCode> {
        println!("[{:?}] unit {} {:?}", conn.id(), unit.0, request);
        match request {
            RequestPdu::ReadCoils { address, quantity } => Ok(ResponsePdu::ReadCoils {
                coils: read(&self.coils, address, quantity)?,
            }),
            RequestPdu::ReadDiscreteInputs { address, quantity } => {
                Ok(ResponsePdu::ReadDiscreteInputs {
                    inputs: read(&self.discrete, address, quantity)?,
                })
            }
            RequestPdu::ReadHoldingRegisters { address, quantity } => {
                Ok(ResponsePdu::ReadHoldingRegisters {
                    registers: read(&self.holding, address, quantity)?
                        .into_iter()
                        .map(RegisterValue)
                        .collect(),
                })
            }
            RequestPdu::ReadInputRegisters { address, quantity } => {
                Ok(ResponsePdu::ReadInputRegisters {
                    registers: read(&self.input, address, quantity)?
                        .into_iter()
                        .map(RegisterValue)
                        .collect(),
                })
            }
            RequestPdu::WriteSingleCoil { address, value } => {
                write(&self.coils, address, &[value])?;
                Ok(ResponsePdu::WriteSingleCoil { address, value })
            }
            RequestPdu::WriteSingleRegister { address, value } => {
                write(&self.holding, address, &[value.0])?;
                Ok(ResponsePdu::WriteSingleRegister { address, value })
            }
            RequestPdu::WriteMultipleCoils { address, ref coils } => {
                write(&self.coils, address, coils)?;
                Ok(ResponsePdu::WriteMultipleCoils {
                    address,
                    quantity: Quantity(u16::try_from(coils.len()).unwrap_or(u16::MAX)),
                })
            }
            RequestPdu::WriteMultipleRegisters {
                address,
                ref registers,
            } => {
                let values: Vec<u16> = registers.iter().map(|value| value.0).collect();
                write(&self.holding, address, &values)?;
                Ok(ResponsePdu::WriteMultipleRegisters {
                    address,
                    quantity: Quantity(u16::try_from(values.len()).unwrap_or(u16::MAX)),
                })
            }
            // Everything else this device does not implement (SV-R-012).
            _ => Err(ExceptionCode::IllegalFunction),
        }
    }

    async fn on_connect(&self, conn: &Connection) -> rust_modbus::Acceptance {
        println!("[{:?}] connected from {:?}", conn.id(), conn.peer());
        rust_modbus::Acceptance::Accept
    }

    async fn on_disconnect(&self, conn: &Connection, reason: Disconnect) {
        println!("[{:?}] disconnected: {reason:?}", conn.id());
    }

    async fn on_error(&self, conn: &Connection, error: &rust_modbus::Error) {
        println!("[{:?}] error: {error}", conn.id());
    }
}

#[tokio::main]
async fn main() -> rust_modbus::Result<()> {
    let address: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:5030".to_owned())
        .parse()
        .expect("the first argument is a socket address");

    let device = Device::default();
    let listener = TcpListener::bind(address).await?;
    println!("listening on {}", listener.local_addr()?);

    let server = Server::with_config(
        device,
        ServerConfig {
            unit: Some(UnitId(1)),
        },
    );
    let handle = server.handle();

    // An optional second argument shuts the server down after that many seconds,
    // which is what makes this usable from a script: it demonstrates the drain of
    // SV-R-044 and needs no signal handling (tokio's `signal` feature is not a
    // dependency of this crate).
    if let Some(seconds) = std::env::args().nth(2) {
        let seconds: u64 = seconds.parse().expect("the second argument is seconds");
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
            println!("shutting down");
            handle.shutdown().await;
        });
    }

    server.serve(listener).await
}
