//! Sequencer data structures and timing
//!
//! Channels are the fundamental unit of sound in the DAW.
//! Each channel owns all its data: source config, mixer routing, and pattern data.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::plugin_host::PluginParamId;

// ============================================================================
// Note (unchanged)
// ============================================================================

/// A note in the piano roll
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// Unique identifier
    pub id: String,
    /// MIDI pitch (0-127, display range typically 36-84 = C2-C6)
    pub pitch: u8,
    /// Starting step (0-15)
    pub start_step: usize,
    /// Duration in steps (1-16)
    pub duration: usize,
    /// Velocity (0.0-1.0)
    #[serde(default = "default_velocity")]
    pub velocity: f32,
}

fn default_velocity() -> f32 {
    0.8
}

#[allow(dead_code)]
impl Note {
    /// Create a new note with auto-generated ID
    pub fn new(pitch: u8, start_step: usize, duration: usize) -> Self {
        Self::with_velocity(pitch, start_step, duration, default_velocity())
    }

    /// Create a new note with custom velocity
    pub fn with_velocity(pitch: u8, start_step: usize, duration: usize, velocity: f32) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            pitch,
            start_step,
            duration,
            velocity: velocity.clamp(0.0, 1.0),
        }
    }

    /// Check if this note covers a given step
    pub fn covers_step(&self, step: usize) -> bool {
        step >= self.start_step && step < self.start_step + self.duration
    }
}

// ============================================================================
// Channel - the new encapsulated model
// ============================================================================

/// Sound source type for a channel
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChannelSource {
    /// Sample-based - plays an audio file via step sequencer
    Sampler {
        #[serde(default)]
        path: Option<String>,
    },
    /// Plugin-based - plays MIDI notes through a CLAP plugin
    Plugin {
        path: String,
        #[serde(default)]
        params: HashMap<PluginParamId, f32>,
    },
}

impl Default for ChannelSource {
    fn default() -> Self {
        Self::Sampler { path: None }
    }
}

/// A channel's sequencer data for a single pattern
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatternSlice {
    /// Step sequencer triggers
    #[serde(default)]
    pub steps: Vec<bool>,
    /// Piano roll notes
    #[serde(default)]
    pub notes: Vec<Note>,
}

impl PatternSlice {
    /// Create a new pattern slice with the given length
    pub fn new(length: usize) -> Self {
        Self {
            steps: vec![false; length],
            notes: Vec::new(),
        }
    }

    /// Toggle a step on/off
    pub fn toggle_step(&mut self, step: usize) {
        if step < self.steps.len() {
            self.steps[step] = !self.steps[step];
        }
    }

    /// Get step state
    pub fn get_step(&self, step: usize) -> bool {
        self.steps.get(step).copied().unwrap_or(false)
    }

    /// Set step state
    pub fn set_step(&mut self, step: usize, active: bool) {
        if step < self.steps.len() {
            self.steps[step] = active;
        }
    }

    /// Add a note
    pub fn add_note(&mut self, note: Note) {
        self.notes.push(note);
    }

    /// Remove a note by ID
    pub fn remove_note(&mut self, note_id: &str) -> Option<Note> {
        if let Some(idx) = self.notes.iter().position(|n| n.id == note_id) {
            Some(self.notes.remove(idx))
        } else {
            None
        }
    }

    /// Find a note at a specific pitch and step
    pub fn get_note_at(&self, pitch: u8, step: usize) -> Option<&Note> {
        self.notes
            .iter()
            .find(|n| n.pitch == pitch && n.covers_step(step))
    }

    /// Find a note starting at a specific pitch and step
    #[allow(dead_code)]
    pub fn get_note_starting_at(&self, pitch: u8, step: usize) -> Option<&Note> {
        self.notes
            .iter()
            .find(|n| n.pitch == pitch && n.start_step == step)
    }
}

fn default_mixer_track() -> usize {
    1
}

