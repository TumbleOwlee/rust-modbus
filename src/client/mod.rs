//! Async Modbus client (initiator). See `docs/specs/client/`.

mod framing;
#[cfg(feature = "sync")]
mod sync;

use core::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::Instant;

use crate::error::{Error, Result};
use alloc::vec::Vec;

use crate::frame::{
    Address, DiagnosticSubFunction, ExceptionStatus, FileRecordRead, FileRecordReadResponse,
    FileRecordWrite, FunctionCode, Mask, MeiRequest, MeiResponse, Quantity, RegisterValue,
    RequestPdu, ResponsePdu, TransactionId, UnitId,
};
use crate::transport::FrameTransport;

pub use framing::ClientFraming;
#[cfg(all(feature = "sync", feature = "rtu"))]
pub use sync::{SyncAsciiClient, SyncRtuClient};
#[cfg(feature = "sync")]
pub use sync::{SyncClient, SyncRtuOverTcpClient, SyncTcpClient};

/// How a client waits (CL-R-030).
///
/// One field: CL-R-033 rules out retry and reconnect, so there is no policy for
/// them to configure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClientConfig {
    /// How long a response may take before the exchange is abandoned.
    pub response_timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            response_timeout: Duration::from_secs(1),
        }
    }
}

/// Why a client became unusable (CL-R-037).
///
/// These values name the observation the client made at the moment it became
/// desynchronized. They report what the client observed, never whether the peer
/// is alive — on TCP a peer that vanished without a FIN is indistinguishable
/// from an idle one until something is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnusableReason {
    /// The peer's end of stream was seen (CL-R-037).
    PeerClosed,
    /// The platform reported an I/O failure in either direction (CL-R-037).
    Io {
        /// The I/O error kind the platform reported.
        kind: std::io::ErrorKind,
    },
    /// The response timeout elapsed with no matching response (CL-R-031,
    /// CL-R-037).
    Silent,
    /// A frame did not decode on a framing that is not self-locating
    /// (CL-R-023, CL-R-037).
    Undecodable,
}

/// What a client knows about its own usability (CL-R-035).
///
/// These values report what *this client observed*, never whether the peer is
/// alive: on TCP a peer that vanished without a FIN is indistinguishable from an
/// idle one until something is written. [`ClientState::Answered`] is therefore a
/// statement about the past, and [`ClientState::Untried`] a statement about this
/// client. There is no liveness probe (CL-R-039) — proving a server still
/// answers costs a request, and which requests reach the wire is the caller's to
/// authorize (CL-R-033).
///
/// [`ClientState::Answered`] says the peer replied to the last request this
/// client sent, not that one would reply now. On TCP a peer that vanished
/// without a FIN is indistinguishable from an idle one until bytes are written,
/// so no local check can do better. A failover built on [`ClientState::Answered`]
/// meaning "alive" is built on a guarantee that does not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    /// No exchange has been attempted, or only broadcast writes have been
    /// (CL-R-036).
    Untried,
    /// The last exchange was answered, including with an exception (CL-R-036).
    Answered,
    /// The last exchange was not answered, and the client is still usable
    /// (CL-R-023).
    Unanswered,
    /// Every further request will be refused (CL-R-032). The reason names what
    /// the client observed at the moment it became so.
    Unusable(UnusableReason),
}

/// A client over a TCP socket.
pub type TcpClient = Client<tokio::net::TcpStream, crate::frame::Tcp>;

/// A client over a TCP socket carrying RTU-over-stream framing, for a
/// transparent serial gateway (TR-R-024).
///
/// Unlike [`RtuClient`], this is not behind the `rtu` feature: it opens no
/// serial port (TR-R-033).
pub type RtuOverTcpClient = Client<tokio::net::TcpStream, crate::frame::RtuOverTcp>;

/// A client over a serial line in RTU framing.
#[cfg(feature = "rtu")]
pub type RtuClient = Client<tokio_serial::SerialStream, crate::frame::Rtu>;

/// A client over a serial line in ASCII framing.
#[cfg(feature = "rtu")]
pub type AsciiClient = Client<tokio_serial::SerialStream, crate::frame::Ascii>;

/// A Modbus client (CL-R-001).
///
/// One type for every framing: what differs between RTU, ASCII, and TCP is
/// named by [`ClientFraming`] and nothing else.
///
/// Every request takes `&mut self`, which is how CL-R-005 holds without a
/// run-time flag: the borrow checker permits one exchange at a time.
#[derive(Debug)]
pub struct Client<S, F> {
    /// The established transport this client speaks over (CL-R-002).
    transport: FrameTransport<S, F>,
    /// How long a response may take (CL-R-030).
    config: ClientConfig,
    /// The identifier the next request will carry (CL-R-011).
    next_transaction: TransactionId,
    /// What this client has observed about its own usability (CL-R-035). The
    /// desynchronization flag of CL-R-031 is one of its cases rather than a
    /// second value beside it, so the two cannot disagree (CL-R-034).
    state: ClientState,
}

impl<S, F> Client<S, F>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    F: ClientFraming,
{
    /// Build a client over an established transport, with the default
    /// configuration (CL-R-002).
    pub fn new(transport: FrameTransport<S, F>) -> Self {
        Self::with_config(transport, ClientConfig::default())
    }

    /// Build a client over an established transport.
    pub fn with_config(transport: FrameTransport<S, F>, config: ClientConfig) -> Self {
        Self {
            transport,
            config,
            // Identifier 0 is never allocated, so a matched response is never
            // matched against an unset field (CL-R-011).
            next_transaction: TransactionId(1),
            state: ClientState::Untried,
        }
    }

    /// Surrender the transport (CL-R-006).
    ///
    /// The only recovery from desynchronization is to discard the client; this
    /// is how the connection underneath can be inspected or replaced.
    pub fn into_inner(self) -> FrameTransport<S, F> {
        self.transport
    }

    /// Whether this client has given up on the byte stream (CL-R-034).
    ///
    /// A projection of [`Client::state`], not a value beside it: the two cannot
    /// disagree.
    #[must_use]
    pub fn is_desynchronized(&self) -> bool {
        matches!(self.state, ClientState::Unusable(_))
    }

    /// What this client knows about its own usability (CL-R-035).
    ///
    /// Answers from what has already been observed: it touches neither the
    /// transport nor the clock, and never blocks (CL-R-038). It reports this
    /// client's history, **not** whether the peer is alive — see [`ClientState`]
    /// for why no local check can do better.
    #[must_use]
    pub fn state(&self) -> ClientState {
        self.state
    }

