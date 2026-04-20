//! In-process backend for offline operation.
//!
//! Uses local osmpbfreader and optimizer crates directly,
//! without any network calls.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;

use super::Backend;

/// In-process backend that runs operations locally without network.
pub struct InProcessBackend {
    // Cache for loaded PBF files
    pbf_cache: Option<PathBuf>,
}

impl InProcessBackend {
    pub fn new() -> Self {
        Self { pbf_cache: None }
    }

    /// Set the PBF file to use for extractions
    pub fn with_pbf(mut self, pbf_path: PathBuf) -> Self {
        self.pbf_cache = Some(pbf_path);
        self
    }

    /// Set the PBF file (mutable setter)
    pub fn set_pbf(&mut self, pbf_path: PathBuf) {
        self.pbf_cache = Some(pbf_path);
    }
}

impl Default for InProcessBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for InProcessBackend {
    fn extract_osm(&self, bbox: &[f64], _highway: Option<&[String]>) -> Result<Value> {
        // For offline mode, we need a local PBF file
        let pbf_path = self.pbf_cache.as_ref()
            .context("No PBF file configured for offline extraction. Set RMPCA_OFFLINE_MAP or use --offline-map")?;

        // Convert bbox slice to tuple
        anyhow::ensure!(bbox.len() == 4, "bbox must have 4 values: west, south, east, north");
        let bbox_tuple = (bbox[0], bbox[1], bbox[2], bbox[3]);

        // Use the osm parser directly
        let ways = crate::osm::parse_pbf_in_bbox(pbf_path, bbox_tuple)?;
        
        // Convert ways to GeoJSON (using geo::Way which has geometry.coordinates)
        let features: Vec<Value> = ways.iter().map(|way| {
            serde_json::json!({
                "type": "Feature",
                "geometry": {
                    "type": "LineString",
                    "coordinates": way.geometry.coordinates.iter()
                        .map(|c| [c.lon, c.lat])
                        .collect::<Vec<_>>()
                },
                "properties": way.tags
            })
        }).collect();

        Ok(serde_json::json!({
            "type": "FeatureCollection",
            "features": features
        }))
    }

    fn optimize(&self, geojson: &Value, _profile: &str) -> Result<Value> {
        // Use the optimizer directly
        let mut optimizer = crate::optimizer::RouteOptimizer::new();
        let result = optimizer.optimize(geojson)?;
        
        // Convert result to JSON
        Ok(serde_json::json!({
            "route": result.route.iter().map(|p| [p.longitude, p.latitude]).collect::<Vec<_>>(),
            "total_distance_km": result.total_distance,
            "edge_count": result.edge_count,
            "node_count": result.node_count
        }))
    }

    fn is_online(&self) -> bool {
        false
    }
}