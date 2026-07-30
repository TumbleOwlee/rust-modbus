//! Recovery from a corrupted frame on a self-locating framing, end to end
//! (FR-R-144, CL-R-023, SV-R-050, TR-R-044).
//!
//! This crate's own client against this crate's own server over a link with a
//! noisy cable in the middle: a relay that corrupts one nominated frame and
//! passes everything else through untouched. Both self-locating framings are
//! run through it — RTU, delimited by silence, and ASCII, delimited by
//! characters. The unit tests pin each layer's decision in isolation; these are
//! the tests that would have failed before the change, because before it a
//! single corrupted frame ended the exchange for good.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_modbus::{
    Address, Ascii, Client, ClientConfig, ClientFraming, Connection, Disconnect, Error,
    ExceptionCode, FrameTransport, Quantity, RegisterValue, RequestPdu, ResponsePdu, Rtu, Server,
    ServerConfig, ServerFraming, Service, UnitId,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, duplex, split};

/// The unit every exchange below addresses.
const UNIT: UnitId = UnitId(0x11);

/// The register the service answers from, and its value.
const ADDRESS: Address = Address(4);
const VALUE: RegisterValue = RegisterValue(0x022B);

/// Shorter than the 1 s default (CL-R-030), so a link that has genuinely gone
/// silent fails the test quickly rather than stalling the suite.
const TIMEOUT: Duration = Duration::from_millis(200);

/// What a connection's callbacks recorded.
#[derive(Debug, Default)]
struct Log {
    requests: usize,
    errors: Vec<Error>,
    disconnects: Vec<Disconnect>,
}

/// A service that answers one register and records what happened to it.
#[derive(Debug, Clone, Default)]
struct Recorder {
    log: Arc<Mutex<Log>>,
}

impl Recorder {
    fn log(&self) -> std::sync::MutexGuard<'_, Log> {
        self.log.lock().expect("no test poisons the lock")
    }
}

impl Service for Recorder {
    async fn on_request(
        &self,
        _conn: &Connection,
        _unit: UnitId,
        request: RequestPdu,
    ) -> Result<ResponsePdu, ExceptionCode> {
        self.log().requests += 1;
        match request {
            RequestPdu::ReadHoldingRegisters { quantity, .. } => {
                Ok(ResponsePdu::ReadHoldingRegisters {
                    registers: vec![VALUE; usize::from(quantity.0)],
                })
            }
            _ => Err(ExceptionCode::IllegalFunction),
        }
    }

    async fn on_error(&self, _conn: &Connection, error: &Error) {
        self.log().errors.push(error.clone());
    }

    async fn on_disconnect(&self, _conn: &Connection, reason: Disconnect) {
        self.log().disconnects.push(reason);
    }
}

/// Corrupt an RTU frame: the last byte is half its CRC (FR-R-100).
fn corrupt_crc(frame: &mut [u8]) {
    if let Some(last) = frame.last_mut() {
        *last ^= 0xFF;
    }
}

/// Corrupt an ASCII frame: the two characters before its CRLF are the LRC
/// (FR-R-110), and a different hex digit there is still a hex digit — the frame
/// is delimited exactly as before and fails only its integrity check.
fn corrupt_lrc(frame: &mut [u8]) {
    let at = frame.len().saturating_sub(3);
    if let Some(digit) = frame.get_mut(at) {
        *digit = if *digit == b'0' { b'1' } else { b'0' };
    }
}

/// Copy bytes from `from` to `to`, applying `corrupt` to the `nth` frame.
///
/// A frame here is one write's worth of bytes, which is what a transport emits
/// per ADU: the relay is a noisy cable, not a Modbus participant. Each framing
/// is corrupted where its integrity check lives and nowhere near its boundary,
/// so what the far end meets is a delimited frame that fails to verify — the
/// case FR-R-144 is about. `nth` of 0 corrupts nothing.
async fn relay<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut from: R,
    mut to: W,
    nth: usize,
    corrupt: fn(&mut [u8]),
) {
    let mut seen = 0usize;
    let mut chunk = [0u8; 512];
    loop {
        let read = match from.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        seen += 1;
        let Some(frame) = chunk.get_mut(..read) else {
            return;
        };
        if seen == nth {
            corrupt(frame);
        }
        if to.write_all(frame).await.is_err() {
            return;
        }
    }
}