    /// Issue a request and return the response as received (CL-R-061).
    ///
    /// An exception response is returned, not raised: `call` does not
    /// reinterpret what the server said (CL-R-041). `None` means the request
    /// was a broadcast, which no server answers (CL-R-053).
    ///
    /// # Errors
    ///
    /// Fails if the request cannot be encoded, if the transport fails, if no
    /// matching response arrives within the response timeout, or if the client
    /// is already desynchronized.
    pub async fn call(&mut self, unit: UnitId, request: RequestPdu) -> Result<Option<ResponsePdu>> {
        if self.is_desynchronized() {
            return Err(Error::Desynchronized);
        }

        // Encoded before an identifier is spent or a byte is written, so an
        // unencodable request costs nothing (CL-R-012).
        let expected = request.function();
        request.encode()?;

        let transaction = self.next_transaction;
        let header = F::request_header(unit, transaction);
        if let Err(error) = self.transport.send_request(&header, &request).await {
            // The ADU may be on the wire in part. Nothing a later request can
            // do repairs that, so the failure is not recoverable (CL-R-013).
            self.state = ClientState::Unusable(classify_unusable_reason(&error));
            return Err(error);
        }
        self.next_transaction = next(transaction);

        if F::is_broadcast(unit) {
            // Nothing was heard, and nothing was expected to be: a broadcast is
            // no evidence either way, so the report stands as it was (CL-R-036).
            return Ok(None);
        }

        // Absolute, and fixed once the request is on the wire: waiting is never
        // extended by the time spent writing (CL-R-014) or by discarding a
        // response that was not ours (CL-R-021).
        let deadline = Instant::now() + self.config.response_timeout;
        loop {
            let received =
                match tokio::time::timeout_at(deadline, self.transport.recv_response()).await {
                    Ok(result) => result,
                    Err(_elapsed) => {
                        // Nothing failed; the wait ran out. What the peer sends
                        // next is now unaccounted for (CL-R-031).
                        self.state = ClientState::Unusable(UnusableReason::Silent);
                        return Err(Error::Timeout { what: "response" });
                    }
                };
            let (header_in, response) = match received {
                Ok(received) => received,
                Err(error) => {
                    // An I/O failure ends the stream whatever the framing
                    // (CL-R-031). A *frame* failure costs the link only where
                    // the next boundary was carried by the frame that failed;
                    // silence and delimiters are still on the wire, so there
                    // the failure costs exactly that frame (CL-R-023).
                    self.state = if error.ends_stream() || !F::boundary().is_self_locating() {
                        ClientState::Unusable(classify_unusable_reason(&error))
                    } else {
                        // The link survives, but this exchange got no answer
                        // (CL-R-035).
                        ClientState::Unanswered
                    };
                    return Err(error);
                }
            };
            if !F::is_response_to(&header, &header_in) {
                // Another exchange's reply, or a late one. Discard it and keep
                // waiting against the same deadline (CL-R-021).
                continue;
            }
            // The peer answered. What it said may still be wrong, but the link
            // carried a frame that corresponds to the request (CL-R-036).
            self.state = ClientState::Answered;
            let actual = response.function();
            if actual != expected {
                return Err(Error::UnexpectedFunction { expected, actual });
            }
            return Ok(Some(response));
        }
    }

    /// Issue a request whose answer the caller needs (CL-R-052).
    ///
    /// Every typed method funnels through here, so the exception mapping of
    /// CL-R-040 and the broadcast rule of CL-R-052 each exist once.
    async fn exchange(&mut self, unit: UnitId, request: RequestPdu) -> Result<ResponsePdu> {
        if F::is_broadcast(unit) {
            // Rejected before `call`, so nothing reaches the wire (CL-R-052).
            return Err(Error::IllegalValue {
                field: "broadcast read",
                value: 0,
            });
        }
        let response = self
            .call(unit, request)
            .await?
            .ok_or(Error::Desynchronized)?;
        match response {
            ResponsePdu::Exception(exception) => Err(Error::Exception {
                function: exception.function,
                exception: exception.exception,
            }),
            response => Ok(response),
        }
    }

    /// Issue a request whose answer carries nothing but an echo (CL-R-064).
    ///
    /// A broadcast is written and not awaited (CL-R-051).
    async fn write(&mut self, unit: UnitId, request: RequestPdu) -> Result<()> {
        if F::is_broadcast(unit) {
            self.call(unit, request).await?;
            return Ok(());
        }
        self.exchange(unit, request).await?;
        Ok(())
    }

    /// Read coils, function code 1 (CL-R-060).
    ///
    /// # Errors
    ///
    /// Fails on an exception response, on a transport failure, on a timeout, or
    /// if `unit` is a broadcast address (CL-R-052).
    pub async fn read_coils(
        &mut self,
        unit: UnitId,
        address: Address,
        quantity: Quantity,
    ) -> Result<Vec<bool>> {
        let response = self
            .exchange(unit, RequestPdu::ReadCoils { address, quantity })
            .await?;
        match response {
            ResponsePdu::ReadCoils { coils } => Ok(truncate(coils, quantity)),
            other => Err(mismatch(FunctionCode::ReadCoils, &other)),
        }
    }

    /// Read discrete inputs, function code 2 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`].
    pub async fn read_discrete_inputs(
        &mut self,
        unit: UnitId,
        address: Address,
        quantity: Quantity,
    ) -> Result<Vec<bool>> {
        let response = self
            .exchange(unit, RequestPdu::ReadDiscreteInputs { address, quantity })
            .await?;
        match response {
            ResponsePdu::ReadDiscreteInputs { inputs } => Ok(truncate(inputs, quantity)),
            other => Err(mismatch(FunctionCode::ReadDiscreteInputs, &other)),
        }
    }

    /// Read holding registers, function code 3 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`].
    pub async fn read_holding_registers(
        &mut self,
        unit: UnitId,
        address: Address,
        quantity: Quantity,
    ) -> Result<Vec<RegisterValue>> {
        let response = self
            .exchange(unit, RequestPdu::ReadHoldingRegisters { address, quantity })
            .await?;
        match response {
            ResponsePdu::ReadHoldingRegisters { registers } => Ok(registers),
            other => Err(mismatch(FunctionCode::ReadHoldingRegisters, &other)),
        }
    }

    /// Read input registers, function code 4 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`].
    pub async fn read_input_registers(
        &mut self,
        unit: UnitId,
        address: Address,
        quantity: Quantity,
    ) -> Result<Vec<RegisterValue>> {
        let response = self
            .exchange(unit, RequestPdu::ReadInputRegisters { address, quantity })
            .await?;
        match response {
            ResponsePdu::ReadInputRegisters { registers } => Ok(registers),
            other => Err(mismatch(FunctionCode::ReadInputRegisters, &other)),
        }
    }

    /// Write a single coil, function code 5 (CL-R-060).
    ///
    /// # Errors
    ///
    /// Fails on an exception response, on a transport failure, or on a timeout.
    /// A broadcast is written and not awaited (CL-R-051).
    pub async fn write_single_coil(
        &mut self,
        unit: UnitId,
        address: Address,
        value: bool,
    ) -> Result<()> {
        self.write(unit, RequestPdu::WriteSingleCoil { address, value })
            .await
    }

    /// Write a single register, function code 6 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::write_single_coil`].
    pub async fn write_single_register(
        &mut self,
        unit: UnitId,
        address: Address,
        value: RegisterValue,
    ) -> Result<()> {
        self.write(unit, RequestPdu::WriteSingleRegister { address, value })
            .await
    }

    /// Write multiple coils, function code 15 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::write_single_coil`].
    pub async fn write_multiple_coils(
        &mut self,
        unit: UnitId,
        address: Address,
        coils: &[bool],
    ) -> Result<()> {
        self.write(
            unit,
            RequestPdu::WriteMultipleCoils {
                address,
                coils: coils.to_vec(),
            },
        )
        .await
    }

    /// Write multiple registers, function code 16 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::write_single_coil`].
    pub async fn write_multiple_registers(
        &mut self,
        unit: UnitId,
        address: Address,
        registers: &[RegisterValue],
    ) -> Result<()> {
        self.write(
            unit,
            RequestPdu::WriteMultipleRegisters {
                address,
                registers: registers.to_vec(),
            },
        )
        .await
    }

    /// Mask-write a register, function code 22 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::write_single_coil`].
    pub async fn mask_write_register(
        &mut self,
        unit: UnitId,
        address: Address,
        and_mask: Mask,
        or_mask: Mask,
    ) -> Result<()> {
        self.write(
            unit,
            RequestPdu::MaskWriteRegister {
                address,
                and_mask,
                or_mask,
            },
        )
        .await
    }

    /// Write then read registers in one exchange, function code 23
    /// (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`]: the read half means a broadcast cannot serve.
    pub async fn read_write_multiple_registers(
        &mut self,
        unit: UnitId,
        read_address: Address,
        read_quantity: Quantity,
        write_address: Address,
        registers: &[RegisterValue],
    ) -> Result<Vec<RegisterValue>> {
        let response = self
            .exchange(
                unit,
                RequestPdu::ReadWriteMultipleRegisters {
                    read_address,
                    read_quantity,
                    write_address,
                    registers: registers.to_vec(),
                },
            )
            .await?;
        match response {
            ResponsePdu::ReadWriteMultipleRegisters { registers } => Ok(registers),
            other => Err(mismatch(FunctionCode::ReadWriteMultipleRegisters, &other)),
        }
    }

