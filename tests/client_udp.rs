//! `UdpClient` end to end against this crate's own UDP server (CL-R-081).

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use rust_modbus::{
    Address, Connection, ExceptionCode, Quantity, RegisterValue, RequestPdu, ResponsePdu, Server,
    Service, UdpClient, UdpConfig, UnitId, connect_udp,
};

fn ephemeral() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
}

#[derive(Debug, Clone, Default)]
struct Registers(Arc<Mutex<HashMap<u16, u16>>>);

impl Service for Registers {
    async fn on_request(
        &self,
        _conn: &Connection,
        _unit: UnitId,
        request: RequestPdu,
    ) -> Result<ResponsePdu, ExceptionCode> {
        match request {
            RequestPdu::WriteSingleRegister { address, value } => {
                self.0
                    .lock()
                    .expect("no test poisons the lock")
                    .insert(address.0, value.0);
                Ok(ResponsePdu::WriteSingleRegister { address, value })
            }
            RequestPdu::ReadHoldingRegisters { address, quantity } => {
                let table = self.0.lock().expect("no test poisons the lock");
                let registers = (0..quantity.0)
                    .map(|offset| {
                        table
                            .get(&(address.0 + offset))
                            .copied()
                            .map(RegisterValue)
                            .ok_or(ExceptionCode::IllegalDataAddress)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ResponsePdu::ReadHoldingRegisters { registers })
            }
            _ => Err(ExceptionCode::IllegalFunction),
        }
    }
}

#[tokio::test]
/// CL-R-081 — a `UdpClient` built from `connect_udp`'s output round-trips a
/// write and a read through `Client`'s typed request methods, against this
/// crate's own `serve_udp`.
async fn it_udp_client_round_trips_typed_requests() {
    let socket = tokio::net::UdpSocket::bind(ephemeral())
        .await
        .expect("binds");
    let addr = socket.local_addr().expect("reports its address");
    let serving = tokio::spawn(Server::new(Registers::default()).serve_udp(socket));

    let transport = connect_udp(addr, UdpConfig::default())
        .await
        .expect("connects");
    let mut client: UdpClient = rust_modbus::Client::new(transport);

    client
        .write_single_register(UnitId(1), Address(5), RegisterValue(42))
        .await
        .expect("writes");
    let read = client
        .read_holding_registers(UnitId(1), Address(5), Quantity(1))
        .await
        .expect("reads");
    assert_eq!(read, vec![RegisterValue(42)]);

    serving.abort();
}