/// A client and a served link, with a relay corrupting the `to_server`th frame
/// on the way out and the `to_client`th frame on the way back.
fn noisy_link<F>(
    to_server: usize,
    to_client: usize,
    corrupt: fn(&mut [u8]),
) -> (Client<DuplexStream, F>, Recorder)
where
    F: ClientFraming + ServerFraming + Send + 'static,
    F::Header: Send + Sync,
{
    let (client_end, client_side) = duplex(1024);
    let (server_side, server_end) = duplex(1024);
    let (from_client, to_client_half) = split(client_side);
    let (from_server, to_server_half) = split(server_side);
    tokio::spawn(relay(from_client, to_server_half, to_server, corrupt));
    tokio::spawn(relay(from_server, to_client_half, to_client, corrupt));

    let service = Recorder::default();
    let server = Server::with_config(service.clone(), ServerConfig { unit: Some(UNIT) });
    tokio::spawn(server.serve_link(FrameTransport::<_, F>::new(server_end)));

    let client = Client::with_config(
        FrameTransport::new(client_end),
        ClientConfig {
            response_timeout: TIMEOUT,
        },
    );
    (client, service)
}

/// A corrupted *response* costs exactly one request and no more: the client
/// reports the integrity failure, stays synchronized, and its next request is
/// answered over the same link.
async fn corrupted_response_costs_one_request<F>(corrupt: fn(&mut [u8]))
where
    F: ClientFraming + ServerFraming + Send + 'static,
    F::Header: Send + Sync,
{
    let (mut client, service) = noisy_link::<F>(0, 1, corrupt);

    let failed = client
        .read_holding_registers(UNIT, ADDRESS, Quantity(1))
        .await;
    assert!(
        matches!(failed, Err(Error::Checksum { .. })),
        "expected the corrupted frame to fail its integrity check, got {failed:?}"
    );
    assert!(
        !client.is_desynchronized(),
        "the frame carried its own boundary; the link is still usable"
    );

    assert_eq!(
        client
            .read_holding_registers(UNIT, ADDRESS, Quantity(1))
            .await
            .expect("the link survived the corrupted response"),
        vec![VALUE]
    );

    let log = service.log();
    assert_eq!(log.requests, 2, "the server answered both requests");
    assert!(
        log.errors.is_empty(),
        "nothing reached the server corrupted: {:?}",
        log.errors
    );
}

/// A corrupted *request* does not take the server off the bus: it reports the
/// failure, answers nothing, and serves the next request over the same link.
/// That the next request is answered at all is what proves the far end never
/// went away.
///
/// The corrupted request is a broadcast, which no server answers (CL-R-053), so
/// the client is never left waiting for a reply that the corruption destroyed —
/// a wait it could only end by timing out, which desynchronizes it whatever the
/// framing (CL-R-031) and would prove nothing about the server.
async fn corrupted_request_leaves_the_server_serving<F>(corrupt: fn(&mut [u8]))
where
    F: ClientFraming + ServerFraming + Send + 'static,
    F::Header: Send + Sync,
{
    let (mut client, service) = noisy_link::<F>(1, 0, corrupt);

    assert_eq!(
        client
            .call(
                UnitId(0),
                RequestPdu::WriteSingleRegister {
                    address: ADDRESS,
                    value: VALUE,
                },
            )
            .await
            .expect("a broadcast is sent, not awaited"),
        None
    );
    // A gap between the corrupted frame and the next one. RTU needs it — its
    // boundary *is* the gap — and nothing here waits for a reply that would
    // otherwise provide it.
    tokio::time::sleep(Duration::from_millis(10)).await;

    assert_eq!(
        client
            .read_holding_registers(UNIT, ADDRESS, Quantity(1))
            .await
            .expect("the server is still serving the link"),
        vec![VALUE]
    );

    let log = service.log();
    assert_eq!(
        log.requests, 1,
        "the corrupted request never reached dispatch"
    );
    assert!(
        matches!(log.errors.as_slice(), [Error::Checksum { .. }]),
        "the server reported the corrupted frame exactly once: {:?}",
        log.errors
    );
    assert!(
        log.disconnects.is_empty(),
        "and stayed on the bus: {:?}",
        log.disconnects
    );
}

#[tokio::test]
/// CL-R-023, FR-R-144 — one corrupted response on RTU, whose boundary is
/// silence, costs one request.
async fn it_corrupted_response_costs_one_request_on_rtu() {
    corrupted_response_costs_one_request::<Rtu>(corrupt_crc).await;
}

#[tokio::test]
/// TR-R-044 — one corrupted response on ASCII, whose boundary is a delimiter,
/// costs one request.
async fn it_corrupted_response_costs_one_request_on_ascii() {
    corrupted_response_costs_one_request::<Ascii>(corrupt_lrc).await;
}

#[tokio::test]
/// SV-R-050 — one corrupted request on RTU does not end the connection.
async fn it_corrupted_request_leaves_the_server_serving_on_rtu() {
    corrupted_request_leaves_the_server_serving::<Rtu>(corrupt_crc).await;
}

#[tokio::test]
/// SV-R-051 — one corrupted request on ASCII does not end the connection
/// either, and the failure never leaves `serve_link`.
async fn it_corrupted_request_leaves_the_server_serving_on_ascii() {
    corrupted_request_leaves_the_server_serving::<Ascii>(corrupt_lrc).await;
}
