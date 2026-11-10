mod activity;
mod archive;
mod reorg;
mod snapshot;

use anyhow::Result;
use chrono::Utc;
use tracing::info;

use crate::{config::Config, publisher::GitPublisher, rpc::ChiaRpc, store::Store};
use activity::{collect_blocks, initialize_cursor};
use archive::{materialize_archive, update_activity_completion};
use reorg::reconcile_recent_chain;
use snapshot::capture_database_snapshot;

pub(crate) async fn collect_once(
    config: &Config,
    rpc: Option<&ChiaRpc>,
    store: &Store,
    publisher: Option<&GitPublisher>,
) -> Result<()> {
    let now = Utc::now();
    let today = now.date_naive();
    let mut snapshot = capture_database_snapshot(config, store, now)?;

    // Do not ask the node for activity until its SQLite database exists. This
    // makes the database file the first durable observation and avoids
    // creating an activity cursor before the node has initialized its store.
    if snapshot.is_none() {
        materialize_archive(config, store, publisher, today, false)?;
        return Ok(());
    }

    let Some(rpc) = rpc else {
        materialize_archive(config, store, publisher, today, false)?;
        return Ok(());
    };

    let node = match rpc.blockchain_state().await {
        Ok(node) => node,
        Err(error) => {
            materialize_archive(config, store, publisher, today, false)?;
            return Err(error);
        }
    };

    if let (Some(snapshot), Some(peak_height)) = (snapshot.as_mut(), node.peak_height) {
        snapshot.peak_height = peak_height;
        store.upsert_snapshot(snapshot)?;
    }

    let Some(peak_height) = node.peak_height else {
        info!("full node has no peak yet; activity collection is waiting");
        materialize_archive(config, store, publisher, today, false)?;
        return Ok(());
    };

    let initial_network_contact = match initial_network_contact_observed(rpc, store).await {
        Ok(observed) => observed,
        Err(error) => {
            materialize_archive(config, store, publisher, today, false)?;
            return Err(error);
        }
    };
    if !initial_network_contact {
        info!("full node has not contacted a full-node peer; activity collection is waiting");
        materialize_archive(config, store, publisher, today, false)?;
        return Ok(());
    }

    let mut cursor = initialize_cursor(config, store, &node, peak_height)?;

    cursor = reconcile_recent_chain(config, rpc, store, peak_height, cursor).await?;
    let (next_cursor, processed) = collect_blocks(config, rpc, store, peak_height, cursor).await?;
    cursor = next_cursor;

    let cursor_caught_up = cursor > peak_height;
    // A partially synced node can temporarily catch up to its local tip. It
    // must not mark historical days final or publish the archive until the
    // node itself has reached the network tip.
    let fully_caught_up = node.synced && cursor_caught_up;
    update_activity_completion(store, today, fully_caught_up)?;
    materialize_archive(config, store, publisher, today, fully_caught_up)?;

    let (db_bytes, wal_bytes) = snapshot
        .as_ref()
        .map(|snapshot| (snapshot.db_bytes, snapshot.wal_bytes))
        .unwrap_or((0, 0));
    info!(
        peak_height,
        cursor,
        processed,
        cursor_caught_up,
        node_synced = node.synced,
        fully_caught_up,
        size_sampled = snapshot.is_some(),
        db_bytes,
        wal_bytes,
        "collection cycle complete"
    );
    Ok(())
}

async fn initial_network_contact_observed(rpc: &ChiaRpc, store: &Store) -> Result<bool> {
    if store.initial_network_contact_observed()? {
        return Ok(true);
    }

    let peer_count = rpc.full_node_peer_count().await?;
    if peer_count == 0 {
        return Ok(false);
    }

    store.set_initial_network_contact_observed(true)?;
    info!(
        peer_count,
        "observed initial full-node peer contact; activity collection can begin"
    );
    Ok(true)
}
