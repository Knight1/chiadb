mod transport;
mod types;
mod validation;

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;

use crate::config::Config;
pub use types::{AdditionsAndRemovals, BlockRecord, NodeState};
use types::{
    AdditionsAndRemovalsResponse, BlockRecordsResponse, BlockchainStateResponse,
    ConnectionsResponse,
};
pub use validation::validate_block_range;

#[derive(Clone)]
pub struct ChiaRpc {
    client: Client,
    base_url: String,
}

impl ChiaRpc {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            client: transport::build_client(config)?,
            base_url: config.rpc_url.trim_end_matches('/').to_owned(),
        })
    }

    pub async fn blockchain_state(&self) -> Result<NodeState> {
        let value = self.post_value("get_blockchain_state", json!({})).await?;
        let response: BlockchainStateResponse =
            serde_json::from_value(value).context("decode get_blockchain_state response")?;
        Ok(NodeState {
            synced: response.blockchain_state.sync.synced,
            peak_height: response.blockchain_state.peak.map(|peak| peak.height),
        })
    }

    pub async fn full_node_peer_count(&self) -> Result<usize> {
        let value = self
            .post_value("get_connections", json!({ "node_type": 1 }))
            .await?;
        let response: ConnectionsResponse =
            serde_json::from_value(value).context("decode get_connections response")?;
        Ok(response.connections.len())
    }

    pub async fn block_records(&self, start: u32, end: u32) -> Result<Vec<BlockRecord>> {
        let value = self
            .post_value("get_block_records", json!({ "start": start, "end": end }))
            .await?;
        let response: BlockRecordsResponse =
            serde_json::from_value(value).context("decode get_block_records response")?;
        Ok(response
            .block_records
            .into_iter()
            .map(|record| BlockRecord {
                height: record.height,
                header_hash: record.header_hash,
                timestamp: record.timestamp,
            })
            .collect())
    }

    pub async fn additions_and_removals(&self, header_hash: &str) -> Result<AdditionsAndRemovals> {
        let value = self
            .post_value(
                "get_additions_and_removals",
                json!({ "header_hash": header_hash }),
            )
            .await?;
        let response: AdditionsAndRemovalsResponse =
            serde_json::from_value(value).context("decode get_additions_and_removals response")?;
        let additions =
            u32::try_from(response.additions.len()).context("additions count exceeded u32")?;
        let removals =
            u32::try_from(response.removals.len()).context("removals count exceeded u32")?;
        Ok(AdditionsAndRemovals {
            additions,
            removals,
        })
    }
}
