//! SQLite setup and migrations.
//!
//! The schema is a simple ordered list of SQL batches. Each entry gets an
//! implicit version (1-based index) stored in `PRAGMA user_version`. To add a
//! migration, append a new batch to `MIGRATIONS` – never edit old ones.

use std::path::Path;

use anyhow::Context;
use rusqlite::Connection;

/// Schema migrations, applied in order. Append-only.
const MIGRATIONS: &[&str] = &[
    // v1 – demo table so the DB is exercised from day one.
    // Replace with the real schema once the data model is known.
    "CREATE TABLE IF NOT EXISTS notes (
        id   INTEGER PRIMARY KEY AUTOINCREMENT,
        text TEXT NOT NULL
    );",
];

/// Opens (creating if needed) the app database inside `data_dir` and applies
/// pending migrations.
pub fn init(data_dir: &Path) -> anyhow::Result<Connection> {
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("failed to create data dir {}", data_dir.display()))?;

    let db_path = data_dir.join("lana.db");
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;

    conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get::<_, String>(0))
        .context("failed to enable WAL mode")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    migrate(&conn)?;
    Ok(conn)
}

/// Applies all migration batches newer than the current `user_version`.
fn migrate(conn: &Connection) -> anyhow::Result<()> {
    let mut current: u32 =
        conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    for (idx, batch) in MIGRATIONS.iter().enumerate() {
        let version = (idx + 1) as u32;
        if version > current {
            let apply = || -> anyhow::Result<()> {
                conn.execute_batch(batch)?;
                conn.pragma_update(None, "user_version", version)?;
                Ok(())
            };
            // The whole database file is the transaction unit here: apply and
            // stamp atomically so a crash cannot leave a half-applied batch.
            conn.execute_batch("BEGIN;")?;
            match apply() {
                Ok(()) => conn.execute_batch("COMMIT;")?,
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK;");
                    return Err(e);
                }
            }
            current = version;
        }
    }

    Ok(())
}

/// Current schema version – exposed as a Tauri command in `lib.rs`.
pub fn user_version(conn: &Connection) -> rusqlite::Result<u32> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_applies_migrations() {
        let dir = std::env::temp_dir().join(format!("lana-test-{}", std::process::id()));
        let conn = init(&dir).unwrap();
        assert_eq!(user_version(&conn).unwrap(), MIGRATIONS.len() as u32);

        // Re-opening must be idempotent.
        drop(conn);
        let conn = init(&dir).unwrap();
        assert_eq!(user_version(&conn).unwrap(), MIGRATIONS.len() as u32);
        std::fs::remove_dir_all(&dir).ok();
    }
}
