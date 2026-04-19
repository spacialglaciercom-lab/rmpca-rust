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
 * CPP (Chinese Postman Problem) route optimizer.
 * Spawns the rmpca CLI as a subprocess to compute area-coverage routes
 * from local .osm.pbf files. Fully offline — no network required.
 */

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import gettext from 'gettext';

import {BoundingBox} from './boundingBox.js';
import {TurnPoint, Route} from './route.js';
import {CPPRoute} from './cppRoute.js';
import * as Utils from './utils.js';

const _ = gettext.gettext;

/* Default path to the rmpca binary */
const RMPCA_DEFAULT_PATH = 'rmpca';

const SIGTERM = 15;

/**
 * CPPOptimizer — subprocess wrapper around `rmpca serve`
 *
 * Usage:
 *   let opt = new CPPOptimizer({ rmpcaPath: '/usr/local/bin/rmpca' });
 *   let route = opt.optimize(bbox, { offlineMapFile, profile, depot }, (progress) => { ... });
 *   opt.cancelCurrentRequest();
 */
export class CPPOptimizer {

    constructor({ rmpcaPath } = {}) {
        this._rmpcaPath = rmpcaPath || RMPCA_DEFAULT_PATH;
        this._subprocess = null;
        this._route = new CPPRoute();
    }

    get route() {
        return this._route;
    }

    /**
     * Cancel a running optimization by sending SIGTERM to the subprocess.
     */
    cancelCurrentRequest() {
        if (this._subprocess) {
            this._subprocess.send_signal(SIGTERM);
            this._subprocess = null;
        }
    }

    /**
     * Run CPP optimization for the given polygon and options.
     *
     * @param {Array} polygon - Array of [lon, lat] coordinates forming the polygon
     * @param {Object} options
     * @param {string} options.offlineMapFile - Path to .osm.pbf file
     * @param {string} [options.profile='truck'] - Vehicle profile
     * @param {[number,number]} [options.depot] - Optional [lat, lon] depot
     * @param {function} onProgress - Called with {message, percent}
     */
    optimize(polygon, options, onProgress) {
        let request = {
            polygon: { coordinates: polygon },
            offline_map_file: options.offlineMapFile,
            profile: options.profile || 'truck',
        };
        if (options.depot)
            request.depot = options.depot;

        let requestJSON = JSON.stringify(request);
        let argv = [this._rmpcaPath, 'serve'];

        Utils.debug('CPP: spawning ' + argv.join(' '));

        let launcher = new Gio.SubprocessLauncher({
            flags: Gio.SubprocessFlags.STDIN_PIPE |
                   Gio.SubprocessFlags.STDOUT_PIPE |
                   Gio.SubprocessFlags.STDERR_PIPE,
        });

        try {
            this._subprocess = launcher.spawnv(argv);
        } catch (e) {
            Utils.debug('CPP: failed to spawn rmpca: ' + e.message);
            this._route.error(_("Failed to start route optimizer. Is rmpca installed?"));
            return;
        }

        // Write request to stdin
        let stdinStream = this._subprocess.get_stdin_pipe();
        let stdinBytes = new GLib.Bytes(requestJSON + '\n');

        stdinStream.write_bytes_async(stdinBytes, GLib.PRIORITY_DEFAULT, null,
            (stream, res) => {
                try {
                    stream.write_bytes_finish(res);
                    stream.close(null);
                } catch (e) {
                    Utils.debug('CPP: stdin write error: ' + e.message);
                }
            });

        // Read stderr for progress
        let stderrStream = this._subprocess.get_stderr_pipe();
        this._readLines(stderrStream, (line) => {
            try {
                let evt = JSON.parse(line);
                if (evt.event === 'progress' && onProgress)
                    onProgress({ message: evt.message, percent: evt.percent });
            } catch (_e) {
                // Not JSON — ignore
            }
        });

        // Read stdout for result
        let stdoutStream = this._subprocess.get_stdout_pipe();
        let stdoutBuf = '';

        this._readAll(stdoutStream, (data) => {
            stdoutBuf += data;
        }, () => {
            // stdout closed — parse result
            this._subprocess.wait_async(null, (proc, res) => {
                this._subprocess = null;
                try {
                    proc.wait_finish(res);
                    let exitStatus = proc.get_exit_status();
                    if (exitStatus !== 0) {
                        // Try to parse as error JSON
                        try {
                            let errResult = JSON.parse(stdoutBuf);
                            if (errResult.error) {
                                this._route.error(errResult.error);
                                return;
                            }
                        } catch (_e2) { /* not JSON */ }
                        this._route.error(_("Route optimization failed (exit code %d)").format(exitStatus));
                        return;
                    }

                    let result = JSON.parse(stdoutBuf);
                    if (!result.success) {
                        this._route.error(result.error || _("Unknown optimization error"));
                        return;
                    }

                    // Convert to Route format
                    let path = result.route.map((pt) => ({
                        latitude: pt.latitude,
                        longitude: pt.longitude,
                    }));

                    let turnPoints = this._createTurnPoints(path);

                    // Build bbox from path
                    let routeBBox = new BoundingBox();
                    path.forEach(({ latitude, longitude }) => {
                        routeBBox.extend(latitude, longitude);
                    });

                    // Update the standard Route (for mapView rendering)
                    this._route.update({
                        path: path,
                        turnPoints: turnPoints,
                        distance: result.total_distance_km * 1000, // meters
                        time: 0, // CPP routes don't have time estimates
                        bbox: routeBBox,
                        // CPP-specific fields
                        deadheadDistanceKm: result.deadhead_distance_km,
                        efficiencyPercent: result.efficiency_percent,
                        edgeCount: result.edge_count,
                        nodeCount: result.node_count,
                        profile: result.profile,
                    });
                } catch (e) {
                    Utils.debug('CPP: result parse error: ' + e.message);
                    this._route.error(_("Failed to parse optimization result"));
                }
            });
        });
    }

