/* -*- Mode: JS2; indent-tabs-mode: nil; js2-basic-offset: 4 -*- */
/* vim: set et ts=4 sw=4: */
/*
 * Copyright (c) 2025 RouteMasterPro Team
 *
 * GNOME Maps is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 2 of the License, or (at your
 * option) any later version.
 *
 * GNOME Maps is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
 * or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License
 * for more details.
 *
 * You should have received a copy of the GNU General Public License along
 * with GNOME Maps; if not, see <http://www.gnu.org/licenses/>.
 *
 * CPPView — sidebar panel for area-coverage route optimization.
 *
 * Provides UI for:
 *   - Selecting a local .osm.pbf offline map file
 *   - Choosing a vehicle profile (truck / car / delivery)
 *   - Drawing a bounding box on the map
 *   - Running CPP optimization via CPPOptimizer
 *   - Viewing results (distance, deadhead, efficiency)
 *   - Exporting the resulting route as GPX
 */

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Gtk from 'gi://Gtk';

import gettext from 'gettext';
import {CPPOptimizer} from './cppOptimizer.js';
import {Application} from './application.js';
import * as Utils from './utils.js';

const _ = gettext.gettext;

const PROFILE_NAMES = ['truck', 'car', 'delivery'];

export class CPPView extends Gtk.Box {

    constructor({mapView, ...params}) {
        super(params);

        this._mapView = mapView;
        this._settings = Application.settings;
        
        let rmpcaPath = this._settings.get('rmpca-path');
        this._optimizer = new CPPOptimizer({ rmpcaPath: rmpcaPath });
        
        this._offlineMapFile = this._settings.get('cpp-offline-map-file');
        this._polygon = null;
        this._drawingArea = false;
        this._running = false;

        /* ---- Wire up UI elements ---- */

        if (this._offlineMapFile) {
            this._fileButton.label = GLib.path_get_basename(this._offlineMapFile);
        }

        this._fileButton.connect('clicked', () => this._onSelectFile());

        this._profileDropDown.connect('notify::selected', () => {
            this._profile = PROFILE_NAMES[this._profileDropDown.selected] || 'truck';
        });

        this._drawAreaButton.connect('clicked', () => this._onDrawAreaClicked());
        this._optimizeButton.connect('clicked', () => this._onOptimizeClicked());
        this._exportButton.connect('clicked', () => this._onExport());

        /* ---- Connect optimizer route signals ---- */

        this._optimizer.route.connect('update', () => this._onRouteUpdate());
        this._optimizer.route.connect('error', (_, msg) => this._onRouteError(msg));
        this._optimizer.route.connect('reset', () => this._onRouteReset());
    }

    /* ================================================================== */
    /*  File selection                                                     */
    /* ================================================================== */

    _onSelectFile() {
        let dialog = new Gtk.FileDialog();
        dialog.title = _('Select OSM PBF File');

        let filter = new Gtk.FileFilter();
        filter.add_pattern('*.osm.pbf');
        filter.name = _('OSM PBF files');

        let filters = new Gio.ListStore(Gtk.FileFilter.Gtype);
        filters.append(filter);
        dialog.filters = filters;
        dialog.default_filter = filter;

        dialog.open(this.get_root(), null, (dlg, result) => {
            try {
                let file = dlg.open_finish(result);
                this._offlineMapFile = file.get_path();
                this._settings.set('cpp-offline-map-file', this._offlineMapFile);
                this._fileButton.label = GLib.path_get_basename(this._offlineMapFile);
            } catch (e) {
                // cancelled or error — ignore
            }
        });
    }

    /* ================================================================== */
    /*  Area drawing                                                       */
    /* ================================================================== */

    _onDrawAreaClicked() {
        if (this._drawingArea) {
            this._cancelDrawing();
            return;
        }

        this._drawingArea = true;
        this._drawAreaButton.label = _('Cancel Drawing');

        this._mapView.enablePolygonSelection((polygon, closed) => {
            this._polygon = polygon;
            if (closed) {
                this._drawingArea = false;
                this._drawAreaButton.label = _('Area Selected');
            }
        });
    }

    _cancelDrawing() {
        this._drawingArea = false;
        this._mapView.disablePolygonSelection();
        this._drawAreaButton.label = _('Draw Area on Map');
    }

    /* ================================================================== */
    /*  Optimization                                                       */
    /* ================================================================== */

