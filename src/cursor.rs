//! Panel cursor state structs
//!
//! Each panel that has cursor/viewport state gets its own struct.
//! This reduces the god-object nature of App and groups related state.

use crate::coords::AppCol;

/// Channel rack cursor and viewport state
#[derive(Debug, Clone)]
pub struct ChannelRackCursor {
    /// Currently selected channel index
    pub channel: usize,
    /// Column position (mute zone, sample zone, or step 0-15)
    pub col: AppCol,
    /// First visible row in the viewport
    pub viewport_top: usize,
}

impl Default for ChannelRackCursor {
    fn default() -> Self {
        Self {
            channel: 0,
            col: AppCol::FIRST_STEP,
            viewport_top: 0,
        }
    }
}

/// Piano roll cursor and viewport state
#[derive(Debug, Clone)]
pub struct PianoRollCursor {
    /// Current pitch (MIDI note number)
    pub pitch: u8,
    /// Current step (0-15)
    pub step: usize,
    /// Highest visible pitch in viewport
    pub viewport_top: u8,
    /// If Some, we're placing a note starting at this step
    pub placing_note: Option<usize>,
}

impl Default for PianoRollCursor {
    fn default() -> Self {
        Self {
            pitch: 60, // Middle C (C4)
            step: 0,
            viewport_top: 72, // Around C5
            placing_note: None,
        }
    }
}

/// Playlist cursor and viewport state
#[derive(Debug, Clone)]
pub struct PlaylistCursor {
    /// Current pattern row
    pub row: usize,
    /// Current bar (0 = mute column, 1-16 = bars)
    pub bar: usize,
    /// First visible row in the viewport
    pub viewport_top: usize,
}

impl Default for PlaylistCursor {
    fn default() -> Self {
        Self {
            row: 0,
            bar: 1, // Start on first bar, not mute column
            viewport_top: 0,
        }
    }
}

/// Aggregated cursor states for all panels
#[derive(Debug, Clone, Default)]
pub struct CursorStates {
    /// Channel rack cursor and viewport
    pub channel_rack: ChannelRackCursor,
    /// Piano roll cursor and viewport
    pub piano_roll: PianoRollCursor,
    /// Playlist cursor and viewport
    pub playlist: PlaylistCursor,
}
