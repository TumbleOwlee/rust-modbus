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
/// A declarative macro rather than a derive dependency: the generated code is
/// three impls per type, which is less than the cost of a proc-macro crate in
/// the tree of a protocol library.
macro_rules! value {
    ($(#[$meta:meta])* $name:ident($inner:ty)) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    };
}

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
}
