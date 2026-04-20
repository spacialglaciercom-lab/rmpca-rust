# Implementation Plan: Offline Geocoding for RouteMasterPro

## Overview

Add local geocoding (address/place search) to enable 100% offline operation. Currently, GNOME Maps relies on online services (GraphHopper/Photon) for search. This plan adds a `rmpca geocode` command to the Rust backend and `RmpcaGeocode.js` in the frontend.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        GNOME Maps Frontend                          │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  geocode.js (factory)                                        │   │
│  │  ├── GraphHopperGeocode (online)                            │   │
│  │  ├── PhotonGeocode (online)                                 │   │
│  │  └── RmpcaGeocode (offline) ← NEW                          │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              │                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  RmpcaGeocode.js                                             │   │
│  │  ├── search(query, lat, lon) → spawns rmpca geocode        │   │
│  │  ├── JSON request on stdin                                   │   │
│  │  └── JSON response on stdout → Place[] objects              │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        Rust Backend (rmpca)                        │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  Commands::Geocode                                           │   │
│  │  ├── Parse .osm.pbf                                          │   │
│  │  ├── Index named places (name, addr:* tags)                 │   │
│  │  ├── Search by text query (fuzzy matching)                  │   │
│  │  ├── Optional location bias (lat/lon priority)              │   │
│  │  └── Return JSON results compatible with Photon format      │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              │                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  osm::geocode_parser.rs (NEW)                               │   │
│  │  ├── extract_named_places() → Vec<NamedPlace>               │   │
│  │  ├── build_search_index() → SearchIndex                     │   │
│  │  └── search_index() → Vec<SearchResult>                      │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Rust Backend — `rmpca geocode` Command

### 1.1 Data Structures

**File: `src/commands/geocode.rs`**

```rust
/// JSON request schema (stdin)
#[derive(Debug, Deserialize)]
struct GeocodeRequest {
    /// Path to local .osm.pbf file
    offline_map_file: String,
    /// Search query string
    q: String,
    /// Optional location bias [lat, lon]
    #[serde(default)]
    location: Option<(f64, f64)>,
    /// Maximum results to return
    #[serde(default = "default_limit")]
    limit: usize,
}

/// JSON response schema (stdout)
#[derive(Serialize)]
struct GeocodeResponse {
    success: bool,
    features: Vec<GeocodeFeature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Single search result (Photon-compatible format)
#[derive(Serialize)]
struct GeocodeFeature {
    geometry: Geometry,
    properties: Properties,
}

#[derive(Serialize)]
struct Geometry {
    coordinates: [f64; 2], // [lon, lat]
    #[serde(rename = "type")]
    geom_type: String, // "Point"
}

#[derive(Serialize)]
struct Properties {
    name: String,
    osm_id: i64,
    osm_type: String, // "N", "W", "R"
    #[serde(skip_serializing_if = "Option::is_none")]
    city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    street: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    housenumber: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    postcode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    countrycode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    osm_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    osm_value: Option<String>,
}
```

### 1.2 Named Place Extraction

**File: `src/osm/geocode_parser.rs`**

```rust
/// Named place extracted from OSM data
pub struct NamedPlace {
    pub osm_id: i64,
    pub osm_type: OsmType, // Node, Way, Relation
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub tags: HashMap<String, String>,
}

pub enum OsmType {
    Node,
    Way,
    Relation,
}

/// Tags that indicate a named, searchable place
const SEARCHABLE_TAGS: &[&str] = &[
    "name",
    "addr:housenumber",
    "addr:street",
    "addr:city",
    "addr:postcode",
    "amenity",
    "shop",
    "tourism",
    "historic",
    "leisure",
    "office",
    "public_transport",
    "highway", // for named streets
];

/// Extract named places from PBF
pub fn extract_named_places<P: AsRef<Path>>(
    file_path: P,
    bbox: Option<(f64, f64, f64, f64)>,
) -> Result<Vec<NamedPlace>> {
    // Parse PBF, filter by:
    // 1. bbox (if provided)
    // 2. Has any searchable tag
    // 3. Has a name or address
}
```

### 1.3 Search Index

```rust
/// In-memory search index for fast fuzzy matching
pub struct SearchIndex {
    places: Vec<NamedPlace>,
    // Simple approach: store lowercase names for substring search
    // Advanced: use trie or BK-tree for fuzzy matching
    name_index: Vec<(String, usize)>, // (lowercase_name, place_index)
}

impl SearchIndex {
    pub fn build(places: Vec<NamedPlace>) -> Self {
        // Build index from extracted places
    }

    pub fn search(&self, query: &str, location: Option<(f64, f64)>, limit: usize) -> Vec<SearchResult> {
        // 1. Normalize query (lowercase, remove diacritics)
        // 2. Find places with name containing query (substring match)
        // 3. If location provided, sort by distance
        // 4. Return top `limit` results
    }
}
```

