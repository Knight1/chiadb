use std::{fs, path::PathBuf, process::Command};

use rusqlite::Connection;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "chia-db-startup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }

    fn run_once(&self) -> Connection {
        let output = Command::new(env!("CARGO_BIN_EXE_chia-db-collector"))
            .env_clear()
            .env("TOKIO_WORKER_THREADS", "2")
            .args(["--once", "--rpc-insecure-skip-verify"])
            .arg("--db-path")
            .arg(self.0.join("chia.sqlite"))
            .arg("--state-db-path")
            .arg(self.0.join("state.sqlite"))
            .arg("--archive-path")
            .arg(self.0.join("archive/metrics.ndjson"))
            .arg("--rpc-client-cert")
            .arg(self.0.join("not-created-yet.crt"))
            .arg("--rpc-client-key")
            .arg(self.0.join("not-created-yet.key"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        Connection::open(self.0.join("state.sqlite")).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn records_exact_bytes_before_chia_creates_tls_files() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("chia.sqlite"), [0_u8; 8192]).unwrap();
    fs::write(fixture.0.join("chia.sqlite-wal"), [0_u8; 1024]).unwrap();
    let db = fixture.run_once();

    let sample: (i64, i64) = db
        .query_row(
            "SELECT db_bytes, wal_bytes FROM daily_snapshot",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(sample, (8192, 1024));
    let activity: u32 = db
        .query_row("SELECT COUNT(*) FROM block_activity", [], |row| row.get(0))
        .unwrap();
    assert_eq!(activity, 0);
}

#[test]
fn missing_database_and_tls_files_are_a_retryable_startup_state() {
    let fixture = Fixture::new();
    let db = fixture.run_once();
    let samples: u32 = db
        .query_row("SELECT COUNT(*) FROM daily_snapshot", [], |row| row.get(0))
        .unwrap();
    assert_eq!(samples, 0);
    let cursors: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM metadata WHERE key = 'next_height'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cursors, 0);
}
