//! Channel rack input handling
//!
//! Uses vim state machine - just passes keys and executes returned actions

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Panel};
use crate::command::AppCommand;
use crate::coords::{AppCol, VimCol};
use crate::mode::ViewMode;
use crate::plugin_host::params::build_editor_params;
use crate::sequencer::ChannelSource;

use super::common::key_to_vim_char;
use super::vim::{Position, Range, RangeType, VimAction};

/// Handle keyboard input for channel rack
pub fn handle_key(key: KeyEvent, app: &mut App) {
    // Special keys not handled by vim
    match key.code {
        // 'm' to cycle mute state: normal -> muted -> solo -> normal
        KeyCode::Char('m') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            let slot = app.ui.cursors.channel_rack.channel;
            if app.get_channel_at_slot(slot).is_some() {
                app.dispatch(AppCommand::CycleChannelMuteState(slot));
            }
            return;
        }
        // 's' to preview current generator's sample/plugin (hold for plugins)
        KeyCode::Char('s') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Start preview (release is handled at top level of handle_key)
            if !app.ui.is_previewing {
                app.start_preview(app.ui.cursors.channel_rack.channel);
            }
            return;
        }
        // 'S' (shift+s) to toggle solo on current channel's mixer track
        KeyCode::Char('S') => {
            let slot = app.ui.cursors.channel_rack.channel;
            if app.get_channel_at_slot(slot).is_some() {
                app.dispatch(AppCommand::ToggleSolo(slot));
            }
            return;
        }
        // 'i' to open piano roll for current generator
        KeyCode::Char('i') => {
            app.set_view_mode(ViewMode::PianoRoll);
            return;
        }
        // 'p' in sample zone to paste channel from register, otherwise open plugin editor
        KeyCode::Char('p') => {
            let slot = app.ui.cursors.channel_rack.channel;
            if app.cursor_zone() == "sample" {
                // Paste channel from register
                if let Some(channel) = app.ui.channel_register.clone() {
                    // Create a new channel at the current slot with the stored data
                    let mut new_channel = channel;
                    new_channel.slot = slot;
                    // Find next available mixer track
                    let mixer_track = app.find_available_mixer_track();
                    new_channel.mixer_track = mixer_track;
                    app.channels.push(new_channel);
                    // Update mixer routing
                    let channel_idx = app.channels.len() - 1;
                    app.mixer.auto_assign_generator(channel_idx);
                    // Sync routing to audio thread
                    app.audio.set_generator_track(channel_idx, mixer_track);
                    app.mark_dirty();
                }
            } else {
                // Open plugin editor for plugin channels
                let plugin_info = app.get_channel_at_slot(slot).and_then(|channel| {
                    if let ChannelSource::Plugin { .. } = &channel.source {
                        let params = build_editor_params(channel.plugin_params());
                        Some((channel.name.clone(), params))
                    } else {
                        None
                    }
                });
                if let Some((name, params)) = plugin_info {
                    app.ui.plugin_editor.open(slot, &name, params);
                }
            }
            return;
        }
        // '[' to switch to previous pattern
        KeyCode::Char('[') => {
            app.dispatch(AppCommand::PreviousPattern);
            return;
        }
        // ']' to switch to next pattern (or create new)
        KeyCode::Char(']') => {
            app.dispatch(AppCommand::NextPattern);
            return;
        }
        // 'd' in sample zone to delete channel (and yank to register)
        KeyCode::Char('d') if app.cursor_zone() == "sample" => {
            let slot = app.ui.cursors.channel_rack.channel;
            // Store channel in register before deleting (vim-like yank on delete)
            if let Some(channel) = app.get_channel_at_slot(slot) {
                app.ui.channel_register = Some(channel.clone());
            }
            app.dispatch(AppCommand::DeleteChannel(slot));
            return;
        }
        // 'x' or Enter in non-steps zones - zone-aware action
        // In steps zone, let vim handle 'x' (for visual mode delete, counts, etc.)
        KeyCode::Char('x') | KeyCode::Enter if !app.ui.cursors.channel_rack.col.is_step_zone() => {
            let slot = app.ui.cursors.channel_rack.channel;
            if app.ui.cursors.channel_rack.col.is_mute_zone() {
                // Cycle mute state on the mixer track
                if app.get_channel_at_slot(slot).is_some() {
                    app.dispatch(AppCommand::CycleChannelMuteState(slot));
                }
            } else if app.ui.cursors.channel_rack.col.is_track_zone() {
                // Cycle to next mixer track (1-15, wrap around)
                app.dispatch(AppCommand::IncrementChannelRouting(slot));
            } else if app.ui.cursors.channel_rack.col.is_sample_zone() {
                // Open sample browser - record position for Ctrl+O
                let current = app.current_jump_position();
                app.ui.global_jumplist.push(current);
                app.ui
                    .browser
                    .start_selection(app.ui.cursors.channel_rack.channel);
                app.ui.mode.switch_panel(Panel::Browser);
                app.ui.show_browser = true;
            }
            return;
        }
        // '+' or '=' to increment track assignment (when in track zone)
        KeyCode::Char('+') | KeyCode::Char('=')
            if app.ui.cursors.channel_rack.col.is_track_zone() =>
        {
            let slot = app.ui.cursors.channel_rack.channel;
            app.dispatch(AppCommand::IncrementChannelRouting(slot));
            return;
        }
        // '-' to decrement track assignment (when in track zone)
        KeyCode::Char('-') if app.ui.cursors.channel_rack.col.is_track_zone() => {
            let slot = app.ui.cursors.channel_rack.channel;
            app.dispatch(AppCommand::DecrementChannelRouting(slot));
            return;
        }
        // Arrow keys mapped to vim motions
        KeyCode::Left => {
            let vim_col: VimCol = app.ui.cursors.channel_rack.col.into();
            let cursor = Position::new(app.ui.cursors.channel_rack.channel, vim_col.0);
            let actions = app.ui.vim.channel_rack.process_key('h', false, cursor);
            for action in actions {
                execute_vim_action(action, app);
            }
            return;
        }
        KeyCode::Right => {
            let vim_col: VimCol = app.ui.cursors.channel_rack.col.into();
            let cursor = Position::new(app.ui.cursors.channel_rack.channel, vim_col.0);
            let actions = app.ui.vim.channel_rack.process_key('l', false, cursor);
            for action in actions {
                execute_vim_action(action, app);
            }
            return;
        }
        _ => {}
    }

    // Convert crossterm key to char for vim (for j/k/w/b/e/gg/G/v/d/y/c etc)
    let Some((ch, ctrl)) = key_to_vim_char(key) else {
        return;
    };

    // Get current cursor position (convert to vim space)
    let vim_col: VimCol = app.ui.cursors.channel_rack.col.into();
    let cursor = Position::new(app.ui.cursors.channel_rack.channel, vim_col.0);

    // Let vim process the key
    let actions = app.ui.vim.channel_rack.process_key(ch, ctrl, cursor);

    // Execute each action
    for action in actions {
        execute_vim_action(action, app);
    }
}

