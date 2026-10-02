//! Server configuration, read from environment variables.

use std::net::SocketAddr;

use anyhow::Context;

/// Where the server listens unless `LANA_BIND_ADDR` says otherwise.
pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";

/// Resolves the bind address: `LANA_BIND_ADDR` if set, else `0.0.0.0:8080`.
pub fn bind_addr() -> anyhow::Result<SocketAddr> {
    let addr = resolve_bind_addr(std::env::var("LANA_BIND_ADDR").ok());
    addr.parse().context("invalid LANA_BIND_ADDR")
}

fn resolve_bind_addr(env_override: Option<String>) -> String {
    env_override.unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string())
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
}