### 1.4 Command Implementation

**File: `src/commands/geocode.rs`**

```rust
#[derive(Debug, Args)]
pub struct GeocodeArgs {
    /// Read request from file instead of stdin
    #[arg(long)]
    input: Option<PathBuf>,

    /// Pretty-print JSON output
    #[arg(long)]
    pretty: bool,

    /// Cache the search index to disk for faster subsequent searches
    #[arg(long)]
    cache_index: bool,
}

pub fn run(args: GeocodeArgs, config: &Config) -> Result<()> {
    // 1. Read JSON request from stdin (or file)
    // 2. Parse PBF and extract named places
    // 3. Build search index
    // 4. Execute search
    // 5. Return Photon-compatible JSON response
}
```

### 1.5 Register Command

**File: `src/commands/mod.rs`**

```rust
pub mod geocode;  // Add
pub use geocode::GeocodeArgs;  // Add
```

**File: `src/main.rs`**

```rust
enum Commands {
    // ... existing commands
    /// Offline geocoding (place search) on a local .osm.pbf
    #[command(aliases = &["search"])]
    Geocode(commands::GeocodeArgs),
}
```

---

## Phase 2: Frontend — `RmpcaGeocode.js`

### 2.1 Create Geocoder Class

**File: `gnome-maps-main/src/rmpcaGeocode.js`**

```javascript
/* RmpcaGeocode — offline place search via the rmpca CLI.
 *
 * Shells out to `rmpca geocode` with a JSON request.
 * Returns Place objects compatible with GraphHopper/Photon format.
 */
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GeocodeGlib from 'gi://GeocodeGlib';
import {Place} from './place.js';
import * as Utils from './utils.js';

const RMPCA_DEFAULT_PATH = 'rmpca';

export class RmpcaGeocode {
    constructor({ rmpcaPath, offlineMapFile, limit }) {
        this._rmpcaPath = rmpcaPath || RMPCA_DEFAULT_PATH;
        this._offlineMapFile = offlineMapFile;
        this._limit = limit || 20;
        this._subprocess = null;
    }

    setOfflineMapFile(path) {
        this._offlineMapFile = path;
    }

    get attribution() {
        return 'OpenStreetMap contributors';
    }

    get attributionUrl() {
        return 'https://www.openstreetmap.org/copyright';
    }

    get name() {
        return 'Rmpca (Offline)';
    }

    get url() {
        return 'https://github.com/spacialglaciercom-lab/rmpca-rust';
    }

    search(query, latitude, longitude, cancellable, callback) {
        if (!this._offlineMapFile) {
            callback(null, 'No offline map file selected');
            return;
        }

        const request = {
            offline_map_file: this._offlineMapFile,
            q: query,
            limit: this._limit,
        };

        if (latitude !== null && longitude !== null) {
            request.location = [latitude, longitude];
        }

        this._spawn(request, (result, error) => {
            if (error) {
                Utils.debug('rmpca geocode error: ' + error);
                callback(null, error);
                return;
            }
            if (!result || !result.success) {
                callback(null, (result && result.error) || 'Search failed');
                return;
            }
            callback(this._parseResults(result.features), null);
        });
    }

    _spawn(request, onDone) {
        // Same pattern as RmpcaRouter._spawn
        let argv = [this._rmpcaPath, 'geocode'];
        // ... spawn subprocess, pipe JSON, parse response
    }

    _parseResults(features) {
        return features.map(f => this._parseFeature(f)).filter(p => p !== null);
    }

    _parseFeature(feature) {
        const [lon, lat] = feature.geometry.coordinates;
        const props = feature.properties;

        const location = new GeocodeGlib.Location({
            latitude: lat,
            longitude: lon,
            accuracy: 0.0
        });

        return new Place({
            name: props.name,
            location: location,
            osmId: String(props.osm_id),
            osmType: this._parseOsmType(props.osm_type),
            town: props.city,
            street: props.street,
            // ... other properties
        });
    }

    _parseOsmType(t) {
        switch (t) {
            case 'N': return GeocodeGlib.PlaceOsmType.NODE;
            case 'W': return GeocodeGlib.PlaceOsmType.WAY;
            case 'R': return GeocodeGlib.PlaceOsmType.RELATION;
            default: return GeocodeGlib.PlaceOsmType.UNKNOWN;
        }
    }
}
```

### 2.2 Update Geocode Factory

**File: `gnome-maps-main/src/geocode.js`**

