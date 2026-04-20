//! HTTP backend for communicating with remote jail services.

use crate::config::Config;
use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use super::Backend;

/// HTTP backend that communicates with remote jail services.
pub struct HttpBackend {
    client: Client,
    config: Config,
}

impl HttpBackend {
    pub fn new(config: &Config) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs()))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            config: config.clone(),
        }
    }
}

impl Backend for HttpBackend {
    fn extract_osm(&self, bbox: &[f64], highway: Option<&[String]>) -> Result<Value> {
        // Synchronous wrapper for async operation
        let url = format!("{}/api/extract/osm", self.config.extract_url());
        
        let mut payload = serde_json::json!({ "bbox": bbox });
        if let Some(hw) = highway {
            payload["highway"] = serde_json::json!(hw);
        }

        // Use tokio runtime for sync context
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let resp = self.client.post(&url).json(&payload).send().await
                .context("HTTP POST failed")?;
            let status = resp.status();
            if !status.is_success() {
                let error_body = resp.text().await.unwrap_or_default();
                anyhow::bail!("HTTP {} from {}: {}", status, url, error_body);
            }
            resp.json().await.context("Failed to parse response JSON")
        })
    }

    fn optimize(&self, geojson: &Value, profile: &str) -> Result<Value> {
        let url = format!("{}/api/optimize", self.config.optimizer_url());
        
        let payload = serde_json::json!({
            "geojson": geojson,
            "profile": profile
        });

        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let resp = self.client.post(&url).json(&payload).send().await
                .context("HTTP POST failed")?;
            let status = resp.status();
            if !status.is_success() {
                let error_body = resp.text().await.unwrap_or_default();
                anyhow::bail!("HTTP {} from {}: {}", status, url, error_body);
            }
            resp.json().await.context("Failed to parse response JSON")
        })
    }

    fn is_online(&self) -> bool {
        true
    }
}