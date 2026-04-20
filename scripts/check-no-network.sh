#!/usr/bin/env bash
# Check that no network-dependent hostnames are embedded in the offline build
#
# This script greps the built Rust binary and GNOME Maps JS bundle for
# forbidden hostnames that would indicate network calls in offline mode.
#
# Usage:
#   ./scripts/check-no-network.sh [--release]
#
# Exit codes:
#   0 - No forbidden hostnames found
#   1 - Forbidden hostnames detected

set -euo pipefail

RELEASE=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --release)
            RELEASE=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Forbidden hostnames - these should never appear in an offline build
FORBIDDEN_HOSTNAMES=(
    "graphhopper.com"
    "api.transitous.org"
    "tile.openstreetmap.org"
    "tileserver.gnome.org"
    "wikipedia.org"
    "wikidata.org"
    "cloudflarestorage.com"
    "r2.cloudflarestorage.com"
    "api.openstreetmap.org"
    "openstreetmap.org"
    "photon.komoot.io"
    "graphhopper.com/api"
    "api.mapbox.com"
    "protomaps.com"
)

# Determine binary path
if [[ "$RELEASE" == true ]]; then
    BINARY="./target/release/rmpca"
else
    BINARY="./target/debug/rmpca"
fi

# GNOME Maps JS bundle
MAPS_BUNDLE="./gnome-maps-main/build_v2/src/org.gnome.Maps.src.gresource"

ERRORS=0

echo "=== Checking for forbidden hostnames ==="
echo "Binary: $BINARY"
echo "Maps bundle: $MAPS_BUNDLE"
echo ""

# Check Rust binary
if [[ -f "$BINARY" ]]; then
    echo "Checking Rust binary..."
    for hostname in "${FORBIDDEN_HOSTNAMES[@]}"; do
        if strings "$BINARY" | grep -qF "$hostname"; then
            echo "ERROR: Found '$hostname' in Rust binary"
            ERRORS=$((ERRORS + 1))
        fi
    done
else
    echo "WARNING: Binary not found at $BINARY"
fi

# Check GNOME Maps JS bundle
if [[ -f "$MAPS_BUNDLE" ]]; then
    echo "Checking GNOME Maps bundle..."
    for hostname in "${FORBIDDEN_HOSTNAMES[@]}"; do
        if strings "$MAPS_BUNDLE" | grep -qF "$hostname"; then
            echo "ERROR: Found '$hostname' in GNOME Maps bundle"
            ERRORS=$((ERRORS + 1))
        fi
    done
else
    echo "WARNING: Maps bundle not found at $MAPS_BUNDLE"
fi

echo ""
if [[ $ERRORS -eq 0 ]]; then
    echo "PASS: No forbidden hostnames found"
    exit 0
else
    echo "FAIL: Found $ERRORS forbidden hostname(s)"
    echo ""
    echo "These hostnames indicate network calls that would fail in offline mode."
    echo "Ensure the code is properly feature-gated behind offline mode checks."
    exit 1
fi