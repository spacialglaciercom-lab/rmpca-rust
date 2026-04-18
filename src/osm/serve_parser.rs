//! OSM PBF parsing for offline route optimization
//!
//! This module provides functionality to parse .osm.pbf files and extract
//! road networks within a bounding box for the CPP optimizer.

use crate::geo::{Coordinate, Way};
use anyhow::{Context, Result};
use osmpbfreader::{OsmPbfReader, OsmObj, NodeId};
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

/// Road types we consider for optimization (highway tags)
const HIGHWAY_TAGS: &[&str] = &[
    "motorway", "trunk", "primary", "secondary", "tertiary",
    "unclassified", "residential", "service", "motorway_link",
    "trunk_link", "primary_link", "secondary_link", "tertiary_link",
    "living_street", "pedestrian", "track", "road",
];

/// Parse a .osm.pbf file and extract ways within a bounding box
///
/// # Arguments
/// * `file_path` - Path to the .osm.pbf file
/// * `bbox` - Bounding box as (west, south, east, north)
///
/// # Returns
/// Vector of Way objects representing road segments within the bbox
pub fn parse_pbf_in_bbox<P: AsRef<Path>>(file_path: P, bbox: (f64, f64, f64, f64)) -> Result<Vec<Way>> {
    let (west, south, east, north) = bbox;

    let file = File::open(&file_path)
        .with_context(|| format!("Failed to open OSM PBF file: {:?}", file_path.as_ref()))?;

    let mut reader = OsmPbfReader::new(file);

    // First pass: collect all nodes within the bounding box
    let mut nodes: HashMap<NodeId, Coordinate> = HashMap::new();

    for obj in reader.iter() {
        match obj? {
            OsmObj::Node(node) => {
                let lat = node.lat();
                let lon = node.lon();

                if lat >= south && lat <= north && lon >= west && lon <= east {
                    nodes.insert(node.id, Coordinate::new(lat, lon));
                }
            }
            _ => {}
        }
    }

    // Second pass: collect ways that have nodes within the bbox
    // Re-open the file since iterator consumes it
    let file = File::open(&file_path)?;
    let mut reader = OsmPbfReader::new(file);

    let mut ways = Vec::new();

    for obj in reader.iter() {
        if let OsmObj::Way(osm_way) = obj? {
            // Filter by highway tag
            if !is_highway(&osm_way.tags) {
                continue;
            }

            // Check if way has nodes within our bbox
            let mut has_bbox_nodes = false;
            let mut way_coords = Vec::new();
            let mut node_ids = Vec::new();

            for node_id in &osm_way.nodes {
                if let Some(coord) = nodes.get(node_id) {
                    has_bbox_nodes = true;
                    way_coords.push(*coord);
                    node_ids.push(node_id.0.to_string());
                }
            }

            if has_bbox_nodes && way_coords.len() >= 2 {
                let tags: HashMap<String, String> = osm_way.tags
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();

                let way = Way::new(osm_way.id.0.to_string(), node_ids, tags)
                    .with_geometry(way_coords);
                ways.push(way);
            }
        }
    }

    Ok(ways)
}

