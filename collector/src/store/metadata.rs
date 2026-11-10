use anyhow::{Context, Result};
use chrono::NaiveDate;
use rusqlite::{OptionalExtension, params};

use super::Store;

impl Store {
    pub fn cursor(&self) -> Result<Option<u32>> {
        self.metadata_value("next_height")?
            .map(|value| {
                value
                    .parse::<u32>()
                    .context("parse persisted collector cursor")
            })
            .transpose()
    }

    pub fn set_cursor(&self, next_height: u32) -> Result<()> {
        self.set_metadata("next_height", &next_height.to_string())
    }

    pub fn initial_network_contact_observed(&self) -> Result<bool> {
        Ok(self
            .metadata_value("initial_network_contact_observed")?
            .as_deref()
            == Some("1"))
    }

    pub fn set_initial_network_contact_observed(&self, observed: bool) -> Result<()> {
        self.set_metadata(
            "initial_network_contact_observed",
            if observed { "1" } else { "0" },
        )
    }

    pub fn bootstrap_pending(&self) -> Result<bool> {
        Ok(self.metadata_value("bootstrap_pending")?.as_deref() == Some("1"))
    }

    pub fn set_bootstrap_pending(&self, pending: bool) -> Result<()> {
        self.set_metadata("bootstrap_pending", if pending { "1" } else { "0" })
    }

    pub fn activity_start_complete(&self) -> Result<bool> {
        Ok(self.metadata_value("activity_start_complete")?.as_deref() == Some("1"))
    }

    pub fn set_activity_start_complete(&self, complete: bool) -> Result<()> {
        self.set_metadata("activity_start_complete", if complete { "1" } else { "0" })
    }

    pub fn activity_complete_through(&self) -> Result<Option<NaiveDate>> {
        self.metadata_date("activity_complete_through")
    }

    pub fn set_activity_complete_through(&self, day: Option<NaiveDate>) -> Result<()> {
        match day {
            Some(day) => self.set_metadata("activity_complete_through", &day.to_string()),
            None => self.delete_metadata("activity_complete_through"),
        }
    }

    pub fn publish_pending(&self) -> Result<bool> {
        Ok(self.metadata_value("publish_pending")?.as_deref() == Some("1"))
    }

    pub fn set_publish_pending(&self, pending: bool) -> Result<()> {
        self.set_metadata("publish_pending", if pending { "1" } else { "0" })
    }

    fn metadata_date(&self, key: &str) -> Result<Option<NaiveDate>> {
        self.metadata_value(key)?
            .map(|value| {
                NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                    .with_context(|| format!("parse {key} metadata date"))
            })
            .transpose()
    }

    fn metadata_value(&self, key: &str) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT value FROM metadata WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .context("read collector metadata")
    }

    fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        self.connection.execute(
            "
            INSERT INTO metadata (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            ",
            params![key, value],
        )?;
        Ok(())
    }

    fn delete_metadata(&self, key: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM metadata WHERE key = ?1", params![key])?;
        Ok(())
    }
}
