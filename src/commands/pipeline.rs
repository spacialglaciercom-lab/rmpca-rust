use crate::client::RmpClient;
use anyhow::{Context, Result};
use clap::Args;
use geojson::FeatureCollection;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct PipelineArgs {
    /// Bounding box: WEST,SOUTH,EAST,NORTH
    #[arg(long, required_unless_present = "input")]
    bbox: Option<String>,

    /// Pre-extracted GeoJSON input (skips extraction step)
    #[arg(long)]
    input: Option<PathBuf>,

    /// Data source: overture or osm
    #[arg(long, default_value = "overture")]
    source: String,

    /// Output file
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Export as GPX
    #[arg(long)]
    gpx: bool,

    /// Skip clean step
    #[arg(long)]
    no_clean: bool,

    /// Use local Rust optimizer instead of remote jail
    #[arg(long)]
    local: bool,

    /// Starting depot location as LAT,LON
    #[arg(long)]
    depot: Option<String>,

    /// Suppress progress output
    #[arg(short, long)]
    quiet: bool,
}

pub async fn run(args: PipelineArgs, client: &RmpClient) -> Result<()> {
    // Step 1: Extract (unless --input provided)
    let geojson_raw = if let Some(input_path) = &args.input {
        if !args.quiet { eprintln!("[1/3] Using pre-extracted input..."); }
        std::fs::read_to_string(input_path)
            .context("Failed to read input file")?
    } else {
        let bbox = args.bbox.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Either --bbox or --input is required"))?;
        if !args.quiet { eprintln!("[1/3] Extracting road data ({})...", args.source); }

        let extract_url = match args.source.as_str() {
            "overture" => format!("{}/api/extract/overture", client.config.extract_url()),
            "osm" => format!("{}/api/extract/osm", client.config.extract_url()),
            other => anyhow::bail!("Unknown source '{}', expected 'overture' or 'osm'", other),
        };

        let parts: Vec<f64> = bbox.split(',')
            .map(|s| s.parse::<f64>())
            .collect::<std::result::Result<_, _>>()
            .context("Invalid bbox format")?;
        anyhow::ensure!(parts.len() == 4, "bbox requires 4 values");

        let result = client.post_json(&extract_url, &serde_json::json!({ "bbox": parts })).await?;
        serde_json::to_string(&result)?
    };

    // Step 2: Clean (unless --no-clean)
    let cleaned = if args.no_clean {
        if !args.quiet { eprintln!("[2/3] Skipping clean step."); }
        geojson_raw
    } else {
        if !args.quiet { eprintln!("[2/3] Cleaning GeoJSON..."); }
        let fc: FeatureCollection = geojson_raw.parse()
            .context("Failed to parse GeoJSON for cleaning")?;
        let cleaned_fc = super::clean::clean_feature_collection(fc, true, true, None, None);
        serde_json::to_string(&cleaned_fc)?
    };

    // Step 3: Optimize
    if !args.quiet { eprintln!("[3/3] Optimizing route..."); }
    let optimize_args = super::optimize::OptimizeArgs {
        input: PathBuf::new(), // Not used — we pass data directly
        output: args.output,
        gpx: args.gpx,
        clean: false, // Already cleaned
        local: args.local,
        host: None,
        port: None,
        turn_left: 0.0,
        turn_right: 0.0,
        turn_u: 0.0,
        depot: args.depot,
        stats: true,
        quiet: args.quiet,
    };

    super::optimize::run_with_raw(optimize_args, &cleaned, client).await
}
