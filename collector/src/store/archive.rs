use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use rusqlite::params;

use super::{Store, calendar::fill_date_gaps};
use crate::model::{ArchiveRecord, ArchiveReport, FLAG_SIZE_OBSERVED};

impl Store {
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
