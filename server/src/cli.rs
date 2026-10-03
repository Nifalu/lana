//! CLI dispatch for the `lana-server` binary (kept in the library so the
//! crate-internal pieces it drives stay `pub(crate)`).

use std::net::SocketAddr;

use crate::api;
use crate::config;
use crate::db;
use crate::import;
use crate::ods::OdsClient;
use crate::poller;

/// What the binary should do, chosen by the first CLI argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Serve,
    Import,
    Poll,
}

impl Mode {
    /// Parses the mode from the first CLI argument (if any).
    pub fn from_arg(arg: Option<&str>) -> Option<Self> {
        match arg {
            Some("serve") => Some(Self::Serve),
            Some("import") => Some(Self::Import),
            Some("poll") => Some(Self::Poll),
            _ => None,
        }
    }
}

/// Entry point of the binary: dispatches `mode`, exits non-zero on failure.
pub fn main() {
    match Mode::from_arg(std::env::args().nth(1).as_deref()) {
        Some(mode) => {
            let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
            if let Err(err) = runtime.block_on(dispatch(mode)) {
                eprintln!("error: {err:#}");
                std::process::exit(1);
            }
        }
        None => {
            eprintln!("usage: lana-server <serve|import|poll>");
            std::process::exit(2);
        }
    }
}

async fn dispatch(mode: Mode) -> anyhow::Result<()> {
    match mode {
        Mode::Serve => serve().await,
        Mode::Import => import_mode().await,
        Mode::Poll => poll().await,
    }
}

/// Applies migrations, then serves the API with the background poller
/// (live measurements refresh every [`config::poll_interval`], first cycle
/// immediately) until killed.
async fn serve() -> anyhow::Result<()> {
    // Applies the schema on startup, then serves live data from Postgres.
    let pool = db::init().await?;

    let interval = config::poll_interval()?;
    let source = OdsClient::from_env();
    let poller_pool = pool.clone();
    tokio::spawn(async move {
        poller::run_loop(poller_pool, source, interval).await;
    });

    let addr: SocketAddr = config::bind_addr()?;
    let app = api::router(pool.clone());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("lana-server listening on http://{addr} (polling live data every {interval:?})");
    axum::serve(listener, app).await?;
    pool.close().await;
    Ok(())
}

/// Idempotent refresh of the open-data imports: fetches every dataset from
/// data.bs.ch, parses it and upserts it (safe to re-run any time).
async fn import_mode() -> anyhow::Result<()> {
    let pool = db::init().await?;
    let summary = import::run(&pool, &OdsClient::from_env()).await?;
    println!(
        "import complete: {} fountains, {} swim areas, {} cool places, {} air stations",
        summary.fountains, summary.swim_areas, summary.cool_places, summary.air_stations
    );
    pool.close().await;
    Ok(())
}

/// Manual poll trigger: one poll cycle of the live measurements, logging
/// what it fetched (used by tests/demos instead of waiting for the timer).
async fn poll() -> anyhow::Result<()> {
    let pool = db::init().await?;
    let summary = poller::run(&pool, &OdsClient::from_env()).await?;
    println!("{summary}");
    pool.close().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mode parser accepts exactly the documented mode names.
    #[test]
    fn mode_parsing() {
        assert_eq!(Mode::from_arg(Some("serve")), Some(Mode::Serve));
        assert_eq!(Mode::from_arg(Some("import")), Some(Mode::Import));
        assert_eq!(Mode::from_arg(Some("poll")), Some(Mode::Poll));
        assert_eq!(Mode::from_arg(Some("other")), None);
        assert_eq!(Mode::from_arg(None), None);
    }
}
