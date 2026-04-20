use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct BundleArgs {
    #[command(subcommand)]
    command: BundleCommands,
}

#[derive(Debug, Subcommand)]
pub enum BundleCommands {
    /// Verify bundle manifest and check all files exist with correct checksums
    Verify {
        /// Path to bundle directory or manifest.json
        #[arg(short, long)]
        path: PathBuf,

        /// Show verbose output for each file
        #[arg(short, long)]
        verbose: bool,
    },
    /// Create a manifest.json for a bundle directory
    Create {
        /// Path to bundle directory
        #[arg(short, long)]
        path: PathBuf,

        /// Bundle name/region identifier
        #[arg(long)]
        name: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BundleManifest {
    pub name: String,
    pub version: String,
    pub created: String,
    pub files: HashMap<String, FileEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub sha256: String,
    pub size: u64,
}

pub async fn run(args: BundleArgs) -> Result<()> {
    match args.command {
        BundleCommands::Verify { path, verbose } => verify_bundle(&path, verbose).await,
        BundleCommands::Create { path, name } => create_manifest(&path, &name).await,
    }
}

async fn verify_bundle(path: &PathBuf, verbose: bool) -> Result<()> {
    let manifest_path = if path.is_dir() {
        path.join("manifest.json")
    } else {
        path.clone()
    };

    let file = File::open(&manifest_path)
        .with_context(|| format!("Cannot open manifest: {}", manifest_path.display()))?;
    let manifest: BundleManifest = serde_json::from_reader(file)
        .with_context(|| "Failed to parse manifest.json")?;

    let bundle_dir = manifest_path.parent().unwrap_or(path);

    println!("Verifying bundle: {} v{}", manifest.name, manifest.version);
    println!("Created: {}", manifest.created);
    println!("Files: {}", manifest.files.len());
    println!();

    let mut errors = 0;
    let mut missing = 0;
    let mut checksum_errors = 0;

    for (rel_path, entry) in &manifest.files {
        let file_path = bundle_dir.join(rel_path);

        if !file_path.exists() {
            missing += 1;
            println!("MISSING: {}", rel_path);
            errors += 1;
            continue;
        }

        // Verify checksum
        let actual_sha = compute_sha256(&file_path)?;
        if actual_sha != entry.sha256 {
            checksum_errors += 1;
            println!("CHECKSUM MISMATCH: {} (expected {}, got {})", 
                     rel_path, entry.sha256, actual_sha);
            errors += 1;
            continue;
        }

        // Verify size
        let metadata = std::fs::metadata(&file_path)?;
        if metadata.len() != entry.size {
            println!("SIZE MISMATCH: {} (expected {}, got {})",
                     rel_path, entry.size, metadata.len());
            errors += 1;
            continue;
        }

        if verbose {
            println!("OK: {} ({} bytes)", rel_path, entry.size);
        }
    }

    println!();
    if errors == 0 {
        println!("Bundle verification PASSED: {} files checked", manifest.files.len());
        Ok(())
    } else {
        println!("Bundle verification FAILED: {} errors ({} missing, {} checksum)", 
                 errors, missing, checksum_errors);
        anyhow::bail!("Bundle verification failed with {} errors", errors);
    }
}

async fn create_manifest(path: &PathBuf, name: &str) -> Result<()> {
    if !path.is_dir() {
        anyhow::bail!("Path must be a directory: {}", path.display());
    }

    let mut files = HashMap::new();
    let mut total_size = 0u64;

    // Walk the directory and hash all files
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let file_path = entry.path();
        let rel_path = file_path.strip_prefix(path)?
            .to_string_lossy()
            .to_string();

        // Skip manifest.json itself if it exists
        if rel_path == "manifest.json" {
            continue;
        }

        let sha256 = compute_sha256(file_path)?;
        let metadata = std::fs::metadata(file_path)?;
        let size = metadata.len();
        total_size += size;

        files.insert(rel_path, FileEntry { sha256, size });
    }

    let manifest = BundleManifest {
        name: name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        created: chrono::Utc::now().to_rfc3339(),
        files,
    };

    let manifest_path = path.join("manifest.json");
    let file = File::create(&manifest_path)?;
    serde_json::to_writer_pretty(file, &manifest)?;

    println!("Created manifest: {}", manifest_path.display());
    println!("Files: {}", manifest.files.len());
    println!("Total size: {} bytes", total_size);

    Ok(())
}

fn compute_sha256(path: &std::path::Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    
    Ok(format!("{:x}", hasher.finalize()))
}