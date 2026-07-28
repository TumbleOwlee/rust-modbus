//! File record access bodies (FR-R-050 … FR-R-056).
//!
//! Function codes 20 and 21 are the only ones whose body is a list of
//! variable-length sub-items rather than a single fixed layout, so their
//! sub-request and sub-response shapes are named types of their own.
//!
//! Every body here is a 1-byte length followed by exactly that many bytes of
//! sub-items. The length is carved out first and the sub-items are then parsed
//! within it, so a sub-item overrunning the region is caught against the stated
//! length rather than against whatever the caller happened to supply.

use alloc::vec::Vec;

use winnow::Parser;
use winnow::binary::{be_u8, be_u16};
use winnow::token::take;

use crate::error::{Error, Result};
use crate::frame::pdu::{registers_from_bytes, registers_to_bytes};
use crate::parse::{self, Input, ParseResult};

/// The only reference type Modbus defines for file records (FR-R-055).
const REFERENCE_TYPE: u8 = 6;

/// Highest record number a file may name (FR-R-056).
const MAX_RECORD_NUMBER: u32 = 9_999;

/// Bytes in a Read File Record sub-request (FR-R-050).
const READ_SUBREQUEST_LEN: u32 = 7;

/// Read File Record request byte count bounds (FR-R-051).
const MIN_READ_BYTE_COUNT: u32 = 7;
/// Read File Record request byte count bounds (FR-R-051).
const MAX_READ_BYTE_COUNT: u32 = 245;

/// Read File Record response data length bounds (FR-R-052).
const MIN_READ_DATA_LEN: u32 = 7;
/// Read File Record response data length bounds (FR-R-052).
const MAX_READ_DATA_LEN: u32 = 245;

/// Write File Record request data length bounds (FR-R-054).
const MIN_WRITE_DATA_LEN: u32 = 9;
/// Write File Record request data length bounds (FR-R-054).
const MAX_WRITE_DATA_LEN: u32 = 251;

/// A Read File Record sub-request (FR-R-050).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecordRead {
    /// File to read from; 1–65535 (FR-R-056).
    pub file_number: u16,
    /// First record within the file; 0–9999 (FR-R-056).
    pub record_number: u16,
    /// Number of registers to read.
    pub record_length: u16,
}

/// A Read File Record sub-response (FR-R-052).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecordReadResponse {
    /// Record data read.
    pub values: Vec<u16>,
}

/// A Write File Record sub-request, echoed unchanged in the response
/// (FR-R-053).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecordWrite {
    /// File to write to; 1–65535 (FR-R-056).
    pub file_number: u16,
    /// First record within the file; 0–9999 (FR-R-056).
    pub record_number: u16,
    /// Record data to write.
    pub values: Vec<u16>,
}

impl FileRecordRead {
    /// Parse one 7-byte sub-request (FR-R-050).
    fn parse(input: &mut Input<'_>) -> ParseResult<Self> {
        reference_type(input)?;
        let file_number = be_u16.parse_next(input)?;
        let record_number = be_u16.parse_next(input)?;
        let record_length = be_u16.parse_next(input)?;
        parse::lift(check_numbers(file_number, record_number))?;
        Ok(Self {
            file_number,
            record_number,
            record_length,
        })
    }

    /// Append the sub-request's 7 bytes to `out`.
    fn encode_into(&self, out: &mut Vec<u8>) -> Result<()> {
        check_numbers(self.file_number, self.record_number)?;
        out.push(REFERENCE_TYPE);
        out.extend_from_slice(&self.file_number.to_be_bytes());
        out.extend_from_slice(&self.record_number.to_be_bytes());
        out.extend_from_slice(&self.record_length.to_be_bytes());
        Ok(())
    }
}

impl FileRecordReadResponse {
    /// Parse one sub-response (FR-R-052).
    ///
    /// The file response length counts the reference type byte as well as the
    /// record data, so it is one more than an even number — an even length
    /// would claim an odd number of data bytes and could not hold whole
    /// registers.
    fn parse(input: &mut Input<'_>) -> ParseResult<Self> {
        let length = be_u8.parse_next(input)?;
        if length == 0 || length % 2 == 0 {
            return parse::fail(Error::IllegalValue {
                field: "file response length",
                value: u16::from(length),
            });
        }
        reference_type(input)?;
        let data = take(usize::from(length - 1)).parse_next(input)?;
        Ok(Self {
            values: registers_from_bytes(data),
        })
    }

    /// Append the sub-response to `out`.
    fn encode_into(&self, out: &mut Vec<u8>) -> Result<()> {
        let data = registers_to_bytes(&self.values);
        // The reference type byte counts towards the file response length, so
        // the whole sub-response occupies one byte more than that length.
        let length = u8::try_from(data.len().saturating_add(1)).map_err(|_| Error::OutOfRange {
            field: "file response length",
            value: as_u32(data.len().saturating_add(1)),
            min: 1,
            max: MAX_READ_DATA_LEN,
        })?;
        out.push(length);
        out.push(REFERENCE_TYPE);
        out.extend_from_slice(&data);
        Ok(())
    }
}

impl FileRecordWrite {
    /// Parse one sub-request (FR-R-053).
    fn parse(input: &mut Input<'_>) -> ParseResult<Self> {
        reference_type(input)?;
        let file_number = be_u16.parse_next(input)?;
        let record_number = be_u16.parse_next(input)?;
        let record_length = be_u16.parse_next(input)?;
        parse::lift(check_numbers(file_number, record_number))?;
        let data = take(usize::from(record_length).saturating_mul(2)).parse_next(input)?;
        Ok(Self {
            file_number,
            record_number,
            values: registers_from_bytes(data),
        })
    }

