use anyhow::{Context, Result};
use tracing::info;

use crate::{config::Config, publisher::GitPublisher, store::Store};

pub(super) fn materialize_archive(
    config: &Config,
    store: &Store,
    publisher: Option<&GitPublisher>,
    today: chrono::NaiveDate,
    caught_up: bool,
) -> Result<()> {
    if caught_up && store.bootstrap_pending()? {
        store.set_bootstrap_pending(false)?;
        info!("historical activity backfill is complete; materializing the archive");
    }

    let archive = if store.bootstrap_pending()? {
        info!("historical backfill is in progress; archive publishing is deferred");
        Default::default()
    } else {
        store.materialize_closed_archive(&config.archive_path, today)?
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
    Ok(())
}

pub(super) fn update_activity_completion(
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
