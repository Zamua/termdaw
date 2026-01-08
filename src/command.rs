//! Application command pattern
//!
//! All state mutations go through the command system, enabling:
//! - Centralized undo/redo recording
//! - Testable input handlers
//! - Event logging and debugging

use crate::effects::{EffectParamId, EffectType};
use crate::sequencer::{Channel, Note};

/// Application commands representing all possible state mutations.
///
/// Input handlers produce commands, which are dispatched through App::dispatch().
/// This enables automatic undo recording and keeps mutation logic centralized.
#[derive(Debug, Clone)]
pub enum AppCommand {
    // ========================================================================
    // Transport
    // ========================================================================
    /// Toggle playback (play/pause)
    TogglePlayback,

    /// Stop playback and reset position
    StopPlayback,

    /// Set tempo in BPM
    SetBpm(f64),

    // ========================================================================
    // Pattern selection
    // ========================================================================
    /// Switch to previous pattern
    PreviousPattern,

    /// Switch to next pattern (creates new if at end)
    NextPattern,

    /// Create a new pattern
    CreatePattern,

    /// Delete a pattern by ID
    DeletePattern(usize),

    // ========================================================================
    // Channel operations
    // ========================================================================
    /// Cycle channel's mixer track mute state: normal -> muted -> solo -> normal
    CycleChannelMuteState(usize),

    /// Toggle solo on channel's mixer track
    ToggleSolo(usize),

    /// Delete a channel at slot
    DeleteChannel(usize),

    /// Add/paste a channel at a slot
    AddChannel { slot: usize, channel: Channel },

    /// Set channel's sample path
    SetChannelSample { slot: usize, path: String },

    /// Set channel as plugin
    SetChannelPlugin { slot: usize, path: String },

    /// Set channel's mixer track routing
    SetChannelRouting { slot: usize, track: usize },

    /// Increment channel's mixer track routing (with wrap)
    IncrementChannelRouting(usize),

    /// Decrement channel's mixer track routing (with wrap)
    DecrementChannelRouting(usize),

    // ========================================================================
    // Step grid (channel rack)
    // ========================================================================
    /// Toggle a single step
    ToggleStep {
        channel: usize,
        pattern: usize,
        step: usize,
    },

    /// Set multiple steps (for paste operations)
    SetSteps {
        channel: usize,
        pattern: usize,
        steps: Vec<(usize, bool)>,
    },

    /// Clear steps in a range (for delete operations)
    ClearSteps {
        channel: usize,
        pattern: usize,
        start_step: usize,
        end_step: usize,
    },

    /// Batch step operations across multiple channels (for vim yank/paste)
    BatchSetSteps {
        pattern: usize,
        /// Vec of (channel, step, value) tuples
        operations: Vec<(usize, usize, bool)>,
    },

    /// Batch clear steps across multiple channels
    BatchClearSteps {
        pattern: usize,
        /// Vec of (channel, start_step, end_step) tuples
        operations: Vec<(usize, usize, usize)>,
    },

    // ========================================================================
    // Piano roll
    // ========================================================================
    /// Add a note
    AddNote {
        channel: usize,
        pattern: usize,
        note: Note,
    },

    /// Delete a note at position
    DeleteNote {
        channel: usize,
        pattern: usize,
        pitch: u8,
        start_step: usize,
    },

    /// Batch add notes
    BatchAddNotes {
        channel: usize,
        pattern: usize,
        notes: Vec<Note>,
    },

    /// Batch delete notes
    BatchDeleteNotes {
        channel: usize,
        pattern: usize,
        /// Vec of (pitch, start_step) positions
        positions: Vec<(u8, usize)>,
    },

    // ========================================================================
    // Playlist / Arrangement
    // ========================================================================
    /// Place a pattern at a bar
    PlacePattern { pattern_id: usize, bar: usize },

    /// Remove pattern placement
    RemovePlacement { pattern_id: usize, bar: usize },

    /// Toggle pattern mute in arrangement
    TogglePatternMute(usize),

    // ========================================================================
    // Mixer
    // ========================================================================
    /// Set track volume (0.0 - 1.0)
    SetTrackVolume { track: usize, volume: f32 },

    /// Set track pan (-1.0 to 1.0)
    SetTrackPan { track: usize, pan: f32 },

    /// Toggle track mute
    ToggleTrackMute(usize),

    /// Toggle track solo
    ToggleTrackSolo(usize),

