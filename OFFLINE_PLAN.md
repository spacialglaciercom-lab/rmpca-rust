# Offline Capability Plan

Plan to take `rmpca-rust` + the bundled `gnome-maps-main` fork from a
partially-connected system to a **fully offline**, air-gap-capable stack.
The target deployment is a FreeBSD host or jail with no outbound network.

---

## 1. Current State Audit

### 1.1 What is already offline-capable
- Core Rust crate (`src/optimizer`, `src/osm`, `src/geo`) parses local
  `.osm.pbf` and `.geojson` directly — no network calls in the hot path.
- `compile-map` / `optimize` / `validate` / `clean` / `serve` commands
  operate purely on local files.
- `rkyv` zero-copy `.rmp` graph caches load from disk.

### 1.2 Remaining online dependencies
| # | Component | File | Nature |
|---|-----------|------|--------|
| 1 | R2 PMTiles fetch (`/vsis3/`) | `src/commands/extract_overture.rs` | HTTPS to Cloudflare R2 |
| 2 | OSM extract via jail HTTP API | `src/commands/extract_osm.rs`, `src/client.rs` | HTTP to `rmpca_extract_host:4000` |
| 3 | Backend/optimizer HTTP jails | `src/client.rs`, `src/config.rs` | HTTP to jail services |
| 4 | Status/logs endpoints | `src/commands/status.rs`, `src/commands/logs.rs` | HTTP health checks |
| 5 | Raster map tiles | `gnome-maps-main/data/maps-service.json` → `tile.openstreetmap.org` | HTTPS tile server |
| 6 | GraphHopper routing + geocoding | `gnome-maps-main/src/graphHopper.js`, `graphHopperGeocode.js` | HTTPS third-party API |
| 7 | Transitous transit routing | `gnome-maps-main/src/transitous.js` (`api.transitous.org`) | HTTPS third-party API |
| 8 | Photon geocoder | `gnome-maps-main/src/photonUtils.js`, Photon backend | HTTPS third-party API |
| 9 | Wikipedia/Wikidata thumbnails | `gnome-maps-main/src/wikipedia.js`, `placeView.js` | HTTPS REST |
| 10 | OSM edit / OAuth | `gnome-maps-main/src/osmConnection.js`, `osmEditDialog.js` | HTTPS api.openstreetmap.org |
| 11 | Send-to / share URLs | `gnome-maps-main/src/sendToDialog.js` | launches external URLs |
| 12 | `cargo build` itself | `Cargo.toml` | needs crates.io on first build |

---

## 2. Guiding Principles

1. **Vendor everything or disable it.** Every remote call is either
   replaced by a local equivalent, swapped for a file-backed cache, or
   compile-time gated behind a feature flag.
2. **Fail closed, not open.** When offline mode is active and a feature
   needs the network, the code must return a clear typed error, never
   silently hang on DNS.
3. **Single toggle.** One cargo feature `offline` + one GSettings key
   `offline-mode` flips the whole stack.
4. **Reproducible builds.** Build artifacts (crates, OSM PBF, tiles,
   Photon index) ship as versioned bundles that can be checksummed.

---

## 3. Workstreams

### WS1 — Rust build reproducibility (offline `cargo build`)
- [ ] Add a `vendor/` directory via `cargo vendor` and commit it (or
      publish a tarball artifact).
- [ ] Check in `.cargo/config.toml` redirecting `crates-io` to
      `vendor/` when `CARGO_NET_OFFLINE=true`.
- [ ] Document `cargo build --offline --release` as the canonical
      offline build command in `README.md`.
- [ ] CI job that runs `cargo build --offline` with network disabled
      to prevent regression.

### WS2 — R2 / PMTiles → local-only tile cache
Affected: `src/commands/extract_overture.rs`, `src/config.rs`,
`INSTRUCTIONS.md`.
- [ ] Introduce `cargo` feature `r2` (default-off). Gate the `/vsis3/`
      path, `AWS_*` env wiring, and the `reqwest` R2 code paths.
