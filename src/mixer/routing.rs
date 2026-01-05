//! Routing graph for mixer tracks
//!
//! Pure data structure with no audio/UI dependencies.
//! Handles track-to-track routing and parallel sends.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Number of mixer tracks (including master at index 0)
pub const NUM_TRACKS: usize = 16;

/// Master track is always index 0
pub const MASTER_TRACK: usize = 0;

/// A track identifier (0 = Master, 1-15 = regular tracks)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(pub usize);

impl TrackId {
    pub const MASTER: TrackId = TrackId(MASTER_TRACK);

    #[allow(dead_code)]
    pub fn new(idx: usize) -> Option<Self> {
        if idx < NUM_TRACKS {
            Some(TrackId(idx))
        } else {
            None
        }
    }

    pub fn index(self) -> usize {
        self.0
    }

    #[allow(dead_code)]
    pub fn is_master(self) -> bool {
        self.0 == MASTER_TRACK
    }
}

impl Default for TrackId {
    fn default() -> Self {
        TrackId::MASTER
    }
}

/// Where a track routes its output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteDestination {
    /// Route to master track (default)
    Master,
    /// Route to another track (for buses/groups)
    Track(TrackId),
}

impl Default for RouteDestination {
    fn default() -> Self {
        RouteDestination::Master
    }
}

/// A parallel send from one track to another
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Send {
    /// Target track
    pub target: TrackId,
    /// Send amount (0.0 - 1.0)
    pub amount: f32,
    /// Pre-fader (true) or post-fader (false)
    pub pre_fader: bool,
}

impl Send {
    #[allow(dead_code)]
    pub fn new(target: TrackId, amount: f32) -> Self {
        Self {
            target,
            amount: amount.clamp(0.0, 1.0),
            pre_fader: false,
        }
    }
}

/// Error when routing would create a cycle
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CycleError {
    pub from: TrackId,
    pub to: TrackId,
}

impl std::fmt::Display for CycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Routing track {} to track {} would create a cycle",
            self.from.0, self.to.0
        )
    }
}

impl std::error::Error for CycleError {}

/// Routing graph for mixer tracks
///
/// Tracks routes and sends between tracks. Ensures no cycles exist
/// and can compute topological processing order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingGraph {
    /// Main route destination for each track (track_idx -> destination)
    routes: [RouteDestination; NUM_TRACKS],
    /// Parallel sends for each track
    sends: [Vec<Send>; NUM_TRACKS],
}

impl Default for RoutingGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl RoutingGraph {
    /// Create a new routing graph with default routing (all to master)
    pub fn new() -> Self {
        // Master routes nowhere (it's the final destination)
        // All other tracks route to master by default
        let routes = std::array::from_fn(|i| {
            if i == MASTER_TRACK {
                RouteDestination::Master // Master doesn't route anywhere
            } else {
                RouteDestination::Master
            }
        });

        Self {
            routes,
            sends: std::array::from_fn(|_| Vec::new()),
        }
    }

    /// Get the route destination for a track
    pub fn get_route(&self, track: TrackId) -> RouteDestination {
        self.routes[track.0]
    }

    /// Set the route destination for a track
    ///
    /// Returns error if this would create a cycle.
    /// Master track cannot be rerouted.
    #[allow(dead_code)]
    pub fn set_route(&mut self, from: TrackId, to: RouteDestination) -> Result<(), CycleError> {
        // Master track cannot be rerouted
        if from.is_master() {
            return Ok(());
        }

        // Check for direct self-loop
        if let RouteDestination::Track(target) = to {
            if target == from {
                return Err(CycleError { from, to: target });
            }
        }

        // Temporarily set the route to check for cycles
        let old_route = self.routes[from.0];
        self.routes[from.0] = to;

        if self.has_cycle() {
            // Restore old route
            self.routes[from.0] = old_route;
            let target = match to {
                RouteDestination::Master => TrackId::MASTER,
                RouteDestination::Track(t) => t,
            };
            return Err(CycleError { from, to: target });
        }

        Ok(())
    }

