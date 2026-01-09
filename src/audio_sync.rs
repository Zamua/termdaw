//! Audio synchronization coordinator
//!
//! Provides dirty tracking and batched updates to the audio thread.
//! Instead of calling `sync_mixer_to_audio()` after every change,
//! call `mark_mixer_dirty()` and let `flush()` batch updates per frame.

use crate::audio::{AudioHandle, AudioMixerState};
use crate::effects::{EffectParamId, EffectType};
use crate::mixer::{Mixer, NUM_TRACKS};

/// Dirty flags for audio sync batching
#[derive(Debug, Clone, Copy, Default)]
pub struct DirtyFlags {
    /// Mixer volumes, pans, or mutes changed
    pub mixer: bool,
    /// Channel routing changed
    pub routing: bool,
    /// Effects changed (slot, param, or bypass)
    pub effects: bool,
}

impl DirtyFlags {
    /// Check if any flags are set
    pub fn any(&self) -> bool {
        self.mixer || self.routing || self.effects
    }

    /// Clear all flags
    pub fn clear(&mut self) {
        self.mixer = false;
        self.routing = false;
        self.effects = false;
    }
}

/// Pending effect change
#[derive(Debug, Clone)]
pub enum EffectChange {
    /// Set effect type for a slot (None to remove)
    SetSlot {
        track: usize,
        slot: usize,
        effect_type: Option<EffectType>,
    },
    /// Set effect parameter value
    SetParam {
        track: usize,
        slot: usize,
        param_id: EffectParamId,
        value: f32,
    },
    /// Set effect bypass state
    SetEnabled {
        track: usize,
        slot: usize,
        enabled: bool,
    },
}

/// Pending routing change
#[derive(Debug, Clone, Copy)]
pub struct RoutingChange {
    /// Channel (generator) index
    pub channel: usize,
    /// Mixer track index
    pub track: usize,
}

/// Coordinates audio thread synchronization with dirty tracking
#[derive(Debug, Default)]
pub struct AudioSync {
    /// Which categories of state need syncing
    dirty: DirtyFlags,
    /// Pending routing changes
    routing_changes: Vec<RoutingChange>,
    /// Pending effect changes
    effect_changes: Vec<EffectChange>,
}

impl AudioSync {
    /// Create a new AudioSync coordinator
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark mixer state as dirty (volume, pan, mute, solo changes)
    pub fn mark_mixer_dirty(&mut self) {
        self.dirty.mixer = true;
    }

    /// Queue a channel routing change
    pub fn mark_routing_dirty(&mut self, channel: usize, track: usize) {
        self.dirty.routing = true;
        self.routing_changes.push(RoutingChange { channel, track });
    }

    /// Queue an effect slot change (add or remove effect)
    pub fn queue_effect_slot(
        &mut self,
        track: usize,
        slot: usize,
        effect_type: Option<EffectType>,
    ) {
        self.dirty.effects = true;
        self.effect_changes.push(EffectChange::SetSlot {
            track,
            slot,
            effect_type,
        });
    }

    /// Queue an effect parameter change
    pub fn queue_effect_param(
        &mut self,
        track: usize,
        slot: usize,
        param_id: EffectParamId,
        value: f32,
    ) {
        self.dirty.effects = true;
        self.effect_changes.push(EffectChange::SetParam {
            track,
            slot,
            param_id,
            value,
        });
    }

    /// Queue an effect bypass change
    pub fn queue_effect_enabled(&mut self, track: usize, slot: usize, enabled: bool) {
        self.dirty.effects = true;
        self.effect_changes.push(EffectChange::SetEnabled {
            track,
            slot,
            enabled,
        });
    }

    /// Flush all pending changes to the audio thread.
    /// Call this once per frame in App::tick().
    pub fn flush(&mut self, audio: &AudioHandle, mixer: &Mixer) {
        if !self.dirty.any() {
            return;
        }

        // Sync mixer state (volumes, pans, mutes)
        if self.dirty.mixer {
            let state = Self::build_mixer_state(mixer);
            audio.update_mixer_state(state);
        }

        // Sync routing changes
        if self.dirty.routing {
            for change in self.routing_changes.drain(..) {
                audio.set_generator_track(change.channel, change.track);
            }
        }

        // Sync effect changes
        if self.dirty.effects {
            for change in self.effect_changes.drain(..) {
                match change {
                    EffectChange::SetSlot {
                        track,
                        slot,
                        effect_type,
                    } => {
                        audio.set_effect(track, slot, effect_type);
                    }
                    EffectChange::SetParam {
                        track,
                        slot,
                        param_id,
                        value,
                    } => {
                        audio.set_effect_param(track, slot, param_id, value);
                    }
                    EffectChange::SetEnabled {
                        track,
                        slot,
                        enabled,
                    } => {
                        audio.set_effect_enabled(track, slot, enabled);
                    }
                }
            }
        }

        self.dirty.clear();
    }

    /// Build audio mixer state from mixer data
    fn build_mixer_state(mixer: &Mixer) -> AudioMixerState {
        let has_solo = mixer.has_solo();

        let mut state = AudioMixerState {
            track_volumes: [0.0; NUM_TRACKS],
            track_pans: [0.0; NUM_TRACKS],
            track_mutes: [false; NUM_TRACKS],
        };

        for (i, track) in mixer.tracks.iter().enumerate() {
            state.track_volumes[i] = track.volume;
            state.track_pans[i] = track.pan;
            // Effective mute considers solo state
            // Master (track 0) is never muted by solo
            state.track_mutes[i] = track.muted || (has_solo && !track.solo && i != 0);
        }

        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dirty_flags_default() {
        let flags = DirtyFlags::default();
        assert!(!flags.any());
    }

    #[test]
    fn test_dirty_flags_any() {
        let flags = DirtyFlags {
            mixer: true,
            ..Default::default()
        };
        assert!(flags.any());
    }

    #[test]
    fn test_dirty_flags_clear() {
        let mut flags = DirtyFlags {
            mixer: true,
            routing: true,
            effects: true,
        };
        flags.clear();
        assert!(!flags.any());
    }

    #[test]
    fn test_mark_mixer_dirty() {
        let mut sync = AudioSync::new();
        sync.mark_mixer_dirty();
        assert!(sync.dirty.mixer);
        assert!(!sync.dirty.routing);
        assert!(!sync.dirty.effects);
    }

    #[test]
    fn test_mark_routing_dirty() {
        let mut sync = AudioSync::new();
        sync.mark_routing_dirty(0, 1);
        assert!(sync.dirty.routing);
        assert_eq!(sync.routing_changes.len(), 1);
        assert_eq!(sync.routing_changes[0].channel, 0);
        assert_eq!(sync.routing_changes[0].track, 1);
    }

    #[test]
    fn test_queue_effect_slot() {
        let mut sync = AudioSync::new();
        sync.queue_effect_slot(0, 1, Some(EffectType::Filter));
        assert!(sync.dirty.effects);
        assert_eq!(sync.effect_changes.len(), 1);
    }

    #[test]
    fn test_queue_effect_param() {
        let mut sync = AudioSync::new();
        sync.queue_effect_param(0, 1, EffectParamId::FilterCutoff, 0.5);
        assert!(sync.dirty.effects);
        assert_eq!(sync.effect_changes.len(), 1);
    }
}
