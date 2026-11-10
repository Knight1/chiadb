use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use tracing::info;

use crate::{config::Config, model::DatabaseSnapshot, store::Store};

pub(super) fn capture_database_snapshot(
    config: &Config,
    store: &Store,
    now: DateTime<Utc>,
) -> Result<Option<DatabaseSnapshot>> {
    let db_bytes = match database_size(&config.db_path) {
        Ok(db_bytes) => db_bytes,
        Err(error) if is_not_found(&error) => {
            info!(
                database = %config.db_path.display(),
                retry_seconds = config.poll_seconds,
                "Chia database file is not ready; will retry"
            );
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let snapshot = DatabaseSnapshot {
        day: now.date_naive(),
        db_bytes,
        wal_bytes: sidecar_size(&config.db_path, "-wal")?,
        peak_height: 0,
        captured_at: now.to_rfc3339(),
    };
    store.upsert_snapshot(&snapshot)?;
    Ok(Some(snapshot))
}

fn is_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
    })
}

fn database_size(path: &Path) -> Result<u64> {
    fs::metadata(path)
        .with_context(|| format!("stat Chia database {}", path.display()))
        .map(|metadata| metadata.len())
}

fn sidecar_size(database_path: &Path, suffix: &str) -> Result<u64> {
    let sidecar = sqlite_sidecar_path(database_path, suffix)?;
    match fs::metadata(&sidecar) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => {
            Err(error).with_context(|| format!("stat SQLite sidecar {}", sidecar.display()))
        }
    }
}

fn sqlite_sidecar_path(database_path: &Path, suffix: &str) -> Result<PathBuf> {
    let file_name = database_path
        .file_name()
        .and_then(|name| name.to_str())
        .context("database path has no UTF-8 file name")?;
    Ok(database_path.with_file_name(format!("{file_name}{suffix}")))
}