- [ ] When `r2` is disabled, `resolve_input` must require an existing
      local file and emit `InputNotFound` error with guidance:
      "place the `.pmtiles` file at `<path>` (see
      `docs/offline-bundles.md`)".
- [ ] Add a `rmpca bundle verify` subcommand that walks a bundle
      manifest (SHA256-checked) to catch missing/corrupt tiles.

### WS3 — Replace HTTP jail calls with a local library path
Affected: `src/client.rs`, `src/commands/extract_osm.rs`,
`src/commands/status.rs`, `src/commands/logs.rs`.
- [ ] Extract the extract/optimizer jail APIs into a `LocalBackend`
      trait with two implementations:
      - `HttpBackend` (current behavior, feature-gated `network`)
      - `InProcessBackend` that calls the same code in-process via
        `osmpbfreader` + `optimizer` crates.
- [ ] `extract-osm` uses `InProcessBackend` when `--offline` is
      passed or `RMPCA_OFFLINE=1` is set; the HTTP call is only used
      for legacy remote deployments.
- [ ] `status` and `logs` degrade gracefully offline: read a local
      `/var/run/rmpca/*.pid` + `/var/log/rmpca/*.log` instead of
      poking `:4000`.

### WS4 — GNOME Maps tile server → MBTiles / vector PMTiles
Affected: `gnome-maps-main/data/maps-service.json`,
`gnome-maps-main/src/mapSource.js`, `lib/maps-sync-map-source.c`.
- [ ] Ship a local tile server (one of):
      - Option A (recommended): `pmtiles serve` from a pinned
        binary + the city PMTiles on disk; point
        `maps-service.json` at `http://127.0.0.1:<port>/...`.
      - Option B: embed libpmtiles and let
        `MapsSyncMapSource::fill_tile` read tiles directly from a
        local `.pmtiles` file without a loopback server.
- [ ] Add a GSettings key `org.gnome.Maps.offline-mode` (bool) and a
      `tile-bundle-path` (string). Wire these through
      `src/mapSource.js` so the Mapbox/OSM URL is swapped at runtime.
- [ ] Replace the hard-coded `https://tile.openstreetmap.org/...`
      with a templated URL sourced from GSettings.

### WS5 — GraphHopper (routing + geocoding) → `rmpca serve`
Affected: `gnome-maps-main/src/graphHopper.js`,
`graphHopperGeocode.js`, `routingDelegator.js`.
- [ ] Introduce an `RmpcaRouter` delegate (already scaffolded in
      `rmpcaRouter.js` per the grep) and route all standard
      car/bike/pedestrian requests through it by spawning
      `rmpca serve --json`.
- [ ] Feature-flag `graphHopper.js` behind a compile-time
      `with-graphhopper` constant in `constants.js`; default off.
- [ ] For geocoding, ship a local geocoder:
      - Phase 1: prefix-trie built from the PBF "name=*" index,
        surfaced via `rmpca geocode` subcommand.
      - Phase 2 (optional): bundle Photon's Elasticsearch index as a
        tarball and run Photon locally on 127.0.0.1.

### WS6 — Transitous → optional feature, disabled by default
Affected: `gnome-maps-main/src/transitous.js`,
`routingDelegator.js`.
- [ ] Hide the "Public transit" routing chip in the UI when
      `offline-mode` is true.
- [ ] If transit is required offline, import a GTFS feed once and
      serve it via `OpenTripPlanner` on localhost — out of scope for
      v1 of this plan.

### WS7 — Wikipedia / Wikidata / OSM edit / share links
Affected: `placeView.js`, `wikipedia.js`, `osmConnection.js`,
`osmEditDialog.js`, `sendToDialog.js`.
- [ ] In offline mode:
      - Skip Wikipedia thumbnail fetch entirely; show the place card
        without the photo (no spinner, no error dialog).
      - Grey out the "Edit on OpenStreetMap" menu item.
      - Grey out "Send to → Copy OSM link" (still allow copy of
        `geo:` URI which is local).
- [ ] Centralize these decisions in one helper `offline.js` that
      reads the GSettings key once per session.