/// A channel - fully encapsulated unit of sound in the DAW
///
/// Owns all data related to this channel: source config, mixer routing,
/// and pattern data across all patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    /// Display name
    pub name: String,

    /// UI slot number (0-98) - allows sparse channel storage
    #[serde(default)]
    pub slot: usize,

    /// Sound source (sampler or plugin)
    #[serde(default)]
    pub source: ChannelSource,

    /// Which mixer track this channel routes to (1-15, 0 = master)
    #[serde(default = "default_mixer_track")]
    pub mixer_track: usize,

    /// Sequencer data for each pattern (keyed by pattern ID)
    #[serde(default)]
    pub pattern_data: HashMap<usize, PatternSlice>,
}

#[allow(dead_code)]
impl Channel {
    /// Create a new empty sampler channel at slot 0 with default mixer track
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            slot: 0,
            source: ChannelSource::Sampler { path: None },
            mixer_track: 1,
            pattern_data: HashMap::new(),
        }
    }

    /// Create a new empty sampler channel at a specific slot with a unique mixer track
    pub fn new_at_slot(name: &str, slot: usize, mixer_track: usize) -> Self {
        Self {
            name: name.to_string(),
            slot,
            source: ChannelSource::Sampler { path: None },
            mixer_track,
            pattern_data: HashMap::new(),
        }
    }

    /// Create a new sampler channel with a sample
    pub fn with_sample(name: &str, sample_path: &str) -> Self {
        Self {
            name: name.to_string(),
            slot: 0,
            source: ChannelSource::Sampler {
                path: Some(sample_path.to_string()),
            },
            mixer_track: 1,
            pattern_data: HashMap::new(),
        }
    }

    /// Create a new sampler channel with a sample at a specific slot
    pub fn with_sample_at_slot(
        name: &str,
        sample_path: &str,
        slot: usize,
        mixer_track: usize,
    ) -> Self {
        Self {
            name: name.to_string(),
            slot,
            source: ChannelSource::Sampler {
                path: Some(sample_path.to_string()),
            },
            mixer_track,
            pattern_data: HashMap::new(),
        }
    }

    /// Create a new plugin channel
    pub fn with_plugin(name: &str, plugin_path: &str) -> Self {
        Self {
            name: name.to_string(),
            slot: 0,
            source: ChannelSource::Plugin {
                path: plugin_path.to_string(),
                params: HashMap::new(),
            },
            mixer_track: 1,
            pattern_data: HashMap::new(),
        }
    }

    /// Create a new plugin channel at a specific slot
    pub fn with_plugin_at_slot(
        name: &str,
        plugin_path: &str,
        slot: usize,
        mixer_track: usize,
    ) -> Self {
        Self {
            name: name.to_string(),
            slot,
            source: ChannelSource::Plugin {
                path: plugin_path.to_string(),
                params: HashMap::new(),
            },
            mixer_track,
            pattern_data: HashMap::new(),
        }
    }

    /// Check if this is a plugin channel
    pub fn is_plugin(&self) -> bool {
        matches!(self.source, ChannelSource::Plugin { .. })
    }

    /// Get the plugin path if this is a plugin channel
    pub fn plugin_path(&self) -> Option<&str> {
        match &self.source {
            ChannelSource::Plugin { path, .. } => Some(path),
            ChannelSource::Sampler { .. } => None,
        }
    }

    /// Get the sample path if this is a sampler channel
    pub fn sample_path(&self) -> Option<&str> {
        match &self.source {
            ChannelSource::Sampler { path } => path.as_deref(),
            ChannelSource::Plugin { .. } => None,
        }
    }

    /// Get plugin params (returns empty map for samplers)
    pub fn plugin_params(&self) -> &HashMap<PluginParamId, f32> {
        static EMPTY: std::sync::LazyLock<HashMap<PluginParamId, f32>> =
            std::sync::LazyLock::new(HashMap::new);
        match &self.source {
            ChannelSource::Plugin { params, .. } => params,
            ChannelSource::Sampler { .. } => &EMPTY,
        }
    }

    /// Get mutable plugin params (returns None for samplers)
    pub fn plugin_params_mut(&mut self) -> Option<&mut HashMap<PluginParamId, f32>> {
        match &mut self.source {
            ChannelSource::Plugin { params, .. } => Some(params),
            ChannelSource::Sampler { .. } => None,
        }
    }

    /// Get or create pattern data for a pattern
    pub fn get_or_create_pattern(&mut self, pattern_id: usize, length: usize) -> &mut PatternSlice {
        self.pattern_data
            .entry(pattern_id)
            .or_insert_with(|| PatternSlice::new(length))
    }

    /// Get pattern data (read-only)
    pub fn get_pattern(&self, pattern_id: usize) -> Option<&PatternSlice> {
        self.pattern_data.get(&pattern_id)
    }

    /// Get pattern data (mutable)
    pub fn get_pattern_mut(&mut self, pattern_id: usize) -> Option<&mut PatternSlice> {
        self.pattern_data.get_mut(&pattern_id)
    }

    /// Ensure pattern data exists for a pattern
    pub fn ensure_pattern(&mut self, pattern_id: usize, length: usize) {
        self.pattern_data
            .entry(pattern_id)
            .or_insert_with(|| PatternSlice::new(length));
    }

    /// Remove pattern data for a pattern
    pub fn remove_pattern(&mut self, pattern_id: usize) {
        self.pattern_data.remove(&pattern_id);
    }
}

