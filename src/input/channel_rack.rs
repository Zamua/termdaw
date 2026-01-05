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

/// Cycle mute state on the mixer track for a generator: normal -> muted -> solo -> normal
fn cycle_generator_mute_state(app: &mut App, gen_idx: usize) {
    let track_id = app.mixer.get_generator_track(gen_idx);
    let track = app.mixer.track_mut(track_id);

    if track.solo {
        track.solo = false;
        track.muted = false;
    } else if track.muted {
        track.muted = false;
        track.solo = true;
    } else {
        track.muted = true;
    }
}

/// Handle keyboard input for channel rack
pub fn handle_key(key: KeyEvent, app: &mut App) {
    // Special keys not handled by vim
    match key.code {
        // 'm' to cycle mute state: normal -> muted -> solo -> normal
        KeyCode::Char('m') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            let gen_idx = app.channel_rack.channel;
            if gen_idx < app.generators.len() {
                cycle_generator_mute_state(app, gen_idx);
                app.sync_mixer_to_audio();
                app.mark_dirty();
            }
            return;
        }
        // 's' to preview current generator's sample/plugin (hold for plugins)
        KeyCode::Char('s') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Start preview (release is handled at top level of handle_key)
            if !app.is_previewing {
                app.start_preview(app.channel_rack.channel);
            }
            return;
        }
        // 'S' (shift+s) to toggle solo on current generator's mixer track
        KeyCode::Char('S') => {
            let gen_idx = app.channel_rack.channel;
            if gen_idx < app.generators.len() {
                let track_id = app.mixer.get_generator_track(gen_idx);
                app.mixer.toggle_solo(track_id);
                app.sync_mixer_to_audio();
                app.mark_dirty();
            }
            return;
        }
        // 'i' to open piano roll for current generator
        KeyCode::Char('i') => {
            app.set_view_mode(ViewMode::PianoRoll);
            return;
        }
        // 'p' to open plugin editor for plugin generators
        KeyCode::Char('p') => {
            use crate::sequencer::GeneratorType;
            if let Some(generator) = app.generators.get(app.channel_rack.channel) {
                if let GeneratorType::Plugin { .. } = &generator.generator_type {
                    // Build params list using stored values or defaults from registry
                    let params = build_editor_params(&generator.plugin_params);
                    app.plugin_editor
                        .open(app.channel_rack.channel, &generator.name, params);
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
                let num_generators = app.generators.len();
                app.patterns
                    .push(crate::sequencer::Pattern::new(new_id, num_generators, 16));
                app.current_pattern = new_id;
            }
            app.mark_dirty();
            return;
        }
        // 'd' in sample zone to delete generator
        KeyCode::Char('d') if app.cursor_zone() == "sample" => {
            if app.channel_rack.channel < app.generators.len() {
                // Remove the generator
                app.generators.remove(app.channel_rack.channel);

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
                if app.channel_rack.channel >= app.generators.len() && app.channel_rack.channel > 0
                {
                    app.channel_rack.channel = app.generators.len().saturating_sub(1);
                }

                app.mark_dirty();
            }
            return;
        }
        // 'x' or Enter in non-steps zones - zone-aware action
        // In steps zone, let vim handle 'x' (for visual mode delete, counts, etc.)
        KeyCode::Char('x') | KeyCode::Enter if !app.channel_rack.col.is_step_zone() => {
            if app.channel_rack.col.is_mute_zone() {
                // Cycle mute state on the mixer track
                let gen_idx = app.channel_rack.channel;
                if gen_idx < app.generators.len() {
                    cycle_generator_mute_state(app, gen_idx);
                    app.sync_mixer_to_audio();
                    app.mark_dirty();
                }
            } else if app.channel_rack.col.is_track_zone() {
                // Cycle to next mixer track (1-15, wrap around)
                let gen_idx = app.channel_rack.channel;
                if gen_idx < app.generators.len() {
                    let current = app.mixer.get_generator_track(gen_idx);
                    // Cycle through tracks 1-15 (skip master)
                    let next = if current.index() >= 15 {
                        1
                    } else {
                        current.index() + 1
                    };
                    app.mixer
                        .generator_routing
                        .set(gen_idx, crate::mixer::TrackId(next));
                    app.audio.set_generator_track(gen_idx, next);
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
        // '+' or '=' to increment track assignment (when in track zone)
        KeyCode::Char('+') | KeyCode::Char('=') if app.channel_rack.col.is_track_zone() => {
            let gen_idx = app.channel_rack.channel;
            if gen_idx < app.generators.len() {
                let current = app.mixer.get_generator_track(gen_idx);
                let next = if current.index() >= 15 {
                    1
                } else {
                    current.index() + 1
                };
                app.mixer
                    .generator_routing
                    .set(gen_idx, crate::mixer::TrackId(next));
                app.audio.set_generator_track(gen_idx, next);
                app.mark_dirty();
            }
            return;
        }
        // '-' to decrement track assignment (when in track zone)
        KeyCode::Char('-') if app.channel_rack.col.is_track_zone() => {
            let gen_idx = app.channel_rack.channel;
            if gen_idx < app.generators.len() {
                let current = app.mixer.get_generator_track(gen_idx);
                let prev = if current.index() <= 1 {
                    15
                } else {
                    current.index() - 1
                };
                app.mixer
                    .generator_routing
                    .set(gen_idx, crate::mixer::TrackId(prev));
                app.audio.set_generator_track(gen_idx, prev);
                app.mark_dirty();
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
                if let Some(channel) = app.generators.get(app.channel_rack.channel) {
                    use crate::sequencer::GeneratorType;
                    if matches!(&channel.generator_type, GeneratorType::Plugin { .. }) {
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

        VimAction::NextTab => {
            // Switch to Playlist view and focus it
            app.view_mode = ViewMode::Playlist;
            app.mode.switch_panel(Panel::Playlist);
        }

        VimAction::PrevTab => {
            // Switch to Playlist view (only 2 tabs, so same as next)
            app.view_mode = ViewMode::Playlist;
            app.mode.switch_panel(Panel::Playlist);
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
            if let Some((row, vim_col)) = app.screen_areas.channel_rack_cell_at(*x, *y) {
                // Exit visual mode if active
                if app.vim_channel_rack.is_visual() {
                    let vim_col_current: VimCol = app.channel_rack.col.into();
                    let cursor = Position::new(app.channel_rack.channel, vim_col_current.0);
                    let actions = app.vim_channel_rack.process_key('\x1b', false, cursor);
                    for action in actions {
                        execute_vim_action(action, app);
                    }
                }

                // Move cursor to clicked cell
                app.channel_rack.channel = row.min(98);
                app.channel_rack.col = AppCol::from(VimCol(vim_col)).clamp();
                update_viewport(app);

                // Handle zone-specific click behavior
                let col = AppCol::from(VimCol(vim_col));
                if col.is_mute_zone() {
                    // Click on mute column - cycle mute state via mixer track
                    if row < app.generators.len() {
                        cycle_generator_mute_state(app, row);
                        app.sync_mixer_to_audio();
                        app.mark_dirty();
                    }
                } else if col.is_step_zone() {
                    // Click on step - toggle it (if sampler generator)
                    if let Some(generator) = app.generators.get(row) {
                        use crate::sequencer::GeneratorType;
                        if matches!(&generator.generator_type, GeneratorType::Plugin { .. }) {
                            // Plugin generators open piano roll on click
                            app.set_view_mode(ViewMode::PianoRoll);
                        } else {
                            // Toggle step for sampler generators
                            app.toggle_step();
                        }
                    } else {
                        // Empty slot - toggle anyway
                        app.toggle_step();
                    }
                }
                // Sample zone click just moves cursor
            }
        }

        MouseAction::DoubleClick { x, y } => {
            // Double-click to preview sample
            if let Some((row, vim_col)) = app.screen_areas.channel_rack_cell_at(*x, *y) {
                let col = AppCol::from(VimCol(vim_col));
                if col.is_sample_zone() {
                    // Preview the channel on double-click
                    app.start_preview(row);
                }
            }
        }

        MouseAction::DragStart { x, y, .. } => {
            // Start selection drag in step zone
            if let Some((row, vim_col)) = app.screen_areas.channel_rack_cell_at(*x, *y) {
                let col = AppCol::from(VimCol(vim_col));
                if col.is_step_zone() {
                    // Move cursor to start position
                    app.channel_rack.channel = row.min(98);
                    app.channel_rack.col = col.clamp();
                    update_viewport(app);

                    // Enter visual block mode
                    let cursor = Position::new(row, vim_col);
                    let actions = app.vim_channel_rack.process_key('v', true, cursor); // Ctrl+v for block
                    for action in actions {
                        execute_vim_action(action, app);
                    }
                }
            }
        }

        MouseAction::DragMove { x, y, .. } => {
            // Extend selection
            if app.vim_channel_rack.is_visual() {
                if let Some((row, vim_col)) = app.screen_areas.channel_rack_cell_at(*x, *y) {
                    // Move cursor to extend selection
                    app.channel_rack.channel = row.min(98);
                    app.channel_rack.col = AppCol::from(VimCol(vim_col)).clamp();
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
                app.channel_rack.viewport_top = app.channel_rack.viewport_top.saturating_sub(3);
            } else {
                // Scroll down
                app.channel_rack.viewport_top = (app.channel_rack.viewport_top + 3).min(98);
            }
        }

        MouseAction::RightClick { x, y } => {
            // Show context menu for channel rack
            if let Some((row, _vim_col)) = app.screen_areas.channel_rack_cell_at(*x, *y) {
                use crate::sequencer::GeneratorType;
                use crate::ui::context_menu::{channel_rack_menu, MenuContext};

                // Determine generator properties for menu
                let (has_sample, is_plugin) = app
                    .generators
                    .get(row)
                    .map(|gen| {
                        let has_sample = gen.sample_path.is_some();
                        let is_plugin = matches!(&gen.generator_type, GeneratorType::Plugin { .. });
                        (has_sample, is_plugin)
                    })
                    .unwrap_or((false, false));

                let items = channel_rack_menu(has_sample, is_plugin);
                let context = MenuContext::ChannelRack { channel: row };
                app.context_menu.show(*x, *y, items, context);
            }
        }
    }
}

/// Update viewport to keep cursor visible
fn update_viewport(app: &mut App) {
    let visible_rows = 15; // Approximate
    if app.channel_rack.channel >= app.channel_rack.viewport_top + visible_rows {
        app.channel_rack.viewport_top = app.channel_rack.channel - visible_rows + 1;
    }
    if app.channel_rack.channel < app.channel_rack.viewport_top {
        app.channel_rack.viewport_top = app.channel_rack.channel;
    }
}
