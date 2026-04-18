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
 */

import GObject from 'gi://GObject';

import {Route} from './route.js';

/**
 * CPPRoute — extends Route with Chinese Postman Problem-specific fields.
 *
 * In addition to the standard Route fields (path, turnPoints, distance, time, bbox),
 * CPPRoute carries:
 *   deadheadDistanceKm  — distance on repeated edges (km)
 *   efficiencyPercent  — unique edge distance / total distance * 100
 *   edgeCount          — number of edges in the original graph
 *   nodeCount          — number of nodes in the original graph
 *   profile            — vehicle profile used (truck, car, delivery)
 *
 * Signals inherited from Route: 'update', 'reset', 'error'
 */
export class CPPRoute extends Route {

    constructor() {
        super();
        this.reset();
    }

    update({ path, turnPoints, distance, time, bbox,
             deadheadDistanceKm, efficiencyPercent,
             edgeCount, nodeCount, profile }) {
        // Call parent update for standard fields
        super.update({ path, turnPoints, distance, time, bbox });

        // CPP-specific fields
        this.deadheadDistanceKm = deadheadDistanceKm || 0;
        this.efficiencyPercent = efficiencyPercent || 100;
        this.edgeCount = edgeCount || 0;
        this.nodeCount = nodeCount || 0;
        this.profile = profile || 'truck';
    }

    reset() {
        super.reset();
        this.deadheadDistanceKm = 0;
        this.efficiencyPercent = 100;
        this.edgeCount = 0;
        this.nodeCount = 0;
        this.profile = 'truck';
    }

    /**
     * Total route distance as a human-readable string.
     */
    get distanceText() {
        return this._formatDistance(this.distance);
    }

    /**
     * Deadhead distance as a human-readable string.
     */
    get deadheadText() {
        return this._formatDistance(this.deadheadDistanceKm * 1000);
    }

    /**
     * Summary string for the route header.
     */
    get summaryText() {
        let eff = this.efficiencyPercent.toFixed(1);
        return _("%s total, %s deadhead (%s%% efficiency)").format(
            this.distanceText, this.deadheadText, eff);
    }

    _formatDistance(meters) {
        if (meters >= 1000) {
            let km = meters / 1000;
            if (km >= 100)
                return _("%.0f km").format(km);
            else
                return _("%.1f km").format(km);
        } else {
            return _("%.0f m").format(meters);
        }
    }
}

GObject.registerClass({}, CPPRoute);
