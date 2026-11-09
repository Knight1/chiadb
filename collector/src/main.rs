mod config;
mod model;
mod publisher;
mod rpc;
mod store;

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use clap::Parser;
use futures_util::{StreamExt, TryStreamExt, stream};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::{
    config::Config,
    model::{BlockActivity, DatabaseSnapshot},
    publisher::GitPublisher,
    rpc::{BlockRecord, ChiaRpc},
    store::Store,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,chia_db_collector=info")),
        )
        .with_target(false)
        .compact()
        .init();

    let config = Config::parse();
    config.validate()?;

    let store = Store::open(&config.state_db_path)?;
    let rpc = ChiaRpc::new(&config)?;
    let publisher = config
        .git_publish
        .then(|| GitPublisher::from_config(&config));

    loop {
        let result = collect_once(&config, &rpc, &store, publisher.as_ref()).await;
        if config.once {
            return result;
        }
        if let Err(error) = result {
            error!(error = ?error, "collection cycle failed; will retry");
        }
        tokio::time::sleep(Duration::from_secs(config.poll_seconds)).await;
    }
}

async fn collect_once(
    config: &Config,
    rpc: &ChiaRpc,
    store: &Store,
    publisher: Option<&GitPublisher>,
) -> Result<()> {
    let node = rpc.blockchain_state().await?;
    if !node.synced {
        info!("full node is not synced; skipping collection");
        return Ok(());
    }
    let Some(peak_height) = node.peak_height else {
        info!("full node has no peak yet; skipping collection");
        return Ok(());
    };

    let now = Utc::now();
    let snapshot = DatabaseSnapshot {
        day: now.date_naive(),
        db_bytes: database_size(&config.db_path)?,
        wal_bytes: sidecar_size(&config.db_path, "-wal")?,
        peak_height,
        captured_at: now.to_rfc3339(),
    };
    store.upsert_snapshot(&snapshot)?;

    let stored_cursor = store.cursor()?;
    let mut cursor = stored_cursor.unwrap_or_else(|| {
        config
            .backfill_start_height
            .unwrap_or_else(|| peak_height.saturating_add(1))
    });
    if stored_cursor.is_none() {
        store.set_cursor(cursor)?;
        store.set_bootstrap_pending(config.backfill_start_height.is_some())?;
        store.set_activity_start_complete(cursor == 0)?;
        if config.backfill_start_height.is_some() {
            info!(cursor, "initialized historical activity backfill");
        } else {
            info!(cursor, "initialized live-only collection at current peak");
        }
    }

    cursor = reconcile_recent_chain(config, rpc, store, peak_height, cursor).await?;
    let mut processed = 0usize;

    if cursor <= peak_height {
        let end = cursor
            .saturating_add(config.max_blocks_per_poll)
            .min(peak_height.saturating_add(1));
        let mut records = rpc.block_records(cursor, end).await?;
        records.sort_by_key(|record| record.height);
        validate_range(&records, cursor, end)?;

        let mut requested = Vec::new();
        for record in records {
            match store.block_hash(record.height)? {
                Some(existing_hash) if existing_hash == record.header_hash => {}
                Some(_) => {
                    warn!(height = record.height, "chain changed during collection");
                    store.delete_blocks_from(record.height)?;
                    requested.push(record);
                }
                None => requested.push(record),
            }
        }

        let mut activities = stream::iter(
            requested
                .into_iter()
                .map(|record| fetch_block_activity(rpc, record)),
        )
        .buffer_unordered(config.rpc_concurrency);

        while let Some(activity) = activities.try_next().await? {
            store.upsert_block(&activity)?;
            processed += 1;
        }
        cursor = end;
        store.set_cursor(cursor)?;
    }

    let caught_up = cursor > peak_height;
    update_activity_completion(store, now.date_naive(), caught_up)?;

    if caught_up && store.bootstrap_pending()? {
        store.set_bootstrap_pending(false)?;
        info!("historical activity backfill is complete; materializing the archive");
    }

    let archive = if store.bootstrap_pending()? {
        info!("historical backfill is in progress; archive publishing is deferred");
        Default::default()
    } else {
        store.materialize_closed_archive(&config.archive_path, now.date_naive())?
    };
    if archive.changed && archive.last_day.is_some() {
        store.set_publish_pending(true)?;
        info!(
            records = archive.record_count,
            archive = %config.archive_path.display(),
            "updated closed daily archive"
        );
    }

    if config.git_publish && caught_up && store.publish_pending()? {
        let last_day = archive
            .last_day
            .context("Git publishing was pending but the archive has no closed day")?;
        let publisher = publisher.context("Git publisher was not initialized")?;
        publisher.publish_archive(&config.archive_path, last_day)?;
        store.set_publish_pending(false)?;
    }

    info!(
        peak_height,
        cursor,
        processed,
        caught_up,
        db_bytes = snapshot.db_bytes,
        wal_bytes = snapshot.wal_bytes,
        "collection cycle complete"
    );
    Ok(())
}

