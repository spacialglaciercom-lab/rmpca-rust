/* -*- Mode: JS2; indent-tabs-mode: nil; js2-basic-offset: 4 -*- */
/* vim: set et ts=4 sw=4: */
/*
 * Copyright (c) 2025 RouteMasterPro Team
 *
 * Offline mode helper for GNOME Maps.
 * 
 * Centralizes offline mode checks and feature availability.
 * Reads GSettings key once per session for performance.
 */

import GLib from 'gi://GLib';
import {Application} from './application.js';

let _offlineMode = null;
let _tileBundlePath = null;
let _localTileServerPort = null;

/**
 * Initialize offline mode settings from GSettings.
 * Called once at application startup.
 */
export function init() {
    let settings = Application.settings;
    _offlineMode = settings.get('offline-mode');
    _tileBundlePath = settings.get('tile-bundle-path');
    _localTileServerPort = settings.get('local-tile-server-port');
    
    if (_offlineMode) {
        console.log('[offline] Offline mode enabled');
        if (_tileBundlePath) {
            console.log(`[offline] Tile bundle: ${_tileBundlePath}`);
        }
    }
}

/**
 * Check if offline mode is enabled.
 * @returns {boolean}
 */
export function isOffline() {
    if (_offlineMode === null)
        init();
    return _offlineMode;
}

/**
 * Get the tile bundle path (PMTiles file or local server URL).
 * @returns {string|null}
 */
export function getTileBundlePath() {
    if (_tileBundlePath === null)
        init();
    return _tileBundlePath || null;
}

/**
 * Get the local tile server port.
 * @returns {number}
 */
export function getLocalTileServerPort() {
    if (_localTileServerPort === null)
        init();
    return _localTileServerPort || 8080;
}

/**
 * Get the local tile server URL.
 * @returns {string|null}
 */
export function getLocalTileServerUrl() {
    if (!isOffline() || !_tileBundlePath)
        return null;
    return `http://127.0.0.1:${getLocalTileServerPort()}`;
}

/**
 * Check if a feature is available in the current mode.
 * @param {string} feature - Feature name: 'wikipedia', 'osm-edit', 'transit', 'geocode', 'routing'
 * @returns {boolean}
 */
export function isFeatureAvailable(feature) {
    if (!isOffline())
        return true;
    
    // Features unavailable in offline mode
    const offlineUnavailable = [
        'wikipedia',      // Wikipedia thumbnails
        'osm-edit',        // OSM editing via OAuth
        'transit',        // Public transit (Transitous)
        'geocode',        // Remote geocoding (Photon/GraphHopper)
    ];
    
    // Features available in offline mode
    const offlineAvailable = [
        'routing',        // Local routing via rmpca
        'coverage',       // Area coverage routing
        'tiles',          // Local tiles (if bundle configured)
    ];
    
    if (offlineUnavailable.includes(feature))
        return false;
    
    if (offlineAvailable.includes(feature))
        return true;
    
    // Unknown feature - default to unavailable in offline mode
    return false;
}

/**
 * Get the tile URL pattern for the current mode.
 * In offline mode, returns local tile server URL.
 * In online mode, returns the default OSM tile URL.
 * @returns {string}
 */
export function getTileUrlPattern() {
    if (isOffline() && _tileBundlePath) {
        // Use local tile server
        return `http://127.0.0.1:${getLocalTileServerPort()}/{z}/{x}/{y}.pbf`;
    }
    
    // Online mode: use default vector tiles
    return 'https://tileserver.gnome.org/data/v3/{z}/{x}/{y}.pbf';
}