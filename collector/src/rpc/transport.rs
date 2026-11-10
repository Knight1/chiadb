use std::{fs, time::Duration};

use anyhow::{Context, Result, bail};
use reqwest::{Certificate, Client, Identity};
use serde_json::Value;
use tracing::warn;

use super::ChiaRpc;
use crate::config::Config;

pub(super) fn build_client(config: &Config) -> Result<Client> {
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
    let identity =
        Identity::from_pem(&identity_pem).context("parse Chia RPC client certificate and key")?;

    let mut builder = Client::builder()
        .tls_backend_rustls()
        .identity(identity)
        .timeout(Duration::from_secs(config.rpc_timeout_seconds));

    if let Some(ca_path) = &config.rpc_ca_cert {
        let ca_pem = fs::read(ca_path)
            .with_context(|| format!("read Chia RPC CA certificate {}", ca_path.display()))?;
        let ca = Certificate::from_pem(&ca_pem).context("parse Chia RPC CA certificate")?;
        builder = builder.tls_certs_merge([ca]);
    }

    if config.rpc_insecure_skip_verify {
        warn!("CHIA_RPC_INSECURE_SKIP_VERIFY is enabled; use it only on a private network");
        builder = builder.tls_danger_accept_invalid_certs(true);
    }
    builder.build().context("build Chia RPC HTTP client")
}

impl ChiaRpc {
    pub(super) async fn post_value(&self, route: &str, body: Value) -> Result<Value> {
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
