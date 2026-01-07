//! View model for channel rack step grid
//!
//! Contains all data needed to render the step grid, extracted from App.
//! This decouples rendering from the App struct for better testability.

// Some fields are kept for completeness/testability even if not currently used in rendering
#![allow(dead_code)]

use crate::app::App;
use crate::coords::AppCol;
use crate::input::vim::{Position, Range};
use crate::mixer::TrackId;

/// Total number of channel slots
const TOTAL_CHANNEL_SLOTS: usize = 99;

/// Data for one channel row in step grid
#[derive(Debug, Clone)]
pub struct ChannelRowView {
    /// Channel slot index (0-98)
    pub slot: usize,
    /// Display name for the channel
    pub name: String,
    /// Mixer track this channel routes to
    pub mixer_track: usize,
    /// Whether this slot has an allocated channel
    pub is_allocated: bool,
    /// Whether this is a plugin channel
    pub is_plugin: bool,
    /// Whether the mixer track is muted
    pub is_muted: bool,
    /// Whether the mixer track is soloed
    pub is_solo: bool,
    /// Step data for current pattern (16 steps)
    pub steps: [bool; 16],
}

/// Complete data to render channel rack step grid
#[derive(Debug, Clone)]
pub struct ChannelRackViewModel {
    /// Visible channel rows
    pub rows: Vec<ChannelRowView>,
    /// First slot index in viewport
    pub viewport_top: usize,
    /// Cursor row (channel slot index)
    pub cursor_row: usize,
    /// Cursor column in app coordinates
    pub cursor_col: AppCol,
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

impl ChannelRackViewModel {
    /// Build view model from App state
    pub fn from_app(app: &App, visible_rows: usize, focused: bool) -> Self {
        let viewport_top = app.cursors.channel_rack.viewport_top;
        let cursor_row = app.cursors.channel_rack.channel;
        let cursor_col = app.cursors.channel_rack.col;
        let current_pattern = app.current_pattern;
        let is_playing = app.is_playing();
        let playhead_step = app.playhead_step();

        // Compute visual selection
        let vim_col: crate::coords::VimCol = cursor_col.into();
        let cursor_pos = Position::new(cursor_row, vim_col.0);
        let selection = app.vim.channel_rack.get_selection(cursor_pos);

        // Build visible rows
        let mut rows = Vec::with_capacity(visible_rows);
        for row_idx in 0..visible_rows {
            let slot = viewport_top + row_idx;
            if slot >= TOTAL_CHANNEL_SLOTS {
                break;
            }

            let row = Self::build_row(app, slot, current_pattern);
            rows.push(row);
        }

        Self {
            rows,
            viewport_top,
            cursor_row,
            cursor_col,
            selection,
            is_focused: focused,
            current_pattern,
            is_playing,
            playhead_step,
        }
    }

    /// Build a single row's data
    fn build_row(app: &App, slot: usize, pattern_id: usize) -> ChannelRowView {
        // Extract channel data if slot is allocated
        let channel_data = app.get_channel_at_slot(slot).map(|c| {
            (
                c.mixer_track,
                c.name.clone(),
                c.is_plugin(),
                c.sample_path().map(|s| s.to_string()),
            )
        });

        let is_allocated = channel_data.is_some();
        let (mixer_track, name, is_plugin) =
            if let Some((mt, n, plugin, sample_path)) = channel_data {
                // Build display name
                let display_name = if plugin || sample_path.is_some() {
                    n.clone()
                } else {
                    format!("Slot {}", slot + 1)
                };
                (mt, display_name, plugin)
            } else {
                (1, format!("Slot {}", slot + 1), false)
            };

        // Get mute/solo from mixer track
        let track_id = TrackId(mixer_track);
        let mixer_track_state = app.mixer.track(track_id);
        let is_muted = mixer_track_state.muted;
        let is_solo = mixer_track_state.solo;

        // Get step data for current pattern
        let steps_vec: Vec<bool> = app
            .get_channel_at_slot(slot)
            .and_then(|c| c.get_pattern(pattern_id))
            .map(|s| s.steps.clone())
            .unwrap_or_else(|| vec![false; 16]);

        let mut steps = [false; 16];
        for (i, &s) in steps_vec.iter().take(16).enumerate() {
            steps[i] = s;
        }

        ChannelRowView {
            slot,
            name,
            mixer_track,
            is_allocated,
            is_plugin,
            is_muted,
            is_solo,
            steps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioHandle;
    use tempfile::TempDir;

    /// Create a test App with dummy audio in a temp directory
    fn create_test_app() -> (App, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_path = temp_dir.path().join("test-project");
        std::fs::create_dir_all(&project_path).expect("Failed to create project dir");

        let audio = AudioHandle::dummy();
        let app = App::new(project_path.to_str().unwrap(), audio);
        (app, temp_dir)
    }

    #[test]
    fn test_view_model_rows_count() {
        // ViewModel should contain the requested number of visible rows
        // (up to TOTAL_CHANNEL_SLOTS)
        let (app, _temp) = create_test_app();
        let vm = ChannelRackViewModel::from_app(&app, 10, true);
        assert_eq!(vm.rows.len(), 10);
    }

    #[test]
    fn test_view_model_respects_viewport() {
        let (mut app, _temp) = create_test_app();
        app.cursors.channel_rack.viewport_top = 5;
        let vm = ChannelRackViewModel::from_app(&app, 10, true);
        assert_eq!(vm.viewport_top, 5);
        assert_eq!(vm.rows[0].slot, 5);
        assert_eq!(vm.rows[9].slot, 14);
    }

    #[test]
    fn test_unallocated_slot_has_default_name() {
        let (mut app, _temp) = create_test_app();
        // Slot 50 should be unallocated in test app
        app.cursors.channel_rack.viewport_top = 50;
        let vm = ChannelRackViewModel::from_app(&app, 1, true);
        assert!(!vm.rows[0].is_allocated);
        assert_eq!(vm.rows[0].name, "Slot 51");
    }
}
