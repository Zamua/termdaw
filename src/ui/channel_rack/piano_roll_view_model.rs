//! View model for piano roll
//!
//! Contains all data needed to render the piano roll, extracted from App.
//! This decouples rendering from the App struct for better testability.

// Some fields are kept for completeness/testability even if not currently used
#![allow(dead_code)]

use crate::app::App;
use crate::input::vim::{Position, Range};
use crate::mixer::TrackId;

/// Minimum visible pitch (C2)
const MIN_PITCH: u8 = 36;
/// Maximum visible pitch (C6)
const MAX_PITCH: u8 = 84;

/// Data for one channel row in the piano roll sidebar
#[derive(Debug, Clone)]
pub struct ChannelSidebarView {
    /// Channel slot index (0-98)
    pub slot: usize,
    /// Display name for the channel
    pub name: String,
    /// Mixer track this channel routes to
    pub mixer_track: usize,
    /// Whether this slot has an allocated channel
    pub is_allocated: bool,
    /// Whether the mixer track is muted
    pub is_muted: bool,
    /// Whether the mixer track is soloed
    pub is_solo: bool,
    /// Whether this is the currently selected channel
    pub is_selected: bool,
}

/// Data for a single note in the piano roll
#[derive(Debug, Clone)]
pub struct NoteView {
    /// MIDI pitch (0-127)
    pub pitch: u8,
    /// Starting step (0-15)
    pub start_step: usize,
    /// Duration in steps
    pub duration: usize,
    /// Note ID for identification
    pub id: String,
}

impl NoteView {
    /// Check if this note covers a specific step
    pub fn covers_step(&self, step: usize) -> bool {
        step >= self.start_step && step < self.start_step + self.duration
    }

    /// Check if this note starts at a specific step
    pub fn is_start(&self, step: usize) -> bool {
        self.start_step == step
    }
}

/// Complete data to render the piano roll
#[derive(Debug, Clone)]
pub struct PianoRollViewModel {
    /// Notes for the current channel's pattern
    pub notes: Vec<NoteView>,
    /// Channel sidebar data (visible channels)
    pub sidebar_channels: Vec<ChannelSidebarView>,
    /// Currently selected channel slot
    pub selected_channel: usize,
    /// First channel slot visible in sidebar
    pub channel_viewport_top: usize,
    /// Cursor pitch (MIDI note number)
    pub cursor_pitch: u8,
    /// Cursor step (0-15)
    pub cursor_step: usize,
    /// Top pitch in the viewport
    pub pitch_viewport_top: u8,
    /// Placing note start step (if in placement mode)
    pub placing_note_start: Option<usize>,
    /// Visual selection range (if any)
    pub selection: Option<Range>,
    /// Whether the panel is focused
    pub is_focused: bool,
    /// Current pattern being displayed
    pub current_pattern: usize,
    /// Whether playback is active
    pub is_playing: bool,
    /// Current playhead step (0-15)
    pub playhead_step: usize,
}