impl Default for Channel {
    fn default() -> Self {
        Self::new("New Channel")
    }
}

// ============================================================================
// Pattern - now just metadata
// ============================================================================

/// Pattern metadata (channel data is now stored in Channel)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: usize,
    pub name: String,
    /// Number of steps in the pattern
    pub length: usize,
}

#[allow(dead_code)]
impl Pattern {
    pub fn new(id: usize, length: usize) -> Self {
        Self {
            id,
            name: format!("Pattern {}", id + 1),
            length,
        }
    }
}

impl Default for Pattern {
    fn default() -> Self {
        Self::new(0, 16)
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Default channel configuration - returns empty vec for clean project start
pub fn default_channels() -> Vec<Channel> {
    Vec::new()
}

/// Default patterns
#[allow(dead_code)]
pub fn default_patterns() -> Vec<Pattern> {
    vec![Pattern::new(0, 16)]
}

// ============================================================================
// Yanked data types for vim registers
// ============================================================================

/// Yanked note data for piano roll copy/paste
/// Uses relative offsets so paste works at any position
#[derive(Debug, Clone)]
pub struct YankedNote {
    /// Offset from the anchor pitch (can be negative)
    pub pitch_offset: i32,
    /// Offset from the anchor step
    pub step_offset: i32,
    /// Note duration
    pub duration: usize,
}

/// Yanked placement data for playlist copy/paste
/// Uses relative bar offset so paste works at any position
#[derive(Debug, Clone)]
pub struct YankedPlacement {
    /// Offset from the anchor bar
    pub bar_offset: i32,
    /// The pattern ID being placed
    pub pattern_id: usize,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Bug fix: On launch, we should not prefill empty channels.
    /// default_channels() should return an empty vec so new projects start clean.
    #[test]
    fn test_default_channels_returns_empty() {
        let channels = default_channels();
        assert!(
            channels.is_empty(),
            "default_channels() should return empty vec, got {} channels",
            channels.len()
        );
    }

    #[test]
    fn test_channel_new_creates_empty_sampler() {
        let channel = Channel::new("Test");
        assert_eq!(channel.name, "Test");
        assert!(matches!(
            channel.source,
            ChannelSource::Sampler { path: None }
        ));
        assert!(channel.pattern_data.is_empty());
    }

    #[test]
    fn test_channel_with_sample() {
        let channel = Channel::with_sample("Kick", "kick.wav");
        assert_eq!(channel.name, "Kick");
        assert_eq!(channel.sample_path(), Some("kick.wav"));
    }

    #[test]
    fn test_pattern_slice_step_operations() {
        let mut slice = PatternSlice::new(16);
        assert!(!slice.get_step(0));

        slice.toggle_step(0);
        assert!(slice.get_step(0));

        slice.set_step(0, false);
        assert!(!slice.get_step(0));
    }
}
