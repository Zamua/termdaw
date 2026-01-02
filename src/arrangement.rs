//! Arrangement data structures for the playlist

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

/// A pattern placement in the arrangement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternPlacement {
    /// Unique identifier
    pub id: String,
    /// Which pattern this placement refers to
    pub pattern_id: usize,
    /// Starting bar position (0-15)
    pub start_bar: usize,
    /// Length in bars (currently always 1)
    pub length: usize,
}

impl PatternPlacement {
    /// Create a new placement with auto-generated ID
    pub fn new(pattern_id: usize, start_bar: usize) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            pattern_id,
            start_bar,
            length: 1,
        }
    }

    /// Check if this placement covers a given bar
    pub fn covers_bar(&self, bar: usize) -> bool {
        bar >= self.start_bar && bar < self.start_bar + self.length
    }
}

/// The arrangement containing all pattern placements
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Arrangement {
    /// All pattern placements
    pub placements: Vec<PatternPlacement>,
    /// Set of muted pattern IDs
    #[serde(default)]
    pub muted_patterns: HashSet<usize>,
    /// Set of soloed pattern IDs
    #[serde(default)]
    pub soloed_patterns: HashSet<usize>,
}

impl Arrangement {
    /// Create a new empty arrangement
    pub fn new() -> Self {
        Self::default()
    }

    /// Get placement at a specific pattern and bar
    pub fn get_placement_at(&self, pattern_id: usize, bar: usize) -> Option<&PatternPlacement> {
        self.placements
            .iter()
            .find(|p| p.pattern_id == pattern_id && p.covers_bar(bar))
    }

    /// Toggle a placement at pattern/bar (add if missing, remove if present)
    pub fn toggle_placement(&mut self, pattern_id: usize, bar: usize) {
        if let Some(idx) = self
            .placements
            .iter()
            .position(|p| p.pattern_id == pattern_id && p.start_bar == bar)
        {
            self.placements.remove(idx);
        } else {
            self.placements.push(PatternPlacement::new(pattern_id, bar));
        }
    }

    /// Add a placement
    pub fn add_placement(&mut self, placement: PatternPlacement) {
        self.placements.push(placement);
    }

    /// Remove a placement by ID
    pub fn remove_placement(&mut self, placement_id: &str) -> Option<PatternPlacement> {
        if let Some(idx) = self.placements.iter().position(|p| p.id == placement_id) {
            Some(self.placements.remove(idx))
        } else {
            None
        }
    }

    /// Remove all placements for a pattern in a bar range
    pub fn remove_placements_in_range(
        &mut self,
        pattern_id: usize,
        start_bar: usize,
        end_bar: usize,
    ) -> Vec<PatternPlacement> {
        let mut removed = Vec::new();
        self.placements.retain(|p| {
            if p.pattern_id == pattern_id && p.start_bar >= start_bar && p.start_bar <= end_bar {
                removed.push(p.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    /// Cycle pattern state: normal -> muted -> solo -> normal
    pub fn cycle_pattern_state(&mut self, pattern_id: usize) {
        let is_muted = self.muted_patterns.contains(&pattern_id);
        let is_soloed = self.soloed_patterns.contains(&pattern_id);

        if is_soloed {
            // Solo -> Normal
            self.soloed_patterns.remove(&pattern_id);
        } else if is_muted {
            // Muted -> Solo
            self.muted_patterns.remove(&pattern_id);
            self.soloed_patterns.insert(pattern_id);
        } else {
            // Normal -> Muted
            self.muted_patterns.insert(pattern_id);
        }
    }

    /// Toggle mute state for a pattern (legacy - use cycle_pattern_state instead)
    pub fn toggle_pattern_mute(&mut self, pattern_id: usize) {
        if self.muted_patterns.contains(&pattern_id) {
            self.muted_patterns.remove(&pattern_id);
        } else {
            self.muted_patterns.insert(pattern_id);
        }
    }

    /// Check if a pattern is muted
    pub fn is_pattern_muted(&self, pattern_id: usize) -> bool {
        self.muted_patterns.contains(&pattern_id)
    }

    /// Check if a pattern is soloed
    pub fn is_pattern_soloed(&self, pattern_id: usize) -> bool {
        self.soloed_patterns.contains(&pattern_id)
    }

    /// Check if any pattern is soloed
    pub fn has_soloed_patterns(&self) -> bool {
        !self.soloed_patterns.is_empty()
    }

    /// Get all active placements at a given bar (not muted, respecting solo)
    pub fn get_active_placements_at_bar(&self, bar: usize) -> Vec<&PatternPlacement> {
        let has_solo = !self.soloed_patterns.is_empty();

        self.placements
            .iter()
            .filter(|p| {
                if !p.covers_bar(bar) {
                    return false;
                }
                if has_solo {
                    // If any pattern is soloed, only play soloed patterns
                    self.soloed_patterns.contains(&p.pattern_id)
                } else {
                    // Otherwise, play non-muted patterns
                    !self.muted_patterns.contains(&p.pattern_id)
                }
            })
            .collect()
    }
}