    /// Read the exception status output byte, function code 7 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`].
    pub async fn read_exception_status(&mut self, unit: UnitId) -> Result<ExceptionStatus> {
        let response = self.exchange(unit, RequestPdu::ReadExceptionStatus).await?;
        match response {
            ResponsePdu::ReadExceptionStatus { status } => Ok(status),
            other => Err(mismatch(FunctionCode::ReadExceptionStatus, &other)),
        }
    }

    /// Run a diagnostic sub-function, function code 8 (CL-R-060).
    ///
    /// The data words are raw: what they mean is the sub-function's to decide
    /// (FR-R-062), so naming them a domain value would claim more than is true.
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`].
    pub async fn diagnostics(
        &mut self,
        unit: UnitId,
        sub_function: DiagnosticSubFunction,
        data: &[u16],
    ) -> Result<Vec<u16>> {
        let response = self
            .exchange(
                unit,
                RequestPdu::Diagnostics {
                    sub_function,
                    data: data.to_vec(),
                },
            )
            .await?;
        match response {
            ResponsePdu::Diagnostics { data, .. } => Ok(data),
            other => Err(mismatch(FunctionCode::Diagnostics, &other)),
        }
    }

    /// Get the comm event counter, function code 11 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`].
    pub async fn get_comm_event_counter(&mut self, unit: UnitId) -> Result<CommEventCounter> {
        let response = self.exchange(unit, RequestPdu::GetCommEventCounter).await?;
        match response {
            ResponsePdu::GetCommEventCounter {
                status,
                event_count,
            } => Ok(CommEventCounter {
                status,
                event_count,
            }),
            other => Err(mismatch(FunctionCode::GetCommEventCounter, &other)),
        }
    }

    /// Get the comm event log, function code 12 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`].
    pub async fn get_comm_event_log(&mut self, unit: UnitId) -> Result<CommEventLog> {
        let response = self.exchange(unit, RequestPdu::GetCommEventLog).await?;
        match response {
            ResponsePdu::GetCommEventLog {
                status,
                event_count,
                message_count,
                events,
            } => Ok(CommEventLog {
                status,
                event_count,
                message_count,
                events,
            }),
            other => Err(mismatch(FunctionCode::GetCommEventLog, &other)),
        }
    }

    /// Report the server's identification, function code 17 (CL-R-060).
    ///
    /// The body is opaque past its byte count: how it splits into an id, a run
    /// indicator, and additional data is device-specific.
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`].
    pub async fn report_server_id(&mut self, unit: UnitId) -> Result<Vec<u8>> {
        let response = self.exchange(unit, RequestPdu::ReportServerId).await?;
        match response {
            ResponsePdu::ReportServerId { data } => Ok(data),
            other => Err(mismatch(FunctionCode::ReportServerId, &other)),
        }
    }

    /// Read file records, function code 20 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`].
    pub async fn read_file_record(
        &mut self,
        unit: UnitId,
        records: &[FileRecordRead],
    ) -> Result<Vec<FileRecordReadResponse>> {
        let response = self
            .exchange(
                unit,
                RequestPdu::ReadFileRecord {
                    records: records.to_vec(),
                },
            )
            .await?;
        match response {
            ResponsePdu::ReadFileRecord { records } => Ok(records),
            other => Err(mismatch(FunctionCode::ReadFileRecord, &other)),
        }
    }

    /// Write file records, function code 21 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::write_single_coil`].
    pub async fn write_file_record(
        &mut self,
        unit: UnitId,
        records: &[FileRecordWrite],
    ) -> Result<()> {
        self.write(
            unit,
            RequestPdu::WriteFileRecord {
                records: records.to_vec(),
            },
        )
        .await
    }

    /// Read a FIFO queue, function code 24 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`].
    pub async fn read_fifo_queue(
        &mut self,
        unit: UnitId,
        address: Address,
    ) -> Result<Vec<RegisterValue>> {
        let response = self
            .exchange(unit, RequestPdu::ReadFifoQueue { address })
            .await?;
        match response {
            ResponsePdu::ReadFifoQueue { values } => Ok(values),
            other => Err(mismatch(FunctionCode::ReadFifoQueue, &other)),
        }
    }

    /// Send an encapsulated interface request, function code 43 (CL-R-060).
    ///
    /// # Errors
    ///
    /// As [`Client::read_coils`].
    pub async fn encapsulated_interface_transport(
        &mut self,
        unit: UnitId,
        request: MeiRequest,
    ) -> Result<MeiResponse> {
        let response = self
            .exchange(unit, RequestPdu::EncapsulatedInterfaceTransport(request))
            .await?;
        match response {
            ResponsePdu::EncapsulatedInterfaceTransport(response) => Ok(response),
            other => Err(mismatch(
                FunctionCode::EncapsulatedInterfaceTransport,
                &other,
            )),
        }
    }
}

/// The two counters of a Get Comm Event Counter response (FR-R-064).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommEventCounter {
    /// Non-zero while the server is still processing a program function.
    pub status: u16,
    /// Events the server has recorded.
    pub event_count: u16,
}

/// A Get Comm Event Log response (FR-R-065).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommEventLog {
    /// Non-zero while the server is still processing a program function.
    pub status: u16,
    /// Events the server has recorded.
    pub event_count: u16,
    /// Messages the server has processed.
    pub message_count: u16,
    /// The event bytes themselves, most recent first. Their meaning is
    /// device-specific, so they are carried raw.
    pub events: Vec<u8>,
}

/// Classify an error that leaves the client desynchronized into an
/// [`UnusableReason`] (CL-R-037).
///
/// The reason names what the client observed at the moment it became unusable:
/// it does not assert anything about the peer's condition, which the client
/// cannot observe. The classification is:
/// - `ConnectionClosed` or `Io { kind: UnexpectedEof }` → `PeerClosed`
/// - Other `Io { kind }` → `Io { kind }`
/// - `Timeout` → `Silent`
/// - Anything else (frame decoding failures on TCP) → `Undecodable`
fn classify_unusable_reason(error: &Error) -> UnusableReason {
    match error {
        Error::ConnectionClosed => UnusableReason::PeerClosed,
        Error::Io { kind } => {
            if *kind == std::io::ErrorKind::UnexpectedEof {
                UnusableReason::PeerClosed
            } else {
                UnusableReason::Io { kind: *kind }
            }
        }
        Error::Timeout { .. } => UnusableReason::Silent,
        _ => UnusableReason::Undecodable,
    }
}

/// Keep the bits asked for and drop the padding the wire adds (CL-R-062).
///
/// A bit response carries whole bytes and no quantity of its own (FR-R-044), so
/// only the request knows where the real values end.
fn truncate(mut bits: Vec<bool>, quantity: Quantity) -> Vec<bool> {
    bits.truncate(usize::from(quantity.0));
    bits
}

/// The failure for a response whose body is not its function code's.
///
/// `call` has already matched the code (CL-R-022), so this is unreachable for a
/// well-formed response; it exists because a panic on peer input is forbidden.
fn mismatch(expected: FunctionCode, actual: &ResponsePdu) -> Error {
    Error::UnexpectedFunction {
        expected,
        actual: actual.function(),
    }
}

