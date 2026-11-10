use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;

#[derive(Clone, Debug, Parser)]
#[command(name = "chia-db-collector", version, about)]
pub struct Config {
    #[arg(long, env = "CHIA_RPC_URL", default_value = "https://chia:8555")]
    pub rpc_url: String,

    #[arg(
        long,
        env = "CHIA_RPC_CLIENT_CERT",
        default_value = "/chia/mainnet/config/ssl/full_node/private_full_node.crt"
    )]
    pub rpc_client_cert: PathBuf,

    #[arg(
        long,
        env = "CHIA_RPC_CLIENT_KEY",
        default_value = "/chia/mainnet/config/ssl/full_node/private_full_node.key"
    )]
    pub rpc_client_key: PathBuf,

    #[arg(long, env = "CHIA_RPC_CA_CERT")]
    pub rpc_ca_cert: Option<PathBuf>,

    #[arg(long, env = "CHIA_RPC_INSECURE_SKIP_VERIFY", default_value_t = false)]
    pub rpc_insecure_skip_verify: bool,

    #[arg(long, env = "CHIA_RPC_TIMEOUT_SECONDS", default_value_t = 300)]
    pub rpc_timeout_seconds: u64,

    #[arg(
        long,
        env = "CHIA_DB_PATH",
        default_value = "/chia/mainnet/db/blockchain_v2_mainnet.sqlite"
    )]
    pub db_path: PathBuf,

    #[arg(
        long,
        env = "COLLECTOR_DB_PATH",
        default_value = "/state/collector.sqlite"
    )]
    pub state_db_path: PathBuf,

    #[arg(
        long,
        env = "ARCHIVE_PATH",
        default_value = "/output/archive/metrics.ndjson"
    )]
    pub archive_path: PathBuf,

    #[arg(long, env = "POLL_SECONDS", default_value_t = 60)]
    pub poll_seconds: u64,

    #[arg(long, env = "MAX_BLOCKS_PER_POLL", default_value_t = 256)]
    pub max_blocks_per_poll: u32,

    #[arg(long, env = "RPC_CONCURRENCY", default_value_t = 4)]
    pub rpc_concurrency: usize,

    #[arg(long, env = "REORG_CHECK_DEPTH", default_value_t = 200)]
    pub reorg_check_depth: u32,

    #[arg(long, env = "BACKFILL_START_HEIGHT")]
    pub backfill_start_height: Option<u32>,

    #[arg(long, env = "ONCE", default_value_t = false)]
    pub once: bool,

    #[arg(long, env = "GIT_PUBLISH", default_value_t = false)]
    pub git_publish: bool,

    #[arg(long, env = "GIT_WORKTREE", default_value = "/output")]
    pub git_worktree: PathBuf,

    #[arg(long, env = "GIT_REMOTE", default_value = "origin")]
    pub git_remote: String,

    #[arg(long, env = "GIT_BRANCH", default_value = "main")]
    pub git_branch: String,

    #[arg(long, env = "GIT_AUTHOR_NAME", default_value = "chia-db-collector")]
    pub git_author_name: String,

    #[arg(
        long,
        env = "GIT_AUTHOR_EMAIL",
        default_value = "chia-db-collector@localhost"
    )]
    pub git_author_email: String,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if !self.rpc_url.starts_with("https://") {
            bail!("CHIA_RPC_URL must use https://");
        }
        if !self.rpc_insecure_skip_verify && self.rpc_ca_cert.is_none() {
            bail!("set CHIA_RPC_CA_CERT or explicitly set CHIA_RPC_INSECURE_SKIP_VERIFY=true");
        }
        if self.poll_seconds == 0 {
            bail!("POLL_SECONDS must be greater than zero");
        }
        if self.rpc_timeout_seconds == 0 {
            bail!("CHIA_RPC_TIMEOUT_SECONDS must be greater than zero");
        }
        if self.max_blocks_per_poll == 0 {
            bail!("MAX_BLOCKS_PER_POLL must be greater than zero");
        }
        if self.rpc_concurrency == 0 {
            bail!("RPC_CONCURRENCY must be greater than zero");
        }
        if self.git_publish
            && (self.git_remote.trim().is_empty()
                || self.git_branch.trim().is_empty()
                || self.git_author_name.trim().is_empty()
                || self.git_author_email.trim().is_empty())
        {
            bail!("Git publishing requires remote, branch, author name, and author email");
        }
        Ok(())
    }
}
