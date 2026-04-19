pub mod abstractions;
pub mod graph;
pub mod hierholzer;
pub mod matching;
pub mod types;

use crate::geo::spatial::coord_distance;
use crate::geo::types::{Coordinate, Way as GeoWay};
use crate::optimizer::abstractions::SpatialProvider;
use crate::optimizer::types::{Node, OptimizationResult, RoutePoint, Way};
use anyhow::Result;
use petgraph::graph::{DiGraph, NodeIndex, EdgeIndex};
use petgraph::visit::EdgeRef;
use std::collections::HashMap;

/// Route optimizer using directed Chinese Postman approach.
///
/// Algorithm:
/// 1. Parse GeoJSON → directed graph (handles one-way streets)
/// 2. Balance in/out degrees to make graph Eulerian
/// 3. Find Eulerian circuit via iterative Hierholzer's
/// 4. Post-process to eliminate unnecessary U-turns
pub struct RouteOptimizer {
    graph: DiGraph<Node, f64>,
    node_index: HashMap<String, NodeIndex>,
    spatial_registry: HashMap<String, Coordinate>,
    augmented_edges: std::collections::HashSet<(NodeIndex, NodeIndex)>,
}

impl RouteOptimizer {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_index: HashMap::new(),
            spatial_registry: HashMap::new(),
            augmented_edges: std::collections::HashSet::new(),
        }
    }

    /// Build directed graph from parsed ways and spatial registry.
    ///
    /// Edge weights are Haversine distances derived from the spatial registry.
    pub fn build_graph(&mut self, ways: &[Way]) -> Result<()> {
        for way in ways {
            let is_oneway = way.tags.get("oneway")
                .map_or(false, |v| v == "yes" || v == "1" || v == "true");

            let mut prev_idx: Option<NodeIndex> = None;
            for node_id in &way.nodes {
                let idx = *self.node_index.entry(node_id.clone())
                    .or_insert_with(|| self.graph.add_node(Node::new(node_id.clone())));

                if let Some(prev) = prev_idx {
                    let prev_id = &self.graph[prev].id;
                    let distance = self.haversine_between(prev_id, node_id);

                    self.add_edge(prev, idx, distance, false);
                    if !is_oneway {
                        self.add_edge(idx, prev, distance, false);
                    }
                }
                prev_idx = Some(idx);
            }
        }
        Ok(())
    }

    /// Build directed graph from geo::types::Way (from PBF parser).
    pub fn build_graph_from_geo_ways(&mut self, ways: &[GeoWay]) -> Result<()> {
        for way in ways {
            let is_oneway = way.is_oneway();

            let mut prev_idx: Option<NodeIndex> = None;
            for node_id in &way.node_ids {
                let idx = *self.node_index.entry(node_id.clone())
                    .or_insert_with(|| self.graph.add_node(Node::new(node_id.clone())));

                if let Some(prev) = prev_idx {
                    let prev_id = &self.graph[prev].id;
                    let distance = self.haversine_between(prev_id, node_id);

                    self.add_edge(prev, idx, distance, false);
                    if !is_oneway {
                        self.add_edge(idx, prev, distance, false);
                    }
                }
                prev_idx = Some(idx);
            }
        }
        Ok(())
    }

    /// Add an edge to the graph, optionally marking it as augmented (added for balancing).
    fn add_edge(&mut self, from: NodeIndex, to: NodeIndex, weight: f64, is_augmented: bool) -> EdgeIndex {
        let edge = self.graph.add_edge(from, to, weight);
        if is_augmented {
            self.augmented_edges.insert((from, to));
        }
        edge
    }

    /// Calculate Haversine distance between two node IDs using the spatial registry.
    fn haversine_between(&self, id_a: &str, id_b: &str) -> f64 {
        match (self.spatial_registry.get(id_a), self.spatial_registry.get(id_b)) {
            (Some(c1), Some(c2)) => coord_distance(c1, c2),
            _ => 0.0,
        }
    }

    /// Balance vertex degrees to make graph Eulerian.
    ///
    /// For directed graphs: in-degree must equal out-degree at every vertex.
    /// Find imbalanced vertices, compute shortest paths between them,
    /// then add minimum-cost augmenting edges.
    pub fn make_eulerian(&mut self) -> Result<()> {
        // Expand supply/demand into individual node entries for matching
        let mut supply_nodes: Vec<NodeIndex> = Vec::new();
        let mut demand_nodes: Vec<NodeIndex> = Vec::new();

        for idx in self.graph.node_indices() {
            let in_deg = self.graph.edges_directed(idx, petgraph::Direction::Incoming).count();
            let out_deg = self.graph.edges_directed(idx, petgraph::Direction::Outgoing).count();
            let diff = out_deg as i64 - in_deg as i64;
            if diff > 0 {
                for _ in 0..diff {
                    supply_nodes.push(idx);
                }
            } else if diff < 0 {
                for _ in 0..(-diff) {
                    demand_nodes.push(idx);
                }
            }
        }

        if supply_nodes.len() != demand_nodes.len() {
            anyhow::bail!(
                "Graph cannot be balanced: supply ({}) != demand ({})",
                supply_nodes.len(), demand_nodes.len()
            );
        }

        if supply_nodes.is_empty() {
            return Ok(());
        }

        // Compute cost matrix: shortest path from each unique supply to each unique demand
        let unique_supply: Vec<NodeIndex> = {
            let mut s: Vec<NodeIndex> = supply_nodes.clone();
            s.sort();
            s.dedup();
            s
        };
        let unique_demand: Vec<NodeIndex> = {
            let mut d: Vec<NodeIndex> = demand_nodes.clone();
            d.sort();
            d.dedup();
            d
        };

        // Dijkstra from each unique supply node, cache distances to all unique demand nodes
        let mut cost: HashMap<NodeIndex, HashMap<NodeIndex, f64>> = HashMap::new();
        for &s_node in &unique_supply {
            let distances = petgraph::algo::dijkstra(
                &self.graph, s_node, None, |e| *e.weight(),
            );
            let mut row: HashMap<NodeIndex, f64> = HashMap::new();
            for &d_node in &unique_demand {
                if let Some(&d) = distances.get(&d_node) {
                    row.insert(d_node, d);
                }
            }
            cost.insert(s_node, row);
        }

        // Greedy matching using the cost matrix
        let mut demand_remaining: HashMap<NodeIndex, i64> = HashMap::new();
        for &d in &demand_nodes {
            *demand_remaining.entry(d).or_insert(0) += 1;
        }

        for &s_node in &supply_nodes {
            if let Some(row) = cost.get(&s_node) {
                // Find cheapest remaining demand
                let mut best_demand = None;
                let mut best_cost = f64::INFINITY;
                for (&d_node, &c) in row.iter() {
                    if let Some(&remaining) = demand_remaining.get(&d_node) {
                        if remaining > 0 && c < best_cost {
                            best_cost = c;
                            best_demand = Some(d_node);
                        }
                    }
                }
                if let Some(d_node) = best_demand {
                    *demand_remaining.get_mut(&d_node).unwrap() -= 1;
                    self.add_edge(s_node, d_node, best_cost, true);
                }
            }
        }

        Ok(())
    }

    /// Find Eulerian circuit using iterative Hierholzer's (directed version).
    pub fn find_circuit(&self, start: &str) -> Result<Vec<NodeIndex>> {
        let start_idx = self.node_index.get(start)
            .ok_or_else(|| anyhow::anyhow!("Start node '{}' not in graph", start))?;
        hierholzer::directed_eulerian_circuit(&self.graph, *start_idx)
    }

    /// Full optimization pipeline from GeoJSON input.
    pub fn optimize(&mut self, geojson: &serde_json::Value) -> Result<OptimizationResult> {
        // 1. Parse GeoJSON into ways + spatial registry
        let parse_result = graph::parse_ways_from_geojson(geojson)?;
        self.spatial_registry = parse_result.spatial_registry;

        // 2. Build directed graph
        self.build_graph(&parse_result.ways)?;

        // 3. Make Eulerian
        self.make_eulerian()?;

        // 4. Find circuit (start from first node)
        let start = parse_result.ways.first()
            .and_then(|w| w.nodes.first())
            .ok_or_else(|| anyhow::anyhow!("No nodes in input"))?;
        let circuit = self.find_circuit(start)?;

        // 5. Convert circuit to route points
        let route: Vec<RoutePoint> = circuit.iter()
            .filter_map(|&idx| {
                let node = &self.graph[idx];
                let coord = self.spatial_registry.get(&node.id)?;
                Some(RoutePoint::with_node_id(coord.lat, coord.lon, &node.id))
            })
            .collect();

        // 6. Sum edge weights along circuit
        let total_distance: f64 = circuit.windows(2)
            .filter_map(|w| {
                let from = w[0];
                let to = w[1];
                self.graph.edges_connecting(from, to).next().map(|e| *e.weight())
            })
            .sum();

        Ok(OptimizationResult::new(route, total_distance / 1000.0))
    }

    /// Set the spatial registry (node ID -> coordinate mapping).
    /// This must be called before build_graph if not using optimize() from GeoJSON.
    pub fn set_spatial_registry(&mut self, registry: HashMap<String, Coordinate>) {
        self.spatial_registry = registry;
    }

    /// Retain only the largest strongly connected component in the graph.
    ///
    /// Road networks from bounding box extracts are typically disconnected
    /// (roads entering/exiting the bbox create dead-ends). The Eulerian
    /// circuit algorithm requires a strongly connected graph.
    fn retain_largest_scc(&mut self) -> Result<()> {
        if self.graph.node_count() == 0 {
            return Ok(());
        }

        // Build reverse adjacency
        let mut reverse_adj: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
        for node in self.graph.node_indices() {
            for neighbor in self.graph.neighbors_directed(node, petgraph::Direction::Outgoing) {
                reverse_adj.entry(neighbor).or_default().push(node);
            }
        }

        // Find all SCCs using forward+reverse BFS
        let mut visited: std::collections::HashSet<NodeIndex> = std::collections::HashSet::new();
        let mut best_scc: std::collections::HashSet<NodeIndex> = std::collections::HashSet::new();

        for start in self.graph.node_indices() {
            if visited.contains(&start) {
                continue;
            }

            // Forward BFS
            let mut forward = std::collections::HashSet::new();
            let mut queue = std::collections::VecDeque::new();
            queue.push_back(start);
            forward.insert(start);
            while let Some(n) = queue.pop_front() {
                for neighbor in self.graph.neighbors_directed(n, petgraph::Direction::Outgoing) {
                    if forward.insert(neighbor) {
                        queue.push_back(neighbor);
                    }
                }
            }

            // Reverse BFS
            let mut backward = std::collections::HashSet::new();
            queue.push_back(start);
            backward.insert(start);
            while let Some(n) = queue.pop_front() {
                if let Some(preds) = reverse_adj.get(&n) {
                    for &pred in preds {
                        if backward.insert(pred) {
                            queue.push_back(pred);
                        }
                    }
                }
            }

            // SCC = intersection
            let scc: std::collections::HashSet<NodeIndex> = forward
                .intersection(&backward)
                .copied()
                .collect();

            for &node in &scc {
                visited.insert(node);
            }

            if scc.len() > best_scc.len() {
                best_scc = scc;
            }
        }

        if best_scc.is_empty() {
            anyhow::bail!("Graph has no strongly connected components");
        }

        eprintln!("DEBUG: Largest SCC has {} nodes out of {}", best_scc.len(), self.graph.node_count());

        // Rebuild graph with only SCC nodes
        let mut new_graph = DiGraph::new();
        let mut old_to_new: HashMap<NodeIndex, NodeIndex> = HashMap::with_capacity(best_scc.len());

        for old_idx in self.graph.node_indices() {
            if best_scc.contains(&old_idx) {
                let node = self.graph[old_idx].clone();
                let new_idx = new_graph.add_node(node);
                old_to_new.insert(old_idx, new_idx);
            }
        }

        for old_idx in self.graph.node_indices() {
            if let Some(&new_from) = old_to_new.get(&old_idx) {
                for edge in self.graph.edges(old_idx) {
                    if let Some(&new_to) = old_to_new.get(&edge.target()) {
                        new_graph.add_edge(new_from, new_to, *edge.weight());
                    }
                }
            }
        }

        self.graph = new_graph;

        // Rebuild node_index
        self.node_index.clear();
        for idx in self.graph.node_indices() {
            self.node_index.insert(self.graph[idx].id.clone(), idx);
        }

        self.augmented_edges.clear();

        Ok(())
    }

    /// Populate spatial registry from geo::types::Way (which has geometry).
    pub fn populate_spatial_registry_from_geo_ways(&mut self, ways: &[GeoWay]) {
        for way in ways {
            for (node_id, coord) in way.node_ids.iter().zip(way.geometry.coordinates.iter()) {
                self.spatial_registry.insert(node_id.clone(), *coord);
            }
        }
    }

    /// Full optimization pipeline from pre-parsed ways.
    /// This is the preferred entry point for offline PBF processing.
    pub fn optimize_with_geo_ways(&mut self, ways: &[GeoWay]) -> Result<OptimizationResult> {
        // 1. Populate spatial registry from way geometries
        self.populate_spatial_registry_from_geo_ways(ways);

        // 2. Build directed graph (uses spatial_registry for edge weights)
        self.build_graph_from_geo_ways(ways)?;

        // Track original graph size for stats BEFORE any modification
        let original_edge_count = self.graph.edge_count();
        let original_node_count = self.graph.node_count();

        // 3. Reduce to largest strongly connected component
        self.retain_largest_scc()?;

        eprintln!("DEBUG: After SCC: {} nodes, {} edges",
            self.graph.node_count(), self.graph.edge_count());

        // Verify strong connectivity
        if let Some(start_check) = self.graph.node_indices().next() {
            let forward = petgraph::algo::dijkstra(&self.graph, start_check, None, |e| *e.weight());
            let reachable = forward.len();
            eprintln!("DEBUG: Reachable from start: {}/{}", reachable, self.graph.node_count());
        }

        // 4. Make Eulerian (adds augmented edges)
        self.make_eulerian()?;

        // Verify balance
        let mut max_imbalance: i64 = 0;
        for idx in self.graph.node_indices() {
            let in_deg = self.graph.edges_directed(idx, petgraph::Direction::Incoming).count();
            let out_deg = self.graph.edges_directed(idx, petgraph::Direction::Outgoing).count();
            let diff = (out_deg as i64 - in_deg as i64).abs();
            if diff > max_imbalance {
                max_imbalance = diff;
            }
        }
        eprintln!("DEBUG: After balancing: {} edges, max imbalance: {}",
            self.graph.edge_count(), max_imbalance);

        // 5. Find circuit (start from first remaining node)
        let start_idx = self.graph.node_indices().next()
            .ok_or_else(|| anyhow::anyhow!("No nodes in graph after filtering"))?;
        let start = &self.graph[start_idx].id;
        let circuit = self.find_circuit(start)?;

        // 5. Convert circuit to route points using spatial registry
        let route: Vec<RoutePoint> = circuit.iter()
            .filter_map(|&idx| {
                let node = &self.graph[idx];
                let coord = self.spatial_registry.get(&node.id)?;
                Some(RoutePoint::with_node_id(coord.lat, coord.lon, &node.id))
            })
            .collect();

        // 6. Calculate total and deadhead distances
        let mut total_distance = 0.0;
        let mut deadhead_distance = 0.0;

        for window in circuit.windows(2) {
            let from = window[0];
            let to = window[1];

            // In an Eulerian circuit, there might be multiple edges between two nodes.
            // We need to check if this specific traversal corresponds to an augmented edge.
            if let Some(edge) = self.graph.edges_connecting(from, to).next() {
                let dist = *edge.weight();
                total_distance += dist;

                if self.augmented_edges.contains(&(from, to)) {
                    deadhead_distance += dist;
                }
            }
        }

        // 7. Build result with stats
        let total_km = total_distance / 1000.0;
        let deadhead_km = deadhead_distance / 1000.0;

        let result = OptimizationResult::new(route, total_km)
            .with_deadhead(deadhead_km)
            .with_graph_size(original_edge_count, original_node_count);

        Ok(result)
    }
}

