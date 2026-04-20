//! Backend abstraction for rmpca operations.
//!
//! Provides two implementations:
//! - `HttpBackend`: Communicates with remote jail services (online mode)
//! - `InProcessBackend`: Runs operations in-process (offline mode)

use anyhow::Result;
use serde_json::Value;

pub mod http;
pub mod in_process;

pub use http::HttpBackend;
pub use in_process::InProcessBackend;

/// Trait for backend operations.
/// Implementations can use HTTP (remote jails) or in-process (offline).
pub trait Backend {
    /// Extract OSM data for a bounding box
    fn extract_osm(&self, bbox: &[f64], highway: Option<&[String]>) -> Result<Value>;
    
    /// Optimize a route
    fn optimize(&self, geojson: &Value, profile: &str) -> Result<Value>;
    
    /// Check if backend is online (HTTP) or offline (in-process)
    fn is_online(&self) -> bool;
}

/// Create appropriate backend based on offline mode.
pub fn create_backend(offline: bool, config: &crate::config::Config) -> Box<dyn Backend + Send + Sync> {
    if offline {
        Box::new(InProcessBackend::new())
    } else {
        Box::new(HttpBackend::new(config))
    }
}

/// Create offline backend with a specific PBF file.
pub fn create_offline_backend(pbf_path: std::path::PathBuf) -> Box<dyn Backend + Send + Sync> {
    Box::new(InProcessBackend::new().with_pbf(pbf_path))
}