/// Execute a vim action on the app
fn execute_vim_action(action: VimAction, app: &mut App) {
    match action {
        VimAction::None => {}

        VimAction::MoveCursor(pos) => {
            // Clamp to valid channel range (99 slots)
            app.ui.cursors.channel_rack.channel = pos.row.min(98);
            // Convert vim col back to cursor_col
            app.ui.cursors.channel_rack.col = AppCol::from(VimCol(pos.col)).clamp();

            // Update viewport to keep cursor visible
            // Assume ~15 visible rows (will be recalculated at render time)
            let visible_rows = 15;
            if app.ui.cursors.channel_rack.channel
                >= app.ui.cursors.channel_rack.viewport_top + visible_rows
            {
                app.ui.cursors.channel_rack.viewport_top =
                    app.ui.cursors.channel_rack.channel - visible_rows + 1;
            }
            if app.ui.cursors.channel_rack.channel < app.ui.cursors.channel_rack.viewport_top {
                app.ui.cursors.channel_rack.viewport_top = app.ui.cursors.channel_rack.channel;
            }
        }

        VimAction::Toggle => {
            // Only toggle step if in steps zone
            if app.ui.cursors.channel_rack.col.is_step_zone() {
                // For plugin channels, open piano roll instead of toggling step
                let slot = app.ui.cursors.channel_rack.channel;
                if let Some(channel) = app.get_channel_at_slot(slot) {
                    if matches!(&channel.source, ChannelSource::Plugin { .. }) {
                        app.set_view_mode(ViewMode::PianoRoll);
                        return;
                    }
                }
                // Toggle the step via command dispatch
                let step = app.cursor_step();
                let pattern = app.current_pattern;
                app.dispatch(AppCommand::ToggleStep {
                    channel: slot,
                    pattern,
                    step,
                });
            }
        }

        VimAction::Yank(range) => {
            let data = get_pattern_data(app, &range);
            app.ui.vim.channel_rack.store_yank(data, range.range_type);
        }

        VimAction::Delete(range) => {
            // Store deleted data in register 1 (and shift history) before deleting
            let data = get_pattern_data(app, &range);
            app.ui.vim.channel_rack.store_delete(data, range.range_type);

            // Convert range to BatchClearSteps operations for undo support
            let pattern = app.current_pattern;
            let operations = range_to_clear_operations(app, &range);
            if !operations.is_empty() {
                app.dispatch(AppCommand::BatchClearSteps {
                    pattern,
                    operations,
                });
            }
        }

        VimAction::Paste => {
            paste_at_cursor(app, false);
            app.mark_dirty();
        }

        VimAction::PasteBefore => {
            paste_at_cursor(app, true);
            app.mark_dirty();
        }

        VimAction::SelectionChanged(_range) => {
            // UI will query vim.get_selection() during render
        }

        VimAction::ModeChanged(_mode) => {
            // UI will query vim.mode() during render
        }

        VimAction::Escape => {
            // Could do cleanup here if needed
        }

        VimAction::ScrollViewport(delta) => {
            // Scroll viewport without moving cursor
            let visible_rows = 15usize;
            if delta > 0 {
                // Scroll down
                let max_top = 99usize.saturating_sub(visible_rows);
                app.ui.cursors.channel_rack.viewport_top =
                    (app.ui.cursors.channel_rack.viewport_top + delta as usize).min(max_top);
            } else {
                // Scroll up
                app.ui.cursors.channel_rack.viewport_top = app
                    .ui
                    .cursors
                    .channel_rack
                    .viewport_top
                    .saturating_sub((-delta) as usize);
            }
            // Keep cursor visible
            if app.ui.cursors.channel_rack.channel < app.ui.cursors.channel_rack.viewport_top {
                app.ui.cursors.channel_rack.channel = app.ui.cursors.channel_rack.viewport_top;
            } else if app.ui.cursors.channel_rack.channel
                >= app.ui.cursors.channel_rack.viewport_top + visible_rows
            {
                app.ui.cursors.channel_rack.channel =
                    app.ui.cursors.channel_rack.viewport_top + visible_rows - 1;
            }
        }

        VimAction::NextTab => {
            // Switch to Playlist view and focus it
            // Use set_view_mode() to record position in global jumplist
            app.set_view_mode(ViewMode::Playlist);
            app.ui.mode.switch_panel(Panel::Playlist);
        }

        VimAction::PrevTab => {
            // Switch to Playlist view (only 2 tabs, so same as next)
            // Use set_view_mode() to record position in global jumplist
            app.set_view_mode(ViewMode::Playlist);
            app.ui.mode.switch_panel(Panel::Playlist);
        }

        VimAction::RecordJump => {
            // Record current position in global jumplist before a jump movement (G, gg)
            let current = app.current_jump_position();
            app.ui.global_jumplist.push(current);
        }
    }
}

