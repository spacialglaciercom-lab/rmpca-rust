use anyhow::{Context, Result};
use serde::Deserialize;

/// Configuration loaded purely from environment variables.
/// No Clap derive — avoids parser conflicts with main CLI.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Extract jail address
    #[serde(default = "default_extract_host")]
    pub rmpca_extract_host: String,

    /// Backend jail address
    #[serde(default = "default_backend_host")]
    pub rmpca_backend_host: String,

    /// Optimizer nginx address
    #[serde(default = "default_optimizer_host")]
    pub rmpca_optimizer_host: String,

    /// Optimizer port
    #[serde(default = "default_optimizer_port")]
    pub rmpca_optimizer_port: u16,

    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub rmpca_timeout_secs: u64,

    /// R2 account ID
    #[serde(default)]
    pub rmpca_r2_account_id: String,

    /// R2 bucket name
    #[serde(default)]
    pub rmpca_r2_bucket: String,

    /// R2 access key ID (sensitive — prefer env var)
    #[serde(default)]
    pub rmpca_r2_access_key_id: String,

    /// R2 secret access key (sensitive — prefer env var)
    #[serde(default)]
    pub rmpca_r2_secret_access_key: String,

    /// Offline mode - disable all network calls
    #[serde(default)]
    pub rmpca_offline: bool,

    /// Path to offline map file (.osm.pbf)
    #[serde(default)]
    pub rmpca_offline_map: String,
}

fn default_extract_host() -> String { "10.10.0.2".into() }
fn default_backend_host() -> String { "10.10.0.3".into() }
fn default_optimizer_host() -> String { "10.10.0.7".into() }
fn default_optimizer_port() -> u16 { 8000 }
fn default_timeout() -> u64 { 120 }

impl Config {
    /// Load configuration from environment variables using envy.
    pub fn from_env() -> Result<Self> {
        envy::from_env::<Config>()
            .context("Failed to load configuration from environment")
    }

    /// Optimizer URL
    pub fn optimizer_url(&self) -> String {
        format!("http://{}:{}", self.rmpca_optimizer_host, self.rmpca_optimizer_port)
    }

    /// Backend URL
    pub fn backend_url(&self) -> String {
        format!("http://{}:3000", self.rmpca_backend_host)
    }

    /// Extract URL
    pub fn extract_url(&self) -> String {
        format!("http://{}:4000", self.rmpca_extract_host)
    }

    /// Timeout in seconds
    pub fn timeout_secs(&self) -> u64 {
        self.rmpca_timeout_secs
    }

    /// Whether R2 is configured (all required fields present)
    pub fn is_r2_configured(&self) -> bool {
        !self.rmpca_r2_account_id.is_empty()
            && !self.rmpca_r2_bucket.is_empty()
            && !self.rmpca_r2_access_key_id.is_empty()
            && !self.rmpca_r2_secret_access_key.is_empty()
    }

    /// R2 S3 API endpoint
    pub fn r2_s3_endpoint(&self) -> String {
        format!("{}.r2.cloudflarestorage.com", self.rmpca_r2_account_id)
    }

    /// Check if offline mode is enabled
    pub fn is_offline(&self) -> bool {
        self.rmpca_offline
    }

    /// Get offline map path if configured
    pub fn offline_map_path(&self) -> Option<std::path::PathBuf> {
        if self.rmpca_offline_map.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(&self.rmpca_offline_map))
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rmpca_extract_host: default_extract_host(),
            rmpca_backend_host: default_backend_host(),
            rmpca_optimizer_host: default_optimizer_host(),
            rmpca_optimizer_port: default_optimizer_port(),
            rmpca_timeout_secs: default_timeout(),
            rmpca_r2_account_id: String::new(),
            rmpca_r2_bucket: String::new(),
            rmpca_r2_access_key_id: String::new(),
            rmpca_r2_secret_access_key: String::new(),
            rmpca_offline: false,
            rmpca_offline_map: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.rmpca_extract_host, "10.10.0.2");
        assert_eq!(config.rmpca_backend_host, "10.10.0.3");
        assert_eq!(config.rmpca_optimizer_host, "10.10.0.7");
        assert_eq!(config.rmpca_optimizer_port, 8000);
    }

    #[test]
    fn test_url_construction() {
        let config = Config::default();
        assert_eq!(config.optimizer_url(), "http://10.10.0.7:8000");
        assert_eq!(config.backend_url(), "http://10.10.0.3:3000");
        assert_eq!(config.extract_url(), "http://10.10.0.2:4000");
    }
}
