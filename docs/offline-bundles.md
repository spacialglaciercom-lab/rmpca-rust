# Offline Bundle Format

This document defines the bundle layout for fully offline rmpca deployments.

## Bundle Layout

```
rmpca-bundle-<region>-<date>/
  manifest.json          # SHA256 checksums of all files
  tiles/<region>.pmtiles # Vector tiles (Mapbox/Protomaps format)
  osm/<region>.osm.pbf   # OSM data for routing
  graphs/<region>.rmp    # Precompiled rmpca graph cache (optional)
  geocoder/<region>.trie # Name index for offline geocoding (optional)
  VERSION               # Bundle version identifier
```

## manifest.json Format

```json
{
  "name": "montreal",
  "version": "0.1.0",
  "created": "2025-01-15T12:00:00Z",
  "files": {
    "tiles/montreal.pmtiles": {
      "sha256": "abc123...",
      "size": 123456789
    },
    "osm/montreal.osm.pbf": {
      "sha256": "def456...",
      "size": 98765432
    }
  }
}
```

## Creating a Bundle

On an online machine:

```bash
# 1. Download OSM PBF for your region
wget https://download.geofabrik.de/north-america/canada/quebec-latest.osm.pbf

# 2. Extract roads for your area of interest
rmpca extract-osm --bbox -73.98,-73.35,45.35,45.7 -o roads.geojson

# 3. Compile the graph cache
rmpca compile-map roads.geojson -o graphs/region.rmp

# 4. Download vector tiles (requires pmtiles CLI)
pmtiles extract https://example.com/region.pmtiles tiles/region.pmtiles

# 5. Create the bundle manifest
rmpca bundle create --path ./rmpca-bundle-montreal-2025-01-15 --name montreal

# 6. Package the bundle
tar -I zstd -cf rmpca-bundle-montreal-2025-01-15.tar.zst rmpca-bundle-montreal-2025-01-15/
```

## Verifying a Bundle

On the offline machine:

```bash
# Verify all files exist with correct checksums
rmpca bundle verify --path /path/to/bundle

# Or verify from manifest directly
rmpca bundle verify --path /path/to/bundle/manifest.json -v
```

## Installing a Bundle

```bash
# Copy bundle to target location
sudo cp rmpca-bundle-montreal-2025-01-15.tar.zst /usr/local/share/rmpca/
cd /usr/local/share/rmpca/
sudo tar -I zstd -xf rmpca-bundle-montreal-2025-01-15.tar.zst

# Configure rmpca to use the bundle
export RMPCA_OFFLINE=1
export RMPCA_OFFLINE_MAP=/usr/local/share/rmpca/rmpca-bundle-montreal-2025-01-15/osm/montreal.osm.pbf

# For GNOME Maps, set GSettings
gsettings set org.gnome.Maps offline-mode true
gsettings set org.gnome.Maps tile-bundle-path /usr/local/share/rmpca/rmpca-bundle-montreal-2025-01-15/tiles/montreal.pmtiles
gsettings set org.gnome.Maps cpp-offline-map-file /usr/local/share/rmpca/rmpca-bundle-montreal-2025-01-15/osm/montreal.osm.pbf
```

## Bundle Sources

### OSM PBF Files
- Geofabrik: https://download.geofabrik.de/
- OSM Planet: https://planet.openstreetmap.org/

### Vector Tiles
- Protomaps: https://protomaps.com/downloads
- Mapbox: https://www.mapbox.com/downloads/
- Generate your own with Planetiler

### Pre-built Bundles
Official bundles are published at: https://releases.routemaster.pro/bundles/

## File Size Guidelines

| Region | OSM PBF | PMTiles | Graph Cache | Total |
|--------|---------|---------|-------------|-------|
| City (100km²) | 10-50 MB | 50-200 MB | 5-20 MB | 65-270 MB |
| Metro (1000km²) | 50-200 MB | 200-500 MB | 20-50 MB | 270-750 MB |
| Province (10000km²) | 200-500 MB | 500MB-2GB | 50-200 MB | 750MB-2.7GB |

## Troubleshooting

### "Input file not found locally"
The bundle is incomplete or the path is wrong. Verify with:
```bash
rmpca bundle verify --path /path/to/bundle -v
```

### Tiles not loading in GNOME Maps
1. Check `tile-bundle-path` GSetting points to the .pmtiles file
2. Start the local tile server: `pmtiles serve /path/to/tiles.pmtiles --port 8080`
3. Or use embedded libpmtiles (requires compilation with libpmtiles)

### Routing fails with "No PBF file configured"
Set `RMPCA_OFFLINE_MAP` or `cpp-offline-map-file` GSetting to the .osm.pbf path.