//! Mixer module - FL Studio-style mixer with routing
//!
//! This module is separate from the sequencer (generators).
//! Generators don't know about mixer tracks - the mapping lives here.

pub mod routing;

use serde::{Deserialize, Serialize};

use crate::effects::{EffectSlot, EFFECT_SLOTS};

pub use routing::{
    CycleError, GeneratorRouting, RouteDestination, RoutingGraph, TrackId, MASTER_TRACK, NUM_TRACKS,
};

/// A mixer track (channel strip)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerTrack {
    /// Track name (e.g., "Master", "Drums", "Bass")
    pub name: String,
    /// Volume level (0.0 - 1.0)
    pub volume: f32,
    /// Pan position (-1.0 = full left, 0.0 = center, 1.0 = full right)
    pub pan: f32,
    /// Muted state
    pub muted: bool,
    /// Solo state
    pub solo: bool,
    /// Insert effect slots (8 slots per track)
    #[serde(default = "default_effects")]
    pub effects: [Option<EffectSlot>; EFFECT_SLOTS],
}

fn default_effects() -> [Option<EffectSlot>; EFFECT_SLOTS] {
    [None, None, None, None, None, None, None, None]
}

impl Default for MixerTrack {
    fn default() -> Self {
        Self {
            name: String::new(),
            volume: 0.8,
            pan: 0.0,
            muted: false,
            solo: false,
            effects: default_effects(),
        }
    }
}

impl MixerTrack {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    pub fn master() -> Self {
        Self {
            name: "Master".to_string(),
            volume: 1.0,
            pan: 0.0,
            muted: false,
            solo: false,
            effects: default_effects(),
        }
    }

    /// Get effect in a slot
    #[allow(dead_code)]
    pub fn get_effect(&self, slot: usize) -> Option<&EffectSlot> {
        self.effects.get(slot).and_then(|e| e.as_ref())
    }

    /// Set effect in a slot
    #[allow(dead_code)]
    pub fn set_effect(&mut self, slot: usize, effect: Option<EffectSlot>) {
        if slot < EFFECT_SLOTS {
            self.effects[slot] = effect;
        }
    }
}

/// Stereo peak levels for a track (0.0 - 1.0)
#[derive(Debug, Clone, Copy, Default)]
pub struct StereoLevels {
    pub left: f32,
    pub right: f32,
}

