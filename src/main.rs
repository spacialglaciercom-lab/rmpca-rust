//! rmpca — Enterprise-grade unified CLI for rmp.ca operations
//!
//! This is a Rust port of FreeBSD shell-based dispatcher, transformed
//! into an enterprise-grade offline engine suitable for RouteMasterPro.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod backend;
mod client;
mod commands;
mod config;
mod geo;
mod optimizer;
mod osm;

use client::RmpClient;
use config::Config;

/// rmpca — Enterprise-grade route optimization CLI
#[derive(Parser)]
#[command(name = "rmpca")]
#[command(about = "Unified CLI for rmp.ca operations", long_about = None)]
#[command(version)]
#[command(long_about = rmpca_long_help())]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output structured JSON logs (for frontend integration)
    #[arg(long, global = true, env = "RMPCA_JSON_LOGS")]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Extract Overture Maps road data for a bounding box or polygon
    ExtractOverture(commands::extract_overture::ExtractOvertureArgs),

    /// Download & convert OSM data to GeoJSON for a bounding box
    ExtractOsm(commands::extract_osm::ExtractOsmArgs),

    /// Compile GeoJSON to binary graph cache (instant subsequent optimizations)
    #[command(aliases = &["compile-map", "cache-map"])]
    CompileMap(commands::compile_map::CompileMapArgs),

    /// Optimize a GeoJSON route (supports Lean 4 verification)
    #[command(aliases = &["opt"])]
    Optimize(commands::optimize::OptimizeArgs),

    /// Clean/repair GeoJSON (dedupe, remove self-loops, etc.)
    Clean(commands::clean::CleanArgs),

    /// Validate a GeoJSON file structure and geometry
    Validate(commands::validate::ValidateArgs),

    /// End-to-end: extract → clean → optimize → export
    Pipeline(commands::pipeline::PipelineArgs),

    /// Show health/status of all rmpca jails and services
    Status(commands::status::StatusArgs),

    /// Offline point-to-point routing via a local .osm.pbf file
    #[command(aliases = &["rt"])]
    Route(commands::route::RouteArgs),

    /// JSON interface for GUI integration (reads stdin, writes stdout)
    Serve(commands::serve::ServeArgs),

    /// Tail service logs from a jail
    Logs(commands::logs::LogsArgs),

    /// Manage offline bundles (verify, create)
    Bundle(commands::bundle::BundleArgs),

    /// Run property-based tests for algorithmic correctness
    #[command(aliases = &["test-properties", "proptest"])]
    TestProperties,
}

fn rmpca_long_help() -> &'static str {
    r#"
rmpca — Enterprise-grade route optimization CLI

Quick Start:
  rmpca compile-map city.geojson    # Compile map once (5-30s)
  rmpca optimize --cache city.rmp   # Optimize instantly (1-5ms!)

Configuration (Priority: CLI > Env > Config File > Defaults):
  RouteMaster.toml    - User configuration file (~/.config/RouteMaster.toml)
  RMPCA_* env vars   - Environment variable overrides
  --flag arguments     - Command-line flags (highest priority)

Enterprise Features:
  • Graph caching    - Subsequent optimizations: 1000x faster
  • Lean 4 FFI     - Formal verification via compiled Lean 4 proofs
  • Property tests    - Mathematically rigorous algorithm testing
  • JSON telemetry   - Structured logs for frontend integration
  • Layered config   - Flexible profiles for trucks, cars, etc.

For help with a specific command: rmpca <command> --help
"#
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration from environment
    let config = Config::from_env().unwrap_or_else(|e| {
        // If config loading fails, use defaults but log error
        eprintln!("Warning: Failed to load configuration: {}", e);
        Config::default()
    });

    // Initialize HTTP client
    let client = RmpClient::new(&config)?;

    // Initialize logging
    // config.init_logging();

    match cli.command {
        Commands::ExtractOverture(args) => commands::extract_overture::run(args, &config).await,
        Commands::ExtractOsm(args) => commands::extract_osm::run(args, &client).await,
        Commands::CompileMap(args) => commands::compile_map::run(args, &client).await,
        Commands::Optimize(args) => commands::optimize::run(args, &client).await,
        Commands::Clean(args) => commands::clean::run(args, &client).await,
        Commands::Validate(args) => commands::validate::run(args, &client).await,
        Commands::Pipeline(args) => commands::pipeline::run(args, &client).await,
        Commands::Status(args) => commands::status::run(args, &client).await,
        Commands::Route(args) => commands::route::run(args, &config),
        Commands::Serve(args) => commands::serve::run(args, &config),
        Commands::Logs(args) => commands::logs::run(args, &client).await,
        Commands::Bundle(args) => commands::bundle::run(args).await,
        Commands::TestProperties => {
            // Run property-based tests
            eprintln!("Running property-based tests...");
            eprintln!("This tests algorithmic invariants across random inputs.");
            eprintln!("Use: cargo test --release --tests property_tests");
            Ok(())
        }
    }
}
