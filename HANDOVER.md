# Handover Document: rmpca-rust + GNOME Maps Fusion

This document outlines the steps required to finalize the system-wide installation of the "Area Coverage" routing integration. These steps require **root privileges** and assume access to a writable `/usr/local`.

## 1. Current State Summary

- **rmpca-rust Engine**: 
  - Source: `/home/drone/Documents/rmpca-rust`
  - Binary: `/home/drone/rmpca-target/release/rmpca` (Built with custom target dir due to permissions)
  - Status: Fully functional with new `serve --json` and `serve --gpx` commands.

- **GNOME Maps Frontend**:
  - Source: `/home/drone/Documents/rmpca-rust/gnome-maps-main`
  - Build Dir: `build_v2`
  - Status: Built and integrated. 
  - **Key Fixes Applied**:
    - Lowered `shumate-1.0` requirement to `1.4.1` in `meson.build`.
    - Fixed `blueprint` syntax errors in `data/ui/cpp-view.blp` (styles block format).
    - Installed `blueprint-compiler` via `pip3 --user` at `/home/drone/.local/bin/blueprint-compiler`.

## 2. Root Execution Plan (System-Wide Install)

If `/usr/local` is writable (e.g. on the FreeBSD host or a thick jail), run the following as root:

### A. Install rmpca-rust
```bash
# Copy binary to system path
cp /home/drone/rmpca-target/release/rmpca /usr/local/bin/rmpca
chmod +x /usr/local/bin/rmpca
```

### B. Install GNOME Maps
```bash
# Set path to find the locally installed blueprint-compiler
export PATH=/home/drone/.local/bin:$PATH

# Install from the build directory
cd /home/drone/Documents/rmpca-rust/gnome-maps-main/build_v2
ninja install
```

### C. Finalize GSettings
```bash
# Compile schemas system-wide
glib-compile-schemas /usr/local/share/glib-2.0/schemas

# Set system-wide defaults (optional, or per-user)
gsettings set org.gnome.Maps rmpca-path "/usr/local/bin/rmpca"
```

### D. Desktop Shortcut
```bash
# Install the desktop entry
cp /home/drone/Documents/rmpca-rust/gnome-maps-rmpca.desktop /usr/local/share/applications/
```

## 3. Environment Overrides (If staying in a Read-Only Jail)

If the jail's `/usr/local` remains read-only, continue using the provided launcher:
`./launch-gnome-maps.sh`

This script handles:
- `GI_TYPELIB_PATH`: Points to local `GnomeMaps-1.0.typelib`.
- `GSETTINGS_SCHEMA_DIR`: Points to local `gschemas.compiled`.
- `GJS_PATH`: Points to local JS source.
- `GSETTINGS_BACKEND=memory`: Allows setting config values without a writable D-Bus/dconf backend.

## 4. Verification Steps

1. Launch: `gnome-maps` (or use the shortcut).
2. Open Sidebar -> Routing.
3. Select "Coverage" mode (Edit-Select-All icon).
4. Verify "Optimize Route" runs using the `rmpca` binary.
5. Verify "Export GPX" produces a valid file.

## 5. Offline Mode Setup

For air-gapped deployments, additional setup is required:

### A. Install Offline Dependencies

```bash
# Install pmtiles CLI for local tile serving
go install github.com/protomaps/go-pmtiles/pmtiles@latest
```

### B. Prepare Offline Bundle

```bash
# Create bundle directory
mkdir -p /usr/local/share/rmpca/bundles

# Copy or create bundle (see docs/offline-bundles.md)
# Bundle should contain:
#   - tiles/region.pmtiles
#   - osm/region.osm.pbf
#   - graphs/region.rmp (optional)
#   - manifest.json
```

### C. Configure Offline Mode

```bash
# Enable offline mode
gsettings set org.gnome.Maps offline-mode true

# Set paths to offline data
gsettings set org.gnome.Maps tile-bundle-path /usr/local/share/rmpca/bundles/montreal/tiles/montreal.pmtiles
gsettings set org.gnome.Maps cpp-offline-map-file /usr/local/share/rmpca/bundles/montreal/osm/montreal.osm.pbf
gsettings set org.gnome.Maps local-tile-server-port 8080

# Set environment for rmpca
export RMPCA_OFFLINE=1
export RMPCA_OFFLINE_MAP=/usr/local/share/rmpca/bundles/montreal/osm/montreal.osm.pbf
```

### D. Start Local Tile Server

```bash
# Create systemd service or run manually
pmtiles serve /usr/local/share/rmpca/bundles/montreal/tiles/montreal.pmtiles --port 8080 &
```

### E. Verify Offline Operation

```bash
# Run offline verification
./scripts/check-no-network.sh --release

# Test bundle integrity
rmpca bundle verify --path /usr/local/share/rmpca/bundles/montreal
```

## 6. Offline Mode Features

| Feature | Status | Notes |
|---------|--------|-------|
| Coverage routing | Available | Uses local .osm.pbf |
| Point-to-point routing | Available | Uses rmpca route |
| Map tiles | Available | Requires PMTiles + local server |
| Wikipedia thumbnails | Disabled | No network access |
| OSM editing | Disabled | Requires OAuth |
| Public transit | Disabled | Requires Transitous API |

---
**Prepared by**: Gemini CLI
**Date**: Friday, April 17, 2026
**Updated for Offline Mode**: April 2025