    /**
     * Export a CPP route as GPX.
     *
     * @param {Array} polygon - Array of [lon, lat] coordinates forming the polygon
     * @param {Object} options
     * @param {string} options.offlineMapFile
     * @param {string} [options.profile='truck']
     * @param {function} onComplete - Called with (gpxString, error)
     */
    exportGPX(polygon, options, onComplete) {
        let request = {
            polygon: { coordinates: polygon },
            offline_map_file: options.offlineMapFile,
            profile: options.profile || 'truck',
        };
        if (options.depot)
            request.depot = options.depot;

        let requestJSON = JSON.stringify(request);
        let argv = [this._rmpcaPath, 'serve', '--gpx'];

        Utils.debug('CPP: exporting GPX ' + argv.join(' '));

        let launcher = new Gio.SubprocessLauncher({
            flags: Gio.SubprocessFlags.STDIN_PIPE |
                   Gio.SubprocessFlags.STDOUT_PIPE |
                   Gio.SubprocessFlags.STDERR_PIPE,
        });

        try {
            let proc = launcher.spawnv(argv);

            // Write request to stdin
            let stdinStream = proc.get_stdin_pipe();
            let stdinBytes = new GLib.Bytes(requestJSON + '\n');
            stdinStream.write_bytes_async(stdinBytes, GLib.PRIORITY_DEFAULT, null, (s, r) => {
                try {
                    s.write_bytes_finish(r);
                    s.close(null);
                } catch (e) {
                    Utils.debug('CPP: GPX stdin write error: ' + e.message);
                }
            });

            // Read GPX from stdout
            let stdoutStream = proc.get_stdout_pipe();
            let gpxBuf = '';

            this._readAll(stdoutStream, (chunk) => {
                gpxBuf += chunk;
            }, () => {
                proc.wait_async(null, (p, r) => {
                    try {
                        p.wait_finish(r);
                    } catch (e) {
                        onComplete(null, e.message);
                        return;
                    }
                    if (p.get_exit_status() === 0)
                        onComplete(gpxBuf, null);
                    else
                        onComplete(null, _("Failed to generate GPX file (exit code %d)").format(p.get_exit_status()));
                });
            });
        } catch (e) {
            onComplete(null, e.message);
        }
    }

    /**
     * Create minimal turn points for a CPP route (just START and END).
     */
    _createTurnPoints(path) {
        if (path.length === 0)
            return [];

        let startPoint = new TurnPoint({
            coordinate: path[0],
            type: TurnPoint.Type.START,
            distance: 0,
            instruction: _("Start coverage route"),
            time: 0,
            turnAngle: 0,
        });

        let endPoint = new TurnPoint({
            coordinate: path[path.length - 1],
            type: TurnPoint.Type.END,
            distance: 0,
            instruction: _("End coverage route"),
            time: 0,
            turnAngle: 0,
        });

        return [startPoint, endPoint];
    }

    /**
     * Read a GInputStream line by line, calling back for each line.
     */
    _readLines(stream, onLine) {
        let dataStream = Gio.DataInputStream.new(stream);
        let lines = [];

        let readLine = () => {
            dataStream.read_line_async(GLib.PRIORITY_DEFAULT, null, (dstream, res) => {
                try {
                    let [line] = dstream.read_line_finish(res);
                    if (line !== null) {
                        let str = Utils.getBufferText(line);
                        if (str.trim())
                            onLine(str);
                        readLine(); // continue reading
                    }
                } catch (_e) {
                    // Stream closed or error
                }
            });
        };

        readLine();
    }

    /**
     * Read all data from a stream into a string.
     */
    _readAll(stream, onData, onClose) {
        let buf = '';

        stream.read_bytes_async(4096, GLib.PRIORITY_DEFAULT, null,
            function readChunk(src, res) {
                try {
                    let bytes = src.read_bytes_finish(res);
                    if (bytes.get_size() > 0) {
                        let chunk = Utils.getBufferText(bytes.get_data());
                        onData(chunk);
                        src.read_bytes_async(4096, GLib.PRIORITY_DEFAULT, null, readChunk);
                    } else {
                        onClose();
                    }
                } catch (_e) {
                    onClose();
                }
            });
    }
}