impl SpatialProvider for RouteOptimizer {
    type Node = Node;

    fn get_coordinate(&self, node: &Self::Node) -> Option<Coordinate> {
        self.spatial_registry.get(&node.id).cloned()
    }

    fn distance(&self, from: &Self::Node, to: &Self::Node) -> Option<f64> {
        let c1 = self.get_coordinate(from)?;
        let c2 = self.get_coordinate(to)?;
        Some(coord_distance(&c1, &c2))
    }
}

impl Default for RouteOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::types::{Coordinate, Way as GeoWay, WayGeometry};
    use std::collections::HashMap;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn geo_way(id: &str, node_ids: &[&str], coords: &[(f64, f64)], oneway: bool) -> GeoWay {
        let mut tags = HashMap::new();
        if oneway {
            tags.insert("oneway".to_string(), "yes".to_string());
        }
        GeoWay {
            id: id.to_string(),
            geometry: WayGeometry {
                coordinates: coords
                    .iter()
                    .map(|&(lat, lon)| Coordinate::new(lat, lon))
                    .collect(),
            },
            node_ids: node_ids.iter().map(|s| s.to_string()).collect(),
            tags,
        }
    }

    fn opt_way(id: &str, node_ids: &[&str], oneway: bool) -> Way {
        let mut w = Way::new(id, node_ids.iter().map(|s| s.to_string()).collect());
        if oneway {
            w = w.with_tag("oneway", "yes");
        }
        w
    }

    fn node_in_deg(opt: &RouteOptimizer, id: &str) -> usize {
        let idx = opt.node_index[id];
        opt.graph
            .edges_directed(idx, petgraph::Direction::Incoming)
            .count()
    }

    fn node_out_deg(opt: &RouteOptimizer, id: &str) -> usize {
        let idx = opt.node_index[id];
        opt.graph
            .edges_directed(idx, petgraph::Direction::Outgoing)
            .count()
    }

    // ── build_graph ──────────────────────────────────────────────────────────

    #[test]
    fn test_build_graph_two_way_creates_bidirectional_edges() {
        let mut opt = RouteOptimizer::new();
        opt.build_graph(&[opt_way("w1", &["A", "B", "C"], false)])
            .unwrap();
        // A–B and B–C, each bidirectional → 4 directed edges
        assert_eq!(opt.graph.node_count(), 3);
        assert_eq!(opt.graph.edge_count(), 4);
        assert_eq!(node_out_deg(&opt, "A"), 1);
        assert_eq!(node_in_deg(&opt, "A"), 1);
    }

    #[test]
    fn test_build_graph_oneway_creates_directed_edges_only() {
        let mut opt = RouteOptimizer::new();
        opt.build_graph(&[opt_way("w1", &["A", "B", "C"], true)])
            .unwrap();
        // A→B and B→C only — no reverse edges
        assert_eq!(opt.graph.node_count(), 3);
        assert_eq!(opt.graph.edge_count(), 2);
        assert_eq!(node_out_deg(&opt, "A"), 1);
        assert_eq!(node_in_deg(&opt, "A"), 0);
        assert_eq!(node_out_deg(&opt, "C"), 0);
        assert_eq!(node_in_deg(&opt, "C"), 1);
    }

    #[test]
    fn test_build_graph_shared_node_deduplicated() {
        let mut opt = RouteOptimizer::new();
        opt.build_graph(&[
            opt_way("w1", &["A", "B"], false),
            opt_way("w2", &["B", "C"], false),
        ])
        .unwrap();
        // B is shared across both ways; must appear exactly once
        assert_eq!(opt.graph.node_count(), 3);
        assert_eq!(opt.node_index.len(), 3);
        // B has edges to/from both A and C
        assert_eq!(node_out_deg(&opt, "B"), 2);
        assert_eq!(node_in_deg(&opt, "B"), 2);
    }

    #[test]
    fn test_build_graph_empty_ways_produces_empty_graph() {
        let mut opt = RouteOptimizer::new();
        opt.build_graph(&[]).unwrap();
        assert_eq!(opt.graph.node_count(), 0);
        assert_eq!(opt.graph.edge_count(), 0);
    }

    // ── make_eulerian ────────────────────────────────────────────────────────

    #[test]
    fn test_make_eulerian_already_balanced_is_noop() {
        // Directed triangle A→B→C→A — every node has in=out=1
        let mut opt = RouteOptimizer::new();
        opt.build_graph(&[
            opt_way("w1", &["A", "B"], true),
            opt_way("w2", &["B", "C"], true),
            opt_way("w3", &["C", "A"], true),
        ])
        .unwrap();
        let edges_before = opt.graph.edge_count();
        opt.make_eulerian().unwrap();
        assert_eq!(opt.graph.edge_count(), edges_before, "no augmenting edges should be added");
        assert!(opt.augmented_edges.is_empty());
    }

    #[test]
    fn test_make_eulerian_empty_graph_succeeds() {
        let mut opt = RouteOptimizer::new();
        opt.make_eulerian().unwrap();
        assert_eq!(opt.graph.edge_count(), 0);
    }

    #[test]
    fn test_make_eulerian_imbalanced_graph_adds_augmenting_edges() {
        // Cycle A→B→C→D→A plus shortcut C→B.
        // C: out=2 (C→D, C→B), in=1 (B→C) → supply
        // B: out=1 (B→C),      in=2 (A→B, C→B) → demand
        let mut opt = RouteOptimizer::new();
        opt.build_graph(&[
            opt_way("w1", &["A", "B"], true),
            opt_way("w2", &["B", "C"], true),
            opt_way("w3", &["C", "D"], true),
            opt_way("w4", &["D", "A"], true),
            opt_way("w5", &["C", "B"], true),
        ])
        .unwrap();
        assert_eq!(node_in_deg(&opt, "B"), 2);
        assert_eq!(node_out_deg(&opt, "B"), 1);

        opt.make_eulerian().unwrap();
        // The function must not panic and must record augmented edges
        assert!(!opt.augmented_edges.is_empty());
    }

    // ── retain_largest_scc ───────────────────────────────────────────────────

    #[test]
    fn test_retain_largest_scc_drops_smaller_component() {
        // Triangle A↔B↔C↔A (3-node SCC) + isolated pair D↔E (2-node SCC)
        let ways = vec![
            geo_way("w1", &["A", "B"], &[(45.500, -73.500), (45.501, -73.501)], false),
            geo_way("w2", &["B", "C"], &[(45.501, -73.501), (45.502, -73.502)], false),
            geo_way("w3", &["C", "A"], &[(45.502, -73.502), (45.500, -73.500)], false),
            geo_way("w4", &["D", "E"], &[(46.000, -74.000), (46.001, -74.001)], false),
        ];
        let mut opt = RouteOptimizer::new();
        opt.populate_spatial_registry_from_geo_ways(&ways);
        opt.build_graph_from_geo_ways(&ways).unwrap();
        opt.retain_largest_scc().unwrap();

        assert_eq!(opt.graph.node_count(), 3, "only the 3-node SCC should remain");
        assert!(opt.node_index.contains_key("A"));
        assert!(opt.node_index.contains_key("B"));
        assert!(opt.node_index.contains_key("C"));
        assert!(!opt.node_index.contains_key("D"));
        assert!(!opt.node_index.contains_key("E"));
    }

    #[test]
    fn test_retain_largest_scc_single_component_unchanged() {
        let ways = vec![
            geo_way("w1", &["A", "B"], &[(45.500, -73.500), (45.501, -73.501)], false),
            geo_way("w2", &["B", "C"], &[(45.501, -73.501), (45.502, -73.502)], false),
            geo_way("w3", &["C", "A"], &[(45.502, -73.502), (45.500, -73.500)], false),
        ];
        let mut opt = RouteOptimizer::new();
        opt.populate_spatial_registry_from_geo_ways(&ways);
        opt.build_graph_from_geo_ways(&ways).unwrap();
        let node_count_before = opt.graph.node_count();
        opt.retain_largest_scc().unwrap();
        assert_eq!(opt.graph.node_count(), node_count_before);
    }

    #[test]
    fn test_retain_largest_scc_empty_graph_is_ok() {
        let mut opt = RouteOptimizer::new();
        opt.retain_largest_scc().unwrap();
        assert_eq!(opt.graph.node_count(), 0);
    }

    #[test]
    fn test_retain_largest_scc_rebuilds_node_index() {
        // After SCC trimming the node_index must only contain surviving nodes
        let ways = vec![
            geo_way("w1", &["A", "B"], &[(45.500, -73.500), (45.501, -73.501)], false),
            geo_way("w2", &["B", "C"], &[(45.501, -73.501), (45.502, -73.502)], false),
            geo_way("w3", &["C", "A"], &[(45.502, -73.502), (45.500, -73.500)], false),
            geo_way("w4", &["X", "Y"], &[(50.000, -80.000), (50.001, -80.001)], false),
        ];
        let mut opt = RouteOptimizer::new();
        opt.populate_spatial_registry_from_geo_ways(&ways);
        opt.build_graph_from_geo_ways(&ways).unwrap();
        assert_eq!(opt.node_index.len(), 5);
        opt.retain_largest_scc().unwrap();
        assert_eq!(opt.node_index.len(), 3);
        // Every key in node_index must map to a valid NodeIndex in the graph
        for (id, &idx) in &opt.node_index {
            assert_eq!(&opt.graph[idx].id, id);
        }
    }

    // ── optimize (GeoJSON pipeline) ──────────────────────────────────────────

    #[test]
    fn test_optimize_eulerian_triangle_returns_closed_circuit() {
        // One-way triangle: already Eulerian, no augmentation needed
        let geojson = serde_json::json!({
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[-73.500,45.500],[-73.501,45.501]]},"properties":{"oneway":"yes"}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[-73.501,45.501],[-73.502,45.502]]},"properties":{"oneway":"yes"}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[-73.502,45.502],[-73.500,45.500]]},"properties":{"oneway":"yes"}}
            ]
        });
        let mut opt = RouteOptimizer::new();
        let result = opt.optimize(&geojson).unwrap();

        assert!(!result.route.is_empty(), "circuit must contain route points");
        let first = &result.route[0];
        let last = result.route.last().unwrap();
        assert!(
            (first.latitude - last.latitude).abs() < 1e-9
                && (first.longitude - last.longitude).abs() < 1e-9,
            "Eulerian circuit must be closed (start == end)"
        );
    }

    #[test]
    fn test_optimize_reports_positive_total_distance() {
        let geojson = serde_json::json!({
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[-73.500,45.500],[-73.600,45.600]]},"properties":{"oneway":"yes"}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[-73.600,45.600],[-73.700,45.700]]},"properties":{"oneway":"yes"}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[-73.700,45.700],[-73.500,45.500]]},"properties":{"oneway":"yes"}}
            ]
        });
        let mut opt = RouteOptimizer::new();
        let result = opt.optimize(&geojson).unwrap();
        assert!(result.total_distance > 0.0);
    }

    #[test]
    fn test_optimize_empty_features_returns_error() {
        let geojson = serde_json::json!({"type":"FeatureCollection","features":[]});
        let mut opt = RouteOptimizer::new();
        assert!(opt.optimize(&geojson).is_err());
    }

    #[test]
    fn test_optimize_two_way_street_circuit_covers_all_edges() {
        // Single bidirectional segment A↔B. Eulerian circuit must traverse both directions.
        let geojson = serde_json::json!({
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[-73.500,45.500],[-73.501,45.501]]},"properties":{}}
            ]
        });
        let mut opt = RouteOptimizer::new();
        let result = opt.optimize(&geojson).unwrap();
        // Route must have at least 3 points: A→B→A
        assert!(result.route.len() >= 3, "bidirectional edge requires both directions in circuit");
    }

    // ── route_between ────────────────────────────────────────────────────────

    #[test]
    fn test_route_between_finds_path_on_linear_graph() {
        let ways = vec![
            geo_way("w1", &["A", "B"], &[(45.500, -73.500), (45.501, -73.501)], false),
            geo_way("w2", &["B", "C"], &[(45.501, -73.501), (45.502, -73.502)], false),
        ];
        let mut opt = RouteOptimizer::new();
        opt.populate_spatial_registry_from_geo_ways(&ways);
        opt.build_graph_from_geo_ways(&ways).unwrap();

        let from = Coordinate::new(45.500, -73.500); // snaps to A
        let to = Coordinate::new(45.502, -73.502);   // snaps to C
        let result = opt.route_between(&from, &to).unwrap();

        assert!(!result.path.is_empty());
        assert!(result.distance_m > 0.0);
        let (end_coord, _) = result.path.last().unwrap();
        assert!((end_coord.lat - 45.502).abs() < 1e-6);
        assert!((end_coord.lon - (-73.502)).abs() < 1e-6);
    }

    #[test]
    fn test_route_between_empty_graph_returns_error() {
        let opt = RouteOptimizer::new();
        let err = opt.route_between(
            &Coordinate::new(45.500, -73.500),
            &Coordinate::new(45.501, -73.501),
        );
        assert!(err.is_err());
    }

    #[test]
    fn test_route_between_no_path_between_disconnected_components() {
        // A→B (oneway) and D→E (oneway, no connection to A/B)
        let ways = vec![
            geo_way("w1", &["A", "B"], &[(45.500, -73.500), (45.501, -73.501)], true),
            geo_way("w2", &["D", "E"], &[(46.000, -74.000), (46.001, -74.001)], true),
        ];
        let mut opt = RouteOptimizer::new();
        opt.populate_spatial_registry_from_geo_ways(&ways);
        opt.build_graph_from_geo_ways(&ways).unwrap();

        // A is nearest to (45.500, -73.500); E is nearest to (46.001, -74.001)
        let err = opt.route_between(
            &Coordinate::new(45.500, -73.500),
            &Coordinate::new(46.001, -74.001),
        );
        assert!(err.is_err(), "no route should exist between disconnected components");
    }

    // ── nearest_node_id ──────────────────────────────────────────────────────

    #[test]
    fn test_nearest_node_id_returns_closest_node() {
        let ways = vec![geo_way(
            "w1",
            &["A", "B"],
            &[(45.500, -73.500), (45.510, -73.510)],
            false,
        )];
        let mut opt = RouteOptimizer::new();
        opt.populate_spatial_registry_from_geo_ways(&ways);

        // Query very close to A
        let nearest = opt.nearest_node_id(&Coordinate::new(45.5001, -73.5001)).unwrap();
        assert_eq!(nearest, "A");

        // Query very close to B
        let nearest = opt.nearest_node_id(&Coordinate::new(45.5099, -73.5099)).unwrap();
        assert_eq!(nearest, "B");
    }

    #[test]
    fn test_nearest_node_id_empty_registry_returns_none() {
        let opt = RouteOptimizer::new();
        assert!(opt.nearest_node_id(&Coordinate::new(45.5, -73.5)).is_none());
    }
}

