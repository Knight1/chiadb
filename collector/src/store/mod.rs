mod activity;
mod archive;
mod calendar;
mod metadata;
mod snapshot;

use std::{fs, path::Path, time::Duration};

use anyhow::{Context, Result};
use rusqlite::Connection;

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| {
                format!("create collector state directory {}", parent.display())
            })?;
        }

        let connection = Connection::open(path)
            .with_context(|| format!("open collector state {}", path.display()))?;
        connection
            .busy_timeout(Duration::from_secs(10))
            .context("configure SQLite busy timeout")?;
        connection.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS block_activity (
                height INTEGER PRIMARY KEY NOT NULL,
                header_hash TEXT NOT NULL,
                day TEXT,
                additions INTEGER NOT NULL,
                removals INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS block_activity_day_idx
                ON block_activity(day);

            CREATE TABLE IF NOT EXISTS daily_snapshot (
                day TEXT PRIMARY KEY NOT NULL,
                db_bytes INTEGER NOT NULL,
                wal_bytes INTEGER NOT NULL,
                peak_height INTEGER NOT NULL,
                captured_at TEXT NOT NULL
            );
            ",
        )?;
        Ok(Self { connection })
    }
}
