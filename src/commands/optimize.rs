use crate::client::RmpClient;
use anyhow::{Context, Result};
use clap::Args;
use geojson::FeatureCollection;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct OptimizeArgs {
    /// Input GeoJSON file (FeatureCollection)
    pub input: PathBuf,

    /// Output file (default: stdout)
    pub output: Option<PathBuf>,

    /// Export as GPX instead of GeoJSON
    pub gpx: bool,

    /// Clean/repair GeoJSON before optimizing
    pub clean: bool,

    /// Use local Rust optimizer instead of remote jail
    pub local: bool,

    /// Optimizer host (overrides RMPCA_OPTIMIZER_HOST)
    pub host: Option<String>,

    /// Optimizer port (overrides default)
    pub port: Option<u16>,

    /// Left turn penalty 0-10
    pub turn_left: f64,

    /// Right turn penalty 0-10
    pub turn_right: f64,

    /// U-turn penalty 0-10
    pub turn_u: f64,

    /// Starting depot location as LAT,LON
    pub depot: Option<String>,

    /// Print route statistics to stderr
    pub stats: bool,

    /// Suppress progress output
    pub quiet: bool,
}

pub async fn run(args: OptimizeArgs, client: &RmpClient) -> Result<()> {
    let raw = std::fs::read_to_string(&args.input)
        .context("Failed to read input file")?;
    run_with_raw(args, &raw, client).await
}

pub async fn run_with_raw(args: OptimizeArgs, raw: &str, client: &RmpClient) -> Result<()> {
    let fc: FeatureCollection = raw.parse()
        .context("Input is not a valid GeoJSON FeatureCollection")?;

    if args.local {
        return run_local(args, fc).await;
    }

    // Build optimization request payload
    let mut payload = serde_json::json!({
        "geojson": serde_json::to_value(&fc)?,
        "clean_before_optimize": args.clean,
    });

    if args.turn_left > 0.0 || args.turn_right > 0.0 || args.turn_u > 0.0 {
        payload["turn_penalties"] = serde_json::json!({
            "left_turn": args.turn_left,
            "right_turn": args.turn_right,
            "u_turn": args.turn_u,
        });
    }

    if let Some(depot) = &args.depot {
        let coords: Vec<&str> = depot.split(',').collect();
        anyhow::ensure!(coords.len() == 2, "Depot must be LAT,LON");
        payload["depot"] = serde_json::json!({
            "lat": coords[0].parse::<f64>().context("Invalid depot latitude")?,
            "lon": coords[1].parse::<f64>().context("Invalid depot longitude")?,
        });
    }

    // Resolve optimizer URL
    let base = match (&args.host, args.port) {
        (Some(h), Some(p)) => format!("http://{}:{}", h, p),
        (Some(h), None)    => format!("http://{}:{}", h, client.config.rmpca_optimizer_port),
        (None, Some(p))    => format!("http://{}:{}", client.config.rmpca_optimizer_host, p),
        (None, None)       => client.config.optimizer_url(),
    };
    let url = format!("{}/api/optimize/sync", base);

    if !args.quiet {
        eprintln!("Sending to optimizer at {}...", base);
    }

    let result = client.post_json(&url, &payload).await?;

    // Print stats
    if args.stats || !args.quiet {
        if let Some(dist) = result.get("total_distance_km") {
            eprintln!("Total distance: {} km", dist);
        }
        if let Some(stats) = result.get("stats") {
            if let Some(eff) = stats.get("efficiency") {
                eprintln!("Efficiency: {}%", eff);
            }
            if let Some(dh) = stats.get("deadhead_distance_km") {
                eprintln!("Deadhead: {} km", dh);
            }
        }
    }

    // Format and write output
    let output_text = if args.gpx {
        convert_to_gpx(&result)?
    } else {
        serde_json::to_string_pretty(&result)?
    };

    match args.output {
        Some(path) => std::fs::write(&path, &output_text)
            .context("Failed to write output file")?,
        None => println!("{}", output_text),
    }

    Ok(())
}

/// Local optimization using Rust optimizer module (no network).
async fn run_local(args: OptimizeArgs, fc: FeatureCollection) -> Result<()> {
    use crate::optimizer::RouteOptimizer;

    let mut optimizer = RouteOptimizer::new();
    let geojson_value = serde_json::to_value(&fc)?;
    let result = optimizer.optimize(&geojson_value)?;

    let output_text = if args.gpx {
        convert_to_gpx(&serde_json::to_value(&result)?)?
    } else {
        serde_json::to_string_pretty(&result)?
    };

    match args.output {
        Some(path) => std::fs::write(&path, &output_text)
            .context("Failed to write output file")?,
        None => println!("{}", output_text),
    }

    Ok(())
}

fn convert_to_gpx(result: &Value) -> Result<String> {
    let route_coords = result
        .get("route")
        .and_then(|r| r.as_array())
        .ok_or_else(|| anyhow::anyhow!("No route in result"))?;

    let mut gpx = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="rmpca-optimize"
     xmlns="http://www.topografix.com/GPX/1/1">
  <trk>
    <name>Optimized Route</name>
    <trkseg>
"#,
    );

    for coord in route_coords {
        // Handle both {latitude, longitude} and [lon, lat] formats
        let (lat, lon) = if let Some(obj) = coord.as_object() {
            let lat = obj.get("latitude").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let lon = obj.get("longitude").and_then(|v| v.as_f64()).unwrap_or(0.0);
            (lat, lon)
        } else if let Some(arr) = coord.as_array() {
            let lon = arr.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let lat = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
            (lat, lon)
        } else {
            (0.0, 0.0)
        };

        gpx.push_str(&format!(
            "      <trkpt lat=\"{}\" lon=\"{}\" />\n", lat, lon
        ));
    }

    gpx.push_str("    </trkseg>\n  </trk>\n</gpx>\n");
    Ok(gpx)
}