### WS8 — Packaging & bundle format
- [ ] Define a bundle layout in `docs/offline-bundles.md`:
      ```
      rmpca-bundle-<region>-<date>/
        manifest.json          # SHA256 of every file
        tiles/<region>.pmtiles # vector tiles
        osm/<region>.osm.pbf   # routing graph source
        graphs/<region>.rmp    # precompiled rmpca graph cache
        geocoder/<region>.trie # name index (WS5)
        VERSION
      ```
- [ ] `format-usb.sh` (already present) extended to copy a bundle
      onto a removable drive and write
      `/usr/local/etc/rmpca/bundle.path` so the app finds it.
- [ ] Add `scripts/build-bundle.sh` that produces the above directory
      for a given bbox from an authoritative online machine, so the
      offline device never has to fetch anything.

### WS9 — Testing & CI gates
- [ ] Property tests already exist — add an integration test that
      runs the whole `extract → compile → optimize → serve` pipeline
      with `unshare -n` (no network namespace) to prove no egress.
- [ ] A `scripts/check-no-network.sh` that greps the built JS bundle
      and Rust binary (via `strings`) for forbidden hostnames
      (`graphhopper.com`, `api.transitous.org`,
      `tile.openstreetmap.org`, `wikipedia.org`,
      `cloudflarestorage.com`) and fails CI if any are present in an
      `offline`-feature build.
- [ ] Unit tests for the new `offline.js` helper and the
      `InProcessBackend`.

### WS10 — Documentation
- [ ] `docs/offline-bundles.md` (new) — bundle layout, how to build,
      how to verify, how to install.
- [ ] `docs/offline-mode.md` (new) — end-user toggle, which features
      are unavailable, troubleshooting (DNS, firewall).
- [ ] Update `README.md` and `INSTRUCTIONS.md` to mark the R2 section
      as "online-only; skip for offline deployments".
- [ ] Update `HANDOVER.md` with the offline install path
      (`pmtiles serve` unit file, GSettings defaults).

---

## 4. Phased Rollout

### Phase 1 — "Build & run without internet" (week 1)
WS1, WS2, WS3. Outcome: `cargo build --offline` succeeds on an
air-gapped box, and the `rmpca` CLI can go through the whole
extract→compile→optimize→serve pipeline with a locally-staged PBF.

### Phase 2 — "Maps UI without internet" (weeks 2–3)
WS4, WS5 (routing portion), WS7. Outcome: GNOME Maps launches, shows
vector tiles from local PMTiles, and can request a coverage route via
`rmpca serve` without any DNS lookups.

### Phase 3 — "Feature parity lite" (week 4+)
WS5 (geocoding), WS6, WS8, WS9, WS10. Outcome: a single
`rmpca-bundle-<region>.tar.zst` is the only artifact needed to bring
up the full app on a fresh machine.

---

## 5. Acceptance Criteria

- `unshare -rn cargo build --release --offline` succeeds.
- `unshare -rn ./target/release/rmpca pipeline --input
  /bundles/montreal/osm/montreal.osm.pbf --bbox ... --output
  final.gpx` succeeds.
- `unshare -rn ./launch-gnome-maps.sh` launches the app, tiles render,
  a coverage route is computed, GPX exports — zero network syscalls
  (verified with `bpftrace` / `dtrace` on connect).
- `scripts/check-no-network.sh` passes on the release artifact.
- No commit removes the online code paths; they are only
  feature-gated, so the same tree still builds the "classic" online
  variant with `--features network,graphhopper,r2`.

---

## 6. Open Questions

1. Licensing: re-hosting OSM tiles offline is fine under ODbL, but
   the GraphHopper API key in `maps-service.json` should be removed
   before shipping offline bundles regardless.
2. Do we need to support map updates in the field, or is a
   "refresh bundle on USB" flow acceptable? (Affects WS8 design.)
3. Is Photon (JVM + Elasticsearch) acceptable on the target jail, or
   must the geocoder be pure-Rust? (Affects WS5 Phase 2 scope.)