/// The mixer - owns all track state and routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mixer {
    /// All mixer tracks (0 = Master, 1-15 = regular)
    pub tracks: [MixerTrack; NUM_TRACKS],
    /// Routing graph (track-to-track routing and sends)
    pub routing: RoutingGraph,
    /// Generator-to-track routing (which track receives each generator's audio)
    pub generator_routing: GeneratorRouting,
    /// Currently selected track in UI
    #[serde(skip)]
    pub selected_track: usize,
    /// Currently selected effect slot (0-7, 8=pan, 9=volume)
    #[serde(skip)]
    pub selected_effect_slot: usize,
    /// Whether bypass column is selected (vs effect name column)
    #[serde(skip)]
    pub on_bypass_column: bool,
    /// Whether effects chain is focused (vs track selection)
    #[serde(skip)]
    pub effects_focused: bool,
    /// Horizontal scroll offset (first visible track after Master)
    #[serde(skip)]
    pub viewport_offset: usize,
    /// Peak levels per track (updated from audio thread)
    #[serde(skip)]
    pub peak_levels: [StereoLevels; NUM_TRACKS],
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    /// Create a new mixer with default configuration
    pub fn new() -> Self {
        let tracks: [MixerTrack; NUM_TRACKS] = std::array::from_fn(|i| {
            if i == MASTER_TRACK {
                MixerTrack::master()
            } else {
                MixerTrack::new(&format!("Track {}", i))
            }
        });

        // All tracks get generic names (Master, Track1, Track2, etc.)
        // Users can rename them as needed

        Self {
            tracks,
            routing: RoutingGraph::new(),
            generator_routing: GeneratorRouting::new(),
            selected_track: 1, // Start on first non-master track
            selected_effect_slot: 0,
            on_bypass_column: false,
            effects_focused: false,
            viewport_offset: 0,
            peak_levels: [StereoLevels::default(); NUM_TRACKS],
        }
    }

    /// Update peak levels from audio thread (called periodically)
    #[allow(dead_code)]
    pub fn update_peak_levels(&mut self, levels: [StereoLevels; NUM_TRACKS]) {
        self.peak_levels = levels;
    }

    /// Decay peak levels (call each frame for smooth falloff)
    #[allow(dead_code)]
    pub fn decay_peak_levels(&mut self, decay_rate: f32) {
        for level in &mut self.peak_levels {
            level.left = (level.left - decay_rate).max(0.0);
            level.right = (level.right - decay_rate).max(0.0);
        }
    }

    /// Get a track by ID
    pub fn track(&self, id: TrackId) -> &MixerTrack {
        &self.tracks[id.index()]
    }

    /// Get a mutable track by ID
    pub fn track_mut(&mut self, id: TrackId) -> &mut MixerTrack {
        &mut self.tracks[id.index()]
    }

    /// Get the currently selected track
    #[allow(dead_code)]
    pub fn selected(&self) -> &MixerTrack {
        &self.tracks[self.selected_track]
    }

    /// Get the currently selected track mutably
    #[allow(dead_code)]
    pub fn selected_mut(&mut self) -> &mut MixerTrack {
        &mut self.tracks[self.selected_track]
    }

    /// Set volume for a track
    pub fn set_volume(&mut self, track: TrackId, volume: f32) {
        self.tracks[track.index()].volume = volume.clamp(0.0, 1.0);
    }

    /// Set pan for a track
    pub fn set_pan(&mut self, track: TrackId, pan: f32) {
        self.tracks[track.index()].pan = pan.clamp(-1.0, 1.0);
    }

    /// Toggle mute for a track
    pub fn toggle_mute(&mut self, track: TrackId) {
        self.tracks[track.index()].muted = !self.tracks[track.index()].muted;
    }

    /// Toggle solo for a track
    pub fn toggle_solo(&mut self, track: TrackId) {
        self.tracks[track.index()].solo = !self.tracks[track.index()].solo;
    }

    /// Check if any track has solo enabled
    pub fn has_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.solo)
    }

    /// Check if a track should be audible (considering solo state)
    pub fn is_track_audible(&self, track: TrackId) -> bool {
        let t = &self.tracks[track.index()];
        if t.muted {
            return false;
        }
        if self.has_solo() && !t.solo {
            return false;
        }
        true
    }

    /// Set the track routing destination
    #[allow(dead_code)]
    pub fn set_track_route(
        &mut self,
        from: TrackId,
        to: RouteDestination,
    ) -> Result<(), CycleError> {
        self.routing.set_route(from, to)
    }

    /// Route a generator to a mixer track
    #[allow(dead_code)]
    pub fn set_generator_track(&mut self, generator_idx: usize, track: TrackId) {
        self.generator_routing.set(generator_idx, track);
    }

    /// Get which track a generator routes to
    pub fn get_generator_track(&self, generator_idx: usize) -> TrackId {
        self.generator_routing.get(generator_idx)
    }

    /// Auto-assign a generator to a track
    pub fn auto_assign_generator(&mut self, generator_idx: usize) {
        self.generator_routing.auto_assign(generator_idx);
    }

    /// Convert to minimal state for audio thread
    #[allow(dead_code)]
    pub fn to_audio_state(&self) -> AudioMixerState {
        let mut track_volumes = [0.0f32; NUM_TRACKS];
        let mut track_pans = [0.0f32; NUM_TRACKS];
        let mut track_mutes = [false; NUM_TRACKS];

        let has_solo = self.has_solo();

        for (i, track) in self.tracks.iter().enumerate() {
            track_volumes[i] = track.volume;
            track_pans[i] = track.pan;
            // Effective mute considers solo state
            track_mutes[i] = track.muted || (has_solo && !track.solo);
        }

        // Get routing as (from, to) pairs
        let mut routing = Vec::new();
        for i in 0..NUM_TRACKS {
            match self.routing.get_route(TrackId(i)) {
                RouteDestination::Master if i != MASTER_TRACK => {
                    routing.push((i, MASTER_TRACK));
                }
                RouteDestination::Track(target) => {
                    routing.push((i, target.index()));
                }
                _ => {}
            }
        }

        // Get sends as (from, to, amount) tuples
        let mut sends = Vec::new();
        for i in 0..NUM_TRACKS {
            for send in self.routing.get_sends(TrackId(i)) {
                sends.push((i, send.target.index(), send.amount, send.pre_fader));
            }
        }

        AudioMixerState {
            track_volumes,
            track_pans,
            track_mutes,
            processing_order: self.routing.processing_order(),
            routing,
            sends,
        }
    }
}

/// Minimal mixer state for audio thread
///
/// Contains only what's needed for audio processing - no strings, no UI state.
/// Main thread computes this and sends it atomically when routing changes.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AudioMixerState {
    /// Volume per track
    pub track_volumes: [f32; NUM_TRACKS],
    /// Pan per track (-1.0 to 1.0)
    pub track_pans: [f32; NUM_TRACKS],
    /// Effective mute state (includes solo logic)
    pub track_mutes: [bool; NUM_TRACKS],
    /// Pre-computed topological order for processing
    pub processing_order: Vec<usize>,
    /// Main route pairs (from_track, to_track)
    pub routing: Vec<(usize, usize)>,
    /// Send tuples (from_track, to_track, amount, pre_fader)
    pub sends: Vec<(usize, usize, f32, bool)>,
}

impl Default for AudioMixerState {
    fn default() -> Self {
        Self {
            track_volumes: [0.8; NUM_TRACKS],
            track_pans: [0.0; NUM_TRACKS],
            track_mutes: [false; NUM_TRACKS],
            processing_order: (0..NUM_TRACKS).collect(),
            routing: (1..NUM_TRACKS).map(|i| (i, MASTER_TRACK)).collect(),
            sends: Vec::new(),
        }
    }
}

impl AudioMixerState {
    /// Apply pan law to get L/R gains
    /// Uses constant-power pan law
    #[allow(dead_code)]
    pub fn pan_gains(&self, track: usize) -> (f32, f32) {
        let pan = self.track_pans[track];
        // Constant power pan law
        let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4; // 0 to PI/2
        let left = angle.cos();
        let right = angle.sin();
        (left, right)
    }
}
