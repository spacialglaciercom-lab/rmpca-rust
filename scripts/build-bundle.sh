#!/usr/bin/env bash
# Build an offline bundle for rmpca
#
# Usage:
#   ./scripts/build-bundle.sh --name montreal --bbox -73.98,-73.35,45.35,45.7 --output ./bundles/
#
# This script must be run on an online machine with network access.
# It downloads OSM data, extracts roads, compiles the graph, and packages everything.

set -euo pipefail

# Parse arguments
NAME=""
BBOX=""
OUTPUT="./bundles"
PMTILES_URL=""
SKIP_TILES=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --name)
            NAME="$2"
            shift 2
            ;;
        --bbox)
            BBOX="$2"
            shift 2
            ;;
        --output)
            OUTPUT="$2"
            shift 2
            ;;
        --pmtiles-url)
            PMTILES_URL="$2"
            shift 2
            ;;
        --skip-tiles)
            SKIP_TILES=true
            shift
            ;;
        --help)
            echo "Usage: $0 --name <region> --bbox <west,south,east,north> [options]"
            echo ""
            echo "Options:"
            echo "  --name <name>         Region name for the bundle"
            echo "  --bbox <coords>       Bounding box: west,south,east,north"
            echo "  --output <dir>        Output directory (default: ./bundles/)"
            echo "  --pmtiles-url <url>   URL to download PMTiles from"
            echo "  --skip-tiles          Skip PMTiles download"
            echo "  --help                Show this help"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

if [[ -z "$NAME" ]]; then
    echo "Error: --name is required"
    exit 1
fi

if [[ -z "$BBOX" ]]; then
    echo "Error: --bbox is required"
    exit 1
fi

# Create bundle directory
DATE=$(date +%Y-%m-%d)
BUNDLE_DIR="$OUTPUT/rmpca-bundle-$NAME-$DATE"
mkdir -p "$BUNDLE_DIR"/{tiles,osm,graphs,geocoder}

echo "=== Building bundle: $NAME ==="
echo "Bundle directory: $BUNDLE_DIR"
echo "Bounding box: $BBOX"

# Check for required tools
command -v rmpca >/dev/null 2>&1 || { echo "Error: rmpca not found in PATH"; exit 1; }
command -v wget >/dev/null 2>&1 || command -v curl >/dev/null 2>&1 || { echo "Error: wget or curl required"; exit 1; }

# 1. Download OSM PBF (if not already present)
OSM_FILE="$BUNDLE_DIR/osm/$NAME.osm.pbf"
if [[ ! -f "$OSM_FILE" ]]; then
    echo ""
    echo "=== Downloading OSM data ==="
    # Try Geofabrik first (smaller extracts)
    GEOFABRIK_BASE="https://download.geofabrik.de"
    
    # For now, require user to provide the PBF or use extract-osm
    echo "Note: For large regions, download OSM PBF manually from:"
    echo "  - Geofabrik: $GEOFABRIK_BASE"
    echo "  - OSM Planet: https://planet.openstreetmap.org/"
    echo ""
    echo "Place the .osm.pbf file at: $OSM_FILE"
    echo "Or use: rmpca extract-osm --bbox $BBOX --output $OSM_FILE"
fi

# 2. Extract roads for the bounding box
echo ""
echo "=== Extracting road network ==="
ROADS_FILE="$BUNDLE_DIR/roads.geojson"
rmpca extract-osm --bbox "$BBOX" --output "$ROADS_FILE" || {
    echo "Warning: extract-osm failed, will use full PBF for routing"
}

# 3. Compile graph cache
if [[ -f "$ROADS_FILE" ]]; then
    echo ""
    echo "=== Compiling graph cache ==="
    rmpca compile-map "$ROADS_FILE" --output "$BUNDLE_DIR/graphs/$NAME.rmp" --stats
fi

# 4. Download PMTiles (if URL provided)
if [[ "$SKIP_TILES" == false ]]; then
    if [[ -n "$PMTILES_URL" ]]; then
        echo ""
        echo "=== Downloading vector tiles ==="
        if command -v wget >/dev/null 2>&1; then
            wget -O "$BUNDLE_DIR/tiles/$NAME.pmtiles" "$PMTILES_URL"
        else
            curl -L -o "$BUNDLE_DIR/tiles/$NAME.pmtiles" "$PMTILES_URL"
        fi
    else
        echo ""
        echo "=== Skipping vector tiles ==="
        echo "To include tiles, use --pmtiles-url or download manually to:"
        echo "  $BUNDLE_DIR/tiles/$NAME.pmtiles"
        echo ""
        echo "Sources:"
        echo "  - Protomaps: https://protomaps.com/downloads"
        echo "  - Generate with: planetiler --area=$NAME --output=$NAME.pmtiles"
    fi
fi

# 5. Create VERSION file
echo "$NAME-$DATE" > "$BUNDLE_DIR/VERSION"

# 6. Create manifest
echo ""
echo "=== Creating bundle manifest ==="
rmpca bundle create --path "$BUNDLE_DIR" --name "$NAME"

# 7. Package the bundle
echo ""
echo "=== Packaging bundle ==="
ARCHIVE="$OUTPUT/rmpca-bundle-$NAME-$DATE.tar.zst"
tar -I zstd -cf "$ARCHIVE" -C "$OUTPUT" "rmpca-bundle-$NAME-$DATE"

echo ""
echo "=== Bundle complete ==="
echo "Bundle: $ARCHIVE"
echo "Size: $(du -h "$ARCHIVE" | cut -f1)"
echo ""
echo "To verify:"
echo "  rmpca bundle verify --path $BUNDLE_DIR"
echo ""
echo "To install on offline machine:"
echo "  tar -I zstd -xf $ARCHIVE -C /usr/local/share/rmpca/"
echo "  export RMPCA_OFFLINE=1"
echo "  export RMPCA_OFFLINE_MAP=/usr/local/share/rmpca/rmpca-bundle-$NAME-$DATE/osm/$NAME.osm.pbf"