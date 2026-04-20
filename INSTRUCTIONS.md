# rmpca CLI Usage Guide

`rmpca` is a high-performance, offline-first routing and optimization engine designed for RouteMasterPro. It transforms complex geographic workflows into rapid, cacheable operations.

## Core Workflow

The typical workflow for preparing and optimizing routes involves four distinct phases:

### 1. Data Extraction (Offline)
Extract specific road data from Overture Maps or OpenStreetMap (OSM) sources.

*   **Overture Data (local file):**
    ```bash
    rmpca extract-overture --input source.pmtiles --output roads.geojson --bbox -74.0,40.7,-73.9,40.8
    ```
*   **Overture Data (R2 remote — auto-detected):**
    ```bash
    # If the input file doesn't exist locally, rmpca resolves it from R2 automatically.
    rmpca extract-overture --input tiles/montreal-v2026-02.pmtiles --output roads.geojson --bbox -73.62,45.49,-73.55,45.53
    ```
*   **OSM Data:**
    ```bash
    rmpca extract-osm --bbox -74.0,40.7,-73.9,40.8 --output osm.geojson
    ```

### 2. Validation & Cleaning
Ensure input data is geometrically sound and free of topological errors before optimization.

*   **Validate:**
    ```bash
    rmpca validate roads.geojson
    ```
*   **Clean/Repair:**
    ```bash
    rmpca clean roads.geojson --output cleaned.geojson
    ```

### 3. Compilation (The Performance Key)
Compile raw GeoJSON files into high-speed binary graph caches (`.rmp`). This step enables subsequent optimizations to run in milliseconds.

```bash
rmpca compile-map cleaned.geojson --output city.rmp
```

### 4. Optimization
Run the optimization engine using the compiled cache.

```bash
rmpca optimize --cache city.rmp input.geojson --output optimized.gpx
```

---

## Unified Pipeline
For convenience, you can execute the entire sequence (Extract → Clean → Optimize) in a single command:

```bash
rmpca pipeline --input tiles/montreal-v2026-02.pmtiles --bbox -73.62,45.49,-73.55,45.53 --output final.gpx
```

---

## Development & Maintenance

### System Status
Monitor the status of local jail services or backends:
```bash
rmpca status --json
```

### Algorithmic Verification
The engine includes property-based tests to ensure mathematical correctness of the routing algorithms.

```bash
# Run property tests (tests invariants across random input sets)
cargo test --release --tests property_tests
```

---

## Configuration

`rmpca` supports layered configuration. Settings are prioritized as follows (highest to lowest):
1. **CLI Flags**
2. **Environment Variables** (`RMPCA_*`)
3. **Configuration File** (`~/.config/RouteMaster.toml`)

### Example `RouteMaster.toml`
```toml
[optimization.profiles.truck]
turn_left_penalty = 3.0
turn_u_penalty = 8.0

[caching]
cache_dir = "/var/db/rmpca/cache"
```

### Cloudflare R2 Configuration (Online-Only)

> **Note:** R2 remote fetch requires network access. For offline deployments, skip this section and use local files.

Set these environment variables to enable remote PMTiles extraction from R2. When `--input` doesn't exist locally, `extract-overture` automatically resolves it as an R2 object key using `/vsis3/`.

| Variable | Description |
|---|---|
| `RMPCA_R2_ACCOUNT_ID` | Cloudflare account ID |
| `RMPCA_R2_BUCKET` | R2 bucket name (e.g., `routemasterpro`) |
| `RMPCA_R2_ACCESS_KEY_ID` | R2 API token access key ID |
| `RMPCA_R2_SECRET_ACCESS_KEY` | R2 API token secret access key |

Credentials can also be stored in a `.env` file in the project root (ensure it's not committed to version control).

**Available R2 keys** (bucket `routemasterpro`):
- `tiles/<city>-v2026-02.pmtiles` — PMTiles extracts for ~100 North American cities
- `overture/montreal-roads-only.geojson` — pre-extracted Montreal road network

---

## Offline Mode

For air-gapped deployments, rmpca can operate entirely without network access.

### Enabling Offline Mode

```bash
export RMPCA_OFFLINE=1
export RMPCA_OFFLINE_MAP=/path/to/region.osm.pbf
```

### Offline Bundle Management

Create and verify offline bundles:

```bash
# Create a bundle manifest
rmpca bundle create --path ./bundle-dir --name montreal

# Verify bundle integrity
rmpca bundle verify --path ./bundle-dir -v
```

### Features in Offline Mode

| Feature | Status |
|---------|--------|
| Coverage routing | Available |
| Point-to-point routing | Available |
| Map tiles | Available (with PMTiles) |
| Wikipedia thumbnails | Disabled |
| OSM editing | Disabled |
| Public transit | Disabled |

See `docs/offline-mode.md` and `docs/offline-bundles.md` for detailed instructions.

## Need Help?
For specific command options, use the `--help` flag:
```bash
rmpca <command> --help
```