impl PianoRollViewModel {
    /// Build view model from App state
    pub fn from_app(app: &App, visible_rows: usize, focused: bool) -> Self {
        let selected_channel = app.ui.cursors.channel_rack.channel;
        let pattern_id = app.current_pattern;
        let channel_viewport_top = app.ui.cursors.channel_rack.viewport_top;
        let pitch_viewport_top = app.ui.cursors.piano_roll.viewport_top.min(MAX_PITCH);
        let cursor_pitch = app.ui.cursors.piano_roll.pitch;
        let cursor_step = app.ui.cursors.piano_roll.step;
        let placing_note_start = app.ui.cursors.piano_roll.placing_note;
        let is_playing = app.is_playing();
        let playhead_step = app.playhead_step();

        // Get notes from selected channel's pattern
        let notes = app
            .channels
            .get(selected_channel)
            .and_then(|c| c.get_pattern(pattern_id))
            .map(|s| {
                s.notes
                    .iter()
                    .map(|n| NoteView {
                        pitch: n.pitch,
                        start_step: n.start_step,
                        duration: n.duration,
                        id: n.id.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Compute visual selection
        let cursor_row = MAX_PITCH.saturating_sub(cursor_pitch) as usize;
        let cursor_pos = Position::new(cursor_row, cursor_step);
        let selection = app.ui.vim.piano_roll.get_selection(cursor_pos);

        // Build sidebar channel data
        let sidebar_channels =
            Self::build_sidebar_channels(app, visible_rows, channel_viewport_top, selected_channel);

        Self {
            notes,
            sidebar_channels,
            selected_channel,
            channel_viewport_top,
            cursor_pitch,
            cursor_step,
            pitch_viewport_top,
            placing_note_start,
            selection,
            is_focused: focused,
            current_pattern: pattern_id,
            is_playing,
            playhead_step,
        }
    }

    /// Build sidebar channel data
    fn build_sidebar_channels(
        app: &App,
        visible_rows: usize,
        viewport_top: usize,
        selected_channel: usize,
    ) -> Vec<ChannelSidebarView> {
        let mut channels = Vec::with_capacity(visible_rows);

        for row_idx in 0..visible_rows {
            let slot = viewport_top + row_idx;

            let channel_data = app.get_channel_at_slot(slot).map(|c| {
                (
                    c.mixer_track,
                    c.name.clone(),
                    c.is_plugin(),
                    c.sample_path().map(|s| s.to_string()),
                )
            });

            let is_allocated = channel_data.is_some();
            let (mixer_track, name) = if let Some((mt, n, is_plugin, sample_path)) = channel_data {
                let display_name = if is_plugin || sample_path.is_some() {
                    n.clone()
                } else {
                    format!("Slot {}", slot + 1)
                };
                (mt, display_name)
            } else {
                (1, format!("Slot {}", slot + 1))
            };

            // Get mute/solo from mixer track
            let track_id = TrackId(mixer_track);
            let mixer_track_state = app.mixer.track(track_id);

            channels.push(ChannelSidebarView {
                slot,
                name,
                mixer_track,
                is_allocated,
                is_muted: mixer_track_state.muted,
                is_solo: mixer_track_state.solo,
                is_selected: slot == selected_channel,
            });
        }

        channels
    }

    /// Get visible pitch range based on viewport
    pub fn visible_pitch_range(&self, visible_rows: usize) -> impl Iterator<Item = u8> {
        let top = self.pitch_viewport_top;
        (0..visible_rows)
            .map(move |i| top.saturating_sub(i as u8))
            .take_while(|&p| p >= MIN_PITCH)
    }

    /// Check if a pitch is a black key
    pub fn is_black_key(pitch: u8) -> bool {
        matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
    }

    /// Get display name for a pitch (e.g., "C4", "F#3")
    pub fn pitch_name(pitch: u8) -> String {
        const PITCH_NAMES: [&str; 12] = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        let note = PITCH_NAMES[(pitch % 12) as usize];
        let octave = pitch / 12;
        format!("{}{}", note, octave)
    }

    /// Check if a step is in the placing preview range
    pub fn is_placing_preview(&self, pitch: u8, step: usize) -> bool {
        self.placing_note_start.is_some_and(|start| {
            let min = start.min(self.cursor_step);
            let max = start.max(self.cursor_step);
            pitch == self.cursor_pitch && step >= min && step <= max
        })
    }

    /// Convert pitch to vim row (high pitches at top = low row numbers)
    pub fn pitch_to_vim_row(pitch: u8) -> usize {
        MAX_PITCH.saturating_sub(pitch) as usize
    }

    /// Check if there's a note at a given pitch and step
    pub fn note_at(&self, pitch: u8, step: usize) -> Option<&NoteView> {
        self.notes
            .iter()
            .find(|n| n.pitch == pitch && n.covers_step(step))
    }

    /// Check if a note starts at a given pitch and step
    pub fn note_starts_at(&self, pitch: u8, step: usize) -> bool {
        self.notes
            .iter()
            .any(|n| n.pitch == pitch && n.start_step == step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioHandle;
    use tempfile::TempDir;

    fn create_test_app() -> (App, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_path = temp_dir.path().join("test-project");
        std::fs::create_dir_all(&project_path).expect("Failed to create project dir");

        let audio = AudioHandle::dummy();
        let app = App::new(project_path.to_str().unwrap(), audio);
        (app, temp_dir)
    }

    #[test]
    fn test_view_model_basic() {
        let (app, _temp) = create_test_app();
        let vm = PianoRollViewModel::from_app(&app, 10, true);
        assert!(vm.is_focused);
        assert!(vm.notes.is_empty()); // No notes in empty project
    }

    #[test]
    fn test_pitch_helpers() {
        assert!(PianoRollViewModel::is_black_key(61)); // C#
        assert!(!PianoRollViewModel::is_black_key(60)); // C
        assert_eq!(PianoRollViewModel::pitch_name(60), "C5");
        assert_eq!(PianoRollViewModel::pitch_name(61), "C#5");
    }

    #[test]
    fn test_note_view_covers_step() {
        let note = NoteView {
            pitch: 60,
            start_step: 4,
            duration: 4,
            id: "test".to_string(),
        };
        assert!(!note.covers_step(3));
        assert!(note.covers_step(4));
        assert!(note.covers_step(7));
        assert!(!note.covers_step(8));
    }
}
