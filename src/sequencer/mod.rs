//! Sequencer data structures and timing

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

/// Type of channel - determines how playback works
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ChannelType {
    /// Sample-based channel (plays audio files via step sequencer)
    #[default]
    Sampler,
    /// Plugin-based channel (plays MIDI notes through a CLAP plugin)
    Plugin { path: String },
}

/// A channel in the sequencer (e.g., Kick, Snare, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    /// Channel type (sampler or plugin)
    #[serde(default)]
    pub channel_type: ChannelType,
    /// Sample path (only used when channel_type is Sampler)
    pub sample_path: Option<String>,
    pub volume: f32,
    pub muted: bool,
    pub solo: bool,
    /// Plugin parameter values (param_name -> value)
    #[serde(default)]
    pub plugin_params: HashMap<String, f32>,
}

#[allow(dead_code)]
impl Channel {
    /// Create a new sampler channel
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            channel_type: ChannelType::Sampler,
            sample_path: None,
            volume: 1.0,
            muted: false,
            solo: false,
            plugin_params: HashMap::new(),
        }
    }

    /// Create a new plugin channel
    pub fn with_plugin(name: &str, plugin_path: &str) -> Self {
        Self {
            name: name.to_string(),
            channel_type: ChannelType::Plugin {
                path: plugin_path.to_string(),
            },
            sample_path: None,
            volume: 1.0,
            muted: false,
            solo: false,
            plugin_params: HashMap::new(),
        }
    }

    /// Check if this is a plugin channel
    pub fn is_plugin(&self) -> bool {
        matches!(self.channel_type, ChannelType::Plugin { .. })
    }

    /// Get the plugin path if this is a plugin channel
    pub fn plugin_path(&self) -> Option<&str> {
        match &self.channel_type {
            ChannelType::Plugin { path } => Some(path),
            ChannelType::Sampler => None,
        }
    }
}

impl Default for Channel {
    fn default() -> Self {
        Self::new("New Channel")
    }
}

/// A pattern containing step data for all channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: usize,
    pub name: String,
    /// Steps per channel - steps[channel_index][step_index]
    pub steps: Vec<Vec<bool>>,
    /// Notes per channel for piano roll - notes[channel_index] = Vec<Note>
    #[serde(default)]
    pub notes: Vec<Vec<Note>>,
    /// Number of steps in the pattern
    pub length: usize,
}

#[allow(dead_code)]
impl Pattern {
    pub fn new(id: usize, num_channels: usize, length: usize) -> Self {
        Self {
            id,
            name: format!("Pattern {}", id + 1),
            steps: vec![vec![false; length]; num_channels],
            notes: vec![Vec::new(); num_channels],
            length,
        }
    }

    /// Toggle a step on/off
    pub fn toggle_step(&mut self, channel: usize, step: usize) {
        if channel < self.steps.len() && step < self.length {
            self.steps[channel][step] = !self.steps[channel][step];
        }
    }

    /// Get step state
    pub fn get_step(&self, channel: usize, step: usize) -> bool {
        self.steps
            .get(channel)
            .and_then(|ch| ch.get(step))
            .copied()
            .unwrap_or(false)
    }

    /// Set step state
    pub fn set_step(&mut self, channel: usize, step: usize, active: bool) {
        if channel < self.steps.len() && step < self.length {
            self.steps[channel][step] = active;
        }
    }

    /// Get notes for a channel
    pub fn get_notes(&self, channel: usize) -> &[Note] {
        self.notes.get(channel).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get mutable notes for a channel
    pub fn get_notes_mut(&mut self, channel: usize) -> Option<&mut Vec<Note>> {
        self.notes.get_mut(channel)
    }

    /// Add a note to a channel
    pub fn add_note(&mut self, channel: usize, note: Note) {
        if let Some(notes) = self.notes.get_mut(channel) {
            notes.push(note);
        }
    }

    /// Remove a note by ID from a channel
    pub fn remove_note(&mut self, channel: usize, note_id: &str) -> Option<Note> {
        if let Some(notes) = self.notes.get_mut(channel) {
            if let Some(idx) = notes.iter().position(|n| n.id == note_id) {
                return Some(notes.remove(idx));
            }
        }
        None
    }

    /// Find a note at a specific pitch and step
    pub fn get_note_at(&self, channel: usize, pitch: u8, step: usize) -> Option<&Note> {
        self.notes
            .get(channel)?
            .iter()
            .find(|n| n.pitch == pitch && n.covers_step(step))
    }

    /// Find a note starting at a specific pitch and step
    pub fn get_note_starting_at(&self, channel: usize, pitch: u8, step: usize) -> Option<&Note> {
        self.notes
            .get(channel)?
            .iter()
            .find(|n| n.pitch == pitch && n.start_step == step)
    }
}

impl Default for Pattern {
    fn default() -> Self {
        Self::new(0, 8, 16)
    }
}

/// Default channel configuration
pub fn default_channels() -> Vec<Channel> {
    vec![
        Channel::new("Kick"),
        Channel::new("Snare"),
        Channel::new("HiHat"),
        Channel::new("OpenHat"),
        Channel::new("Crash"),
        Channel::new("Tom Hi"),
        Channel::new("Synth 1"),
        Channel::new("Synth 2"),
    ]
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
