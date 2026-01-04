//! Channel rack input handling
//!
//! Uses vim state machine - just passes keys and executes returned actions

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Panel};
use crate::coords::{AppCol, VimCol};
use crate::mode::ViewMode;
use crate::plugin_host::params::build_editor_params;

use super::common::key_to_vim_char;
use super::vim::{Position, Range, RangeType, VimAction};

/// Handle keyboard input for channel rack
pub fn handle_key(key: KeyEvent, app: &mut App) {
    // Special keys not handled by vim
    match key.code {
        // 'm' to cycle mute state: normal -> muted -> solo -> normal
        KeyCode::Char('m') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(channel) = app.channels.get_mut(app.channel_rack.channel) {
                channel.cycle_mute_state();
                app.mark_dirty();
            }
            return;
        }
        // 's' to preview current channel's sample/plugin (hold for plugins)
        KeyCode::Char('s') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Start preview (release is handled at top level of handle_key)
            if !app.is_previewing {
                app.start_preview(app.channel_rack.channel);
            }
            return;
        }
        // 'S' (shift+s) to toggle solo on current channel
        KeyCode::Char('S') => {
            if let Some(channel) = app.channels.get_mut(app.channel_rack.channel) {
                channel.solo = !channel.solo;
                app.mark_dirty();
            }
            return;
        }
        // 'i' to open piano roll for current channel
        KeyCode::Char('i') => {
            app.set_view_mode(ViewMode::PianoRoll);
            return;
        }
        // 'p' to open plugin editor for plugin channels
        KeyCode::Char('p') => {
            use crate::sequencer::ChannelType;
            if let Some(channel) = app.channels.get(app.channel_rack.channel) {
                if let ChannelType::Plugin { .. } = &channel.channel_type {
                    // Build params list using stored values or defaults from registry
                    let params = build_editor_params(&channel.plugin_params);
                    app.plugin_editor
                        .open(app.channel_rack.channel, &channel.name, params);
                }
            }
            return;
        }
        // '[' to switch to previous pattern
        KeyCode::Char('[') => {
            if app.current_pattern > 0 {
                app.current_pattern -= 1;
                app.mark_dirty();
            }
            return;
        }
        // ']' to switch to next pattern (or create new)
        KeyCode::Char(']') => {
            if app.current_pattern + 1 < app.patterns.len() {
                app.current_pattern += 1;
            } else {
                // Create a new pattern
                let new_id = app.patterns.len();
                let num_channels = app.channels.len();
                app.patterns
                    .push(crate::sequencer::Pattern::new(new_id, num_channels, 16));
                app.current_pattern = new_id;
            }
            app.mark_dirty();
            return;
        }
        // 'd' in sample zone to delete channel
        KeyCode::Char('d') if app.cursor_zone() == "sample" => {
            if app.channel_rack.channel < app.channels.len() {
                // Remove the channel
                app.channels.remove(app.channel_rack.channel);

                // Remove corresponding steps/notes from all patterns
                for pattern in &mut app.patterns {
                    if app.channel_rack.channel < pattern.steps.len() {
                        pattern.steps.remove(app.channel_rack.channel);
                    }
                    if app.channel_rack.channel < pattern.notes.len() {
                        pattern.notes.remove(app.channel_rack.channel);
                    }
                }

                // Adjust cursor if it's now out of bounds
                if app.channel_rack.channel >= app.channels.len() && app.channel_rack.channel > 0 {
                    app.channel_rack.channel = app.channels.len().saturating_sub(1);
                }

                app.mark_dirty();
            }
            return;
        }
        // 'x' or Enter in non-steps zones - zone-aware action
        // In steps zone, let vim handle 'x' (for visual mode delete, counts, etc.)
        KeyCode::Char('x') | KeyCode::Enter if !app.channel_rack.col.is_step_zone() => {
            if app.channel_rack.col.is_mute_zone() {
                // Cycle mute state: normal -> muted -> solo -> normal
                if let Some(channel) = app.channels.get_mut(app.channel_rack.channel) {
                    channel.cycle_mute_state();
                    app.mark_dirty();
                }
            } else if app.channel_rack.col.is_sample_zone() {
                // Open sample browser
                app.browser.start_selection(app.channel_rack.channel);
                app.mode.switch_panel(Panel::Browser);
                app.show_browser = true;
            }
            return;
        }
        // Arrow keys mapped to vim motions
        KeyCode::Left => {
            let vim_col: VimCol = app.channel_rack.col.into();
            let cursor = Position::new(app.channel_rack.channel, vim_col.0);
            let actions = app.vim_channel_rack.process_key('h', false, cursor);
            for action in actions {
                execute_vim_action(action, app);
            }
            return;
        }
        KeyCode::Right => {
            let vim_col: VimCol = app.channel_rack.col.into();
            let cursor = Position::new(app.channel_rack.channel, vim_col.0);
            let actions = app.vim_channel_rack.process_key('l', false, cursor);
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
    let vim_col: VimCol = app.channel_rack.col.into();
    let cursor = Position::new(app.channel_rack.channel, vim_col.0);

    // Let vim process the key
    let actions = app.vim_channel_rack.process_key(ch, ctrl, cursor);

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
            app.channel_rack.channel = pos.row.min(98);
            // Convert vim col back to cursor_col
            app.channel_rack.col = AppCol::from(VimCol(pos.col)).clamp();

            // Update viewport to keep cursor visible
            // Assume ~15 visible rows (will be recalculated at render time)
            let visible_rows = 15;
            if app.channel_rack.channel >= app.channel_rack.viewport_top + visible_rows {
                app.channel_rack.viewport_top = app.channel_rack.channel - visible_rows + 1;
            }
            if app.channel_rack.channel < app.channel_rack.viewport_top {
                app.channel_rack.viewport_top = app.channel_rack.channel;
            }
        }

        VimAction::Toggle => {
            // Only toggle step if in steps zone
            if app.channel_rack.col.is_step_zone() {
                // For plugin channels, open piano roll instead of toggling step
                if let Some(channel) = app.channels.get(app.channel_rack.channel) {
                    use crate::sequencer::ChannelType;
                    if matches!(&channel.channel_type, ChannelType::Plugin { .. }) {
                        app.set_view_mode(ViewMode::PianoRoll);
                        return;
                    }
                }
                app.toggle_step();
            }
        }

        VimAction::Yank(range) => {
            let data = get_pattern_data(app, &range);
            app.vim_channel_rack.store_yank(data, range.range_type);
        }

        VimAction::Delete(range) => {
            // Store deleted data in register 1 (and shift history) before deleting
            let data = get_pattern_data(app, &range);
            app.vim_channel_rack.store_delete(data, range.range_type);
            delete_pattern_data(app, &range);
            app.mark_dirty();
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
                app.channel_rack.viewport_top =
                    (app.channel_rack.viewport_top + delta as usize).min(max_top);
            } else {
                // Scroll up
                app.channel_rack.viewport_top = app
                    .channel_rack
                    .viewport_top
                    .saturating_sub((-delta) as usize);
            }
            // Keep cursor visible
            if app.channel_rack.channel < app.channel_rack.viewport_top {
                app.channel_rack.channel = app.channel_rack.viewport_top;
            } else if app.channel_rack.channel >= app.channel_rack.viewport_top + visible_rows {
                app.channel_rack.channel = app.channel_rack.viewport_top + visible_rows - 1;
            }
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

    if let Some(pattern) = app.get_current_pattern() {
        for row in start.row..=end.row {
            if row >= pattern.steps.len() {
                continue;
            }

            // Convert vim columns to step indices
            let col_start = match range.range_type {
                RangeType::Block => vim_col_to_step(start.col).unwrap_or(0),
                RangeType::Line => 0,
                RangeType::Char if row == start.row => vim_col_to_step(start.col).unwrap_or(0),
                RangeType::Char => 0,
            };
            let col_end = match range.range_type {
                RangeType::Block => {
                    vim_col_to_step(end.col).unwrap_or(pattern.length.saturating_sub(1))
                }
                RangeType::Line => pattern.length.saturating_sub(1),
                RangeType::Char if row == end.row => {
                    vim_col_to_step(end.col).unwrap_or(pattern.length.saturating_sub(1))
                }
                RangeType::Char => pattern.length.saturating_sub(1),
            };

            // Clamp to valid step range
            let col_start = col_start.min(pattern.length.saturating_sub(1));
            let col_end = col_end.min(pattern.length.saturating_sub(1));

            if col_start <= col_end {
                let row_data: Vec<bool> = (col_start..=col_end)
                    .map(|col| pattern.get_step(row, col))
                    .collect();
                data.push(row_data);
            }
        }
    }

    data
}

