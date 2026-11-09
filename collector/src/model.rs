use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

pub const FLAG_SIZE_OBSERVED: u32 = 1;
pub const FLAG_ACTIVITY_COMPLETE: u32 = 1 << 1;

#[derive(Clone, Debug)]
pub struct BlockActivity {
    pub height: u32,
    pub header_hash: String,
    pub day: Option<NaiveDate>,
    pub additions: u32,
    pub removals: u32,
}

#[derive(Clone, Debug)]
pub struct DatabaseSnapshot {
    pub day: NaiveDate,
    pub db_bytes: u64,
    pub wal_bytes: u64,
    pub peak_height: u32,
    pub captured_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ArchiveRecord {
    pub date: String,
    pub db_bytes: u64,
    pub wal_bytes: u64,
    pub peak_height: u32,
    pub blocks: u32,
    pub additions: u64,
    pub removals: u64,
    pub flags: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ArchiveReport {
    pub changed: bool,
    pub record_count: usize,
    pub last_day: Option<NaiveDate>,
}
