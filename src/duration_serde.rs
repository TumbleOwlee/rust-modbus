//! `Duration` serde helpers, one module per on-wire unit (TR-R-058, TR-R-059,
//! CL-R-065).
//!
//! A config field's serde representation is a whole number in one explicit
//! unit rather than `Duration`'s own `{secs, nanos}` shape, so the unit is
//! part of the field name and cannot silently round (a milliseconds field
//! would round `TransportConfig::default()`'s 2,005,208 ns interval to 2 ms).
//! One module per unit, not one per field, since the representation is the
//! same for every field sharing a unit.

#![cfg(feature = "serde")]

/// A `Duration` represented as a whole number of milliseconds.
pub(crate) mod millis {
    use core::time::Duration;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Serialize as whole milliseconds.
    ///
    /// # Errors
    ///
    /// Fails if the duration's millisecond count does not fit a `u64`, rather
    /// than truncating it.
    pub(crate) fn serialize<S: Serializer>(
        value: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let millis = u64::try_from(value.as_millis()).map_err(serde::ser::Error::custom)?;
        millis.serialize(serializer)
    }

    /// Deserialize from whole milliseconds.
    ///
    /// # Errors
    ///
    /// Fails if the field is not a `u64`, rather than wrapping it into one.
    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        Ok(Duration::from_millis(u64::deserialize(deserializer)?))
    }
}

/// A `Duration` represented as a whole number of nanoseconds.
pub(crate) mod nanos {
    use core::time::Duration;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Serialize as whole nanoseconds.
    ///
    /// # Errors
    ///
    /// Fails if the duration's nanosecond count does not fit a `u64`, rather
    /// than truncating it.
    pub(crate) fn serialize<S: Serializer>(
        value: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let nanos = u64::try_from(value.as_nanos()).map_err(serde::ser::Error::custom)?;
        nanos.serialize(serializer)
    }

    /// Deserialize from whole nanoseconds.
    ///
    /// # Errors
    ///
    /// Fails if the field is not a `u64`, rather than wrapping it into one.
    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        Ok(Duration::from_nanos(u64::deserialize(deserializer)?))
    }
}
