//! The UDP server entry point, answered by this crate's own UDP client
//! (SV-R-057, SV-R-058).
//!
//! Mirrors tests/server_tcp.rs's role: puts both halves of the crate
//! together on a socket, which the unit tests (src/server/mod.rs) cannot.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use rust_modbus::{
    Address, Connection, ExceptionCode, MbapHeader, Quantity, RegisterValue, RequestPdu,
    ResponsePdu, Server, Service, TransactionId, UdpConfig, UnitId, connect_udp,
};

/// An ephemeral loopback address: port 0, so the kernel assigns one.
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
/// SV-R-057 — a write followed by a read, both over UDP, round-trip through
/// this crate's own client and server with no connection ever established.
async fn it_server_answers_udp_client_end_to_end() {
    let socket = tokio::net::UdpSocket::bind(ephemeral())
        .await
        .expect("binds");
    let addr = socket.local_addr().expect("reports its address");
    let serving = tokio::spawn(Server::new(Registers::default()).serve_udp(socket));

    let mut client = connect_udp(addr, UdpConfig::default())
        .await
        .expect("connects");
    client
        .send_request(
            &MbapHeader {
                transaction_id: TransactionId(1),
                unit_id: UnitId(1),
            },
            &RequestPdu::WriteSingleRegister {
                address: Address(5),
                value: RegisterValue(42),
            },
        )
        .await
        .expect("sends a write");
    client
        .recv_response()
        .await
        .expect("receives the write's response");

    client
        .send_request(
            &MbapHeader {
                transaction_id: TransactionId(2),
                unit_id: UnitId(1),
            },
            &RequestPdu::ReadHoldingRegisters {
                address: Address(5),
                quantity: Quantity(1),
            },
        )
        .await
        .expect("sends a read");
    assert_eq!(
        client.recv_response().await,
        Ok((
            MbapHeader {
                transaction_id: TransactionId(2),
                unit_id: UnitId(1),
            },
            ResponsePdu::ReadHoldingRegisters {
                registers: vec![RegisterValue(42)]
            }
        ))
    );
    serving.abort();
}
