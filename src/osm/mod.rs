//! OSM data parsing and processing modules
//!
//! This module provides functionality for parsing OpenStreetMap data
//! from various formats, particularly .osm.pbf files for offline operation.

pub mod serve_parser;

pub use serve_parser::{parse_pbf_in_bbox, parse_pbf_in_polygon, parse_pbf_ways};
