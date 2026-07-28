//! Serial-line diagnostic bodies (FR-R-060 … FR-R-068).

use crate::error::{Error, Result};

/// A Diagnostics sub-function code (FR-R-062, FR-R-063).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSubFunction {
    /// 0 — Return Query Data.
    ReturnQueryData,
    /// 1 — Restart Communications Option.
    RestartCommunicationsOption,
    /// 2 — Return Diagnostic Register.
    ReturnDiagnosticRegister,
    /// 3 — Change ASCII Input Delimiter.
    ChangeAsciiInputDelimiter,
    /// 4 — Force Listen Only Mode.
    ForceListenOnlyMode,
    /// 10 — Clear Counters and Diagnostic Register.
    ClearCountersAndDiagnosticRegister,
    /// 11 — Return Bus Message Count.
    ReturnBusMessageCount,
    /// 12 — Return Bus Communication Error Count.
    ReturnBusCommunicationErrorCount,
    /// 13 — Return Bus Exception Error Count.
    ReturnBusExceptionErrorCount,
    /// 14 — Return Server Message Count.
    ReturnServerMessageCount,
    /// 15 — Return Server No Response Count.
    ReturnServerNoResponseCount,
    /// 16 — Return Server NAK Count.
    ReturnServerNakCount,
    /// 17 — Return Server Busy Count.
    ReturnServerBusyCount,
    /// 18 — Return Bus Character Overrun Count.
    ReturnBusCharacterOverrunCount,
    /// 20 — Clear Overrun Counter and Flag.
    ClearOverrunCounterAndFlag,
    /// Any code the specification does not name, the reserved range 5–9
    /// included (FR-R-063).
    Other(u16),
}

impl DiagnosticSubFunction {
    /// The sub-functions the specification defines, and their codes
    /// (FR-R-062).
    ///
    /// The single source of truth for decoding; encoding matches exhaustively
    /// so a variant added without a code here fails to compile.
    const NAMED: [(u16, Self); 15] = [
        (0, Self::ReturnQueryData),
        (1, Self::RestartCommunicationsOption),
        (2, Self::ReturnDiagnosticRegister),
        (3, Self::ChangeAsciiInputDelimiter),
        (4, Self::ForceListenOnlyMode),
        (10, Self::ClearCountersAndDiagnosticRegister),
        (11, Self::ReturnBusMessageCount),
        (12, Self::ReturnBusCommunicationErrorCount),
        (13, Self::ReturnBusExceptionErrorCount),
        (14, Self::ReturnServerMessageCount),
        (15, Self::ReturnServerNoResponseCount),
        (16, Self::ReturnServerNakCount),
        (17, Self::ReturnServerBusyCount),
        (18, Self::ReturnBusCharacterOverrunCount),
        (20, Self::ClearOverrunCounterAndFlag),
    ];

    /// Decode a sub-function code; every value decodes (FR-R-063).
    #[must_use]
    pub fn decode(raw: u16) -> Self {
        Self::NAMED
            .iter()
            .find(|(code, _)| *code == raw)
            .map_or(Self::Other(raw), |(_, sub_function)| *sub_function)
    }

    /// Encode a sub-function code.
    ///
    /// # Errors
    ///
    /// A [`DiagnosticSubFunction::Other`] holding a code the crate names is
    /// rejected, so no code has two encodings (FR-R-063).
    pub fn encode(self) -> Result<u16> {
        Ok(match self {
            Self::ReturnQueryData => 0,
            Self::RestartCommunicationsOption => 1,
            Self::ReturnDiagnosticRegister => 2,
            Self::ChangeAsciiInputDelimiter => 3,
            Self::ForceListenOnlyMode => 4,
            Self::ClearCountersAndDiagnosticRegister => 10,
            Self::ReturnBusMessageCount => 11,
            Self::ReturnBusCommunicationErrorCount => 12,
            Self::ReturnBusExceptionErrorCount => 13,
            Self::ReturnServerMessageCount => 14,
            Self::ReturnServerNoResponseCount => 15,
            Self::ReturnServerNakCount => 16,
            Self::ReturnServerBusyCount => 17,
            Self::ReturnBusCharacterOverrunCount => 18,
            Self::ClearOverrunCounterAndFlag => 20,
            Self::Other(raw) => {
                if Self::NAMED.iter().any(|(code, _)| *code == raw) {
                    return Err(Error::ReservedCode(as_u8(raw)));
                }
                raw
            }
        })
    }
}

/// Narrow a sub-function code for [`Error::ReservedCode`], which reports the
/// 8-bit codes the rest of the crate deals in.
fn as_u8(raw: u16) -> u8 {
    u8::try_from(raw).unwrap_or(u8::MAX)
}