/// The identifier following `current`, wrapping to 1 rather than to 0
/// (CL-R-011).
fn next(current: TransactionId) -> TransactionId {
    match current.0.checked_add(1) {
        Some(next) => TransactionId(next),
        None => TransactionId(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::frame::{
        Address, DiagnosticSubFunction, ExceptionCode, ExceptionResponse, ExceptionStatus,
        FileNumber, FileRecordRead, FileRecordReadResponse, FileRecordWrite, Framing, FunctionCode,
        Mask, MbapHeader, MeiRequest, MeiResponse, Quantity, ReadDeviceIdCode, RecordLength,
        RecordNumber, RegisterValue, RequestPdu, ResponsePdu, Rtu, Tcp, TransactionId, UnitId,
    };
    use crate::transport::FrameTransport;
    use alloc::vec;
    use core::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, duplex};

    /// A client and the transport a test server answers it on.
    fn pair() -> (Client<DuplexStream, Tcp>, FrameTransport<DuplexStream, Tcp>) {
        let (client, server) = duplex(1024);
        (
            Client::new(FrameTransport::new(client)),
            FrameTransport::new(server),
        )
    }

    fn read_holding() -> RequestPdu {
        RequestPdu::ReadHoldingRegisters {
            address: Address(0x006B),
            quantity: Quantity(3),
        }
    }

    fn registers() -> ResponsePdu {
        ResponsePdu::ReadHoldingRegisters {
            registers: vec![RegisterValue(0x022B)],
        }
    }

    #[tokio::test]
    /// CL-R-010, CL-R-061 — a raw call writes the request and yields the
    /// response as received.
    async fn ut_call_round_trips_a_request() {
        let (mut client, mut server) = pair();
        let answering = tokio::spawn(async move {
            let (header, request) = server.recv_request().await.expect("receives");
            assert_eq!(request, read_holding());
            server
                .send_response(&header, &registers())
                .await
                .expect("responds");
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Ok(Some(registers()))
        );
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-011 — transaction identifiers start at 1 and advance by one per
    /// request, so two requests are distinguishable on the wire.
    async fn ut_transaction_ids_start_at_one_and_advance() {
        let (mut client, mut server) = pair();
        let answering = tokio::spawn(async move {
            let mut seen = vec![];
            for _ in 0..2 {
                let (header, _) = server.recv_request().await.expect("receives");
                seen.push(header.transaction_id);
                server
                    .send_response(&header, &registers())
                    .await
                    .expect("responds");
            }
            seen
        });

        for _ in 0..2 {
            client
                .call(UnitId(0x11), read_holding())
                .await
                .expect("calls");
        }
        assert_eq!(
            answering.await.expect("the server task finishes"),
            vec![TransactionId(1), TransactionId(2)]
        );
    }

    #[tokio::test]
    /// CL-R-011 — the sequence wraps to 1, never to 0: an unallocated
    /// identifier must not be matchable.
    async fn ut_transaction_ids_wrap_past_zero() {
        let (mut client, mut server) = pair();
        client.next_transaction = TransactionId(u16::MAX);
        let answering = tokio::spawn(async move {
            let mut seen = vec![];
            for _ in 0..2 {
                let (header, _) = server.recv_request().await.expect("receives");
                seen.push(header.transaction_id);
                server
                    .send_response(&header, &registers())
                    .await
                    .expect("responds");
            }
            seen
        });

        for _ in 0..2 {
            client
                .call(UnitId(0x11), read_holding())
                .await
                .expect("calls");
        }
        assert_eq!(
            answering.await.expect("the server task finishes"),
            vec![TransactionId(u16::MAX), TransactionId(1)]
        );
    }

    #[tokio::test]
    /// CL-R-021 — a response whose header does not answer the request is
    /// discarded and the wait continues, rather than being handed back as if it
    /// did.
    async fn ut_unmatched_response_is_discarded() {
        let (mut client, mut server) = pair();
        let answering = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            let stale = MbapHeader {
                transaction_id: TransactionId(999),
                unit_id: header.unit_id,
            };
            server
                .send_response(
                    &stale,
                    &ResponsePdu::ReadHoldingRegisters { registers: vec![] },
                )
                .await
                .expect("sends a stale response");
            server
                .send_response(&header, &registers())
                .await
                .expect("responds");
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Ok(Some(registers()))
        );
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-022 — a matching header carrying another function's response is a
    /// protocol error naming both codes, not a silent mismatch.
    async fn ut_wrong_function_code_is_an_error() {
        let (mut client, mut server) = pair();
        let answering = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            server
                .send_response(&header, &ResponsePdu::ReadCoils { coils: vec![true] })
                .await
                .expect("responds");
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::UnexpectedFunction {
                expected: FunctionCode::ReadHoldingRegisters,
                actual: FunctionCode::ReadCoils,
            })
        );
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-040, CL-R-042 — `call` hands an exception response back verbatim
    /// rather than reinterpreting it, and the client stays usable.
    async fn ut_call_returns_an_exception_response_verbatim() {
        let (mut client, mut server) = pair();
        let exception = ResponsePdu::Exception(ExceptionResponse {
            function: FunctionCode::ReadHoldingRegisters,
            exception: ExceptionCode::IllegalDataAddress,
        });
        let expected = exception.clone();
        let answering = tokio::spawn(async move {
            for _ in 0..2 {
                let (header, _) = server.recv_request().await.expect("receives");
                server
                    .send_response(&header, &exception)
                    .await
                    .expect("responds");
            }
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Ok(Some(expected))
        );
        assert!(!client.is_desynchronized());
        assert!(client.call(UnitId(0x11), read_holding()).await.is_ok());
        answering.await.expect("the server task finishes");
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-030, CL-R-031 — a silent server costs the response timeout, then
    /// fails as a timeout naming the response, and leaves the client
    /// desynchronized.
    async fn ut_silence_times_out_and_desynchronizes() {
        let (mut client, mut server) = pair();
        let silent = tokio::spawn(async move {
            let _ = server.recv_request().await;
            // Never answers, and holds the transport open so the wait is
            // silence rather than a closed connection.
            core::future::pending::<()>().await;
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Timeout { what: "response" })
        );
        assert!(client.is_desynchronized());
        silent.abort();
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-023 — over TCP, a response that cannot be decoded fails with the
    /// frame area's own decoding error, unaltered, and leaves the client
    /// unusable: the MBAP length was trusted to read the ADU off the stream, so
    /// once its contents turn out to be nonsense there is no way to know where
    /// the next response starts. The reply below is a well-formed MBAP header
    /// (transaction 1, length 3, unit 0x11) around a PDU claiming function 3
    /// with a byte count of 4 but only one register following it.
    async fn ut_undecodable_response_desynchronizes_on_tcp() {
        let (client, server) = duplex(1024);
        let mut client = Client::<_, Tcp>::new(FrameTransport::new(client));
        let mut server = server;

        let answering = tokio::spawn(async move {
            let mut request = [0u8; 12];
            server
                .read_exact(&mut request)
                .await
                .expect("the request arrives whole");
            server
                .write_all(&[
                    0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x11, 0x03, 0x04, 0x00, 0x2A,
                ])
                .await
                .expect("the reply is written");
            server
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::ByteCountMismatch {
                expected: 4,
                actual: 2
            }),
            "the frame area's own error reaches the caller unaltered"
        );
        assert!(
            client.is_desynchronized(),
            "the stream's alignment is unknown after a malformed response"
        );
        let _ = answering.await;
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-023 — over RTU the same failure costs exactly one frame. The next
    /// boundary is the line falling silent, not a length inside the frame that
    /// went wrong, so the client stays usable and the request after it is
    /// answered normally. The corrupt reply below is a valid ADU with its last
    /// CRC byte flipped.
    async fn ut_undecodable_response_costs_one_frame_on_rtu() {
        let (client, mut server) = duplex(1024);
        let mut client = Client::<_, Rtu>::new(FrameTransport::new(client));

        // The far end is a raw stream, so the corrupt frame can be written
        // exactly as a noisy line would deliver it.
        let good = Rtu::encode_response(&UnitId(0x11), &registers()).expect("the response encodes");
        let mut corrupt = good.clone();
        let last = corrupt.last_mut().expect("the ADU carries a CRC");
        *last ^= 0xFF;

        let answering = tokio::spawn(async move {
            let mut request = [0u8; 8];
            server
                .read_exact(&mut request)
                .await
                .expect("the first request arrives whole");
            server
                .write_all(&corrupt)
                .await
                .expect("writes the corrupt");
            // Silence, so the next frame is a frame of its own (TR-R-011).
            tokio::time::sleep(Duration::from_millis(5)).await;

            server
                .read_exact(&mut request)
                .await
                .expect("the second request arrives whole");
            server.write_all(&good).await.expect("writes the good one");
            server
        });

        let failed = client.call(UnitId(0x11), read_holding()).await;
        assert!(
            matches!(failed, Err(Error::Checksum { .. })),
            "the frame area's own error reaches the caller unaltered, got {failed:?}"
        );
        assert!(
            !client.is_desynchronized(),
            "one corrupt frame took the whole link down"
        );
        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Ok(Some(registers())),
            "the request after the corrupt frame was refused"
        );
        let _ = answering.await;
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-031 — a timeout still desynchronizes on RTU, unchanged by
    /// CL-R-023's framing rule: a late response carries only a unit
    /// identifier, so it would satisfy CL-R-020 for the *next* request and be
    /// delivered as that request's answer.
    async fn ut_timeout_still_desynchronizes_on_rtu() {
        let (client, server) = duplex(1024);
        let mut client = Client::<_, Rtu>::new(FrameTransport::new(client));
        let mut server = FrameTransport::<_, Rtu>::new(server);

        let silent = tokio::spawn(async move {
            let _ = server.recv_request().await;
            core::future::pending::<()>().await;
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Timeout { what: "response" })
        );
        assert!(client.is_desynchronized());
        silent.abort();
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-014 — the deadline is absolute: a stream of unmatched responses
    /// cannot hold a request open past the timeout by restarting it.
    async fn ut_discarding_does_not_extend_the_deadline() {
        let (mut client, mut server) = pair();
        let chatty = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            loop {
                let stale = MbapHeader {
                    transaction_id: TransactionId(999),
                    unit_id: header.unit_id,
                };
                if server.send_response(&stale, &registers()).await.is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(600)).await;
            }
        });

        // Each unmatched response arrives well inside the 1-second timeout; a
        // deadline that restarted on each would never expire.
        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Timeout { what: "response" })
        );
        chatty.abort();
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-032 — a desynchronized client refuses the next request outright,
    /// without writing: the peer's next bytes are no longer accounted for.
    async fn ut_desynchronized_client_writes_nothing() {
        let (mut client, mut server) = pair();
        let silent = tokio::spawn(async move {
            let first = server.recv_request().await;
            assert!(first.is_ok());
            // A second request must never arrive.
            let second = server.recv_request().await;
            assert!(second.is_err(), "a desynchronized client wrote again");
        });

        assert!(client.call(UnitId(0x11), read_holding()).await.is_err());
        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Desynchronized)
        );
        drop(client);
        silent.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-012 — a request that cannot be encoded fails without writing, and
    /// without spending a transaction identifier on it.
    async fn ut_unencodable_request_writes_nothing() {
        let (mut client, mut server) = pair();
        let unencodable = RequestPdu::ReadHoldingRegisters {
            address: Address(0),
            // Beyond the 125 of FR-R-022, so encoding rejects it.
            quantity: Quantity(0xFFFF),
        };

        assert!(matches!(
            client.call(UnitId(0x11), unencodable).await,
            Err(Error::OutOfRange { .. })
        ));
        assert!(!client.is_desynchronized());

        let answering = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            server
                .send_response(&header, &registers())
                .await
                .expect("responds");
            header.transaction_id
        });
        client
            .call(UnitId(0x11), read_holding())
            .await
            .expect("the client is still usable");
        assert_eq!(
            answering.await.expect("the server task finishes"),
            TransactionId(1),
            "the failed request must not consume an identifier"
        );
    }

    #[tokio::test]
    /// CL-R-002, CL-R-006 — a client is built from a transport and gives it
    /// back, which is what makes recovery from desynchronization possible.
    async fn ut_client_surrenders_its_transport() {
        let (client, _server) = pair();
        drop(client.into_inner());
    }

    #[test]
    /// CL-R-030 — the default response timeout is 1 second.
    fn ut_default_response_timeout() {
        assert_eq!(
            ClientConfig::default(),
            ClientConfig {
                response_timeout: Duration::from_secs(1),
            }
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    /// CL-R-065 — `ClientConfig` round-trips through JSON. The timeout keeps
    /// `Duration`'s own representation, so every value `Duration` can hold
    /// survives: one finer than a millisecond, and one far larger than any
    /// nanosecond count would fit.
    fn ut_client_config_serde_roundtrip() {
        let config = ClientConfig {
            response_timeout: Duration::from_millis(1500),
        };
        let text = serde_json::to_string(&config).expect("serializes");
        assert_eq!(text, r#"{"response_timeout":{"secs":1,"nanos":500000000}}"#);
        assert_eq!(
            serde_json::from_str::<ClientConfig>(&text).expect("deserializes"),
            config
        );

        let fine = ClientConfig {
            response_timeout: Duration::from_nanos(1_500_001),
        };
        let text = serde_json::to_string(&fine).expect("serializes");
        assert_eq!(text, r#"{"response_timeout":{"secs":0,"nanos":1500001}}"#);
        assert_eq!(
            serde_json::from_str::<ClientConfig>(&text).expect("deserializes"),
            fine
        );

        // Whole seconds past what a nanosecond count could hold: `u64::MAX`
        // nanoseconds is about 584 years, and this is `u64::MAX` *seconds*.
        // Representing the field as a nanosecond integer would fail here
        // rather than round — which is why it does not.
        let vast = ClientConfig {
            response_timeout: Duration::new(u64::MAX, 999_999_999),
        };
        assert_eq!(
            serde_json::from_str::<ClientConfig>(
                &serde_json::to_string(&vast).expect("serializes")
            )
            .expect("deserializes"),
            vast
        );
    }

    /// Answer `count` requests with whatever `reply` makes of each, and hand
    /// back the requests as received.
    fn responder(
        mut server: FrameTransport<DuplexStream, Tcp>,
        count: usize,
        reply: fn(&RequestPdu) -> ResponsePdu,
    ) -> tokio::task::JoinHandle<Vec<RequestPdu>> {
        tokio::spawn(async move {
            let mut seen = vec![];
            for _ in 0..count {
                let (header, request) = server.recv_request().await.expect("receives");
                let response = reply(&request);
                server
                    .send_response(&header, &response)
                    .await
                    .expect("responds");
                seen.push(request);
            }
            seen
        })
    }

    #[tokio::test]
    /// CL-R-060 — each typed read issues its own function code and returns the
    /// values, in the domain types of FR-R-007 rather than bare integers.
    async fn ut_typed_reads_issue_their_own_function() {
        let (mut client, server) = pair();
        let answering = responder(server, 4, |request| match request {
            RequestPdu::ReadCoils { .. } => ResponsePdu::ReadCoils {
                coils: vec![true, false, true, false, false, false, false, false],
            },
            RequestPdu::ReadDiscreteInputs { .. } => ResponsePdu::ReadDiscreteInputs {
                inputs: vec![false, true, false, false, false, false, false, false],
            },
            RequestPdu::ReadHoldingRegisters { .. } => ResponsePdu::ReadHoldingRegisters {
                registers: vec![RegisterValue(0x022B), RegisterValue(0x0064)],
            },
            _ => ResponsePdu::ReadInputRegisters {
                registers: vec![RegisterValue(0x000A)],
            },
        });

        assert_eq!(
            client
                .read_coils(UnitId(0x11), Address(0x0013), Quantity(3))
                .await,
            Ok(vec![true, false, true])
        );
        assert_eq!(
            client
                .read_discrete_inputs(UnitId(0x11), Address(0x00C4), Quantity(2))
                .await,
            Ok(vec![false, true])
        );
        assert_eq!(
            client
                .read_holding_registers(UnitId(0x11), Address(0x006B), Quantity(2))
                .await,
            Ok(vec![RegisterValue(0x022B), RegisterValue(0x0064)])
        );
        assert_eq!(
            client
                .read_input_registers(UnitId(0x11), Address(0x0008), Quantity(1))
                .await,
            Ok(vec![RegisterValue(0x000A)])
        );

        let seen = answering.await.expect("the server task finishes");
        assert_eq!(
            seen,
            vec![
                RequestPdu::ReadCoils {
                    address: Address(0x0013),
                    quantity: Quantity(3),
                },
                RequestPdu::ReadDiscreteInputs {
                    address: Address(0x00C4),
                    quantity: Quantity(2),
                },
                RequestPdu::ReadHoldingRegisters {
                    address: Address(0x006B),
                    quantity: Quantity(2),
                },
                RequestPdu::ReadInputRegisters {
                    address: Address(0x0008),
                    quantity: Quantity(1),
                },
            ]
        );
    }

    #[tokio::test]
    /// CL-R-062 — a bit read returns exactly the quantity asked for. The wire
    /// carries whole bytes (FR-R-044), so a 3-coil read arrives as 8 values and
    /// the 5 padding bits are the caller's to never see.
    async fn ut_bit_reads_discard_padding() {
        let (mut client, server) = pair();
        // Every padding bit set, so keeping one would be visible.
        let answering = responder(server, 1, |_| ResponsePdu::ReadCoils {
            coils: vec![true, false, true, true, true, true, true, true],
        });

        assert_eq!(
            client
                .read_coils(UnitId(0x11), Address(0), Quantity(3))
                .await,
            Ok(vec![true, false, true])
        );
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-060 — each typed write issues its own function code, and returns
    /// nothing: the echoed fields are not the caller's business (CL-R-064).
    async fn ut_typed_writes_issue_their_own_function() {
        let (mut client, server) = pair();
        let answering = responder(server, 6, |request| match request {
            RequestPdu::WriteSingleCoil { address, value } => ResponsePdu::WriteSingleCoil {
                address: *address,
                value: *value,
            },
            RequestPdu::WriteSingleRegister { address, value } => {
                ResponsePdu::WriteSingleRegister {
                    address: *address,
                    value: *value,
                }
            }
            RequestPdu::WriteMultipleCoils { address, coils } => ResponsePdu::WriteMultipleCoils {
                address: *address,
                quantity: Quantity(u16::try_from(coils.len()).expect("small")),
            },
            RequestPdu::WriteMultipleRegisters { address, registers } => {
                ResponsePdu::WriteMultipleRegisters {
                    address: *address,
                    quantity: Quantity(u16::try_from(registers.len()).expect("small")),
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
            _ => ResponsePdu::ReadWriteMultipleRegisters {
                registers: vec![RegisterValue(0x00FF)],
            },
        });

        assert_eq!(
            client
                .write_single_coil(UnitId(0x11), Address(0x00AC), true)
                .await,
            Ok(())
        );
        assert_eq!(
            client
                .write_single_register(UnitId(0x11), Address(0x0001), RegisterValue(0x0003))
                .await,
            Ok(())
        );
        assert_eq!(
            client
                .write_multiple_coils(UnitId(0x11), Address(0x0013), &[true, false])
                .await,
            Ok(())
        );
        assert_eq!(
            client
                .write_multiple_registers(
                    UnitId(0x11),
                    Address(0x0001),
                    &[RegisterValue(0x000A), RegisterValue(0x0102)],
                )
                .await,
            Ok(())
        );
        assert_eq!(
            client
                .mask_write_register(UnitId(0x11), Address(0x0004), Mask(0x00F2), Mask(0x0025))
                .await,
            Ok(())
        );
        assert_eq!(
            client
                .read_write_multiple_registers(
                    UnitId(0x11),
                    Address(0x0003),
                    Quantity(1),
                    Address(0x000E),
                    &[RegisterValue(0x00FF)],
                )
                .await,
            Ok(vec![RegisterValue(0x00FF)])
        );

        let seen = answering.await.expect("the server task finishes");
        assert_eq!(seen.len(), 6);
        assert_eq!(
            seen.get(2).expect("six requests were seen"),
            &RequestPdu::WriteMultipleCoils {
                address: Address(0x0013),
                coils: vec![true, false],
            }
        );
        assert_eq!(
            seen.get(5).expect("six requests were seen"),
            &RequestPdu::ReadWriteMultipleRegisters {
                read_address: Address(0x0003),
                read_quantity: Quantity(1),
                write_address: Address(0x000E),
                registers: vec![RegisterValue(0x00FF)],
            }
        );
    }

    #[tokio::test]
    /// CL-R-040, CL-R-042 — a typed method surfaces an exception as a failure
    /// carrying both codes, never as a success, and the client stays usable.
    async fn ut_typed_method_fails_on_an_exception() {
        let (mut client, server) = pair();
        let answering = responder(server, 2, |_| {
            ResponsePdu::Exception(ExceptionResponse {
                function: FunctionCode::ReadHoldingRegisters,
                exception: ExceptionCode::IllegalDataAddress,
            })
        });

        assert_eq!(
            client
                .read_holding_registers(UnitId(0x11), Address(0x9999), Quantity(1))
                .await,
            Err(Error::Exception {
                function: FunctionCode::ReadHoldingRegisters,
                exception: ExceptionCode::IllegalDataAddress,
            })
        );
        assert!(!client.is_desynchronized());
        assert!(
            client
                .read_holding_registers(UnitId(0x11), Address(0x9999), Quantity(1))
                .await
                .is_err()
        );
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-041 — an exception code the crate does not name is a legal thing
    /// for a server to send, so it reaches the caller unaltered.
    async fn ut_unnamed_exception_code_reaches_the_caller() {
        let (mut client, server) = pair();
        let answering = responder(server, 1, |_| {
            ResponsePdu::Exception(ExceptionResponse {
                function: FunctionCode::ReadCoils,
                exception: ExceptionCode::Other(0x7F),
            })
        });

        assert_eq!(
            client
                .read_coils(UnitId(0x11), Address(0), Quantity(1))
                .await,
            Err(Error::Exception {
                function: FunctionCode::ReadCoils,
                exception: ExceptionCode::Other(0x7F),
            })
        );
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-064 — an echo that disagrees with what was sent is not an error
    /// here. It is a server defect the caller can detect through `call`; the
    /// client does not fail a completed exchange over it.
    async fn ut_echoed_fields_are_not_compared() {
        let (mut client, server) = pair();
        let answering = responder(server, 1, |_| ResponsePdu::WriteSingleRegister {
            address: Address(0xDEAD),
            value: RegisterValue(0xBEEF),
        });

        assert_eq!(
            client
                .write_single_register(UnitId(0x11), Address(0x0001), RegisterValue(0x0003))
                .await,
            Ok(())
        );
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-063 — range rules belong to the frame area (FR-R-022), so a typed
    /// method surfaces them from encoding rather than checking them itself.
    async fn ut_range_validation_comes_from_encoding() {
        let (mut client, _server) = pair();
        assert_eq!(
            client
                .read_holding_registers(UnitId(0x11), Address(0), Quantity(126))
                .await,
            Err(Error::OutOfRange {
                field: "quantity",
                value: 126,
                min: 1,
                max: 125,
            })
        );
    }

    #[tokio::test]
    /// CL-R-051 — a broadcast write is written and not awaited: no server
    /// answers unit 0, so waiting for one would only ever time out.
    async fn ut_broadcast_write_is_not_awaited() {
        let (client, server) = duplex(1024);
        let mut client = Client::<_, Rtu>::new(FrameTransport::new(client));
        let mut server = FrameTransport::<_, Rtu>::new(server);

        client
            .write_single_coil(UnitId(0), Address(0x00AC), true)
            .await
            .expect("returns without a response");
        assert_eq!(
            server.recv_request().await,
            Ok((
                UnitId(0),
                RequestPdu::WriteSingleCoil {
                    address: Address(0x00AC),
                    value: true,
                }
            ))
        );
    }

    #[tokio::test]
    /// CL-R-052 — a broadcast read fails before anything is written: an answer
    /// that cannot arrive is a caller error, not a silent no-op.
    async fn ut_broadcast_read_is_rejected_before_writing() {
        let (client, server) = duplex(1024);
        let mut client = Client::<_, Rtu>::new(FrameTransport::new(client));
        let mut server = FrameTransport::<_, Rtu>::new(server);

        assert_eq!(
            client.read_coils(UnitId(0), Address(0), Quantity(1)).await,
            Err(Error::IllegalValue {
                field: "broadcast read",
                value: 0,
            })
        );
        drop(client);
        assert!(
            server.recv_request().await.is_err(),
            "nothing may reach the wire"
        );
    }

    #[tokio::test]
    /// CL-R-053 — the raw path can express a broadcast the typed methods
    /// forbid, and says so by yielding no response rather than failing.
    async fn ut_broadcast_call_yields_no_response() {
        let (client, server) = duplex(1024);
        let mut client = Client::<_, Rtu>::new(FrameTransport::new(client));
        let mut server = FrameTransport::<_, Rtu>::new(server);

        assert_eq!(client.call(UnitId(0), read_holding()).await, Ok(None));
        assert_eq!(server.recv_request().await, Ok((UnitId(0), read_holding())));
    }

    #[tokio::test]
    /// CL-R-060 — the diagnostic and identification codes each get their own
    /// typed method: 7, 8, 11, 12 and 17.
    async fn ut_typed_diagnostic_methods() {
        let (mut client, server) = pair();
        let answering = responder(server, 5, |request| match request {
            RequestPdu::ReadExceptionStatus => ResponsePdu::ReadExceptionStatus {
                status: ExceptionStatus(0x6D),
            },
            RequestPdu::Diagnostics { sub_function, data } => ResponsePdu::Diagnostics {
                sub_function: *sub_function,
                data: data.clone(),
            },
            RequestPdu::GetCommEventCounter => ResponsePdu::GetCommEventCounter {
                status: 0xFFFF,
                event_count: 0x0108,
            },
            RequestPdu::GetCommEventLog => ResponsePdu::GetCommEventLog {
                status: 0x0000,
                event_count: 0x0108,
                message_count: 0x0121,
                events: vec![0x20, 0x00],
            },
            _ => ResponsePdu::ReportServerId {
                data: vec![0x11, 0xFF],
            },
        });

        assert_eq!(
            client.read_exception_status(UnitId(0x11)).await,
            Ok(ExceptionStatus(0x6D))
        );
        assert_eq!(
            client
                .diagnostics(
                    UnitId(0x11),
                    DiagnosticSubFunction::ReturnQueryData,
                    &[0xA537],
                )
                .await,
            Ok(vec![0xA537])
        );
        assert_eq!(
            client.get_comm_event_counter(UnitId(0x11)).await,
            Ok(CommEventCounter {
                status: 0xFFFF,
                event_count: 0x0108,
            })
        );
        assert_eq!(
            client.get_comm_event_log(UnitId(0x11)).await,
            Ok(CommEventLog {
                status: 0x0000,
                event_count: 0x0108,
                message_count: 0x0121,
                events: vec![0x20, 0x00],
            })
        );
        assert_eq!(
            client.report_server_id(UnitId(0x11)).await,
            Ok(vec![0x11, 0xFF])
        );
        assert_eq!(answering.await.expect("the server task finishes").len(), 5);
    }

    #[tokio::test]
    /// CL-R-060 — the record, queue and encapsulated-transport codes: 20, 21,
    /// 24 and 43.
    async fn ut_typed_record_and_transport_methods() {
        let (mut client, server) = pair();
        let answering = responder(server, 4, |request| match request {
            RequestPdu::ReadFileRecord { .. } => ResponsePdu::ReadFileRecord {
                // Three registers: a sub-response's data length is at least 7
                // bytes (FR-R-055), which one register cannot reach.
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
            _ => {
                ResponsePdu::EncapsulatedInterfaceTransport(MeiResponse::ReadDeviceIdentification {
                    read_device_id_code: ReadDeviceIdCode::Basic,
                    conformity_level: 0x01,
                    more_follows: false,
                    next_object_id: 0x00,
                    objects: vec![],
                })
            }
        });

        let read = FileRecordRead {
            file_number: FileNumber(4),
            record_number: RecordNumber(1),
            record_length: RecordLength(1),
        };
        assert_eq!(
            client.read_file_record(UnitId(0x11), &[read]).await,
            Ok(vec![FileRecordReadResponse {
                values: vec![
                    RegisterValue(0x0DFE),
                    RegisterValue(0x0000),
                    RegisterValue(0x0001),
                ],
            }])
        );

        let write = FileRecordWrite {
            file_number: FileNumber(4),
            record_number: RecordNumber(7),
            values: vec![RegisterValue(0x06AF)],
        };
        assert_eq!(
            client.write_file_record(UnitId(0x11), &[write]).await,
            Ok(())
        );
        assert_eq!(
            client.read_fifo_queue(UnitId(0x11), Address(0x04DE)).await,
            Ok(vec![RegisterValue(0x01B8)])
        );
        assert_eq!(
            client
                .encapsulated_interface_transport(
                    UnitId(0x11),
                    MeiRequest::ReadDeviceIdentification {
                        read_device_id_code: ReadDeviceIdCode::Basic,
                        object_id: 0x00,
                    },
                )
                .await,
            Ok(MeiResponse::ReadDeviceIdentification {
                read_device_id_code: ReadDeviceIdCode::Basic,
                conformity_level: 0x01,
                more_follows: false,
                next_object_id: 0x00,
                objects: vec![],
            })
        );
        assert_eq!(answering.await.expect("the server task finishes").len(), 4);
    }

    #[tokio::test]
    /// CL-R-013, CL-R-031 — a write that fails leaves the client unusable: a
    /// partially written ADU is on the wire and no later request can repair it,
    /// so the failure must not look recoverable.
    async fn ut_failed_write_desynchronizes() {
        let (client, server) = duplex(1024);
        let mut client = Client::<_, Tcp>::new(FrameTransport::new(client));
        drop(server);

        assert!(client.call(UnitId(0x11), read_holding()).await.is_err());
        assert!(
            client.is_desynchronized(),
            "a failed write must not look recoverable"
        );
        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Desynchronized)
        );
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-024 — a response that arrives after its request timed out is never
    /// handed to a later request. The client refuses to issue one at all, so the
    /// stale bytes cannot be mistaken for the next answer.
    async fn ut_late_response_is_never_delivered_to_a_later_request() {
        let (mut client, mut server) = pair();
        let late = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            // Answers only after the client has given up.
            tokio::time::sleep(Duration::from_secs(2)).await;
            let _ = server.send_response(&header, &registers()).await;
            // Holds the transport so the stale bytes stay readable.
            core::future::pending::<()>().await;
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Timeout { what: "response" })
        );
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Desynchronized),
            "the late response must not answer a later request"
        );
        late.abort();
    }

    #[tokio::test]
    /// CL-R-035 — a client that has done nothing says so, rather than claiming
    /// a health it has no evidence for.
    async fn ut_new_client_is_untried() {
        let (client, _server) = pair();
        assert_eq!(client.state(), ClientState::Untried);
    }

    #[tokio::test]
    /// CL-R-035 — an exchange the peer answered is reported as answered, and
    /// stays that way while nothing further has been observed.
    async fn ut_answered_after_a_matched_response() {
        let (mut client, mut server) = pair();
        let answering = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            server
                .send_response(&header, &registers())
                .await
                .expect("responds");
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Ok(Some(registers()))
        );
        assert_eq!(client.state(), ClientState::Answered);
        assert_eq!(
            client.state(),
            ClientState::Answered,
            "reporting the state must not consume it"
        );
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-036 — an exception is an answer. The server replied; it merely
    /// refused, which says as much about the link as a success does.
    async fn ut_exception_counts_as_answered() {
        let (mut client, mut server) = pair();
        let exception = ResponsePdu::Exception(ExceptionResponse {
            function: FunctionCode::ReadHoldingRegisters,
            exception: ExceptionCode::IllegalDataAddress,
        });
        let answering = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            server
                .send_response(&header, &exception)
                .await
                .expect("responds");
        });

        assert!(
            client
                .read_holding_registers(UnitId(0x11), Address(0x006B), Quantity(3))
                .await
                .is_err(),
            "the typed method surfaces the exception as a failure"
        );
        assert_eq!(client.state(), ClientState::Answered);
        answering.await.expect("the server task finishes");
    }

    #[tokio::test]
    /// CL-R-036 — a response carrying another function's code is still an
    /// answer: the frame corresponded to the request and decoded.
    async fn ut_unexpected_function_counts_as_answered() {
        let (mut client, mut server) = pair();
        let answering = tokio::spawn(async move {
            let (header, _) = server.recv_request().await.expect("receives");
            server
                .send_response(&header, &ResponsePdu::ReadCoils { coils: vec![true] })
                .await
                .expect("responds");
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::UnexpectedFunction {
                expected: FunctionCode::ReadHoldingRegisters,
                actual: FunctionCode::ReadCoils,
            })
        );
        assert_eq!(client.state(), ClientState::Answered);
        answering.await.expect("the server task finishes");
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-036 — a broadcast is heard by no one that answers, so it leaves the
    /// report exactly as it found it: `Untried` stays untried, and an earlier
    /// answer is neither confirmed nor erased.
    async fn ut_broadcast_write_leaves_state_unchanged() {
        let (client, mut server) = duplex(1024);
        let mut client = Client::<_, Rtu>::new(FrameTransport::new(client));
        let good = Rtu::encode_response(&UnitId(0x11), &registers()).expect("the response encodes");

        let answering = tokio::spawn(async move {
            let mut frame = [0u8; 8];
            server
                .read_exact(&mut frame)
                .await
                .expect("the broadcast arrives whole");
            server
                .read_exact(&mut frame)
                .await
                .expect("the request arrives whole");
            server.write_all(&good).await.expect("responds");
            server
                .read_exact(&mut frame)
                .await
                .expect("the second broadcast arrives whole");
            server
        });

        let broadcast = RequestPdu::WriteSingleRegister {
            address: Address(0x0001),
            value: RegisterValue(0x0003),
        };
        assert_eq!(client.call(UnitId(0), broadcast.clone()).await, Ok(None));
        assert_eq!(
            client.state(),
            ClientState::Untried,
            "writing where nobody answers is not evidence of an answer"
        );

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Ok(Some(registers()))
        );
        assert_eq!(client.state(), ClientState::Answered);

        assert_eq!(client.call(UnitId(0), broadcast).await, Ok(None));
        assert_eq!(
            client.state(),
            ClientState::Answered,
            "a broadcast must not overwrite what was actually observed"
        );
        let _ = answering.await;
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-034 — a corrupt frame on a self-locating framing leaves the client
    /// usable but the exchange unanswered, and the boolean report agrees with
    /// the state it is a projection of.
    async fn ut_undecodable_response_on_rtu_reports_unanswered() {
        let (client, mut server) = duplex(1024);
        let mut client = Client::<_, Rtu>::new(FrameTransport::new(client));

        let good = Rtu::encode_response(&UnitId(0x11), &registers()).expect("the response encodes");
        let mut corrupt = good.clone();
        let last = corrupt.last_mut().expect("the ADU carries a CRC");
        *last ^= 0xFF;

        let answering = tokio::spawn(async move {
            let mut request = [0u8; 8];
            server
                .read_exact(&mut request)
                .await
                .expect("the request arrives whole");
            server
                .write_all(&corrupt)
                .await
                .expect("writes the corrupt frame");
            server
        });

        assert!(matches!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Checksum { .. })
        ));
        assert_eq!(client.state(), ClientState::Unanswered);
        assert!(
            !client.is_desynchronized(),
            "the boolean must agree with the state it projects"
        );
        let _ = answering.await;
    }

    #[tokio::test]
    /// CL-R-038 — the report is made of what has already been observed: asking
    /// for it writes nothing, reads nothing, and returns the same answer twice.
    async fn ut_state_reports_without_touching_the_transport() {
        let (client, mut server) = pair();
        assert_eq!(client.state(), ClientState::Untried);
        assert_eq!(client.state(), ClientState::Untried);
        assert!(!client.is_desynchronized());

        // Nothing may have reached the wire, so a receive on the far end must
        // still be waiting when the client is dropped.
        let listening = tokio::spawn(async move { server.recv_request().await });
        drop(client);
        assert!(
            listening.await.expect("the server task finishes").is_err(),
            "reporting the state put bytes on the wire"
        );
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-037 — the response timeout elapses; state is
    /// `Unusable(UnusableReason::Silent)`.
    async fn ut_timeout_reports_silent() {
        let (mut client, mut server) = pair();
        let silent = tokio::spawn(async move {
            let _ = server.recv_request().await;
            core::future::pending::<()>().await;
        });

        assert_eq!(
            client.call(UnitId(0x11), read_holding()).await,
            Err(Error::Timeout { what: "response" })
        );
        assert_eq!(
            client.state(),
            ClientState::Unusable(UnusableReason::Silent)
        );
        silent.abort();
    }

    #[tokio::test(start_paused = true)]
    /// CL-R-037 — a malformed TCP response; state is
    /// `Unusable(UnusableReason::Undecodable)`.
    async fn ut_undecodable_response_on_tcp_reports_undecodable() {
        let (client, server) = duplex(1024);
        let mut client = Client::<_, Tcp>::new(FrameTransport::new(client));
        let mut server = server;

        let answering = tokio::spawn(async move {
            let mut request = [0u8; 12];
            server
                .read_exact(&mut request)
                .await
                .expect("the request arrives whole");
            server
                .write_all(&[
                    0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x11, 0x03, 0x04, 0x00, 0x2A,
                ])
                .await
                .expect("the reply is written");
            server
        });

        let result = client.call(UnitId(0x11), read_holding()).await;
        assert!(result.is_err(), "the decoding error should be reported");
        assert_eq!(
            client.state(),
            ClientState::Unusable(UnusableReason::Undecodable)
        );
        let _ = answering.await;
    }

    #[tokio::test]
    /// CL-R-037 — drop the peer half, then issue a request; state is
    /// `Unusable(UnusableReason::Io { kind })`.
    async fn ut_write_failure_reports_the_io_kind() {
        let (client, server) = duplex(1024);
        let mut client = Client::<_, Tcp>::new(FrameTransport::new(client));
        drop(server);

        let result = client.call(UnitId(0x11), read_holding()).await;
        assert!(result.is_err(), "the write should fail");
        match client.state() {
            ClientState::Unusable(UnusableReason::Io { kind }) => {
                assert!(!matches!(
                    kind,
                    std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                ));
            }
            state => panic!("expected Unusable(Io {{ kind }}), got {:?}", state),
        }
    }

    #[tokio::test]
    /// CL-R-037 — peer reads the request then shuts down; state is
    /// `Unusable(UnusableReason::PeerClosed)`.
    async fn ut_peer_close_before_a_response_reports_peer_closed() {
        let (client, server) = duplex(1024);
        let mut client = Client::<_, Tcp>::new(FrameTransport::new(client));
        let mut server = server;

        let answering = tokio::spawn(async move {
            let mut request = [0u8; 12];
            let read_result = server.read_exact(&mut request).await;
            assert!(read_result.is_ok(), "the request should arrive");
            // Close the peer end without sending a response.
            drop(server);
        });

        let result = client.call(UnitId(0x11), read_holding()).await;
        assert!(result.is_err(), "reading from a closed peer should fail");
        assert_eq!(
            client.state(),
            ClientState::Unusable(UnusableReason::PeerClosed)
        );
        let _ = answering.await;
    }
}
