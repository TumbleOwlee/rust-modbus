//! Parsing primitives built on [`winnow`].
//!
//! Decoding runs over a [`Partial`] stream. That is deliberate: on a partial
//! stream winnow's fixed-width parsers report [`ErrMode::Incomplete`] carrying a
//! [`Needed`] byte count, which is what lets [`run`] name both the bytes expected
//! and the bytes supplied as FR-R-131 requires. `ParserError::from_input` sees
//! only the input position and could not supply that number.
//!
//! Domain failures — an out-of-range quantity, an illegal value, a byte count
//! that disagrees with its data — are raised through [`fail`] as
//! [`ErrMode::Cut`], so they bubble straight out instead of being mistaken for a
//! branch that should be retried.

use alloc::vec::Vec;

use winnow::error::{ErrMode, Needed, ParserError};
use winnow::stream::Stream;
use winnow::{Parser, Partial};

use crate::error::{Error, Result};

/// The stream every decoder parses over.
pub(crate) type Input<'a> = Partial<&'a [u8]>;

/// What a parser step in this crate returns.
pub(crate) type ParseResult<T> = core::result::Result<T, ErrMode<ParseFailure>>;

/// A parse failure, optionally carrying the domain error that caused it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ParseFailure(Option<Error>);

impl ParseFailure {
    /// The domain error, or [`Error::Malformed`] for a bare structural failure.
    pub(crate) fn into_error(self) -> Error {
        self.0.unwrap_or(Error::Malformed)
    }
}

impl<'a> ParserError<Input<'a>> for ParseFailure {
    type Inner = Self;

    fn from_input(_input: &Input<'a>) -> Self {
        Self(None)
    }

    fn into_inner(self) -> core::result::Result<Self::Inner, Self> {
        Ok(self)
    }
}

/// Raise a domain error that must not be backtracked over.
pub(crate) fn fail<T>(error: Error) -> ParseResult<T> {
    lift(Err(error))
}

/// Lift a fallible crate operation into a parser step, preserving its error.
pub(crate) fn lift<T>(result: Result<T>) -> ParseResult<T> {
    result.map_err(|error| ErrMode::Cut(ParseFailure(Some(error))))
}

/// Run `parser` over `bytes`, requiring it to consume the input exactly.
pub(crate) fn run<'a, O, P>(bytes: &'a [u8], mut parser: P) -> Result<O>
where
    P: Parser<Input<'a>, O, ErrMode<ParseFailure>>,
{
    let mut input = Partial::new(bytes);
    match parser.parse_next(&mut input) {
        Ok(value) => match input.eof_offset() {
            0 => Ok(value),
            extra => Err(Error::TrailingBytes { extra }),
        },
        Err(error) => Err(convert(error, bytes.len())),
    }
}

/// Run `parser` over `bytes` repeatedly until the input is exhausted.
///
/// Used for the length-delimited regions that hold a variable number of
/// same-shaped items — file record sub-requests and sub-responses (FR-R-050,
/// FR-R-052, FR-R-053). The region has already been carved out at its stated
/// length, so an item running past its end is reported against that length.
pub(crate) fn run_all<O, P>(bytes: &[u8], mut parser: P) -> Result<Vec<O>>
where
    P: for<'a> FnMut(&mut Input<'a>) -> ParseResult<O>,
{
    let mut input = Partial::new(bytes);
    let mut items = Vec::new();
    while input.eof_offset() > 0 {
        match parser(&mut input) {
            Ok(item) => items.push(item),
            Err(error) => return Err(convert(error, bytes.len())),
        }
    }
    Ok(items)
}

/// Turn a parser failure into the crate error it stands for.
fn convert(error: ErrMode<ParseFailure>, supplied: usize) -> Error {
    match error {
        ErrMode::Incomplete(needed) => Error::Truncated {
            expected: supplied.saturating_add(missing(needed)),
            supplied,
        },
        ErrMode::Backtrack(failure) | ErrMode::Cut(failure) => failure.into_error(),
    }
}

/// Lower bound on the bytes still needed. `Needed::Unknown` only arises from
/// parsers whose appetite is not fixed; none of the Modbus layouts use one, so
/// the bound of 1 is a floor that is never actually reported.
fn missing(needed: Needed) -> usize {
    match needed {
        Needed::Size(n) => n.get(),
        Needed::Unknown => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winnow::binary::{be_u8, be_u16};

    #[test]
    /// FR-R-003 — multi-byte numeric fields are big-endian.
    fn ut_reads_u16_big_endian() {
        assert_eq!(run(&[0x12, 0x34], be_u16), Ok(0x1234));
    }

    #[test]
    /// FR-R-131 — the truncated-input error names bytes expected and supplied.
    fn ut_truncated_reports_expected_and_supplied() {
        assert_eq!(
            run(&[0x12], be_u16),
            Err(Error::Truncated {
                expected: 2,
                supplied: 1,
            })
        );
    }

    #[test]
    /// FR-R-130 — an empty input errors rather than panicking.
    fn ut_empty_input_errors() {
        assert_eq!(
            run(&[], be_u8),
            Err(Error::Truncated {
                expected: 1,
                supplied: 0,
            })
        );
    }

    #[test]
    /// FR-R-132 — surplus bytes are rejected, not silently ignored.
    fn ut_trailing_bytes_rejected() {
        assert_eq!(
            run(&[0x12, 0x34, 0x56], be_u16),
            Err(Error::TrailingBytes { extra: 1 })
        );
    }

    #[test]
    /// FR-R-133 — an exact decode consumes its input completely, leaving nothing
    /// that a re-encode would have to invent.
    fn ut_exact_consumption_succeeds() {
        assert_eq!(run(&[0xAB], be_u8), Ok(0xAB));
    }

    #[test]
    /// FR-R-043 — a domain failure propagates unchanged rather than being
    /// reported as a generic parse error.
    fn ut_domain_error_propagates() {
        let parser = |_: &mut Input<'_>| fail::<u8>(Error::TrailingBytes { extra: 7 });
        assert_eq!(run(&[0x00], parser), Err(Error::TrailingBytes { extra: 7 }));
    }

    #[test]
    /// FR-R-130 — a structural failure carrying no domain cause still yields an
    /// error rather than a panic.
    fn ut_structural_failure_is_malformed() {
        let parser = |input: &mut Input<'_>| {
            Err::<u8, _>(ErrMode::Backtrack(ParseFailure::from_input(input)))
        };
        assert_eq!(run(&[0x00], parser), Err(Error::Malformed));
    }
}
