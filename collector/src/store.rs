use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{
    ArchiveRecord, ArchiveReport, BlockActivity, DatabaseSnapshot, FLAG_ACTIVITY_COMPLETE,
    FLAG_SIZE_OBSERVED,
};

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

    pub fn publish_pending(&self) -> Result<bool> {
        Ok(self.metadata_value("publish_pending")?.as_deref() == Some("1"))
    }

    pub fn set_publish_pending(&self, pending: bool) -> Result<()> {
        self.set_metadata("publish_pending", if pending { "1" } else { "0" })
    }

    pub fn materialize_closed_archive(
        &self,
        archive_path: &Path,
        today: NaiveDate,
    ) -> Result<ArchiveReport> {
        let activity_complete_through = self.activity_complete_through()?;
        let first_activity_day = self.first_activity_day()?;
        let activity_start_complete = self.activity_start_complete()?;
        let raw_records = self.closed_records(today, activity_complete_through)?;
        if raw_records.is_empty() {
            return Ok(ArchiveReport::default());
        }

        let records = fill_date_gaps(
            raw_records,
            first_activity_day,
            activity_complete_through,
            activity_start_complete,
        )?;
        let mut content = String::new();
        for record in &records {
            let line = serde_json::to_string(record).context("encode archive record")?;
            content.push_str(&line);
            content.push('\n');
        }

        let changed = match fs::read(archive_path) {
            Ok(existing) => existing != content.as_bytes(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("read archive {}", archive_path.display()));
            }
        };

        if changed {
            write_atomic(archive_path, content.as_bytes())?;
        }

        Ok(ArchiveReport {
            changed,
            record_count: records.len(),
            last_day: records
                .last()
                .and_then(|record| NaiveDate::parse_from_str(&record.date, "%Y-%m-%d").ok()),
        })
    }

    fn closed_records(
        &self,
        today: NaiveDate,
        activity_complete_through: Option<NaiveDate>,
    ) -> Result<Vec<(NaiveDate, ArchiveRecord)>> {
        let mut statement = self.connection.prepare(
            "
            WITH days AS (
                SELECT day FROM daily_snapshot WHERE day < ?1
                UNION
                SELECT day FROM block_activity
                WHERE day IS NOT NULL AND day < ?1 AND day <= ?2
            ),
            activity AS (
                SELECT
                    day,
                    MAX(height) AS peak_height,
                    COUNT(*) AS blocks,
                    SUM(additions) AS additions,
                    SUM(removals) AS removals
                FROM block_activity
                WHERE day IS NOT NULL AND day < ?1 AND day <= ?2
                GROUP BY day
            )
            SELECT
                days.day,
                daily_snapshot.db_bytes,
                daily_snapshot.wal_bytes,
                daily_snapshot.peak_height,
                activity.peak_height,
                activity.blocks,
                activity.additions,
                activity.removals
            FROM days
            LEFT JOIN daily_snapshot ON daily_snapshot.day = days.day
            LEFT JOIN activity ON activity.day = days.day
            ORDER BY days.day
            ",
        )?;

        let complete_through = activity_complete_through.map(|day| day.to_string());
        let mut rows = statement.query(params![today.to_string(), complete_through])?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            let date_string: String = row.get(0)?;
            let date = NaiveDate::parse_from_str(&date_string, "%Y-%m-%d")
                .with_context(|| format!("parse archive date {date_string}"))?;
            let db_bytes: Option<i64> = row.get(1)?;
            let wal_bytes: Option<i64> = row.get(2)?;
            let snapshot_peak: Option<i64> = row.get(3)?;
            let activity_peak: Option<i64> = row.get(4)?;
            let blocks: Option<i64> = row.get(5)?;
            let additions: Option<i64> = row.get(6)?;
            let removals: Option<i64> = row.get(7)?;

            let flags = if db_bytes.is_some() {
                FLAG_SIZE_OBSERVED
            } else {
                0
            };
            let peak_height = snapshot_peak.or(activity_peak).unwrap_or(0);
            records.push((
                date,
                ArchiveRecord {
                    date: date_string,
                    db_bytes: unsigned(db_bytes.unwrap_or(0), "db_bytes")?,
                    wal_bytes: unsigned(wal_bytes.unwrap_or(0), "wal_bytes")?,
                    peak_height: u32::try_from(peak_height)
                        .context("archive peak height exceeded u32")?,
                    blocks: u32::try_from(blocks.unwrap_or(0))
                        .context("archive block count exceeded u32")?,
                    additions: unsigned(additions.unwrap_or(0), "additions")?,
                    removals: unsigned(removals.unwrap_or(0), "removals")?,
                    flags,
                },
            ));
        }
        Ok(records)
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

fn fill_date_gaps(
    raw_records: Vec<(NaiveDate, ArchiveRecord)>,
    first_activity_day: Option<NaiveDate>,
    activity_complete_through: Option<NaiveDate>,
    activity_start_complete: bool,
) -> Result<Vec<ArchiveRecord>> {
    let first_day = raw_records
        .first()
        .map(|(day, _)| *day)
        .context("archive had no first date")?;
    let last_day = raw_records
        .last()
        .map(|(day, _)| *day)
        .context("archive had no last date")?;

    let mut records = Vec::new();
    let mut cursor = first_day;
    let mut index = 0usize;
    while cursor <= last_day {
        let mut record = if raw_records
            .get(index)
            .is_some_and(|(day, _)| *day == cursor)
        {
            let record = raw_records[index].1.clone();
            index += 1;
            record
        } else {
            ArchiveRecord {
                date: cursor.to_string(),
                db_bytes: 0,
                wal_bytes: 0,
                peak_height: 0,
                blocks: 0,
                additions: 0,
                removals: 0,
                flags: 0,
            }
        };

        let activity_complete = complete_activity_for_day(
            cursor,
            first_activity_day,
            activity_complete_through,
            activity_start_complete,
        );
        if activity_complete {
            record.flags |= FLAG_ACTIVITY_COMPLETE;
        } else if first_activity_day.is_some_and(|first_day| cursor >= first_day) {
            record.blocks = 0;
            record.additions = 0;
            record.removals = 0;
            if record.flags & FLAG_SIZE_OBSERVED == 0 {
                record.peak_height = 0;
            }
        }
        records.push(record);
        cursor = cursor.succ_opt().context("archive date overflow")?;
    }
    Ok(records)
}

fn complete_activity_for_day(
    day: NaiveDate,
    first_activity_day: Option<NaiveDate>,
    activity_complete_through: Option<NaiveDate>,
    activity_start_complete: bool,
) -> bool {
    let (Some(first_day), Some(complete_through)) = (first_activity_day, activity_complete_through)
    else {
        return false;
    };
    if day < first_day || day > complete_through {
        return false;
    }
    activity_start_complete || day > first_day
}

fn unsigned(value: i64, field: &str) -> Result<u64> {
    if value < 0 {
        bail!("archive {field} was negative");
    }
    Ok(value as u64)
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("archive path has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create archive directory {}", parent.display()))?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("archive path has no UTF-8 file name")?;
    let temporary_path: PathBuf = parent.join(format!(".{file_name}.tmp"));
    fs::write(&temporary_path, contents)
        .with_context(|| format!("write temporary archive {}", temporary_path.display()))?;
    fs::rename(&temporary_path, path)
        .with_context(|| format!("replace archive {}", path.display()))?;
    Ok(())
}
