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
    ///
    /// FR-R-007 — an [`ExceptionStatus`] shares the width but is not a unit
    /// identifier:
    ///
    /// ```compile_fail
    /// use rust_modbus::{ExceptionStatus, UnitId};
    /// fn address(_: UnitId) {}
    /// address(ExceptionStatus(1));
    /// ```
    UnitId(u8)
}

value! {
    /// The transaction identifier of an MBAP header (FR-R-101), by which a
    /// response is matched to its request.
    ///
    /// FR-R-007 — an [`Address`] shares the width but is not a transaction
    /// identifier:
    ///
    /// ```compile_fail
    /// use rust_modbus::{Address, TransactionId};
    /// fn transaction(_: TransactionId) {}
    /// transaction(Address(1));
    /// ```
    TransactionId(u16)
}

value! {
    /// A data address: the start of a range, or the single item written.
    ///
    /// FR-R-007 — the right type is accepted:
    ///
    /// ```
    /// use rust_modbus::Address;
    /// fn start(_: Address) {}
    /// start(Address(1));
    /// ```
    ///
    /// FR-R-007 — a [`Quantity`] shares the width but is not an address:
    ///
    /// ```compile_fail
    /// use rust_modbus::{Address, Quantity};
    /// fn start(_: Address) {}
    /// start(Quantity(1));
    /// ```
    Address(u16)
}

value! {
    /// A count of coils, discrete inputs, or registers.
    Quantity(u16)
}

value! {
    /// The contents of one 16-bit register (FR-R-004).
    ///
    /// FR-R-007 — a [`Mask`] shares the width but is not register contents:
    ///
    /// ```compile_fail
    /// use rust_modbus::{Mask, RegisterValue};
    /// fn store(_: RegisterValue) {}
    /// store(Mask(0x00F2));
    /// ```
    RegisterValue(u16)
}

value! {
    /// An AND or OR mask of Mask Write Register (FR-R-035).
    Mask(u16)
}

value! {
    /// The file a record belongs to (FR-R-050).
    ///
    /// FR-R-007 — a [`RecordNumber`] shares the width but is not a file number:
    ///
    /// ```compile_fail
    /// use rust_modbus::{FileNumber, RecordNumber};
    /// fn file(_: FileNumber) {}
    /// file(RecordNumber(4));
    /// ```
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
    /// FR-R-155 — each domain value is a transparent wrapper: the wrapped
    /// integer goes in and comes back out unchanged, in either direction.
    fn ut_values_are_transparent() {
        assert_eq!(UnitId::from(0x11).0, 0x11);
        assert_eq!(u8::from(UnitId(0x11)), 0x11);
        assert_eq!(Address::from(0x006B).0, 0x006B);
        assert_eq!(u16::from(Quantity(3)), 3);
    }

    #[test]
    /// FR-R-155, FR-E-026 — no validation beyond the wire width: every value the field can
    /// hold is constructible, including the reserved unit range and a quantity
    /// no function code accepts.
    fn ut_values_impose_no_validation() {
        assert_eq!(UnitId(250).0, 250);
        assert_eq!(UnitId(u8::MAX).0, u8::MAX);
        assert_eq!(Quantity(u16::MAX).0, u16::MAX);
        assert_eq!(Address(u16::MAX).0, u16::MAX);
    }

    #[test]
    /// FR-R-152, FR-R-158 — a domain value Displays as its bare wrapped value, with no
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

    #[test]
    /// FR-E-022 — Display does not judge legality: a unit identifier in the
    /// reserved range 248–255 renders like any other value.
    fn ut_display_of_protocol_illegal_value() {
        assert_eq!(format!("{}", UnitId(250)), "250");
        assert_eq!(format!("{}", Quantity(2001)), "2001");
    }

    #[cfg(feature = "serde")]
    #[test]
    /// FR-R-157, FR-E-021 — deserialization adds no validation beyond the
    /// integer's width: a reserved unit identifier and a quantity no function
    /// code accepts both deserialize, and only a value too wide for the
    /// wrapped integer fails.
    fn ut_deserialize_imposes_no_validation() {
        assert_eq!(
            serde_json::from_str::<UnitId>("250").expect("fits in u8"),
            UnitId(250)
        );
        assert_eq!(
            serde_json::from_str::<Quantity>("2001").expect("fits in u16"),
            Quantity(2001)
        );
        assert!(serde_json::from_str::<UnitId>("256").is_err());
        assert!(serde_json::from_str::<Quantity>("65536").is_err());
    }
}