```javascript
import {GraphHopperGeocode} from './graphHopperGeocode.js';
import {RmpcaGeocode} from './rmpcaGeocode.js';
import {Application} from './application.js';

var _geocoder = null;
var _offlineGeocoder = null;

export function getGeocoder() {
    // Check if offline mode is enabled
    const settings = Application.settings;
    const offlineMapFile = settings.get('cpp-offline-map-file');
    const useOffline = settings.get('use-offline-geocode');

    if (useOffline && offlineMapFile) {
        if (!_offlineGeocoder) {
            _offlineGeocoder = new RmpcaGeocode({
                rmpcaPath: settings.get('rmpca-path'),
                offlineMapFile: offlineMapFile,
            });
        }
        return _offlineGeocoder;
    }

    // Default: online geocoder
    if (!_geocoder)
        _geocoder = new GraphHopperGeocode();

    return _geocoder;
}

export function setOfflineGeocoder(offlineMapFile) {
    const settings = Application.settings;
    _offlineGeocoder = new RmpcaGeocode({
        rmpcaPath: settings.get('rmpca-path'),
        offlineMapFile: offlineMapFile,
    });
}
```

### 2.3 Add to GResource

**File: `gnome-maps-main/src/org.gnome.Maps.src.gresource.xml.in`**

```xml
<file>rmpcaGeocode.js</file>
```

---

## Phase 3: Settings & UI Integration

### 3.1 Add GSettings Schema

**File: `gnome-maps-main/data/org.gnome.Maps.gschema.xml`**

```xml
<key name="use-offline-geocode" type="b">
  <default>false</default>
  <summary>Use offline geocoding</summary>
  <description>Search for places using local OSM data instead of online services</description>
</key>
```

### 3.2 Add UI Toggle (Optional)

Add a checkbox in the CPPView sidebar or settings dialog to toggle between online/offline geocoding.

---

## Implementation Checklist

### Phase 1: Rust Backend (Priority: High)

- [ ] Create `src/osm/geocode_parser.rs`
  - [ ] `NamedPlace` struct
  - [ ] `extract_named_places()` function
  - [ ] Filter by searchable tags (name, addr:*, amenity, etc.)
  
- [ ] Create `src/osm/search_index.rs`
  - [ ] `SearchIndex` struct
  - [ ] `build()` method
  - [ ] `search()` method with fuzzy matching
  - [ ] Location bias scoring
  
- [ ] Create `src/commands/geocode.rs`
  - [ ] `GeocodeArgs` struct
  - [ ] `GeocodeRequest` / `GeocodeResponse` JSON schemas
  - [ ] `run()` function
  - [ ] Progress events to stderr
  
- [ ] Register command in `src/commands/mod.rs` and `src/main.rs`

- [ ] Add tests in `src/tests/geocode_tests.rs`

### Phase 2: Frontend (Priority: High)

- [ ] Create `gnome-maps-main/src/rmpcaGeocode.js`
  - [ ] `RmpcaGeocode` class
  - [ ] `search()` method
  - [ ] `_spawn()` method (reuse pattern from RmpcaRouter)
  - [ ] `_parseResults()` method
  
- [ ] Update `gnome-maps-main/src/geocode.js`
  - [ ] Import `RmpcaGeocode`
  - [ ] Add offline mode logic
  - [ ] Add `setOfflineGeocoder()` function
  
- [ ] Add to gresource file

### Phase 3: Settings (Priority: Medium)

- [ ] Add `use-offline-geocode` GSettings key
- [ ] Add UI toggle in sidebar or preferences

### Phase 4: Optimization (Priority: Low)

- [ ] Cache search index to disk (`.rmpca/index/`)
- [ ] Incremental index updates
- [ ] Advanced fuzzy matching (Levenshtein distance)
- [ ] Support for reverse geocoding (`rmpca reverse-geocode`)

---

## API Compatibility

The response format is compatible with Photon/GraphHopper:

```json
{
  "success": true,
  "features": [
    {
      "geometry": {
        "type": "Point",
        "coordinates": [-73.5673, 45.5017]
      },
      "properties": {
        "name": "Montreal City Hall",
        "osm_id": 12345678,
        "osm_type": "W",
        "city": "Montreal",
        "street": "Rue Notre-Dame Est",
        "housenumber": "275",
        "osm_key": "amenity",
        "osm_value": "townhall"
      }
    }
  ]
}
```

---

## Estimated Effort

| Component | Effort | Dependencies |
|-----------|--------|--------------|
| Rust geocode command | 2-3 days | osmpbfreader crate |
| Search index | 1-2 days | None |
| Frontend RmpcaGeocode.js | 1 day | RmpcaRouter.js pattern |
| Settings integration | 0.5 day | GSettings |
| Testing | 1 day | All components |

**Total: 5-7 days**

---

## Testing Strategy

1. **Unit tests**: Search index fuzzy matching
2. **Integration tests**: Full geocode pipeline with sample PBF
3. **Frontend tests**: Mock subprocess, verify Place objects
4. **Manual testing**: Search for known places in Montreal PBF