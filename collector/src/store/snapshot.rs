use anyhow::{Context, Result};
use rusqlite::params;

use super::Store;
use crate::model::DatabaseSnapshot;

impl Store {
    pub fn upsert_snapshot(&self, snapshot: &DatabaseSnapshot) -> Result<()> {
        let db_bytes = i64::try_from(snapshot.db_bytes).context("database size exceeded i64")?;
        let wal_bytes = i64::try_from(snapshot.wal_bytes).context("WAL size exceeded i64")?;
        self.connection.execute(
            "
            INSERT INTO daily_snapshot (day, db_bytes, wal_bytes, peak_height, captured_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(day) DO UPDATE SET
                db_bytes = excluded.db_bytes,
                wal_bytes = excluded.wal_bytes,
                peak_height = excluded.peak_height,
                captured_at = excluded.captured_at
            ",
            params![
                snapshot.day.to_string(),
                db_bytes,
                wal_bytes,
                i64::from(snapshot.peak_height),
                snapshot.captured_at,
            ],
        )?;
        Ok(())
    }
}