fn update_activity_completion(
    store: &Store,
    today: chrono::NaiveDate,
    caught_up: bool,
) -> Result<()> {
    let Some(first_day) = store.first_activity_day()? else {
        store.set_activity_complete_through(None)?;
        return Ok(());
    };
    let Some(latest_day) = store.latest_activity_day()? else {
        store.set_activity_complete_through(None)?;
        return Ok(());
    };

    let candidate = if caught_up {
        today.pred_opt().map(|yesterday| yesterday.min(latest_day))
    } else {
        latest_day.pred_opt()
    };
    let complete_through = candidate.filter(|day| *day >= first_day);
    store.set_activity_complete_through(complete_through)?;
    Ok(())
}

async fn reconcile_recent_chain(
    config: &Config,
    rpc: &ChiaRpc,
    store: &Store,
    peak_height: u32,
    mut cursor: u32,
) -> Result<u32> {
    let Some(highest_stored) = store.highest_height()? else {
        return Ok(cursor);
    };

    if highest_stored > peak_height {
        let restart = peak_height.saturating_add(1);
        store.delete_blocks_from(restart)?;
        cursor = cursor.min(restart);
        store.set_cursor(cursor)?;
        return Ok(cursor);
    }

    let start = highest_stored.saturating_sub(config.reorg_check_depth);
    let end = highest_stored.saturating_add(1);
    let mut records = rpc.block_records(start, end).await?;
    records.sort_by_key(|record| record.height);
    validate_range(&records, start, end)?;

    for record in records {
        match store.block_hash(record.height)? {
            Some(stored_hash) if stored_hash == record.header_hash => {}
            Some(_) | None => {
                store.delete_blocks_from(record.height)?;
                cursor = cursor.min(record.height);
                store.set_cursor(cursor)?;
                break;
            }
        }
    }
    Ok(cursor)
}

async fn fetch_block_activity(rpc: &ChiaRpc, record: BlockRecord) -> Result<BlockActivity> {
    let day = match record.timestamp {
        Some(timestamp) => Some(
            DateTime::<Utc>::from_timestamp(timestamp, 0)
                .context("block record contained an invalid timestamp")?
                .date_naive(),
        ),
        None => None,
    };

    let (additions, removals) = if day.is_some() {
        let counts = rpc.additions_and_removals(&record.header_hash).await?;
        (counts.additions, counts.removals)
    } else {
        (0, 0)
    };

    Ok(BlockActivity {
        height: record.height,
        header_hash: record.header_hash,
        day,
        additions,
        removals,
    })
}

fn validate_range(records: &[BlockRecord], start: u32, end: u32) -> Result<()> {
    let expected_count = usize::try_from(end - start).context("block range exceeds usize")?;
    if records.len() != expected_count {
        bail!(
            "get_block_records returned {} records for expected range {start}..{end}",
            records.len()
        );
    }
    for (offset, record) in records.iter().enumerate() {
        let expected = start
            .checked_add(u32::try_from(offset).context("record offset exceeds u32")?)
            .context("block height overflow")?;
        if record.height != expected {
            bail!(
                "get_block_records was not contiguous: expected height {expected}, got {}",
                record.height
            );
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{BlockRecord, validate_range};

    #[test]
    fn validates_contiguous_block_records() {
        let records = vec![
            BlockRecord {
                height: 10,
                header_hash: "0x10".to_owned(),
                timestamp: None,
            },
            BlockRecord {
                height: 11,
                header_hash: "0x11".to_owned(),
                timestamp: None,
            },
        ];
        assert!(validate_range(&records, 10, 12).is_ok());
    }

    #[test]
    fn rejects_missing_block_records() {
        let records = vec![BlockRecord {
            height: 10,
            header_hash: "0x10".to_owned(),
            timestamp: None,
        }];
        assert!(validate_range(&records, 10, 12).is_err());
    }
}
