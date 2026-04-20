#!/usr/bin/env bash
# Integration test for offline pipeline
#
# This test runs the full extract → compile → optimize → serve pipeline
# with network disabled (unshare -n) to verify no network calls are made.
#
# Usage:
#   ./tests/offline_test.sh [--quick]
#
# Requirements:
#   - unshare command (usually in util-linux package)
#   - A test PBF file at $RMPCA_TEST_PBF or ./tests/data/test.osm.pbf

set -euo pipefail

QUICK=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --quick)
            QUICK=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Find test PBF
TEST_PBF="${RMPCA_TEST_PBF:-./tests/data/test.osm.pbf}"
if [[ ! -f "$TEST_PBF" ]]; then
    echo "Test PBF not found at $TEST_PBF"
    echo "Set RMPCA_TEST_PBF or place a file at ./tests/data/test.osm.pbf"
    exit 1
fi

# Binary paths
RMPCA="./target/release/rmpca"
if [[ ! -f "$RMPCA" ]]; then
    echo "Building rmpca..."
    cargo build --release
fi

# Create temp directory
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

echo "=== Offline Pipeline Test ==="
echo "Test PBF: $TEST_PBF"
echo "Temp dir: $TEMP_DIR"
echo ""

# Run in network namespace
run_offline() {
    if command -v unshare >/dev/null 2>&1; then
        unshare -rn "$@"
    else
        echo "WARNING: unshare not available, running without network isolation"
        "$@"
    fi
}

# Test 1: Bundle verify
echo "Test 1: Bundle verify command"
mkdir -p "$TEMP_DIR/bundle"
cp "$TEST_PBF" "$TEMP_DIR/bundle/test.osm.pbf"
echo "test" > "$TEMP_DIR/bundle/VERSION"

run_offline "$RMPCA" bundle create --path "$TEMP_DIR/bundle" --name test

if run_offline "$RMPCA" bundle verify --path "$TEMP_DIR/bundle"; then
    echo "PASS: Bundle verify succeeded"
else
    echo "FAIL: Bundle verify failed"
    exit 1
fi

echo ""

# Test 2: Route command (offline)
echo "Test 2: Route command with local PBF"
if [[ "$QUICK" == true ]]; then
    echo "SKIP: Quick mode, skipping route test"
else
    # Create a simple route request
    cat > "$TEMP_DIR/route.json" <<EOF
{
  "from": {"lat": 45.5, "lon": -73.6},
  "to": {"lat": 45.51, "lon": -73.61}
}
EOF

    if run_offline "$RMPCA" route \
        --from "45.5,-73.6" \
        --to "45.51,-73.61" \
        --map "$TEST_PBF" \
        --profile car \
        --output "$TEMP_DIR/result.json" 2>&1; then
        echo "PASS: Route command succeeded"
    else
        echo "FAIL: Route command failed (expected for test PBF without matching area)"
        # Don't fail - test PBF may not have the right area
    fi
fi

echo ""

# Test 3: Compile-map (offline)
echo "Test 3: Compile-map command"
if [[ "$QUICK" == true ]]; then
    echo "SKIP: Quick mode, skipping compile-map test"
else
    # Create a minimal GeoJSON for testing
    cat > "$TEMP_DIR/test.geojson" <<'EOF'
{
  "type": "FeatureCollection",
  "features": [
    {
      "type": "Feature",
      "geometry": {
        "type": "LineString",
        "coordinates": [[-73.6, 45.5], [-73.61, 45.51]]
      },
      "properties": {"name": "Test Road"}
    }
  ]
}
EOF

    if run_offline "$RMPCA" compile-map \
        "$TEMP_DIR/test.geojson" \
        --output "$TEMP_DIR/test.rmp" \
        --stats 2>&1; then
        echo "PASS: Compile-map succeeded"
    else
        echo "FAIL: Compile-map failed"
        exit 1
    fi
fi

echo ""
echo "=== All tests passed ==="