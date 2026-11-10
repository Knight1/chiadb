use std::{env, fs, path::PathBuf};

use chia_db_collector::{
    model::{
        ArchiveRecord, BlockActivity, DatabaseSnapshot, FLAG_ACTIVITY_COMPLETE, FLAG_SIZE_OBSERVED,
    },
    store::Store,
};
use chrono::NaiveDate;

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn test_directory(name: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "chia-db-collector-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn read_archive(path: &PathBuf) -> Vec<ArchiveRecord> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn materializes_calendar_gaps_with_complete_activity_flags() {
    let directory = test_directory("complete-archive");
    let archive_path = directory.join("archive/metrics.ndjson");
    let store = Store::open(&directory.join("state.sqlite")).unwrap();
    store.set_activity_start_complete(true).unwrap();
    store
        .upsert_block(&BlockActivity {
            height: 1,
            header_hash: "0x01".to_owned(),
            day: Some(day("2026-01-01")),
            additions: 3,
            removals: 2,
        })
        .unwrap();
    store
        .upsert_snapshot(&DatabaseSnapshot {
            day: day("2026-01-03"),
            db_bytes: 123,
            wal_bytes: 4,
            peak_height: 2,
            captured_at: "2026-01-03T23:59:00Z".to_owned(),
        })
        .unwrap();
    store
        .set_activity_complete_through(Some(day("2026-01-03")))
        .unwrap();

    let report = store
        .materialize_closed_archive(&archive_path, day("2026-01-04"))
        .unwrap();
    assert!(report.changed);
    assert_eq!(report.record_count, 3);

    let records = read_archive(&archive_path);
    assert_eq!(records[0].flags, FLAG_ACTIVITY_COMPLETE);
    assert_eq!(records[0].additions, 3);
    assert_eq!(records[1].flags, FLAG_ACTIVITY_COMPLETE);
    assert_eq!(
        records[2].flags,
        FLAG_SIZE_OBSERVED | FLAG_ACTIVITY_COMPLETE
    );
    assert_eq!(records[2].db_bytes, 123);

    drop(store);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn hides_counts_for_an_incomplete_first_activity_day() {
    let directory = test_directory("partial-start");
    let archive_path = directory.join("archive/metrics.ndjson");
    let store = Store::open(&directory.join("state.sqlite")).unwrap();
    store.set_activity_start_complete(false).unwrap();
    store
        .upsert_block(&BlockActivity {
            height: 1,
            header_hash: "0x01".to_owned(),
            day: Some(day("2026-01-01")),
            additions: 3,
            removals: 2,
        })
        .unwrap();
    store
        .upsert_snapshot(&DatabaseSnapshot {
            day: day("2026-01-02"),
            db_bytes: 123,
            wal_bytes: 4,
            peak_height: 2,
            captured_at: "2026-01-02T23:59:00Z".to_owned(),
        })
        .unwrap();
    store
        .set_activity_complete_through(Some(day("2026-01-02")))
        .unwrap();

    store
        .materialize_closed_archive(&archive_path, day("2026-01-03"))
        .unwrap();

    let records = read_archive(&archive_path);
    assert_eq!(records[0].flags, 0);
    assert_eq!(records[0].additions, 0);
    assert_eq!(records[0].removals, 0);
    assert_eq!(
        records[1].flags,
        FLAG_SIZE_OBSERVED | FLAG_ACTIVITY_COMPLETE
    );

    drop(store);
    fs::remove_dir_all(directory).unwrap();
}
