/* -*- Mode: JS2; indent-tabs-mode: nil; js2-basic-offset: 4 -*- */
/* vim: set et ts=4 sw=4: */
/*
 * Copyright (c) 2025 RouteMasterPro Team
 *
 * GNOME Maps is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or (at your
 * option) any later version.
 *
 * PolygonSelector — click-to-place polygon drawing on the map.
 *
 * Click to add vertices, double-click or click near the first vertex
 * to close the polygon. Right-click to undo the last vertex.
 * Renders the polygon in real-time using Shumate.PathLayer.
 */

import Gdk from 'gi://Gdk';
import GLib from 'gi://GLib';
import Gtk from 'gi://Gtk';
import Shumate from 'gi://Shumate';

/* Orange outline matching BBoxSelector */
const STROKE_COLOR = new Gdk.RGBA({ red:   0xff / 255,
                                     green: 0x78 / 255,
                                     blue:  0x00 / 255,
                                     alpha: 0.9 });

/* Fill color (semi-transparent orange) */
const FILL_COLOR = new Gdk.RGBA({ red:   0xff / 255,
                                   green: 0x78 / 255,
                                   blue:  0x00 / 255,
                                   alpha: 0.15 });

/* Close threshold in pixels — click within this distance of first
 * vertex to close the polygon */
const CLOSE_THRESHOLD = 15;

/**
 * PolygonSelector — lets the user draw a polygon on the map.
 *
 * Usage:
 *   let selector = new PolygonSelector(mapView);
 *   selector.enable((polygon) => {
 *       // polygon is [[lon, lat], [lon, lat], ...] (closed ring)
 *   });
 *   selector.disable();
 */
export class PolygonSelector {

    constructor(mapView) {
        this._mapView = mapView;
        this._active = false;
        this._clickGesture = null;
        this._polyLayer = null;
        this._vertices = [];    // [{lat, lon}, ...]
        this._callback = null;
    }

    get active() {
        return this._active;
    }

    /**
     * Activate polygon drawing mode.
     * @param {function} callback - Called with polygon coordinates
     *                              [[lon,lat], ...] when closed.
     */
    enable(callback) {
        if (this._active)
            return;

        this._active = true;
        this._callback = callback;
        this._vertices = [];
        this._lastClickTime = 0;
        this._lastClickX = 0;
        this._lastClickY = 0;

        /* Click gesture for adding vertices - handle all buttons */
        this._clickGesture = new Gtk.GestureClick();
        this._clickGesture.connect('pressed', this._onPressed.bind(this));
        this._mapView.map.add_controller(this._clickGesture);

        /* Path layer for the polygon outline */
        this._polyLayer = new Shumate.PathLayer({
            viewport:     this._mapView.map.viewport,
            stroke_width: 2,
            stroke_color: STROKE_COLOR,
            fill:         true,
            fill_color:   FILL_COLOR,
        });
        this._mapView.map.insert_layer_above(this._polyLayer,
                                              this._mapView._mapLayer);
    }

    /**
     * Deactivate polygon drawing and clean up.
     */
    disable() {
        if (!this._active)
            return;

        this._active = false;
        this._callback = null;
        this._vertices = [];
        this._lastClickTime = 0;

        if (this._clickGesture) {
            this._mapView.map.remove_controller(this._clickGesture);
            this._clickGesture = null;
        }

        if (this._polyLayer) {
            this._polyLayer.remove_all();
            this._mapView.map.remove_layer(this._polyLayer);
            this._polyLayer = null;
        }
    }

    /* ---- Gesture handlers ------------------------------------------------ */

    _onPressed(gesture, nPress, x, y) {
        // Claim the event immediately to prevent map panning
        gesture.set_state(Gtk.EventSequenceState.CLAIMED);

        let button = gesture.get_current_button();

        /* Right-click (button 3): undo last vertex */
        if (button === 3) {
            if (this._vertices.length > 0) {
                this._vertices.pop();
                this._updatePoly();
            }
            return true;
        }

        /* Left-click (button 1) only for adding vertices */
        if (button !== 1)
            return true;

        let viewport = this._mapView.map.viewport;
        let [lat, lon] = viewport.widget_coords_to_location(this._mapView, x, y);

        let now = GLib.get_monotonic_time();
        let isDoubleClick = (now - this._lastClickTime) < 300000; // 300ms

        /* Double-click: close the polygon if we have enough vertices */
        if (isDoubleClick && this._vertices.length >= 3) {
            this._closePoly();
            return true;
        }

        /* Click near first vertex: close the polygon */
        if (this._vertices.length >= 3) {
            let first = this._vertices[0];
            let [fx, fy] = viewport.location_to_widget_coords(this._mapView,
                                                                first.lat, first.lon);
            let dx = x - fx;
            let dy = y - fy;
            if (Math.sqrt(dx * dx + dy * dy) < CLOSE_THRESHOLD) {
                this._closePoly();
                return true;
            }
        }

        /* Add vertex */
        this._vertices.push({ lat, lon });
        this._lastClickTime = now;
        this._lastClickX = x;
        this._lastClickY = y;
        this._updatePoly();
        return true;
    }

    /* ---- Close the polygon and fire callback ---------------------------- */

    _closePoly() {
        if (this._vertices.length < 3)
            return;

        /* Build closed coordinate ring: [[lon, lat], ...] */
        let coords = this._vertices.map(v => [v.lon, v.lat]);
        /* Close the ring by repeating the first point */
        coords.push([this._vertices[0].lon, this._vertices[0].lat]);

        if (this._callback)
            this._callback(coords);

        /* Clean up gesture but keep the polygon visible */
        if (this._clickGesture) {
            this._mapView.map.remove_controller(this._clickGesture);
            this._clickGesture = null;
        }

        this._active = false;
    }

    /* ---- Polygon rendering ---------------------------------------------- */

    _updatePoly() {
        if (!this._polyLayer)
            return;

        this._polyLayer.remove_all();

        if (this._vertices.length === 0)
            return;

        /* Add all vertices as markers to the path layer */
        for (let v of this._vertices) {
            let marker = new Shumate.Marker({
                latitude:  v.lat,
                longitude: v.lon,
                visible:   true,
            });
            this._polyLayer.add_node(marker);
        }

        /* If we have 3+ vertices, close the loop for visual preview */
        if (this._vertices.length >= 3) {
            let first = this._vertices[0];
            let closeMarker = new Shumate.Marker({
                latitude:  first.lat,
                longitude: first.lon,
                visible:   true,
            });
            this._polyLayer.add_node(closeMarker);
        }
    }
}