    _onOptimizeClicked() {
        /* If already running, cancel */
        if (this._running) {
            this._optimizer.cancelCurrentRequest();
            this._running = false;
            this._optimizeButton.label = _('Optimize Route');
            this._optimizeButton.remove_css_class('destructive-action');
            this._optimizeButton.add_css_class('suggested-action');
            this._spinner.visible = false;
            return;
        }

        /* Validate inputs */
        if (!this._offlineMapFile) {
            this._showInlineError(_('Select an offline map file first'));
            return;
        }

        if (!this._polygon || this._polygon.length < 3) {
            this._showInlineError(_('Draw an area with at least 3 points on the map first'));
            return;
        }

        /* Cancel any lingering area drawing */
        if (this._drawingArea)
            this._cancelDrawing();

        /* Parse optional depot coordinates */
        let depot;
        let depotText = this._depotEntry.text.trim();
        if (depotText) {
            let parts = depotText.split(',').map(s => parseFloat(s.trim()));
            if (parts.length === 2 && isFinite(parts[0]) && isFinite(parts[1])) {
                depot = [parts[0], parts[1]];
            }
        }

        /* Update UI to "running" state */
        this._running = true;
        this._spinner.visible = true;
        this._optimizeButton.label = _('Cancel');
        this._optimizeButton.remove_css_class('suggested-action');
        this._optimizeButton.add_css_class('destructive-action');
        this._clearResults();

        /* Run */
        this._optimizer.optimize(this._polygon, {
            offlineMapFile: this._offlineMapFile,
            profile: PROFILE_NAMES[this._profileDropDown.selected] || 'truck',
            depot: depot,
        }, (progress) => {
            if (progress.message)
                this._summaryLabel.label = progress.message;
        });
    }

    /* ================================================================== */
    /*  Route signal handlers                                              */
    /* ================================================================== */

    _onRouteUpdate() {
        let route = this._optimizer.route;

        this._running = false;
        this._spinner.visible = false;
        this._optimizeButton.label = _('Optimize Route');
        this._optimizeButton.remove_css_class('destructive-action');
        this._optimizeButton.add_css_class('suggested-action');

        /* Show results */
        this._resultsBox.visible = true;
        this._exportButton.visible = true;

        this._summaryLabel.label = route.summaryText;
        this._distanceLabel.label = _('Total: %s').format(route.distanceText);
        this._deadheadLabel.label = _('Deadhead: %s').format(route.deadheadText);
        this._efficiencyLabel.label =
            _('Efficiency: %s%%').format(route.efficiencyPercent.toFixed(1));
        this._statsLabel.label =
            _('%d edges, %d nodes').format(route.edgeCount, route.nodeCount);

        /* Render on map */
        this._mapView.showCPPRoute(route);
    }

    _onRouteError(msg) {
        this._running = false;
        this._spinner.visible = false;
        this._optimizeButton.label = _('Optimize Route');
        this._optimizeButton.remove_css_class('destructive-action');
        this._optimizeButton.add_css_class('suggested-action');
        this._showInlineError(msg);
    }

    _onRouteReset() {
        this._clearResults();
        this._mapView.clearCPPRoute();
    }

    /* ================================================================== */
    /*  Helpers                                                            */
    /* ================================================================== */

    _clearResults() {
        this._resultsBox.visible = false;
        this._exportButton.visible = false;
        this._summaryLabel.label = '';
        this._summaryLabel.remove_css_class('error');
        this._distanceLabel.label = '';
        this._deadheadLabel.label = '';
        this._efficiencyLabel.label = '';
        this._statsLabel.label = '';
    }

    _showInlineError(msg) {
        this._resultsBox.visible = true;
        this._summaryLabel.label = msg;
        this._summaryLabel.add_css_class('error');

        GLib.timeout_add(null, 5000, () => {
            this._summaryLabel.remove_css_class('error');
            return GLib.SOURCE_REMOVE;
        });
    }

    _onExport() {
        let dialog = new Gtk.FileDialog();
        dialog.title = _('Export GPX File');
        dialog.initial_name = 'coverage-route.gpx';

        let filter = new Gtk.FileFilter();
        filter.add_pattern('*.gpx');
        filter.name = _('GPX files');

        let filters = new Gio.ListStore(Gtk.FileFilter.Gtype);
        filters.append(filter);
        dialog.filters = filters;

        dialog.save(this.get_root(), null, (dlg, result) => {
            try {
                let file = dlg.save_finish(result);
                
                this._optimizer.exportGPX(this._polygon, {
                    offlineMapFile: this._offlineMapFile,
                    profile: PROFILE_NAMES[this._profileDropDown.selected] || 'truck',
                }, (gpxString, error) => {
                    if (error) {
                        this._showInlineError(error);
                        return;
                    }

                    file.replace_contents_async(new GLib.Bytes(gpxString), null, false, 
                        Gio.FileCreateFlags.REPLACE_DESTINATION, null, (f, res) => {
                            try {
                                f.replace_contents_finish(res);
                            } catch (e) {
                                this._showInlineError(e.message);
                            }
                        });
                });
            } catch (e) {
                // cancelled or error
            }
        });
    }
}

GObject.registerClass({
    Template: 'resource:///org/gnome/Maps/ui/cpp-view.ui',
    InternalChildren: [
        'fileButton',
        'profileDropDown',
        'depotEntry',
        'drawAreaButton',
        'optimizeButton',
        'spinner',
        'resultsBox',
        'summaryLabel',
        'distanceLabel',
        'deadheadLabel',
        'efficiencyLabel',
        'statsLabel',
        'exportButton',
    ],
}, CPPView);
