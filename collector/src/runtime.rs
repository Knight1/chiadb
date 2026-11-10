use std::time::Duration;

use anyhow::Result;
use tracing::{error, info, warn};

use crate::{
    collection::collect_once, config::Config, publisher::GitPublisher, rpc::ChiaRpc, store::Store,
};

pub async fn run(config: Config) -> Result<()> {
    let store = Store::open(&config.state_db_path)?;
    let publisher = config
        .git_publish
        .then(|| GitPublisher::from_config(&config));
    if let Some(publisher) = &publisher {
        publisher.check_available()?;
    }
    let mut rpc = None;

    loop {
        if rpc.is_none() {
            match ChiaRpc::new(&config) {
                Ok(client) => {
                    info!("Chia RPC TLS material is ready");
                    rpc = Some(client);
                }
                Err(error) => {
                    warn!(
                        error = ?error,
                        retry_seconds = config.poll_seconds,
                        "Chia RPC TLS material is not ready; will retry"
                    );
                }
            }
        }

        let result = collect_once(&config, rpc.as_ref(), &store, publisher.as_ref()).await;
        if config.once {
            return result;
        }
        if let Err(error) = result {
            if is_connection_error(&error) {
                info!(
                    error = ?error,
                    retry_seconds = config.poll_seconds,
                    "Chia RPC endpoint is not ready; will retry"
                );
            } else {
                error!(error = ?error, "collection cycle failed; will retry");
            }
        }
        tokio::time::sleep(Duration::from_secs(config.poll_seconds)).await;
    }
}

fn is_connection_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<reqwest::Error>()
            .is_some_and(reqwest::Error::is_connect)
    })
}