/// Convert vim column to step index
/// Vim columns 0-1 are metadata zone (no steps), 2-17 are steps 0-15
fn vim_col_to_step(vim_col: usize) -> Option<usize> {
    VimCol(vim_col).to_step()
}

/// Get pattern data for a range (vim coordinates)
fn get_pattern_data(app: &App, range: &Range) -> Vec<Vec<bool>> {
    let (start, end) = range.normalized();
    let mut data = Vec::new();

    let pattern_id = app.current_pattern;
    let pattern_length = app.get_current_pattern().map(|p| p.length).unwrap_or(16);

    for row in start.row..=end.row {
        // Get the channel's pattern slice for this pattern
        let slice = app
            .channels
            .get(row)
            .and_then(|c| c.get_pattern(pattern_id));

        // Convert vim columns to step indices
        let col_start = match range.range_type {
            RangeType::Block => vim_col_to_step(start.col).unwrap_or(0),
            RangeType::Line => 0,
            RangeType::Char if row == start.row => vim_col_to_step(start.col).unwrap_or(0),
            RangeType::Char => 0,
        };
        let col_end = match range.range_type {
            RangeType::Block => {
                vim_col_to_step(end.col).unwrap_or(pattern_length.saturating_sub(1))
            }
            RangeType::Line => pattern_length.saturating_sub(1),
            RangeType::Char if row == end.row => {
                vim_col_to_step(end.col).unwrap_or(pattern_length.saturating_sub(1))
            }
            RangeType::Char => pattern_length.saturating_sub(1),
        };

        // Clamp to valid step range
        let col_start = col_start.min(pattern_length.saturating_sub(1));
        let col_end = col_end.min(pattern_length.saturating_sub(1));

        if col_start <= col_end {
            let row_data: Vec<bool> = (col_start..=col_end)
                .map(|col| slice.map(|s| s.get_step(col)).unwrap_or(false))
                .collect();
            data.push(row_data);
        }
    }

    data
}