    /// Get sends for a track
    #[allow(dead_code)]
    pub fn get_sends(&self, track: TrackId) -> &[Send] {
        &self.sends[track.0]
    }

    /// Add a send from one track to another
    #[allow(dead_code)]
    pub fn add_send(&mut self, from: TrackId, send: Send) {
        // Don't allow sends from master or sends to self
        if from.is_master() || send.target == from {
            return;
        }
        self.sends[from.0].push(send);
    }

    /// Remove a send by index
    #[allow(dead_code)]
    pub fn remove_send(&mut self, from: TrackId, index: usize) {
        if index < self.sends[from.0].len() {
            self.sends[from.0].remove(index);
        }
    }

    /// Set send amount
    #[allow(dead_code)]
    pub fn set_send_amount(&mut self, from: TrackId, index: usize, amount: f32) {
        if let Some(send) = self.sends[from.0].get_mut(index) {
            send.amount = amount.clamp(0.0, 1.0);
        }
    }

    /// Check if the routing graph has any cycles
    #[allow(dead_code)]
    pub fn has_cycle(&self) -> bool {
        // Use DFS with coloring: 0=white (unvisited), 1=gray (in progress), 2=black (done)
        let mut color = [0u8; NUM_TRACKS];

        fn dfs(graph: &RoutingGraph, node: usize, color: &mut [u8; NUM_TRACKS]) -> bool {
            color[node] = 1; // Gray - currently visiting

            // Check main route
            if let RouteDestination::Track(target) = graph.routes[node] {
                let t = target.0;
                if color[t] == 1 {
                    return true; // Back edge = cycle
                }
                if color[t] == 0 && dfs(graph, t, color) {
                    return true;
                }
            }

            // Also check sends (they contribute to the dependency graph)
            for send in &graph.sends[node] {
                let t = send.target.0;
                if color[t] == 1 {
                    return true;
                }
                if color[t] == 0 && dfs(graph, t, color) {
                    return true;
                }
            }

            color[node] = 2; // Black - done
            false
        }

        for i in 0..NUM_TRACKS {
            if color[i] == 0 && dfs(self, i, &mut color) {
                return true;
            }
        }

        false
    }

    /// Compute topological processing order (leaves first, master last)
    ///
    /// Returns track indices in the order they should be processed.
    /// Tracks that feed into other tracks are processed first.
    #[allow(dead_code)]
    pub fn processing_order(&self) -> Vec<usize> {
        // Build adjacency list (track -> tracks that depend on it)
        let mut dependents: [Vec<usize>; NUM_TRACKS] = std::array::from_fn(|_| Vec::new());
        let mut in_degree = [0usize; NUM_TRACKS];

        for i in 0..NUM_TRACKS {
            // Main route dependency
            match self.routes[i] {
                RouteDestination::Master if i != MASTER_TRACK => {
                    dependents[MASTER_TRACK].push(i);
                    in_degree[i] += 1;
                }
                RouteDestination::Track(target) => {
                    dependents[target.0].push(i);
                    in_degree[i] += 1;
                }
                _ => {}
            }

            // Send dependencies
            for send in &self.sends[i] {
                dependents[send.target.0].push(i);
                in_degree[i] += 1;
            }
        }

        // Kahn's algorithm for topological sort
        // We want leaves first, so we process nodes with in_degree 0
        // But we want the order reversed (process sources before sinks for audio)
        let mut result = Vec::with_capacity(NUM_TRACKS);
        let mut queue: Vec<usize> = (0..NUM_TRACKS).filter(|&i| in_degree[i] == 0).collect();

        while let Some(node) = queue.pop() {
            result.push(node);
            for &dependent in &dependents[node] {
                in_degree[dependent] -= 1;
                if in_degree[dependent] == 0 {
                    queue.push(dependent);
                }
            }
        }

        // Reverse so that sources (generators) come first, master comes last
        result.reverse();
        result
    }

