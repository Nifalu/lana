//! Server configuration, read from environment variables.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context;

/// Where the server listens unless `LANA_BIND_ADDR` says otherwise.
pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";

/// How often the background poller refreshes live measurements unless
/// `LANA_POLL_INTERVAL_SECS` says otherwise (roughly every 10 minutes).
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 600;

/// Resolves the bind address: `LANA_BIND_ADDR` if set, else `0.0.0.0:8080`.
pub fn bind_addr() -> anyhow::Result<SocketAddr> {
    let addr = resolve_bind_addr(std::env::var("LANA_BIND_ADDR").ok());
    addr.parse().context("invalid LANA_BIND_ADDR")
}

fn resolve_bind_addr(env_override: Option<String>) -> String {
    env_override.unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string())
}

/// Resolves the poller interval: `LANA_POLL_INTERVAL_SECS` (in seconds) if
/// set, else 10 minutes. Zero or non-numeric values are errors - the interval
/// must be positive or the poll loop would spin on the API.
pub fn poll_interval() -> anyhow::Result<Duration> {
    resolve_poll_interval(std::env::var("LANA_POLL_INTERVAL_SECS").ok())
        .context("invalid LANA_POLL_INTERVAL_SECS")
}

fn resolve_poll_interval(env_override: Option<String>) -> anyhow::Result<Duration> {
    match env_override {
        Some(raw) => {
            let secs: u64 = raw.parse().context("must be a number of seconds")?;
            anyhow::ensure!(secs > 0, "must be greater than zero");
            Ok(Duration::from_secs(secs))
        }
        None => Ok(Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server binds 0.0.0.0:8080 unless configured otherwise.
    #[test]
    fn default_bind_addr_is_all_interfaces_port_8080() {
        assert_eq!(resolve_bind_addr(None), "0.0.0.0:8080");
    }

    /// An explicit env override wins over the default.
    #[test]
    fn bind_addr_override_wins_over_default() {
        assert_eq!(
            resolve_bind_addr(Some("127.0.0.1:3000".to_string())),
            "127.0.0.1:3000"
        );
    }

    /// The poller refreshes live measurements every 10 minutes by default.
    #[test]
    fn default_poll_interval_is_ten_minutes() {
        assert_eq!(
            resolve_poll_interval(None).unwrap(),
            Duration::from_secs(600)
        );
    }

    /// `LANA_POLL_INTERVAL_SECS` overrides the poll interval; zero and
    /// non-numeric values are rejected so the loop cannot spin.
    #[test]
    fn poll_interval_override_must_be_positive_seconds() {
        assert_eq!(
            resolve_poll_interval(Some("30".to_string())).unwrap(),
            Duration::from_secs(30)
        );
        assert!(resolve_poll_interval(Some("0".to_string())).is_err());
        assert!(resolve_poll_interval(Some("soon".to_string())).is_err());
    }
}
