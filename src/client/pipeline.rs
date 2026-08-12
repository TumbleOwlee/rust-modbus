//! Pipelined clients: several requests in flight at once over one connection,
//! distinguished by MBAP transaction id (CL-R-082 … CL-R-095). Gated behind
//! the `pipeline` feature.

use core::time::Duration;

use super::ClientConfig;

/// How a pipelined client waits and how many requests it lets outstand at
/// once (CL-R-093).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineConfig {
    /// How long a response may take before its request is abandoned.
    pub response_timeout: Duration,
    /// How many requests may be outstanding at once. `u16` already caps this
    /// at 65535, the MBAP transaction id space (CL-R-093) — no separate
    /// validation is needed for the ceiling.
    pub max_in_flight: u16,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            response_timeout: Duration::from_secs(1),
            max_in_flight: 16,
        }
    }
}

impl From<ClientConfig> for PipelineConfig {
    /// Carries `response_timeout` over; `max_in_flight` takes the default,
    /// since `ClientConfig` has no equivalent field to carry (CL-R-095).
    fn from(config: ClientConfig) -> Self {
        Self {
            response_timeout: config.response_timeout,
            max_in_flight: PipelineConfig::default().max_in_flight,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    #[test]
    /// CL-R-093 — the default response timeout is 1 second and the default
    /// in-flight ceiling is 16.
    fn ut_default_pipeline_config() {
        assert_eq!(
            PipelineConfig::default(),
            PipelineConfig {
                response_timeout: Duration::from_secs(1),
                max_in_flight: 16,
            }
        );
    }

    #[test]
    /// CL-R-095 — converting from `ClientConfig` carries the response
    /// timeout over and sets `max_in_flight` to `PipelineConfig`'s own
    /// default, so a caller moving from `Client` need not reconstruct
    /// configuration it already had.
    fn ut_pipeline_config_from_client_config() {
        let client_config = ClientConfig {
            response_timeout: Duration::from_millis(250),
        };
        assert_eq!(
            PipelineConfig::from(client_config),
            PipelineConfig {
                response_timeout: Duration::from_millis(250),
                max_in_flight: PipelineConfig::default().max_in_flight,
            }
        );
    }

    #[test]
    /// CL-R-086 — `try_send`'s failure at the in-flight limit is a distinct
    /// error from desynchronization, not folded into it.
    fn ut_too_many_in_flight_is_distinct_from_desynchronized() {
        assert_ne!(Error::TooManyInFlight, Error::Desynchronized);
    }
}
