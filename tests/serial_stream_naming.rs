//! The serial stream type is nameable through this crate (TR-R-034).
//!
//! An integration test rather than a unit test on purpose: it has to see the
//! crate the way a consumer does, through its public exports only. A unit test
//! inside the crate can always reach `tokio_serial` directly and would prove
//! nothing about what a consumer can name.
#![cfg(feature = "rtu")]

use rust_modbus::{Ascii, AsciiClient, Client, Rtu, RtuClient, SerialStream, SerialTransport};

/// A consumer's own type, generic over the stream — the shape that had no
/// spellable RTU instantiation before this export existed.
///
/// Never constructed on purpose: constructing one needs a real serial port,
/// and the claim under test is that the type can be *named*, which the
/// compiler settles without a device.
#[allow(dead_code)]
struct Poller<S, F> {
    _client: Client<S, F>,
}

#[test]
/// TR-R-034 — the exported stream type is the one the public serial signatures
/// are written in: each conversion below compiles only if `SerialStream` names
/// exactly the type behind `RtuClient`, `AsciiClient` and `SerialTransport`,
/// and a consumer's own generic type can be instantiated with it.
fn it_serial_stream_names_the_type_in_the_public_signatures() {
    // Identity conversions in both directions. A distinct-but-similar type
    // would fail to compile here, which is the whole assertion.
    fn _rtu_client_is_client_of_serial_stream(client: RtuClient) -> Client<SerialStream, Rtu> {
        client
    }
    fn _client_of_serial_stream_is_rtu_client(client: Client<SerialStream, Rtu>) -> RtuClient {
        client
    }
    fn _ascii_client_is_client_of_serial_stream(
        client: AsciiClient,
    ) -> Client<SerialStream, Ascii> {
        client
    }
    fn _serial_transport_is_over_serial_stream(
        transport: SerialTransport<Rtu>,
    ) -> rust_modbus::FrameTransport<SerialStream, Rtu> {
        transport
    }

    // The consumer-side shape from the issue: a struct field holding a client
    // whose stream is named, not hidden behind `impl Trait`.
    fn _poller_over_serial(poller: Poller<SerialStream, Rtu>) -> Poller<SerialStream, Rtu> {
        poller
    }

    // The test body itself has nothing to run: every assertion above is
    // discharged by the compiler. Stating that is more honest than an
    // `assert!(true)` dressed up as a check.
}