    /// Append the sub-request to `out`.
    fn encode_into(&self, out: &mut Vec<u8>) -> Result<()> {
        check_numbers(self.file_number, self.record_number)?;
        let record_length = u16::try_from(self.values.len()).map_err(|_| Error::OutOfRange {
            field: "record length",
            value: as_u32(self.values.len()),
            min: 0,
            max: MAX_WRITE_DATA_LEN,
        })?;
        out.push(REFERENCE_TYPE);
        out.extend_from_slice(&self.file_number.to_be_bytes());
        out.extend_from_slice(&self.record_number.to_be_bytes());
        out.extend_from_slice(&record_length.to_be_bytes());
        out.extend_from_slice(&registers_to_bytes(&self.values));
        Ok(())
    }
}

/// Decode the body of a Read File Record request (FR-R-050, FR-R-051).
pub(super) fn decode_read_requests(input: &mut Input<'_>) -> ParseResult<Vec<FileRecordRead>> {
    let region = region(input, |count| {
        check_bounds(
            "request byte count",
            count,
            MIN_READ_BYTE_COUNT,
            MAX_READ_BYTE_COUNT,
        )?;
        if count % READ_SUBREQUEST_LEN != 0 {
            return Err(Error::IllegalValue {
                field: "request byte count",
                value: as_u16(count),
            });
        }
        Ok(())
    })?;
    parse::lift(parse::run_all(region, FileRecordRead::parse))
}

/// Encode the body of a Read File Record request, byte count included.
pub(super) fn encode_read_requests(records: &[FileRecordRead]) -> Result<Vec<u8>> {
    body(
        "request byte count",
        MIN_READ_BYTE_COUNT,
        MAX_READ_BYTE_COUNT,
        records,
        |record, out| record.encode_into(out),
    )
}

/// Decode the body of a Read File Record response (FR-R-052).
pub(super) fn decode_read_responses(
    input: &mut Input<'_>,
) -> ParseResult<Vec<FileRecordReadResponse>> {
    let region = region(input, |length| {
        check_bounds(
            "response data length",
            length,
            MIN_READ_DATA_LEN,
            MAX_READ_DATA_LEN,
        )
    })?;
    parse::lift(parse::run_all(region, FileRecordReadResponse::parse))
}

/// Encode the body of a Read File Record response, data length included.
pub(super) fn encode_read_responses(records: &[FileRecordReadResponse]) -> Result<Vec<u8>> {
    body(
        "response data length",
        MIN_READ_DATA_LEN,
        MAX_READ_DATA_LEN,
        records,
        |record, out| record.encode_into(out),
    )
}

/// Decode the body of a Write File Record request or response (FR-R-053,
/// FR-R-054).
pub(super) fn decode_write_records(input: &mut Input<'_>) -> ParseResult<Vec<FileRecordWrite>> {
    let region = region(input, |length| {
        check_bounds(
            "request data length",
            length,
            MIN_WRITE_DATA_LEN,
            MAX_WRITE_DATA_LEN,
        )
    })?;
    parse::lift(parse::run_all(region, FileRecordWrite::parse))
}

/// Encode the body of a Write File Record request or response, data length
/// included.
pub(super) fn encode_write_records(records: &[FileRecordWrite]) -> Result<Vec<u8>> {
    body(
        "request data length",
        MIN_WRITE_DATA_LEN,
        MAX_WRITE_DATA_LEN,
        records,
        |record, out| record.encode_into(out),
    )
}

/// Read the leading length byte, check it with `check`, and carve out the
/// region it describes.
fn region<'a>(
    input: &mut Input<'a>,
    check: impl FnOnce(u32) -> Result<()>,
) -> ParseResult<&'a [u8]> {
    let length = be_u8.parse_next(input)?;
    parse::lift(check(u32::from(length)))?;
    take(usize::from(length)).parse_next(input)
}

/// Encode `records` into a body prefixed by its own length.
fn body<T>(
    field: &'static str,
    min: u32,
    max: u32,
    records: &[T],
    mut encode: impl FnMut(&T, &mut Vec<u8>) -> Result<()>,
) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    for record in records {
        encode(record, &mut data)?;
    }
    let length = as_u32(data.len());
    check_bounds(field, length, min, max)?;
    let mut bytes = Vec::with_capacity(data.len().saturating_add(1));
    // The bound just checked keeps this below 256.
    bytes.push(u8::try_from(length).unwrap_or(u8::MAX));
    bytes.extend_from_slice(&data);
    Ok(bytes)
}

/// Consume a reference type byte, requiring it to be 6 (FR-R-055).
fn reference_type(input: &mut Input<'_>) -> ParseResult<()> {
    match be_u8.parse_next(input)? {
        REFERENCE_TYPE => Ok(()),
        other => parse::fail(Error::ReferenceType(other)),
    }
}

/// Check a file number and record number against their fixed ranges
/// (FR-R-056).
///
/// Applied on decode as well as encode: FR-R-133 requires that whatever decodes
/// re-encodes identically, so a value the encoder rejects must not decode.
fn check_numbers(file_number: u16, record_number: u16) -> Result<()> {
    check_bounds(
        "file number",
        u32::from(file_number),
        1,
        u32::from(u16::MAX),
    )?;
    check_bounds(
        "record number",
        u32::from(record_number),
        0,
        MAX_RECORD_NUMBER,
    )
}

/// Reject `value` outside `min..=max`.
fn check_bounds(field: &'static str, value: u32, min: u32, max: u32) -> Result<()> {
    if value < min || value > max {
        return Err(Error::OutOfRange {
            field,
            value,
            min,
            max,
        });
    }
    Ok(())
}

/// Widen a length for error reporting, saturating rather than wrapping.
fn as_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// Narrow a checked length for the fields that report 16 bits.
fn as_u16(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}
