//! lana backend server entry point.
//!
//! One binary, two modes (first CLI argument):
//! - `serve`  – apply migrations, then run the REST API
//! - `import` – idempotent refresh of the open-data imports (ticket 02)

mod api;
mod config;
mod db;
mod geojson;

use std::net::SocketAddr;

/// What the binary should do, chosen by the first CLI argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Serve,
    Import,
}

impl Mode {
    /// Parses the mode from the first CLI argument (if any).
    fn from_arg(arg: Option<&str>) -> Option<Self> {
        match arg {
            Some("serve") => Some(Self::Serve),
            Some("import") => Some(Self::Import),
            _ => None,
        }
    }
}

#[tokio::main]
async fn main() {
    match Mode::from_arg(std::env::args().nth(1).as_deref()) {
        Some(mode) => {
            if let Err(err) = dispatch(mode).await {
                eprintln!("error: {err:#}");
                std::process::exit(1);
            }
        }
        None => {
            eprintln!("usage: lana-server <serve|import>");
            std::process::exit(2);
        }
    }
}

async fn dispatch(mode: Mode) -> anyhow::Result<()> {
    match mode {
        Mode::Serve => serve().await,
        Mode::Import => Err(anyhow::anyhow!("import: not implemented (ticket 02)")),
    }
}

/// Applies migrations, then serves the API until killed.
async fn serve() -> anyhow::Result<()> {
    // Applies the schema on startup; the API itself reads Postgres from
    // ticket 02 on.
    let pool = db::init().await?;

    let addr: SocketAddr = config::bind_addr()?;
    let app = api::router(pool.clone());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("lana-server listening on http://{addr}");
    axum::serve(listener, app).await?;
    pool.close().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `import` exists as a mode but is not implemented yet – it fails loudly
    /// instead of silently doing nothing (ticket 02 fills it in).
    #[tokio::test]
    async fn import_mode_reports_not_implemented() {
        let err = dispatch(Mode::Import)
            .await
            .expect_err("import should not succeed yet");
        assert!(
            err.to_string().contains("not implemented"),
            "unexpected error: {err:#}"
        );
    }

    /// The mode parser accepts exactly the documented mode names.
    #[test]
    fn mode_parsing() {
        assert_eq!(Mode::from_arg(Some("serve")), Some(Mode::Serve));
        assert_eq!(Mode::from_arg(Some("import")), Some(Mode::Import));
        assert_eq!(Mode::from_arg(Some("other")), None);
        assert_eq!(Mode::from_arg(None), None);
    }
}
