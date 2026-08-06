//! TLS transport over TCP, behind the `tls` feature (`TR-R-060` … `TR-R-068`).

/// The IANA-registered port for Modbus over TLS (TR-R-068).
///
/// Documentation constant only: no API in this crate applies it implicitly —
/// [`connect_tls`](crate::transport::connect_tls)/the TLS listener each take
/// an explicit `SocketAddr`, same as their plain-TCP counterparts.
pub const MODBUS_TLS_PORT: u16 = 802;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// TR-R-068 — the documented port for Modbus over TLS.
    fn ut_modbus_tls_port_is_802() {
        assert_eq!(MODBUS_TLS_PORT, 802);
    }
}
