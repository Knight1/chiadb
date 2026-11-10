use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use futures_util::{StreamExt, TryStreamExt, stream};
use tracing::{info, warn};

use crate::{
    config::Config,
    model::BlockActivity,
    rpc::{BlockRecord, ChiaRpc, NodeState, validate_block_range},
    store::Store,
};

pub(super) fn initialize_cursor(
    config: &Config,
    store: &Store,
    node: &NodeState,
    peak_height: u32,
) -> Result<u32> {
    let stored_cursor = store.cursor()?;
    let start_during_initial_sync = !node.synced;
    let cursor = stored_cursor.unwrap_or_else(|| {
        config.backfill_start_height.unwrap_or_else(|| {
            if start_during_initial_sync {
                0
            } else {
                peak_height.saturating_add(1)
            }
        })
    });
    if stored_cursor.is_none() {
        store.set_cursor(cursor)?;
        store.set_bootstrap_pending(
            config.backfill_start_height.is_some() || start_during_initial_sync,
        )?;
        store.set_activity_start_complete(cursor == 0)?;
        if config.backfill_start_height.is_some() {
            info!(cursor, "initialized historical activity backfill");
        } else if start_during_initial_sync {
            info!(
                cursor,
                "initialized activity collection from block 0 during initial sync"
            );
        } else {
            info!(cursor, "initialized live-only collection at current peak");
        }
    }
    Ok(cursor)
}

pub(super) async fn collect_blocks(
    config: &Config,
    rpc: &ChiaRpc,
    store: &Store,
    peak_height: u32,
    mut cursor: u32,
) -> Result<(u32, usize)> {
    let mut processed = 0usize;

    if cursor <= peak_height {
        let end = cursor
            .saturating_add(config.max_blocks_per_poll)
            .min(peak_height.saturating_add(1));
        let mut records = rpc.block_records(cursor, end).await?;
        records.sort_by_key(|record| record.height);
        validate_block_range(&records, cursor, end)?;

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
    Ok((cursor, processed))
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
