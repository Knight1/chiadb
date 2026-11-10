use anyhow::{Context, Result};
use chrono::NaiveDate;

use crate::model::{ArchiveRecord, FLAG_ACTIVITY_COMPLETE, FLAG_SIZE_OBSERVED};

pub(super) fn fill_date_gaps(
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
