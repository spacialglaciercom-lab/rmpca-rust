# Offline Mode

This document describes how to use rmpca and GNOME Maps in fully offline mode.

## Enabling Offline Mode

### Environment Variables

```bash
export RMPCA_OFFLINE=1
export RMPCA_OFFLINE_MAP=/path/to/region.osm.pbf
```

### GSettings (GNOME Maps)

```bash
gsettings set org.gnome.Maps offline-mode true
gsettings set org.gnome.Maps tile-bundle-path /path/to/region.pmtiles
gsettings set org.gnome.Maps cpp-offline-map-file /path/to/region.osm.pbf
gsettings set org.gnome.Maps local-tile-server-port 8080
```

## Features Available in Offline Mode

| Feature | Available | Notes |
|---------|-----------|-------|
| Coverage routing (CPP) | Yes | Uses local .osm.pbf |
| Point-to-point routing | Yes | Uses rmpca route |
| Map tiles | Yes | Requires PMTiles bundle |
| Place search | Limited | No remote geocoding |
| Wikipedia thumbnails | No | Disabled |
| OSM editing | No | Requires OAuth |
| Public transit | No | Requires Transitous API |

## Starting the Local Tile Server

If using PMTiles for offline tiles, start a local tile server:

```bash
# Install pmtiles CLI
go install github.com/protomaps/go-pmtiles/pmtiles@latest

# Serve tiles
pmtiles serve /path/to/region.pmtiles --port 8080 &

# Configure GNOME Maps
gsettings set org.gnome.Maps tile-bundle-path /path/to/region.pmtiles
```

## Troubleshooting

### "No PBF file configured for offline extraction"

Set the `RMPCA_OFFLINE_MAP` environment variable or `cpp-offline-map-file` GSetting.

### Tiles not loading

1. Verify the tile server is running: `curl http://127.0.0.1:8080/0/0/0.pbf`
2. Check GSettings: `gsettings get org.gnome.Maps tile-bundle-path`
3. Ensure offline mode is enabled: `gsettings get org.gnome.Maps offline-mode`

### Routing fails

1. Verify the OSM PBF covers your area of interest
2. Check the PBF is valid: `osmpbfreader --info /path/to/region.osm.pbf`
3. Try extracting a smaller area first

### Build fails with "no matching package"

Run `cargo vendor vendor/` to update vendored dependencies, then rebuild with:
```bash
cargo build --offline --release
```

## See Also

- [Offline Bundles](offline-bundles.md) - Creating and verifying bundles
- [README.md](../README.md) - Build instructions