/// Delete pattern data in a range (vim coordinates)
fn delete_pattern_data(app: &mut App, range: &Range) {
    let (start, end) = range.normalized();

    if let Some(pattern) = app.get_current_pattern_mut() {
        for row in start.row..=end.row {
            if row >= pattern.steps.len() {
                continue;
            }

            // Convert vim columns to step indices
            let col_start = match range.range_type {
                RangeType::Block => vim_col_to_step(start.col).unwrap_or(0),
                RangeType::Line => 0,
                RangeType::Char if row == start.row => vim_col_to_step(start.col).unwrap_or(0),
                RangeType::Char => 0,
            };
            let col_end = match range.range_type {
                RangeType::Block => {
                    vim_col_to_step(end.col).unwrap_or(pattern.length.saturating_sub(1))
                }
                RangeType::Line => pattern.length.saturating_sub(1),
                RangeType::Char if row == end.row => {
                    vim_col_to_step(end.col).unwrap_or(pattern.length.saturating_sub(1))
                }
                RangeType::Char => pattern.length.saturating_sub(1),
            };

            // Clamp to valid step range
            let col_start = col_start.min(pattern.length.saturating_sub(1));
            let col_end = col_end.min(pattern.length.saturating_sub(1));

            for col in col_start..=col_end {
                pattern.set_step(row, col, false);
            }
        }
    }
}

/// Paste clipboard at cursor position
fn paste_at_cursor(app: &mut App, before: bool) {
    let cursor_row = app.channel_rack.channel;
    let cursor_col = app.cursor_step(); // Use method to get step index

    // Clone register data to avoid borrow issues
    let paste_data = app.vim_channel_rack.get_register().cloned();

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

        if let Some(pattern) = app.get_current_pattern_mut() {
            for (row_offset, row_data) in register.data.iter().enumerate() {
                let target_row = paste_row + row_offset;
                if target_row >= pattern.steps.len() {
                    break;
                }

                for (col_offset, &value) in row_data.iter().enumerate() {
                    let target_col = paste_col + col_offset;
                    if target_col < pattern.length {
                        pattern.set_step(target_row, target_col, value);
                    }
                }
            }
        }
    }
}