/// Convert a vim range to BatchClearSteps operations
/// Returns Vec<(channel, start_step, end_step)>
fn range_to_clear_operations(app: &App, range: &Range) -> Vec<(usize, usize, usize)> {
    let (start, end) = range.normalized();
    let pattern_length = app.get_current_pattern().map(|p| p.length).unwrap_or(16);

    let mut operations = Vec::new();

    for row in start.row..=end.row {
        // Convert vim columns to step indices
        let col_start = match range.range_type {
            RangeType::Block => vim_col_to_step(start.col).unwrap_or(0),
            RangeType::Line => 0,
            RangeType::Char if row == start.row => vim_col_to_step(start.col).unwrap_or(0),
            RangeType::Char => 0,
        };
        let col_end = match range.range_type {
            RangeType::Block => {
                vim_col_to_step(end.col).unwrap_or(pattern_length.saturating_sub(1))
            }
            RangeType::Line => pattern_length.saturating_sub(1),
            RangeType::Char if row == end.row => {
                vim_col_to_step(end.col).unwrap_or(pattern_length.saturating_sub(1))
            }
            RangeType::Char => pattern_length.saturating_sub(1),
        };

        // Clamp to valid step range
        let col_start = col_start.min(pattern_length.saturating_sub(1));
        let col_end = col_end.min(pattern_length.saturating_sub(1));

        if col_start <= col_end && row < app.channels.len() {
            operations.push((row, col_start, col_end));
        }
    }

    operations
}

/// Paste clipboard at cursor position
fn paste_at_cursor(app: &mut App, before: bool) {
    let cursor_row = app.ui.cursors.channel_rack.channel;
    let cursor_col = app.cursor_step(); // Use method to get step index

    // Clone register data to avoid borrow issues
    let paste_data = app.ui.vim.channel_rack.get_register().cloned();

    if let Some(register) = paste_data {
        // Compute dimensions from data
        let height = register.data.len();
        let width = register.data.first().map(|r| r.len()).unwrap_or(0);

        // Calculate paste position based on before/after
        let (paste_row, paste_col) = if before {
            // P: paste before - shift by register dimensions
            (
                cursor_row.saturating_sub(height.saturating_sub(1)),
                cursor_col.saturating_sub(width.saturating_sub(1)),
            )
        } else {
            // p: paste at cursor position
            (cursor_row, cursor_col)
        };

        let pattern_id = app.current_pattern;
        let pattern_length = app.get_current_pattern().map(|p| p.length).unwrap_or(16);
        let num_channels = app.channels.len();

        for (row_offset, row_data) in register.data.iter().enumerate() {
            let target_row = paste_row + row_offset;
            if target_row >= num_channels {
                break;
            }

            // Get the channel's pattern slice
            if let Some(channel) = app.channels.get_mut(target_row) {
                let slice = channel.get_or_create_pattern(pattern_id, pattern_length);
                for (col_offset, &value) in row_data.iter().enumerate() {
                    let target_col = paste_col + col_offset;
                    if target_col < pattern_length {
                        slice.set_step(target_col, value);
                    }
                }
            }
        }
    }
}

