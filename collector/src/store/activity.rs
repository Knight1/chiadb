use anyhow::{Context, Result};
use chrono::NaiveDate;
use rusqlite::{OptionalExtension, params};

use super::Store;
use crate::model::BlockActivity;

impl Store {
    pub fn first_activity_day(&self) -> Result<Option<NaiveDate>> {
        self.query_activity_day("MIN(day)", "read first activity day")
    }

    pub fn latest_activity_day(&self) -> Result<Option<NaiveDate>> {
        self.query_activity_day("MAX(day)", "read latest activity day")
    }

    pub fn highest_height(&self) -> Result<Option<u32>> {
        let height: Option<i64> =
            self.connection
                .query_row("SELECT MAX(height) FROM block_activity", [], |row| {
                    row.get(0)
                })?;
        height
            .map(|height| u32::try_from(height).context("stored block height exceeded u32"))
            .transpose()
    }

    pub fn block_hash(&self, height: u32) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT header_hash FROM block_activity WHERE height = ?1",
                params![i64::from(height)],
                |row| row.get(0),
            )
            .optional()
            .context("read stored block hash")
    }

    pub fn delete_blocks_from(&self, height: u32) -> Result<()> {
        let deleted = self.connection.execute(
            "DELETE FROM block_activity WHERE height >= ?1",
            params![i64::from(height)],
        )?;
        if deleted > 0 {
            tracing::warn!(height, deleted, "discarded activity after a chain change");
        }
        Ok(())
    }

    pub fn upsert_block(&self, activity: &BlockActivity) -> Result<()> {
        let day = activity.day.map(|day| day.to_string());
        self.connection.execute(
            "
            INSERT INTO block_activity (height, header_hash, day, additions, removals)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(height) DO UPDATE SET
                header_hash = excluded.header_hash,
                day = excluded.day,
                additions = excluded.additions,
                removals = excluded.removals
            ",
            params![
                i64::from(activity.height),
                activity.header_hash,
                day,
                i64::from(activity.additions),
                i64::from(activity.removals),
            ],
        )?;
        Ok(())
    }

    fn query_activity_day(&self, aggregate: &str, context: &str) -> Result<Option<NaiveDate>> {
        let query = format!("SELECT {aggregate} FROM block_activity WHERE day IS NOT NULL");
        let day: Option<String> = self.connection.query_row(&query, [], |row| row.get(0))?;
        day.map(|day| {
            NaiveDate::parse_from_str(&day, "%Y-%m-%d")
                .with_context(|| format!("{context}: invalid date {day}"))
        })
        .transpose()
    }
}