/// Result of a point-to-point routing query.
pub struct RoutingResult {
    /// Path as (coordinate, node_id) pairs, start → end
    pub path: Vec<(Coordinate, String)>,
    /// Total path distance in metres
    pub distance_m: f64,
}

impl RouteOptimizer {
    /// Return the ID of the node in the spatial registry closest to `coord`.
    pub fn nearest_node_id(&self, coord: &Coordinate) -> Option<&str> {
        self.spatial_registry
            .iter()
            .min_by(|(_, c1), (_, c2)| {
                coord_distance(c1, coord)
                    .partial_cmp(&coord_distance(c2, coord))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(id, _)| id.as_str())
    }

    /// Point-to-point A* routing on the pre-built graph.
    ///
    /// Must be called after `populate_spatial_registry_from_geo_ways` and
    /// `build_graph_from_geo_ways`.
    pub fn route_between(&self, from: &Coordinate, to: &Coordinate) -> Result<RoutingResult> {
        let start_id = self.nearest_node_id(from)
            .ok_or_else(|| anyhow::anyhow!("Graph has no nodes"))?
            .to_string();
        let goal_id = self.nearest_node_id(to)
            .ok_or_else(|| anyhow::anyhow!("Graph has no nodes"))?
            .to_string();

        let start_idx = *self.node_index.get(&start_id)
            .ok_or_else(|| anyhow::anyhow!("Start node not in graph index"))?;
        let goal_idx = *self.node_index.get(&goal_id)
            .ok_or_else(|| anyhow::anyhow!("Goal node not in graph index"))?;

        let goal_coord = *self.spatial_registry.get(&goal_id)
            .ok_or_else(|| anyhow::anyhow!("Goal coordinate not in spatial registry"))?;

        // Borrow fields individually so the closures don't need to capture all of `self`
        let graph = &self.graph;
        let registry = &self.spatial_registry;

        let astar_result = petgraph::algo::astar(
            graph,
            start_idx,
            |n| n == goal_idx,
            |e| *e.weight(),
            |n| {
                registry.get(graph[n].id.as_str())
                    .map(|c| coord_distance(c, &goal_coord))
                    .unwrap_or(0.0)
            },
        );

        match astar_result {
            None => anyhow::bail!(
                "No route found between the two points. \
                 The road network in the PBF file may not connect them."
            ),
            Some((cost, node_path)) => {
                let path: Vec<(Coordinate, String)> = node_path.iter()
                    .filter_map(|&idx| {
                        let id = graph[idx].id.clone();
                        registry.get(&id).map(|&c| (c, id))
                    })
                    .collect();
                Ok(RoutingResult { path, distance_m: cost })
            }
        }
    }
}