// ============================================================================
// Mouse handling
// ============================================================================

use super::mouse::MouseAction;

/// Handle mouse actions for channel rack
///
/// This mirrors the keyboard handler pattern - receives actions from MouseState
/// and executes component-specific behavior.
pub fn handle_mouse_action(action: &MouseAction, app: &mut App) {
    match action {
        MouseAction::Click { x, y, .. } => {
            // Look up which cell was clicked
            if let Some((row, vim_col)) = app.ui.screen_areas.channel_rack_cell_at(*x, *y) {
                // Exit visual mode if active
                if app.ui.vim.channel_rack.is_visual() {
                    let vim_col_current: VimCol = app.ui.cursors.channel_rack.col.into();
                    let cursor =
                        Position::new(app.ui.cursors.channel_rack.channel, vim_col_current.0);
                    let actions = app.ui.vim.channel_rack.process_key('\x1b', false, cursor);
                    for action in actions {
                        execute_vim_action(action, app);
                    }
                }

                // Move cursor to clicked cell
                app.ui.cursors.channel_rack.channel = row.min(98);
                app.ui.cursors.channel_rack.col = AppCol::from(VimCol(vim_col)).clamp();
                update_viewport(app);

                // Handle zone-specific click behavior
                let col = AppCol::from(VimCol(vim_col));
                if col.is_mute_zone() {
                    // Click on mute column - cycle mute state via mixer track
                    if app.get_channel_at_slot(row).is_some() {
                        app.dispatch(AppCommand::CycleChannelMuteState(row));
                    }
                } else if col.is_step_zone() {
                    // Click on step - toggle it (if sampler channel)
                    if let Some(channel) = app.get_channel_at_slot(row) {
                        if matches!(&channel.source, ChannelSource::Plugin { .. }) {
                            // Plugin channels open piano roll on click
                            app.set_view_mode(ViewMode::PianoRoll);
                        } else {
                            // Toggle step for sampler channels via dispatch
                            let step = app.cursor_step();
                            let pattern = app.current_pattern;
                            app.dispatch(AppCommand::ToggleStep {
                                channel: row,
                                pattern,
                                step,
                            });
                        }
                    } else {
                        // Empty slot - toggle anyway via dispatch
                        let step = app.cursor_step();
                        let pattern = app.current_pattern;
                        app.dispatch(AppCommand::ToggleStep {
                            channel: row,
                            pattern,
                            step,
                        });
                    }
                }
                // Sample zone click just moves cursor
            }
        }

        MouseAction::DoubleClick { x, y } => {
            // Double-click to preview sample
            if let Some((row, vim_col)) = app.ui.screen_areas.channel_rack_cell_at(*x, *y) {
                let col = AppCol::from(VimCol(vim_col));
                if col.is_sample_zone() {
                    // Preview the channel on double-click
                    app.start_preview(row);
                }
            }
        }

        MouseAction::DragStart { x, y, .. } => {
            // Start selection drag in step zone
            if let Some((row, vim_col)) = app.ui.screen_areas.channel_rack_cell_at(*x, *y) {
                let col = AppCol::from(VimCol(vim_col));
                if col.is_step_zone() {
                    // Move cursor to start position
                    app.ui.cursors.channel_rack.channel = row.min(98);
                    app.ui.cursors.channel_rack.col = col.clamp();
                    update_viewport(app);

                    // Enter visual block mode
                    let cursor = Position::new(row, vim_col);
                    let actions = app.ui.vim.channel_rack.process_key('v', true, cursor); // Ctrl+v for block
                    for action in actions {
                        execute_vim_action(action, app);
                    }
                }
            }
        }

        MouseAction::DragMove { x, y, .. } => {
            // Extend selection
            if app.ui.vim.channel_rack.is_visual() {
                if let Some((row, vim_col)) = app.ui.screen_areas.channel_rack_cell_at(*x, *y) {
                    // Move cursor to extend selection
                    app.ui.cursors.channel_rack.channel = row.min(98);
                    app.ui.cursors.channel_rack.col = AppCol::from(VimCol(vim_col)).clamp();
                    update_viewport(app);
                }
            }
        }

        MouseAction::DragEnd { .. } => {
            // Selection is complete, vim stays in visual mode
            // User can now press d/y/x to operate on selection
        }

        MouseAction::Scroll { delta, .. } => {
            // Scroll viewport
            if *delta < 0 {
                // Scroll up
                app.ui.cursors.channel_rack.viewport_top =
                    app.ui.cursors.channel_rack.viewport_top.saturating_sub(3);
            } else {
                // Scroll down
                app.ui.cursors.channel_rack.viewport_top =
                    (app.ui.cursors.channel_rack.viewport_top + 3).min(98);
            }
        }

        MouseAction::RightClick { x, y } => {
            // Show context menu for channel rack
            if let Some((row, _vim_col)) = app.ui.screen_areas.channel_rack_cell_at(*x, *y) {
                use crate::ui::context_menu::{channel_rack_menu, MenuContext};

                // Determine channel properties for menu
                let (has_sample, is_plugin) = app
                    .channels
                    .get(row)
                    .map(|channel| {
                        let has_sample = channel.sample_path().is_some();
                        let is_plugin = matches!(&channel.source, ChannelSource::Plugin { .. });
                        (has_sample, is_plugin)
                    })
                    .unwrap_or((false, false));

                let items = channel_rack_menu(has_sample, is_plugin);
                let context = MenuContext::ChannelRack { channel: row };
                app.ui.context_menu.show(*x, *y, items, context);
            }
        }
    }
}

