use crate::config::Config;
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Args)]
pub struct ExtractOvertureArgs {
    /// Bounding box: WEST,SOUTH,EAST,NORTH
    #[arg(long)]
    bbox: Option<String>,

    /// GeoJSON polygon file to use as boundary
    #[arg(long)]
    polygon: Option<PathBuf>,

    /// Input Overture PMTiles or source file (local path or R2 key)
    #[arg(short, long)]
    input: PathBuf,

    /// Output file
    #[arg(short, long)]
    output: PathBuf,

    /// Filter by road class (e.g., residential, secondary)
    #[arg(long)]
    road_class: Option<Vec<String>>,

    /// Suppress progress output
    #[arg(short, long)]
    quiet: bool,
}

pub async fn run(args: ExtractOvertureArgs, config: &Config) -> Result<()> {
    let input_path = resolve_input(&args.input, config)?;

    if !args.quiet {
        if args.input.exists() {
            eprintln!("Extracting Overture road data from local file...");
        } else {
            #[cfg(feature = "r2")]
            eprintln!("Extracting Overture road data from R2 (bucket: {})...", config.rmpca_r2_bucket);
            #[cfg(not(feature = "r2"))]
            eprintln!("Extracting Overture road data...");
        }
    }

    // Build ogr2ogr command
    let mut cmd = Command::new("ogr2ogr");

    // Set basic options
    cmd.arg("-f").arg("GeoJSON");
    cmd.arg("-t_srs").arg("EPSG:4326"); // Reproject to WGS84 (PMTiles are Pseudo-Mercator)
    cmd.arg(&args.output);
    cmd.arg(&input_path);

    // Apply spatial filter
    if let Some(bbox) = &args.bbox {
        let parts: Vec<&str> = bbox.split(',').collect();
        anyhow::ensure!(parts.len() == 4, "bbox must be WEST,SOUTH,EAST,NORTH");
        cmd.arg("-spat")
            .arg(parts[0]).arg(parts[1])
            .arg(parts[2]).arg(parts[3])
            .arg("-spat_srs").arg("EPSG:4326");
    }

    // Apply SQL filter if road classes are provided
    if let Some(classes) = &args.road_class {
        let filter = classes.iter()
            .map(|c| format!("class = '{}'", c))
            .collect::<Vec<_>>()
            .join(" OR ");
        cmd.arg("-where").arg(filter);
    }

    // Set AWS env vars for R2 S3 access when using /vsis3/ (only if r2 feature enabled)
    #[cfg(feature = "r2")]
    if !args.input.exists() && config.is_r2_configured() {
        cmd.env("AWS_ACCESS_KEY_ID", &config.rmpca_r2_access_key_id);
        cmd.env("AWS_SECRET_ACCESS_KEY", &config.rmpca_r2_secret_access_key);
        cmd.env("AWS_S3_ENDPOINT", config.r2_s3_endpoint());
        cmd.env("AWS_REGION", "auto");
        cmd.env("AWS_HTTPS", "YES");
    }

    let status = cmd.status().context("Failed to execute ogr2ogr")?;
    anyhow::ensure!(status.success(), "ogr2ogr failed with status {}", status);

    if !args.quiet {
        eprintln!("Extraction complete: {:?}", args.output);
    }

    Ok(())
}

/// Resolve input: if the file exists locally, use it directly.
/// Otherwise, if R2 is configured and the r2 feature is enabled, construct a /vsis3/ path.
fn resolve_input(input: &PathBuf, config: &Config) -> Result<String> {
    if input.exists() {
        return Ok(input.to_string_lossy().to_string());
    }

    #[cfg(feature = "r2")]
    if config.is_r2_configured() {
        // Treat the input path as an R2 object key
        let key = input.to_string_lossy();
        return Ok(format!("/vsis3/{}/{}", config.rmpca_r2_bucket, key));
    }

    // Offline mode: require local file
    anyhow::bail!(
        "Input file '{}' not found locally.\n\
         For offline deployments, place the .pmtiles file at the specified path.\n\
         See docs/offline-bundles.md for bundle preparation instructions.\n\
         \n\
         If you intended to fetch from R2, rebuild with --features r2",
        input.display()
    );
}