    /// Reset track volume to default
    ResetTrackVolume(usize),

    /// Reset track pan to center
    ResetTrackPan(usize),

    // ========================================================================
    // Effects
    // ========================================================================
    /// Add effect to a slot
    AddEffect {
        track: usize,
        slot: usize,
        effect_type: EffectType,
    },

    /// Remove effect from slot
    RemoveEffect { track: usize, slot: usize },

    /// Set effect parameter
    SetEffectParam {
        track: usize,
        slot: usize,
        param: EffectParamId,
        value: f32,
    },

    /// Toggle effect bypass
    ToggleEffectBypass { track: usize, slot: usize },
}

impl AppCommand {
    /// Check if this command should be recorded for undo
    pub fn is_undoable(&self) -> bool {
        match self {
            // Transport commands are not undoable
            AppCommand::TogglePlayback | AppCommand::StopPlayback => false,

            // Everything else is undoable
            _ => true,
        }
    }

    /// Get a short description for logging/debugging
    pub fn description(&self) -> &'static str {
        match self {
            AppCommand::TogglePlayback => "toggle playback",
            AppCommand::StopPlayback => "stop playback",
            AppCommand::SetBpm(_) => "set tempo",
            AppCommand::PreviousPattern => "previous pattern",
            AppCommand::NextPattern => "next pattern",
            AppCommand::CreatePattern => "create pattern",
            AppCommand::DeletePattern(_) => "delete pattern",
            AppCommand::CycleChannelMuteState(_) => "cycle mute state",
            AppCommand::ToggleSolo(_) => "toggle solo",
            AppCommand::DeleteChannel(_) => "delete channel",
            AppCommand::AddChannel { .. } => "add channel",
            AppCommand::SetChannelSample { .. } => "set channel sample",
            AppCommand::SetChannelPlugin { .. } => "set channel plugin",
            AppCommand::SetChannelRouting { .. } => "set channel routing",
            AppCommand::IncrementChannelRouting(_) => "increment routing",
            AppCommand::DecrementChannelRouting(_) => "decrement routing",
            AppCommand::ToggleStep { .. } => "toggle step",
            AppCommand::SetSteps { .. } => "set steps",
            AppCommand::ClearSteps { .. } => "clear steps",
            AppCommand::BatchSetSteps { .. } => "batch set steps",
            AppCommand::BatchClearSteps { .. } => "batch clear steps",
            AppCommand::AddNote { .. } => "add note",
            AppCommand::DeleteNote { .. } => "delete note",
            AppCommand::BatchAddNotes { .. } => "batch add notes",
            AppCommand::BatchDeleteNotes { .. } => "batch delete notes",
            AppCommand::PlacePattern { .. } => "place pattern",
            AppCommand::RemovePlacement { .. } => "remove placement",
            AppCommand::TogglePatternMute(_) => "toggle pattern mute",
            AppCommand::SetTrackVolume { .. } => "set track volume",
            AppCommand::SetTrackPan { .. } => "set track pan",
            AppCommand::ToggleTrackMute(_) => "toggle track mute",
            AppCommand::ToggleTrackSolo(_) => "toggle track solo",
            AppCommand::ResetTrackVolume(_) => "reset track volume",
            AppCommand::ResetTrackPan(_) => "reset track pan",
            AppCommand::AddEffect { .. } => "add effect",
            AppCommand::RemoveEffect { .. } => "remove effect",
            AppCommand::SetEffectParam { .. } => "set effect param",
            AppCommand::ToggleEffectBypass { .. } => "toggle effect bypass",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_not_undoable() {
        assert!(!AppCommand::TogglePlayback.is_undoable());
        assert!(!AppCommand::StopPlayback.is_undoable());
    }

    #[test]
    fn test_mutations_are_undoable() {
        assert!(AppCommand::ToggleStep {
            channel: 0,
            pattern: 0,
            step: 0
        }
        .is_undoable());
        assert!(AppCommand::SetBpm(120.0).is_undoable());
        assert!(AppCommand::DeleteChannel(0).is_undoable());
    }

    #[test]
    fn test_description() {
        assert_eq!(AppCommand::TogglePlayback.description(), "toggle playback");
        assert_eq!(
            AppCommand::ToggleStep {
                channel: 0,
                pattern: 0,
                step: 0
            }
            .description(),
            "toggle step"
        );
    }
}
