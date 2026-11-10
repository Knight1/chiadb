use anyhow::Result;

use crate::{
    config::Config,
    rpc::{BlockRecord, ChiaRpc, validate_block_range},
    store::Store,
};

pub(super) async fn reconcile_recent_chain(
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
    validate_block_range(&records, start, end)?;

    if let Some(height) = first_changed_height(store, &records)? {
        store.delete_blocks_from(height)?;
        cursor = cursor.min(height);
        store.set_cursor(cursor)?;
    }
    Ok(cursor)
}

fn first_changed_height(store: &Store, records: &[BlockRecord]) -> Result<Option<u32>> {
    for record in records {
        // Requests complete out of order. A timeout can leave gaps in a batch;
        // those heights are retried from the saved cursor, not treated as a reorg.
        if let Some(stored_hash) = store.block_hash(record.height)?
            && stored_hash != record.header_hash
        {
            return Ok(Some(record.height));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "../../tests/unit/reorg.rs"]
mod tests;