    /// Get all destinations for a track (main route + sends)
    #[allow(dead_code)]
    pub fn all_destinations(&self, track: TrackId) -> Vec<(TrackId, f32, bool)> {
        let mut result = Vec::new();

        // Main route (full level, post-fader conceptually)
        match self.routes[track.0] {
            RouteDestination::Master if !track.is_master() => {
                result.push((TrackId::MASTER, 1.0, false));
            }
            RouteDestination::Track(target) => {
                result.push((target, 1.0, false));
            }
            _ => {}
        }

        // Sends
        for send in &self.sends[track.0] {
            result.push((send.target, send.amount, send.pre_fader));
        }

        result
    }
}

/// Generator routing table (which mixer track receives each generator's audio)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorRouting {
    /// generator_idx -> TrackId
    routing: HashMap<usize, TrackId>,
}

impl Default for GeneratorRouting {
    fn default() -> Self {
        Self::new()
    }
}

impl GeneratorRouting {
    pub fn new() -> Self {
        Self {
            routing: HashMap::new(),
        }
    }

    /// Get the target track for a generator (defaults to track 1 if not set)
    pub fn get(&self, generator_idx: usize) -> TrackId {
        self.routing
            .get(&generator_idx)
            .copied()
            .unwrap_or(TrackId(1)) // Default to track 1 (first non-master)
    }

    /// Set the target track for a generator
    pub fn set(&mut self, generator_idx: usize, track: TrackId) {
        self.routing.insert(generator_idx, track);
    }

    /// Auto-assign a generator to the next available track
    pub fn auto_assign(&mut self, generator_idx: usize) {
        // Find first track >= 1 that has fewest generators routed to it
        let mut counts = [0usize; NUM_TRACKS];
        for &track in self.routing.values() {
            counts[track.0] += 1;
        }

        // Start from track 1 (skip master)
        let best_track = (1..NUM_TRACKS).min_by_key(|&i| counts[i]).unwrap_or(1);

        self.routing.insert(generator_idx, TrackId(best_track));
    }

    /// Export as array for audio thread (avoids HashMap lookups)
    #[allow(dead_code)]
    pub fn to_array(&self, num_generators: usize) -> Vec<usize> {
        (0..num_generators).map(|i| self.get(i).index()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_routing() {
        let graph = RoutingGraph::new();

        // All tracks should route to master by default
        for i in 1..NUM_TRACKS {
            assert_eq!(graph.get_route(TrackId(i)), RouteDestination::Master);
        }
    }

    #[test]
    fn test_cycle_detection() {
        let mut graph = RoutingGraph::new();

        // Track 1 -> Track 2
        graph
            .set_route(TrackId(1), RouteDestination::Track(TrackId(2)))
            .unwrap();

        // Track 2 -> Track 3
        graph
            .set_route(TrackId(2), RouteDestination::Track(TrackId(3)))
            .unwrap();

        // Track 3 -> Track 1 should fail (creates cycle)
        let result = graph.set_route(TrackId(3), RouteDestination::Track(TrackId(1)));
        assert!(result.is_err());
    }

    #[test]
    fn test_self_loop_rejected() {
        let mut graph = RoutingGraph::new();

        let result = graph.set_route(TrackId(1), RouteDestination::Track(TrackId(1)));
        assert!(result.is_err());
    }

    #[test]
    fn test_processing_order() {
        let mut graph = RoutingGraph::new();

        // Track 1 -> Track 5 (bus) -> Master
        // Track 2 -> Track 5 (bus)
        // Track 3 -> Master
        graph
            .set_route(TrackId(1), RouteDestination::Track(TrackId(5)))
            .unwrap();
        graph
            .set_route(TrackId(2), RouteDestination::Track(TrackId(5)))
            .unwrap();
        // Track 3 already routes to master
        // Track 5 routes to master by default

        let order = graph.processing_order();

        // Track 1 and 2 must come before Track 5
        let pos_1 = order.iter().position(|&x| x == 1).unwrap();
        let pos_2 = order.iter().position(|&x| x == 2).unwrap();
        let pos_5 = order.iter().position(|&x| x == 5).unwrap();
        let pos_master = order.iter().position(|&x| x == 0).unwrap();

        assert!(pos_1 < pos_5);
        assert!(pos_2 < pos_5);
        assert!(pos_5 < pos_master);
    }
}
