//! Server configuration, read from environment variables.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context;

/// Where the server listens unless `LANA_BIND_ADDR` says otherwise.
pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8090";

/// How often the background poller refreshes live measurements unless
/// `LANA_POLL_INTERVAL_SECS` says otherwise (roughly every 10 minutes).
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 600;

/// Resolves the bind address: `LANA_BIND_ADDR` if set, else `0.0.0.0:8090`.
pub fn bind_addr() -> anyhow::Result<SocketAddr> {
    let addr = resolve_bind_addr(std::env::var("LANA_BIND_ADDR").ok());
    addr.parse().context("invalid LANA_BIND_ADDR")
}

fn resolve_bind_addr(env_override: Option<String>) -> String {
    env_override.unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string())
}

/// Resolves the poller interval: `LANA_POLL_INTERVAL_SECS` (in seconds) if
/// set, else 10 minutes. Zero or non-numeric values are errors – the interval
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

/// Resolves the live-location / closest-helpers API base URL:
/// `LANA_HELPER_API_URL` if set to a non-blank value, else `None` (feature
/// off: locations are not forwarded and SOS matching stays local). A trailing
/// slash is dropped so paths can be appended uniformly.
pub fn helper_api_url() -> Option<String> {
    resolve_helper_api_url(std::env::var("LANA_HELPER_API_URL").ok())
}

fn resolve_helper_api_url(env_override: Option<String>) -> Option<String> {
    let raw = env_override?;
    let url = raw.trim().trim_end_matches('/');
    (!url.is_empty()).then(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server binds 0.0.0.0:8090 unless configured otherwise.
    #[test]
    fn default_bind_addr_is_all_interfaces_port_8090() {
        assert_eq!(resolve_bind_addr(None), "0.0.0.0:8090");
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

    /// The helper API is off unless `LANA_HELPER_API_URL` names it; an unset
    /// or empty (e.g. `${LANA_HELPER_API_URL:-}` in compose) value disables it.
    #[test]
    fn helper_api_is_disabled_when_unset_or_blank() {
        assert_eq!(resolve_helper_api_url(None), None);
        assert_eq!(resolve_helper_api_url(Some(String::new())), None);
        assert_eq!(resolve_helper_api_url(Some("  ".to_string())), None);
    }

    /// The configured URL is used as given, minus whitespace and trailing
    /// slashes.
    #[test]
    fn helper_api_url_tolerates_trailing_slash() {
        assert_eq!(
            resolve_helper_api_url(Some("https://lana.heitzli.ch".to_string())),
            Some("https://lana.heitzli.ch".to_string())
        );
        assert_eq!(
            resolve_helper_api_url(Some(" https://lana.heitzli.ch/ ".to_string())),
            Some("https://lana.heitzli.ch".to_string())
        );
        assert_eq!(
            resolve_helper_api_url(Some("http://127.0.0.1:9000//".to_string())),
            Some("http://127.0.0.1:9000".to_string())
        );
    }
}