/// Parse a .osm.pbf file and extract ways within a polygon
///
/// Uses a two-pass approach:
/// 1. Computes the bbox from the polygon for initial filtering (performance)
/// 2. Applies point-in-polygon filtering on the nodes
///
/// # Arguments
/// * `file_path` - Path to the .osm.pbf file
/// * `polygon` - Polygon vertices as [(lon, lat), ...]
///
/// # Returns
/// Vector of Way objects representing road segments within the polygon
pub fn parse_pbf_in_polygon<P: AsRef<Path>>(file_path: P, polygon: &[(f64, f64)]) -> Result<Vec<Way>> {
    // Compute bbox from polygon for initial filtering
    if polygon.len() < 3 {
        return Ok(Vec::new()); // Not a valid polygon
    }

    let mut west = f64::INFINITY;
    let mut south = f64::INFINITY;
    let mut east = f64::NEG_INFINITY;
    let mut north = f64::NEG_INFINITY;

    for &(lon, lat) in polygon {
        west = west.min(lon);
        south = south.min(lat);
        east = east.max(lon);
        north = north.max(lat);
    }

    // Add a small buffer to the bbox to ensure polygon edges are captured
    let buffer = 0.001; // ~100m at equator
    let bbox = (west - buffer, south - buffer, east + buffer, north + buffer);

    // First pass: collect all nodes within the expanded bbox
    let file = File::open(&file_path)
        .with_context(|| format!("Failed to open OSM PBF file: {:?}", file_path.as_ref()))?;

    let mut reader = OsmPbfReader::new(file);

    // Collect all nodes in bbox, and also track which nodes are inside polygon
    let mut nodes: HashMap<NodeId, Coordinate> = HashMap::new();
    let mut nodes_in_polygon: HashMap<NodeId, Coordinate> = HashMap::new();

    for obj in reader.iter() {
        match obj? {
            OsmObj::Node(node) => {
                let lat = node.lat();
                let lon = node.lon();

                // Check if in expanded bbox first
                if lat >= bbox.1 && lat <= bbox.3 && lon >= bbox.0 && lon <= bbox.2 {
                    let coord = Coordinate::new(lat, lon);
                    nodes.insert(node.id, coord);
                    // Check if point is inside polygon
                    if point_in_polygon((lon, lat), polygon) {
                        nodes_in_polygon.insert(node.id, coord);
                    }
                }
            }
            _ => {}
        }
    }

    // Second pass: collect ways that have nodes within the polygon
    let file = File::open(&file_path)?;
    let mut reader = OsmPbfReader::new(file);

    let mut ways = Vec::new();

    for obj in reader.iter() {
        if let OsmObj::Way(osm_way) = obj? {
            // Filter by highway tag
            if !is_highway(&osm_way.tags) {
                continue;
            }

            // Check if way has nodes inside the polygon
            let mut way_coords = Vec::new();
            let mut node_ids = Vec::new();
            let mut has_polygon_nodes = false;

            for node_id in &osm_way.nodes {
                if let Some(coord) = nodes_in_polygon.get(node_id) {
                    has_polygon_nodes = true;
                    way_coords.push(*coord);
                    node_ids.push(node_id.0.to_string());
                } else if let Some(coord) = nodes.get(node_id) {
                    // Include nodes from bbox that connect polygon nodes
                    // (for way continuity)
                    way_coords.push(*coord);
                    node_ids.push(node_id.0.to_string());
                }
            }

            // Only include ways that have at least 2 nodes inside the polygon
            // or ways that pass through the polygon
            if has_polygon_nodes && way_coords.len() >= 2 {
                let tags: HashMap<String, String> = osm_way.tags
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();

                let way = Way::new(osm_way.id.0.to_string(), node_ids, tags)
                    .with_geometry(way_coords);
                ways.push(way);
            }
        }
    }

    Ok(ways)
}

/// Parse a .osm.pbf file and extract all ways (without bbox filtering)
///
/// This is useful when the bbox filtering should be done at a different level
/// or when processing entire smaller files.
pub fn parse_pbf_ways<P: AsRef<Path>>(file_path: P) -> Result<(Vec<Way>, HashMap<String, Coordinate>)> {
    let file = File::open(&file_path)
        .with_context(|| format!("Failed to open OSM PBF file: {:?}", file_path.as_ref()))?;

    let mut reader = OsmPbfReader::new(file);

    // First pass: collect all nodes
    let mut nodes: HashMap<String, Coordinate> = HashMap::new();

    for obj in reader.iter() {
        if let OsmObj::Node(node) = obj? {
            let lat = node.lat();
            let lon = node.lon();
            nodes.insert(node.id.0.to_string(), Coordinate::new(lat, lon));
        }
    }

    // Second pass: collect ways
    let file = File::open(&file_path)?;
    let mut reader = OsmPbfReader::new(file);

    let mut ways = Vec::new();

    for obj in reader.iter() {
        if let OsmObj::Way(osm_way) = obj? {
            // Filter by highway tag
            if !is_highway(&osm_way.tags) {
                continue;
            }

            let mut way_coords = Vec::new();
            let mut node_ids = Vec::new();

            for node_id in &osm_way.nodes {
                if let Some(coord) = nodes.get(&node_id.0.to_string()) {
                    way_coords.push(*coord);
                    node_ids.push(node_id.0.to_string());
                }
            }

            if way_coords.len() >= 2 {
                let tags: HashMap<String, String> = osm_way.tags
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();

                let way = Way::new(osm_way.id.0.to_string(), node_ids, tags)
                    .with_geometry(way_coords);
                ways.push(way);
            }
        }
    }

    Ok((ways, nodes))
}

/// Check if a point (lon, lat) is inside a polygon using ray-casting algorithm
/// Polygon coordinates are in [lon, lat] format
fn point_in_polygon(point: (f64, f64), polygon: &[(f64, f64)]) -> bool {
    let (x, y) = point;
    let mut inside = false;
    let n = polygon.len();

    for i in 0..n {
        let j = (i + 1) % n;
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];

        // Check if point is on the edge
        if (xi == x && yi == y) || (xj == x && yj == y) {
            return true;
        }

        // Check crossing
        let intersect = ((yi > y) != (yj > y)) 
            && (x < (xj - xi) * (y - yi) / (yj - yi) + xi);

        if intersect {
            inside = !inside;
        }
    }
    inside
}

/// Check if the tags indicate this is a highway (road)
fn is_highway(tags: &osmpbfreader::Tags) -> bool {
    tags.get("highway")
        .map(|value| {
            let value = value.to_lowercase();
            HIGHWAY_TAGS.contains(&value.as_str()) && value != "construction"
        })
        .unwrap_or(false)
}
