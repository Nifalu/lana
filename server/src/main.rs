//! lana backend server entry point.
//!
//! One binary, two modes (first CLI argument):
//! - `serve`  – apply migrations, then run the REST API
//! - `import` – idempotent refresh of the open-data imports (ticket 02)

mod api;
mod config;
mod db;
mod geojson;
mod import;
mod ods;
#[cfg(test)]
mod test_support;

use std::net::SocketAddr;

use crate::ods::OdsClient;

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
        Mode::Import => import().await,
    }
}

/// Applies migrations, then serves the API until killed.
async fn serve() -> anyhow::Result<()> {
    // Applies the schema on startup, then serves live data from Postgres.
    let pool = db::init().await?;

    let addr: SocketAddr = config::bind_addr()?;
    let app = api::router(pool.clone());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("lana-server listening on http://{addr}");
    axum::serve(listener, app).await?;
    pool.close().await;
    Ok(())
}

/// Idempotent refresh of the open-data imports: fetches every dataset from
/// data.bs.ch, parses it and upserts it (safe to re-run any time).
async fn import() -> anyhow::Result<()> {
    let pool = db::init().await?;
    let summary = import::run(&pool, &OdsClient::from_env()).await?;
    println!(
        "import complete: {} fountains, {} swim areas, {} cool places, {} air stations",
        summary.fountains, summary.swim_areas, summary.cool_places, summary.air_stations
    );
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
        assert_eq!(Mode::from_arg(Some("other")), None);
        assert_eq!(Mode::from_arg(None), None);
    }
}
