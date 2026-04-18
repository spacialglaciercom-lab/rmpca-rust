//! `serve` subcommand — machine-readable JSON interface for GUI integration
//!
//! Reads a JSON request from stdin, runs the full optimization pipeline
//! against a local .osm.pbf file, and writes JSON to stdout.
//! Progress messages are written to stderr as JSON lines.

use anyhow::{Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;

use crate::config::Config;
use crate::optimizer::RouteOptimizer;

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Read request from a file instead of stdin
    #[arg(long)]
    input: Option<PathBuf>,

    /// Pretty-print JSON output
    #[arg(long)]
    pretty: bool,

    /// Output GPX instead of JSON (for export)
    #[arg(long)]
    gpx: bool,
}

/// JSON request schema accepted on stdin
#[derive(Debug, Deserialize)]
struct ServeRequest {
    /// Bounding box: [west, south, east, north] in decimal degrees
    #[serde(default)]
    bbox: Option<(f64, f64, f64, f64)>,

    /// Polygon as GeoJSON: { coordinates: [[lon, lat], ...] }
    #[serde(default)]
    polygon: Option<GeoPolygon>,

    /// Path to local .osm.pbf file
    offline_map_file: String,

    /// Vehicle profile: truck, car, delivery
    #[serde(default = "default_profile")]
    profile: String,

    /// Optional depot location as [lat, lon]
    #[serde(default)]
    depot: Option<(f64, f64)>,
}

/// GeoJSON polygon representation
#[derive(Debug, Deserialize)]
struct GeoPolygon {
    /// Array of [longitude, latitude] coordinate pairs
    coordinates: Vec<(f64, f64)>,
}

fn default_profile() -> String {
    "truck".to_string()
}

/// JSON progress event written to stderr
#[derive(Serialize)]
struct ProgressEvent {
    event: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    percent: Option<u8>,
}

/// JSON response written to stdout on success
#[derive(Serialize)]
struct ServeResponse {
    success: bool,
    route: Vec<RoutePointJson>,
    total_distance_km: f64,
    deadhead_distance_km: f64,
    efficiency_percent: f64,
    edge_count: usize,
    node_count: usize,
    profile: String,
}

/// JSON route point for the response
#[derive(Serialize)]
struct RoutePointJson {
    latitude: f64,
    longitude: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    node_id: Option<String>,
}

/// JSON error response
#[derive(Serialize)]
struct ServeError {
    success: bool,
    error: String,
}

pub fn run(args: ServeArgs, _config: &Config) -> Result<()> {
    // 1. Read request
    let request_json = if let Some(path) = &args.input {
        std::fs::read_to_string(path)
            .context("Failed to read request file")?
    } else {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("Failed to read request from stdin")?;
        buf
    };

    let request: ServeRequest = match serde_json::from_str(&request_json) {
        Ok(r) => r,
        Err(e) => {
            let err = ServeError {
                success: false,
                error: format!("Invalid request JSON: {}", e),
            };
            let out = serde_json::to_string(&err).unwrap();
            println!("{}", out);
            std::process::exit(1);
        }
    };

    // 2. Validate .osm.pbf file exists
    let pbf_path = PathBuf::from(&request.offline_map_file);
    if !pbf_path.exists() {
        let err = ServeError {
            success: false,
            error: format!("OSM PBF file not found: {}", request.offline_map_file),
        };
        let out = serde_json::to_string(&err).unwrap();
        println!("{}", out);
        std::process::exit(1);
    }

    let stderr = std::io::stderr();
    let mut stderr_lock = stderr.lock();

    // 3. Parse PBF
    let mut progress = |msg: &str, pct: Option<u8>| -> Result<()> {
        let evt = ProgressEvent {
            event: "progress".to_string(),
            message: msg.to_string(),
            percent: pct,
        };
        writeln!(stderr_lock, "{}", serde_json::to_string(&evt)?)?;
        stderr_lock.flush()?;
        Ok(())
    };

    progress(&format!("Parsing OSM PBF: {}", request.offline_map_file), Some(10))?;

    let ways = if let Some(ref polygon) = request.polygon {
        // Use polygon-based filtering
        match crate::osm::parse_pbf_in_polygon(&pbf_path, &polygon.coordinates) {
            Ok(w) => w,
            Err(e) => {
                let err = ServeError {
                    success: false,
                    error: format!("Failed to parse PBF with polygon: {}", e),
                };
                let out = serde_json::to_string(&err).unwrap();
                println!("{}", out);
                std::process::exit(1);
            }
        }
    } else if let Some(bbox) = request.bbox {
        // Fall back to bbox-based filtering for backward compatibility
        match crate::osm::parse_pbf_in_bbox(&pbf_path, bbox) {
            Ok(w) => w,
            Err(e) => {
                let err = ServeError {
                    success: false,
                    error: format!("Failed to parse PBF: {}", e),
                };
                let out = serde_json::to_string(&err).unwrap();
                println!("{}", out);
                std::process::exit(1);
            }
        }
    } else {
        let err = ServeError {
            success: false,
            error: "Either bbox or polygon must be provided".to_string(),
        };
        let out = serde_json::to_string(&err).unwrap();
        println!("{}", out);
        std::process::exit(1);
    };

    progress(&format!("Extracted {} ways", ways.len()), Some(30))?;

    if ways.is_empty() {
        let err = ServeError {
            success: false,
            error: "No roads found in the selected area. Try a larger bounding box.".to_string(),
        };
        let out = serde_json::to_string(&err).unwrap();
        println!("{}", out);
        std::process::exit(1);
    }

    // 4. Optimize
    progress("Building graph...", Some(50))?;

    let mut optimizer = RouteOptimizer::new();

    progress("Balancing graph...", Some(65))?;

    let result = match optimizer.optimize_with_geo_ways(&ways) {
        Ok(r) => r,
        Err(e) => {
            let err = ServeError {
                success: false,
                error: format!("Optimization failed: {}", e),
            };
            let out = serde_json::to_string(&err).unwrap();
            println!("{}", out);
            std::process::exit(1);
        }
    };

    progress(&format!("Found circuit with {} points", result.route.len()), Some(90))?;

    // 5. Write response
    if args.gpx {
        println!("{}", result.to_gpx());
    } else {
        let response = ServeResponse {
            success: true,
            route: result.route.iter().map(|p| RoutePointJson {
                latitude: p.latitude,
                longitude: p.longitude,
                node_id: p.node_id.clone(),
            }).collect(),
            total_distance_km: result.total_distance,
            deadhead_distance_km: result.deadhead_distance_km,
            efficiency_percent: result.efficiency_percent,
            edge_count: result.edge_count,
            node_count: result.node_count,
            profile: request.profile,
        };

        let stdout = std::io::stdout();
        let mut stdout_lock = stdout.lock();
        if args.pretty {
            serde_json::to_writer_pretty(&mut stdout_lock, &response)?;
        } else {
            serde_json::to_writer(&mut stdout_lock, &response)?;
        }
        writeln!(stdout_lock)?;
    }

    progress("Done", Some(100))?;

    Ok(())
}
