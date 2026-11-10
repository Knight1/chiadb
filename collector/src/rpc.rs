use std::{fs, time::Duration};

use anyhow::{Context, Result, bail};
use reqwest::{Certificate, Client, Identity};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::warn;

use crate::config::Config;

#[derive(Clone)]
pub struct ChiaRpc {
    client: Client,
    base_url: String,
}

#[derive(Clone, Debug)]
pub struct NodeState {
    pub synced: bool,
    pub peak_height: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct BlockRecord {
    pub height: u32,
    pub header_hash: String,
    pub timestamp: Option<i64>,
}

#[derive(Clone, Copy, Debug)]
pub struct AdditionsAndRemovals {
    pub additions: u32,
    pub removals: u32,
}

pub fn validate_block_range(records: &[BlockRecord], start: u32, end: u32) -> Result<()> {
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

impl ChiaRpc {
    pub fn new(config: &Config) -> Result<Self> {
        let mut identity_pem = fs::read(&config.rpc_client_cert).with_context(|| {
            format!(
                "read Chia RPC client certificate {}",
                config.rpc_client_cert.display()
            )
        })?;
        let key_pem = fs::read(&config.rpc_client_key).with_context(|| {
            format!(
                "read Chia RPC client key {}",
                config.rpc_client_key.display()
            )
        })?;
        identity_pem.extend_from_slice(&key_pem);
        let identity = Identity::from_pem(&identity_pem)
            .context("parse Chia RPC client certificate and key")?;

        let mut builder = Client::builder()
            .use_rustls_tls()
            .identity(identity)
            .timeout(Duration::from_secs(60));

        if let Some(ca_path) = &config.rpc_ca_cert {
            let ca_pem = fs::read(ca_path)
                .with_context(|| format!("read Chia RPC CA certificate {}", ca_path.display()))?;
            let ca = Certificate::from_pem(&ca_pem).context("parse Chia RPC CA certificate")?;
            builder = builder.add_root_certificate(ca);
        }

        if config.rpc_insecure_skip_verify {
            warn!("CHIA_RPC_INSECURE_SKIP_VERIFY is enabled; use it only on a private network");
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder.build().context("build Chia RPC HTTP client")?;
        Ok(Self {
            client,
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

    async fn post_value(&self, route: &str, body: Value) -> Result<Value> {
        let url = format!("{}/{}", self.base_url, route);
        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;
        let status = response.status();
        let value: Value = response
            .json()
            .await
            .with_context(|| format!("decode JSON response from {url}"))?;

        if !status.is_success() {
            bail!(
                "Chia RPC {route} returned HTTP {status}: {}",
                rpc_error(&value)
            );
        }
        if value.get("success").and_then(Value::as_bool) == Some(false) {
            bail!("Chia RPC {route} failed: {}", rpc_error(&value));
        }
        Ok(value)
    }
}

fn rpc_error(value: &Value) -> String {
    value
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("unknown Chia RPC error")
        .to_owned()
}

#[derive(Debug, Deserialize)]
struct BlockchainStateResponse {
    blockchain_state: BlockchainStatePayload,
}

#[derive(Debug, Deserialize)]
struct BlockchainStatePayload {
    #[serde(default)]
    sync: SyncState,
    peak: Option<Peak>,
}

#[derive(Debug, Default, Deserialize)]
struct SyncState {
    #[serde(default)]
    synced: bool,
}

#[derive(Debug, Deserialize)]
struct Peak {
    height: u32,
}

#[derive(Debug, Deserialize)]
struct BlockRecordsResponse {
    #[serde(default)]
    block_records: Vec<BlockRecordPayload>,
}

#[derive(Debug, Deserialize)]
struct ConnectionsResponse {
    #[serde(default)]
    connections: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct BlockRecordPayload {
    height: u32,
    header_hash: String,
    timestamp: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AdditionsAndRemovalsResponse {
    #[serde(default)]
    additions: Vec<Value>,
    #[serde(default)]
    removals: Vec<Value>,
}
