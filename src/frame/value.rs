//! Domain value types (FR-R-007).
//!
//! A Modbus address, a quantity, a register value and a unit identifier are
//! different things that happen to share a width. Each gets its own type so
//! passing one where another is meant does not compile.
//!
//! The wrappers are transparent and impose no validation: every value the wire
//! can carry is constructible. Which values are *sensible* is decided where it
//! already was — encoding for the structural ranges (FR-R-021, FR-R-027,
//! FR-R-031), the server for the device map.

/// Define a transparent wrapper over one integer.
///
/// A declarative macro rather than a derive dependency: the generated code for
/// the mandatory impls (`Debug`, the `Copy`/`Eq`/`Ord`/`Hash` family, `From` in
/// both directions, and `Display`) is smaller than the cost of a proc-macro
/// crate in the tree of a protocol library. The `serde` derives are opt-in
/// (FR-R-151) via `#[cfg_attr]`, which does not change that trade-off: they add
/// no dependency unless the feature is enabled.
macro_rules! value {
    ($(#[$meta:meta])* $name:ident($inner:ty)) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        pub struct $name(pub $inner);

        impl From<$inner> for $name {
            fn from(value: $inner) -> Self {
                Self(value)
            }
        }

        impl From<$name> for $inner {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl core::fmt::Display for $name {
            /// FR-R-152 — the bare wrapped value, with no type name, field
            /// name, or surrounding punctuation.
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

/// The address every server on a serial line acts on and none answers
/// (FR-R-096, FR-R-117).
///
/// Only the client and server areas have a use for it, and both are `std`-gated
/// (CL-R-004, SV-R-006).
#[cfg(feature = "std")]
pub(crate) const BROADCAST_UNIT: UnitId = UnitId(0);

value! {
    /// A server address on a serial line (FR-R-096, FR-R-117), or the unit
    /// identifier of an MBAP header (FR-R-101).
    UnitId(u8)
}

value! {
    /// The transaction identifier of an MBAP header (FR-R-101), by which a
    /// response is matched to its request.
    TransactionId(u16)
}

value! {
    /// A data address: the start of a range, or the single item written.
    Address(u16)
}

value! {
    /// A count of coils, discrete inputs, or registers.
    Quantity(u16)
}

value! {
    /// The contents of one 16-bit register (FR-R-004).
    RegisterValue(u16)
}

value! {
    /// An AND or OR mask of Mask Write Register (FR-R-044).
    Mask(u16)
}

value! {
    /// The file a record belongs to (FR-R-050).
    FileNumber(u16)
}

value! {
    /// The record within a file (FR-R-050).
    RecordNumber(u16)
}

value! {
    /// A record length, counted in registers (FR-R-050).
    RecordLength(u16)
}

value! {
    /// The output status byte of Read Exception Status (FR-R-060).
    ExceptionStatus(u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    /// FR-R-007 — each domain value is a transparent wrapper: the wrapped
    /// integer goes in and comes back out unchanged, in either direction.
    fn ut_values_are_transparent() {
        assert_eq!(UnitId::from(0x11).0, 0x11);
        assert_eq!(u8::from(UnitId(0x11)), 0x11);
        assert_eq!(Address::from(0x006B).0, 0x006B);
        assert_eq!(u16::from(Quantity(3)), 3);
    }

    #[test]
    /// FR-R-007 — no validation beyond the wire width: every value the field can
    /// hold is constructible, including the reserved unit range and a quantity
    /// no function code accepts.
    fn ut_values_impose_no_validation() {
        assert_eq!(UnitId(250).0, 250);
        assert_eq!(UnitId(u8::MAX).0, u8::MAX);
        assert_eq!(Quantity(u16::MAX).0, u16::MAX);
        assert_eq!(Address(u16::MAX).0, u16::MAX);
    }

    #[test]
    /// FR-R-152 — a domain value Displays as its bare wrapped value, with no
    /// type name, field name, or punctuation; Debug is unaffected.
    fn ut_values_display_bare() {
        assert_eq!(format!("{}", UnitId(17)), "17");
        assert_eq!(format!("{:?}", UnitId(17)), "UnitId(17)");
    }

    #[cfg(feature = "serde")]
    #[test]
    /// FR-R-151 — a domain value serializes and deserializes transparently:
    /// the JSON text is the bare wrapped integer, not `{"0":17}`. Asserting on
    /// the text, not merely on a round trip, is what would catch
    /// `#[serde(transparent)]` being dropped.
    fn ut_domain_values_serde_transparent() {
        let text = serde_json::to_string(&UnitId(17)).expect("serializes");
        assert_eq!(text, "17");
        assert_eq!(
            serde_json::from_str::<UnitId>(&text).expect("deserializes"),
            UnitId(17)
        );
    }
}