/// Update viewport to keep cursor visible
fn update_viewport(app: &mut App) {
    let visible_rows = 15; // Approximate
    if app.ui.cursors.channel_rack.channel
        >= app.ui.cursors.channel_rack.viewport_top + visible_rows
    {
        app.ui.cursors.channel_rack.viewport_top =
            app.ui.cursors.channel_rack.channel - visible_rows + 1;
    }
    if app.ui.cursors.channel_rack.channel < app.ui.cursors.channel_rack.viewport_top {
        app.ui.cursors.channel_rack.viewport_top = app.ui.cursors.channel_rack.channel;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioHandle;
    use crossterm::event::KeyModifiers;
    use tempfile::TempDir;

    fn create_test_app() -> (App, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_path = temp_dir.path().join("test-project");
        std::fs::create_dir_all(&project_path).expect("Failed to create project dir");
        let audio = AudioHandle::dummy();
        let app = App::new(project_path.to_str().unwrap(), audio);
        (app, temp_dir)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    // ========================================================================
    // Channel delete/paste tests
    // ========================================================================

    #[test]
    fn test_delete_channel_stores_in_register() {
        let (mut app, _temp) = create_test_app();

        // Add a channel at slot 0 with a sample
        app.set_channel_sample(0, "test.wav".to_string());
        assert_eq!(app.channels.len(), 1, "Should have 1 channel");

        // Move cursor to sample zone of slot 0
        app.ui.cursors.channel_rack.channel = 0;
        app.ui.cursors.channel_rack.col = AppCol::SAMPLE_ZONE;

        // Verify register is empty
        assert!(
            app.ui.channel_register.is_none(),
            "Register should be empty initially"
        );

        // Press 'd' to delete (in sample zone)
        handle_key(key(KeyCode::Char('d')), &mut app);

        // Channel should be deleted
        assert_eq!(app.channels.len(), 0, "Channel should be deleted");

        // Register should contain the channel
        assert!(
            app.ui.channel_register.is_some(),
            "Register should contain deleted channel"
        );
        let stored = app.ui.channel_register.as_ref().unwrap();
        assert_eq!(
            stored.sample_path(),
            Some("test.wav"),
            "Stored channel should have correct sample"
        );
    }

    #[test]
    fn test_paste_channel_from_register() {
        let (mut app, _temp) = create_test_app();

        // Manually set up a channel in the register
        let channel = crate::sequencer::Channel::with_sample("Test Channel", "stored.wav");
        app.ui.channel_register = Some(channel);

        // Move cursor to sample zone of empty slot 0
        app.ui.cursors.channel_rack.channel = 0;
        app.ui.cursors.channel_rack.col = AppCol::SAMPLE_ZONE;

        // Verify no channels exist
        assert_eq!(app.channels.len(), 0, "Should have no channels initially");

        // Press 'p' to paste
        handle_key(key(KeyCode::Char('p')), &mut app);

        // Channel should be created at slot 0
        assert_eq!(app.channels.len(), 1, "Should have 1 channel after paste");
        let pasted = app
            .get_channel_at_slot(0)
            .expect("Channel should exist at slot 0");
        assert_eq!(
            pasted.sample_path(),
            Some("stored.wav"),
            "Pasted channel should have correct sample"
        );
    }

    #[test]
    fn test_delete_then_paste_channel_workflow() {
        let (mut app, _temp) = create_test_app();

        // Add a channel at slot 0
        app.set_channel_sample(0, "original.wav".to_string());

        // Move cursor to sample zone of slot 0
        app.ui.cursors.channel_rack.channel = 0;
        app.ui.cursors.channel_rack.col = AppCol::SAMPLE_ZONE;

        // Delete with 'd'
        handle_key(key(KeyCode::Char('d')), &mut app);
        assert_eq!(app.channels.len(), 0, "Channel should be deleted");

        // Move to slot 1
        app.ui.cursors.channel_rack.channel = 1;

        // Paste with 'p'
        handle_key(key(KeyCode::Char('p')), &mut app);

        // Channel should be created at slot 1
        assert_eq!(app.channels.len(), 1, "Should have 1 channel after paste");
        let pasted = app
            .get_channel_at_slot(1)
            .expect("Channel should exist at slot 1");
        assert_eq!(
            pasted.sample_path(),
            Some("original.wav"),
            "Pasted channel should have the original sample"
        );
    }

    #[test]
    fn test_pasted_channel_can_have_steps_toggled() {
        let (mut app, _temp) = create_test_app();

        // Add a channel at slot 0
        app.set_channel_sample(0, "original.wav".to_string());
        app.patterns.push(crate::sequencer::Pattern::new(0, 16));

        // Delete it (stores in register)
        app.ui.cursors.channel_rack.channel = 0;
        app.ui.cursors.channel_rack.col = AppCol::SAMPLE_ZONE;
        handle_key(key(KeyCode::Char('d')), &mut app);

        // Paste to slot 5 (different from original)
        app.ui.cursors.channel_rack.channel = 5;
        handle_key(key(KeyCode::Char('p')), &mut app);

        // Verify channel exists at slot 5
        assert!(
            app.get_channel_at_slot(5).is_some(),
            "Pasted channel should exist at slot 5"
        );

        // Try to toggle a step on the pasted channel
        app.dispatch(AppCommand::ToggleStep {
            channel: 5, // Using slot, not Vec index
            pattern: 0,
            step: 0,
        });

        // Verify the step was toggled
        let channel = app.get_channel_at_slot(5).expect("Channel should exist");
        let slice = channel.get_pattern(0).expect("Pattern should exist");
        assert!(
            slice.get_step(0),
            "Step should be toggled on for pasted channel"
        );
    }

    #[test]
    fn test_paste_does_nothing_with_empty_register() {
        let (mut app, _temp) = create_test_app();

        // Verify register is empty
        assert!(
            app.ui.channel_register.is_none(),
            "Register should be empty"
        );

        // Move cursor to sample zone
        app.ui.cursors.channel_rack.channel = 0;
        app.ui.cursors.channel_rack.col = AppCol::SAMPLE_ZONE;

        // Press 'p' to paste
        handle_key(key(KeyCode::Char('p')), &mut app);

        // No channel should be created
        assert_eq!(
            app.channels.len(),
            0,
            "No channel should be created from empty register"
        );
    }
}
