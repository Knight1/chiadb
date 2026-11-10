use anyhow::Result;
use chia_db_collector::{config::Config, runtime};
use clap::Parser;
use tracing_subscriber::EnvFilter;

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
    runtime::run(config).await
}
