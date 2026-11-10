use serde::Deserialize;
use serde_json::Value;

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

#[derive(Debug, Deserialize)]
pub(super) struct BlockchainStateResponse {
    pub(super) blockchain_state: BlockchainStatePayload,
}

#[derive(Debug, Deserialize)]
pub(super) struct BlockchainStatePayload {
    #[serde(default)]
    pub(super) sync: SyncState,
    pub(super) peak: Option<Peak>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SyncState {
    #[serde(default)]
    pub(super) synced: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct Peak {
    pub(super) height: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct BlockRecordsResponse {
    #[serde(default)]
    pub(super) block_records: Vec<BlockRecordPayload>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ConnectionsResponse {
    #[serde(default)]
    pub(super) connections: Vec<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct BlockRecordPayload {
    pub(super) height: u32,
    pub(super) header_hash: String,
    pub(super) timestamp: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AdditionsAndRemovalsResponse {
    #[serde(default)]
    pub(super) additions: Vec<Value>,
    #[serde(default)]
    pub(super) removals: Vec<Value>,
}
