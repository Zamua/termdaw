//! Input handling for keyboard and mouse events

pub mod vim;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use tui_input::backend::crossterm::EventHandler;

use crate::app::{App, FocusedPanel, ViewMode};
use crate::command_picker::Command;
use vim::{Position, Range, RangeType, VimAction};

/// Handle a keyboard event
/// Returns true if the app should quit
pub fn handle_key(key: KeyEvent, app: &mut App) -> bool {
    // Handle key release events - only specific keys need release handling
    if key.kind == KeyEventKind::Release {
        // Only handle 's' release for stopping plugin preview
        if key.code == KeyCode::Char('s') && app.is_previewing {
            if let Some(channel) = app.preview_channel {
                app.stop_preview(channel);
            }
            // Trigger the release phase of the envelope animation
            app.plugin_editor.stop_preview_animation();
        }
        // Ignore all other release events
        return false;
    }

    // Ignore key repeat events entirely
    if key.kind == KeyEventKind::Repeat {
        return false;
    }

    // From here on, we only handle Press events

    // Handle input mode first (tempo entry, etc.)
    if app.command_picker.input.active {
        return handle_input_mode_key(key, app);
    }

    // Handle command picker (if visible)
    if app.command_picker.visible {
        return handle_command_picker_key(key, app);
    }

    // Handle plugin editor modal (if visible)
    if app.plugin_editor.visible {
        return handle_plugin_editor_key(key, app);
    }

    // Global keybindings (always active)
    match key.code {
        // Tab to cycle focus
        KeyCode::Tab => {
            app.next_panel();
            return false;
        }

        // Space to show command picker
        KeyCode::Char(' ') => {
            app.command_picker.show();
            return false;
        }

        _ => {}
    }

    // Panel-specific keybindings
    match app.focused_panel {
        FocusedPanel::ChannelRack => handle_channel_rack_key(key, app),
        FocusedPanel::Browser => handle_browser_key(key, app),
        FocusedPanel::Mixer => handle_mixer_key(key, app),
        FocusedPanel::Playlist => handle_playlist_key(key, app),
        FocusedPanel::PianoRoll => handle_piano_roll_key(key, app),
    }

    false
}

/// Handle keyboard input for text input mode (tempo, etc.)
fn handle_input_mode_key(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        // Escape cancels input
        KeyCode::Esc => {
            app.command_picker.cancel_input();
            false
        }
        // Enter confirms input
        KeyCode::Enter => {
            if let Some(bpm) = app.command_picker.get_tempo_value() {
                app.bpm = bpm.clamp(20.0, 999.0);
                app.mark_dirty();
            }
            app.command_picker.cancel_input();
            false
        }
        // For tempo input, only allow digits and decimal point
        KeyCode::Char(c) if !(c.is_ascii_digit() || c == '.') => {
            false // Ignore non-numeric characters for tempo
        }
        // Let tui-input handle the rest (digits, backspace, delete, arrows, etc.)
        _ => {
            // Limit input length for tempo
            if app.command_picker.input.input.value().len() < 6
                || key.code == KeyCode::Backspace
                || key.code == KeyCode::Delete
            {
                app.command_picker
                    .input
                    .input
                    .handle_event(&Event::Key(key));
            }
            false
        }
    }
}

/// Handle keyboard input when command picker is visible
fn handle_command_picker_key(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        // Escape closes picker
        KeyCode::Esc => {
            app.command_picker.hide();
            false
        }
        // Any other key - try to find and execute a command
        KeyCode::Char(c) => {
            if let Some(cmd) = app.command_picker.find_command(c) {
                app.command_picker.hide();
                execute_command(cmd, app)
            } else {
                // Unknown key - just close picker
                app.command_picker.hide();
                false
            }
        }
        _ => {
            // Other keys close picker
            app.command_picker.hide();
            false
        }
    }
}

/// Handle plugin editor modal keys
fn handle_plugin_editor_key(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        // Escape closes editor
        KeyCode::Esc => {
            app.plugin_editor.close();
            false
        }
        // 's' to preview the synth sound
        KeyCode::Char('s') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !app.is_previewing {
                app.start_preview(app.plugin_editor.channel_idx);
                app.plugin_editor.start_preview_animation();
            }
            false
        }
        // Navigation: j/k or down/up
        KeyCode::Char('j') | KeyCode::Down => {
            app.plugin_editor.select_next();
            false
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.plugin_editor.select_prev();
            false
        }
        // Adjust value: h/l or left/right
        KeyCode::Char('h') | KeyCode::Left => {
            app.plugin_editor.adjust_value(-1.0);
            send_param_to_plugin(app);
            false
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.plugin_editor.adjust_value(1.0);
            send_param_to_plugin(app);
            false
        }
        // Fine adjust with shift
        KeyCode::Char('H') => {
            app.plugin_editor.adjust_value(-0.1);
            send_param_to_plugin(app);
            false
        }
        KeyCode::Char('L') => {
            app.plugin_editor.adjust_value(0.1);
            send_param_to_plugin(app);
            false
        }
        _ => false,
    }
}

/// Send the currently selected parameter to the plugin and save to channel
fn send_param_to_plugin(app: &mut App) {
    let channel_idx = app.plugin_editor.channel_idx;
    if let Some(param) = app.plugin_editor.selected_param() {
        let param_name = param.name.clone();
        let param_value = param.value;

        // Save to channel's plugin_params for persistence
        if let Some(channel) = app.channels.get_mut(channel_idx) {
            channel.plugin_params.insert(param_name, param_value);
        }

        // Send to audio thread
        if let (Some(param_id), Some(value)) = (
            app.plugin_editor.get_selected_clap_param_id(),
            app.plugin_editor.get_selected_param_value(),
        ) {
            app.audio.plugin_set_param(channel_idx, param_id, value);
        }

        app.mark_dirty();
    }
}

/// Execute a command from the picker
fn execute_command(cmd: Command, app: &mut App) -> bool {
    match cmd {
        Command::ShowPlaylist => {
            app.set_view_mode(ViewMode::Playlist);
            false
        }
        Command::ShowChannelRack => {
            app.set_view_mode(ViewMode::ChannelRack);
            false
        }
        Command::ShowPianoRoll => {
            app.set_view_mode(ViewMode::PianoRoll);
            false
        }
        Command::ToggleBrowser => {
            app.toggle_browser();
            false
        }
        Command::ToggleMixer => {
            app.toggle_mixer();
            false
        }
        Command::PlayStop => {
            app.toggle_play();
            false
        }
        Command::SetTempo => {
            app.command_picker.start_tempo_input(app.bpm);
            false
        }
        Command::Quit => {
            app.should_quit = true;
            true
        }
    }
}

/// Handle keyboard input for channel rack
/// Uses vim state machine - just passes keys and executes returned actions
fn handle_channel_rack_key(key: KeyEvent, app: &mut App) {
    // Special keys not handled by vim
    match key.code {
        // 'm' to cycle mute state: normal -> muted -> solo -> normal
        KeyCode::Char('m') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(channel) = app.channels.get_mut(app.cursor_channel) {
                if channel.solo {
                    channel.solo = false;
                    channel.muted = false;
                } else if channel.muted {
                    channel.muted = false;
                    channel.solo = true;
                } else {
                    channel.muted = true;
                }
                app.mark_dirty();
            }
            return;
        }
        // 's' to preview current channel's sample/plugin (hold for plugins)
        KeyCode::Char('s') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Start preview (release is handled at top level of handle_key)
            if !app.is_previewing {
                app.start_preview(app.cursor_channel);
            }
            return;
        }
        // 'S' (shift+s) to toggle solo on current channel
        KeyCode::Char('S') => {
            if let Some(channel) = app.channels.get_mut(app.cursor_channel) {
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
            if let Some(channel) = app.channels.get(app.cursor_channel) {
                if let ChannelType::Plugin { .. } = &channel.channel_type {
                    // Build params list using stored values or defaults
                    let stored = &channel.plugin_params;
                    let params = vec![
                        crate::plugin_host::PluginParam {
                            id: 0,
                            name: "Attack".to_string(),
                            value: *stored.get("Attack").unwrap_or(&10.0),
                            min: 1.0,
                            max: 5000.0,
                            default: 10.0,
                        },
                        crate::plugin_host::PluginParam {
                            id: 1,
                            name: "Decay".to_string(),
                            value: *stored.get("Decay").unwrap_or(&100.0),
                            min: 1.0,
                            max: 5000.0,
                            default: 100.0,
                        },
                        crate::plugin_host::PluginParam {
                            id: 2,
                            name: "Sustain".to_string(),
                            value: *stored.get("Sustain").unwrap_or(&0.7),
                            min: 0.0,
                            max: 1.0,
                            default: 0.7,
                        },
                        crate::plugin_host::PluginParam {
                            id: 3,
                            name: "Release".to_string(),
                            value: *stored.get("Release").unwrap_or(&200.0),
                            min: 1.0,
                            max: 5000.0,
                            default: 200.0,
                        },
                        crate::plugin_host::PluginParam {
                            id: 4,
                            name: "Gain".to_string(),
                            value: *stored.get("Gain").unwrap_or(&0.5),
                            min: 0.0,
                            max: 1.0,
                            default: 0.5,
                        },
                        crate::plugin_host::PluginParam {
                            id: 5,
                            name: "Waveform".to_string(),
                            value: *stored.get("Waveform").unwrap_or(&2.0), // Saw default
                            min: 0.0,
                            max: 3.0,
                            default: 2.0,
                        },
                    ];
                    app.plugin_editor
                        .open(app.cursor_channel, &channel.name, params);
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
            if app.cursor_channel < app.channels.len() {
                // Remove the channel
                app.channels.remove(app.cursor_channel);

                // Remove corresponding steps/notes from all patterns
                for pattern in &mut app.patterns {
                    if app.cursor_channel < pattern.steps.len() {
                        pattern.steps.remove(app.cursor_channel);
                    }
                    if app.cursor_channel < pattern.notes.len() {
                        pattern.notes.remove(app.cursor_channel);
                    }
                }

                // Adjust cursor if it's now out of bounds
                if app.cursor_channel >= app.channels.len() && app.cursor_channel > 0 {
                    app.cursor_channel = app.channels.len().saturating_sub(1);
                }

                app.mark_dirty();
            }
            return;
        }
        // 'x' or Enter in non-steps zones - zone-aware action
        // In steps zone, let vim handle 'x' (for visual mode delete, counts, etc.)
        KeyCode::Char('x') | KeyCode::Enter if app.cursor_zone() != "steps" => {
            match app.cursor_zone() {
                "mute" => {
                    // Cycle mute state: normal -> muted -> solo -> normal
                    if let Some(channel) = app.channels.get_mut(app.cursor_channel) {
                        if channel.solo {
                            channel.solo = false;
                            channel.muted = false;
                        } else if channel.muted {
                            channel.muted = false;
                            channel.solo = true;
                        } else {
                            channel.muted = true;
                        }
                        app.mark_dirty();
                    }
                }
                "sample" => {
                    // Open sample browser
                    app.browser.start_selection(app.cursor_channel);
                    app.focused_panel = FocusedPanel::Browser;
                    app.show_browser = true;
                }
                _ => {}
            }
            return;
        }
        // Arrow keys mapped to vim motions
        KeyCode::Left => {
            let vim_col = (app.cursor_col + 2).max(0) as usize;
            let cursor = Position::new(app.cursor_channel, vim_col);
            let actions = app.vim_channel_rack.process_key('h', false, cursor);
            for action in actions {
                execute_vim_action(action, app);
            }
            return;
        }
        KeyCode::Right => {
            let vim_col = (app.cursor_col + 2).max(0) as usize;
            let cursor = Position::new(app.cursor_channel, vim_col);
            let actions = app.vim_channel_rack.process_key('l', false, cursor);
            for action in actions {
                execute_vim_action(action, app);
            }
            return;
        }
        _ => {}
    }

    // Convert crossterm key to char for vim (for j/k/w/b/e/gg/G/v/d/y/c etc)
    let (ch, ctrl) = match key.code {
        KeyCode::Char(c) => (c, key.modifiers.contains(KeyModifiers::CONTROL)),
        KeyCode::Esc => ('\x1b', false),
        KeyCode::Up => ('k', false),
        KeyCode::Down => ('j', false),
        _ => return,
    };

    // Get current cursor position (convert to vim space: col + 2)
    let vim_col = (app.cursor_col + 2).max(0) as usize;
    let cursor = Position::new(app.cursor_channel, vim_col);

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
            app.cursor_channel = pos.row.min(98);
            // Convert vim col back to cursor_col (vim col 0-17 -> cursor_col -2 to 15)
            app.cursor_col = (pos.col as i32 - 2).clamp(-2, 15);

            // Update viewport to keep cursor visible
            // Assume ~15 visible rows (will be recalculated at render time)
            let visible_rows = 15;
            if app.cursor_channel >= app.channel_rack_viewport_top + visible_rows {
                app.channel_rack_viewport_top = app.cursor_channel - visible_rows + 1;
            }
            if app.cursor_channel < app.channel_rack_viewport_top {
                app.channel_rack_viewport_top = app.cursor_channel;
            }
        }

        VimAction::Toggle => {
            // Only toggle step if in steps zone
            if app.cursor_zone() == "steps" {
                // For plugin channels, open piano roll instead of toggling step
                if let Some(channel) = app.channels.get(app.cursor_channel) {
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
    }
}

/// Convert vim column to step index
/// Vim columns 0-1 are metadata zone (no steps), 2-17 are steps 0-15
fn vim_col_to_step(vim_col: usize) -> Option<usize> {
    if vim_col >= 2 {
        Some(vim_col - 2)
    } else {
        None // Metadata zone has no steps
    }
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
    let cursor_row = app.cursor_channel;
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

/// Handle keyboard input for browser
fn handle_browser_key(key: KeyEvent, app: &mut App) {
    // Handle Escape to cancel selection mode
    if key.code == KeyCode::Esc {
        if app.browser.selection_mode {
            app.browser.cancel_selection();
        }
        app.focused_panel = FocusedPanel::ChannelRack;
        return;
    }

    // Track previous cursor for auto-preview
    let prev_cursor = app.browser.cursor;

    match key.code {
        // Toggle between Samples and Plugins mode with Shift+Tab or 't'
        KeyCode::BackTab | KeyCode::Char('t') => {
            app.browser.toggle_mode();
            return; // Don't trigger auto-preview after mode switch
        }

        // Navigation
        KeyCode::Char('j') | KeyCode::Down => {
            app.browser.move_down(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.browser.move_up(1);
        }

        // Expand/collapse folders
        KeyCode::Char('l') | KeyCode::Right => {
            app.browser.expand();
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.browser.collapse_or_parent();
        }

        // Jump to first/last (vim style)
        KeyCode::Char('0') => {
            app.browser.go_to_top();
        }
        KeyCode::Char('$') => {
            app.browser.go_to_bottom();
        }
        KeyCode::Char('g') => {
            // gg to go to top (simplified - single g works)
            app.browser.go_to_top();
        }
        KeyCode::Char('G') => {
            app.browser.go_to_bottom();
        }

        // Enter or 'o' to select sample or toggle folder
        KeyCode::Enter | KeyCode::Char('o') => {
            if let Some(entry) = app.browser.current_entry().cloned() {
                if entry.is_dir {
                    // Toggle folder expansion
                    app.browser.toggle_or_select();
                } else if app.browser.selection_mode {
                    // Complete selection and assign sample/plugin to channel
                    let browser_mode = app.browser.mode;
                    if let Some((channel_idx, relative_path)) = app.browser.complete_selection() {
                        match browser_mode {
                            crate::browser::BrowserMode::Samples => {
                                app.set_channel_sample(channel_idx, relative_path);
                            }
                            crate::browser::BrowserMode::Plugins => {
                                app.set_channel_plugin(channel_idx, relative_path);
                            }
                        }
                        app.focused_panel = FocusedPanel::ChannelRack;
                    }
                } else {
                    // Just preview the file
                    let full_path = app.project_path.join("samples").join(
                        entry
                            .path
                            .strip_prefix(app.browser.root_path())
                            .unwrap_or(&entry.path),
                    );
                    app.audio.preview_sample(&full_path);
                }
            }
            return; // Don't trigger auto-preview
        }

        _ => {
            return;
        } // Don't trigger auto-preview for unhandled keys
    }

    // Auto-preview on cursor move (only for audio files in samples mode)
    if app.browser.cursor != prev_cursor {
        if let Some(entry) = app.browser.current_entry() {
            if !entry.is_dir && app.browser.mode == crate::browser::BrowserMode::Samples {
                let full_path = app.project_path.join("samples").join(
                    entry
                        .path
                        .strip_prefix(app.browser.root_path())
                        .unwrap_or(&entry.path),
                );
                app.audio.preview_sample(&full_path);
            }
        }
    }
}

/// Handle keyboard input for mixer
fn handle_mixer_key(key: KeyEvent, app: &mut App) {
    let max_channel = app.channel_count().saturating_sub(1);

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => {
            app.mixer_selected_channel = app.mixer_selected_channel.saturating_sub(1);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.mixer_selected_channel = (app.mixer_selected_channel + 1).min(max_channel);
        }
        // j/k = ±1%, J/K = ±5%
        KeyCode::Char('j') => {
            app.adjust_mixer_volume(-0.01);
        }
        KeyCode::Char('J') => {
            app.adjust_mixer_volume(-0.05);
        }
        KeyCode::Char('k') => {
            app.adjust_mixer_volume(0.01);
        }
        KeyCode::Char('K') => {
            app.adjust_mixer_volume(0.05);
        }
        KeyCode::Char('m') => {
            app.toggle_mute();
        }
        KeyCode::Char('s') => {
            app.toggle_solo();
        }
        // 0-9 for direct volume set: 0 = 100%, 1 = 10%, 2 = 20%, etc.
        KeyCode::Char('0') => {
            if let Some(channel) = app.channels.get_mut(app.mixer_selected_channel) {
                channel.volume = 1.0;
                if channel.is_plugin() {
                    app.audio
                        .plugin_set_volume(app.mixer_selected_channel, channel.volume);
                }
                app.mark_dirty();
            }
        }
        KeyCode::Char(c @ '1'..='9') => {
            if let Some(channel) = app.channels.get_mut(app.mixer_selected_channel) {
                channel.volume = (c as u8 - b'0') as f32 * 0.1;
                if channel.is_plugin() {
                    app.audio
                        .plugin_set_volume(app.mixer_selected_channel, channel.volume);
                }
                app.mark_dirty();
            }
        }
        _ => {}
    }
}

/// Number of bars in the arrangement
const NUM_BARS: usize = 16;

/// Handle keyboard input for playlist
/// Uses vim state machine - routes keys through vim and executes returned actions
fn handle_playlist_key(key: KeyEvent, app: &mut App) {
    // Get non-empty pattern count for navigation bounds
    let pattern_count = get_playlist_pattern_count(app);

    // Component-specific keys (not handled by vim)
    match key.code {
        // 'm' to toggle mute on current pattern
        KeyCode::Char('m') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            handle_playlist_mute(app);
            return;
        }
        _ => {}
    }

    // Configure vim for playlist: rows = patterns, cols = 17 (mute + 16 bars)
    app.vim_playlist
        .update_dimensions(pattern_count.max(1), NUM_BARS + 1);
    let playlist_zones = vim::GridSemantics::with_zones(vec![
        vim::Zone::new(0, 0),                                     // Mute column
        vim::Zone::new(1, NUM_BARS).main().with_word_interval(4), // Bars
    ]);
    app.vim_playlist.set_grid_semantics(playlist_zones);

    // Convert key to char for vim
    let (ch, ctrl) = match key.code {
        KeyCode::Char(c) => (c, key.modifiers.contains(KeyModifiers::CONTROL)),
        KeyCode::Esc => ('\x1b', false),
        KeyCode::Up => ('k', false),
        KeyCode::Down => ('j', false),
        KeyCode::Left => ('h', false),
        KeyCode::Right => ('l', false),
        KeyCode::Enter => ('x', false), // Map Enter to toggle
        _ => return,
    };

    // Get cursor position in vim coordinates
    let cursor = vim::Position::new(app.playlist_cursor_row, app.playlist_cursor_bar);

    // Let vim process the key
    let actions = app.vim_playlist.process_key(ch, ctrl, cursor);

    // Execute returned actions
    for action in actions {
        execute_playlist_vim_action(action, app);
    }
}

/// Execute a vim action for the playlist
fn execute_playlist_vim_action(action: VimAction, app: &mut App) {
    match action {
        VimAction::None => {}

        VimAction::MoveCursor(pos) => {
            let pattern_count = get_playlist_pattern_count(app);
            app.playlist_cursor_row = pos.row.min(pattern_count.saturating_sub(1));
            app.playlist_cursor_bar = pos.col.min(NUM_BARS);

            // Auto-scroll viewport
            let visible_rows = 10;
            if app.playlist_cursor_row >= app.playlist_viewport_top + visible_rows {
                app.playlist_viewport_top = app.playlist_cursor_row - visible_rows + 1;
            }
            if app.playlist_cursor_row < app.playlist_viewport_top {
                app.playlist_viewport_top = app.playlist_cursor_row;
            }
        }

        VimAction::Toggle => {
            handle_playlist_toggle(app);
        }

        VimAction::Yank(range) => {
            let data = get_playlist_data(app, &range);
            app.vim_playlist.store_yank(data, range.range_type);
        }

        VimAction::Delete(range) => {
            let data = get_playlist_data(app, &range);
            app.vim_playlist.store_delete(data, range.range_type);
            delete_playlist_data(app, &range);
            app.mark_dirty();
        }

        VimAction::Paste | VimAction::PasteBefore => {
            paste_playlist_data(app);
            app.mark_dirty();
        }

        VimAction::SelectionChanged(_) | VimAction::ModeChanged(_) | VimAction::Escape => {
            // UI handles these via vim.mode() and vim.get_selection()
        }
    }
}

/// Get the number of patterns to show in playlist
fn get_playlist_pattern_count(app: &App) -> usize {
    let patterns: Vec<_> = app
        .patterns
        .iter()
        .filter(|p| {
            p.steps.iter().any(|ch| ch.iter().any(|&s| s))
                || p.notes.iter().any(|ch| !ch.is_empty())
        })
        .collect();
    if patterns.is_empty() {
        app.patterns.len()
    } else {
        patterns.len()
    }
}

/// Toggle a pattern placement at the current cursor position
fn handle_playlist_toggle(app: &mut App) {
    // cursor_bar 0 = mute column, 1-16 = bars 0-15
    if app.playlist_cursor_bar == 0 {
        // In mute column - toggle mute instead
        handle_playlist_mute(app);
        return;
    }

    // Get the pattern at the current row
    let patterns: Vec<_> = app
        .patterns
        .iter()
        .filter(|p| {
            p.steps.iter().any(|ch| ch.iter().any(|&s| s))
                || p.notes.iter().any(|ch| !ch.is_empty())
        })
        .collect();
    let patterns: Vec<_> = if patterns.is_empty() {
        app.patterns.iter().collect()
    } else {
        patterns
    };

    if let Some(pattern) = patterns.get(app.playlist_cursor_row) {
        let pattern_id = pattern.id;
        // Convert cursor_bar (1-16) to bar index (0-15)
        let bar = app.playlist_cursor_bar - 1;
        app.arrangement.toggle_placement(pattern_id, bar);
        app.mark_dirty();
    }
}

/// Cycle mute/solo state for the pattern at the current cursor row
/// Cycles: normal -> muted -> solo -> normal (same order as channel rack)
fn handle_playlist_mute(app: &mut App) {
    let patterns: Vec<_> = app
        .patterns
        .iter()
        .filter(|p| {
            p.steps.iter().any(|ch| ch.iter().any(|&s| s))
                || p.notes.iter().any(|ch| !ch.is_empty())
        })
        .collect();
    let patterns: Vec<_> = if patterns.is_empty() {
        app.patterns.iter().collect()
    } else {
        patterns
    };

    if let Some(pattern) = patterns.get(app.playlist_cursor_row) {
        let pattern_id = pattern.id;
        app.arrangement.cycle_pattern_state(pattern_id);
        app.mark_dirty();
    }
}

/// Get pattern ID from row index
fn get_pattern_id_at_row(app: &App, row: usize) -> Option<usize> {
    let patterns: Vec<_> = app
        .patterns
        .iter()
        .filter(|p| {
            p.steps.iter().any(|ch| ch.iter().any(|&s| s))
                || p.notes.iter().any(|ch| !ch.is_empty())
        })
        .collect();
    let patterns: Vec<_> = if patterns.is_empty() {
        app.patterns.iter().collect()
    } else {
        patterns
    };
    patterns.get(row).map(|p| p.id)
}

/// Get placements in range as YankedPlacement data
fn get_playlist_data(app: &App, range: &vim::Range) -> Vec<crate::sequencer::YankedPlacement> {
    use crate::sequencer::YankedPlacement;

    let (start, end) = range.normalized();
    let anchor_bar = start.col.saturating_sub(1); // cursor_bar 1-16 -> bar 0-15

    let min_row = start.row;
    let max_row = end.row;
    let min_bar = start.col.saturating_sub(1);
    let max_bar = end.col.saturating_sub(1);

    let mut yanked = Vec::new();

    for row in min_row..=max_row {
        if let Some(pattern_id) = get_pattern_id_at_row(app, row) {
            // Find placements for this pattern in bar range
            for placement in &app.arrangement.placements {
                if placement.pattern_id == pattern_id
                    && placement.start_bar >= min_bar
                    && placement.start_bar <= max_bar
                {
                    yanked.push(YankedPlacement {
                        bar_offset: placement.start_bar as i32 - anchor_bar as i32,
                        pattern_id,
                    });
                }
            }
        }
    }

    yanked
}

/// Delete placements in range
fn delete_playlist_data(app: &mut App, range: &vim::Range) {
    let (start, end) = range.normalized();

    let min_row = start.row;
    let max_row = end.row;
    let min_bar = start.col.saturating_sub(1);
    let max_bar = end.col.saturating_sub(1);

    for row in min_row..=max_row {
        if let Some(pattern_id) = get_pattern_id_at_row(app, row) {
            app.arrangement
                .remove_placements_in_range(pattern_id, min_bar, max_bar);
        }
    }
}

/// Paste placements from register at cursor position
fn paste_playlist_data(app: &mut App) {
    use crate::arrangement::PatternPlacement;

    let cursor_bar = app.playlist_cursor_bar.saturating_sub(1); // cursor_bar 1-16 -> bar 0-15

    // Clone register data to avoid borrow issues
    let paste_data = app.vim_playlist.get_register().cloned();

    if let Some(register) = paste_data {
        for yanked in &register.data {
            let new_bar = (cursor_bar as i32 + yanked.bar_offset).clamp(0, 15) as usize;

            // Use the pattern_id from the yanked data
            app.arrangement
                .add_placement(PatternPlacement::new(yanked.pattern_id, new_bar));
        }
    }
}

/// Piano roll pitch range constants
const PIANO_MIN_PITCH: u8 = 36; // C2
const PIANO_MAX_PITCH: u8 = 84; // C6
const PIANO_PITCH_RANGE: usize = (PIANO_MAX_PITCH - PIANO_MIN_PITCH + 1) as usize; // 49 pitches
const PIANO_NUM_STEPS: usize = 16;

/// Convert pitch to vim row (row 0 = highest pitch)
fn pitch_to_row(pitch: u8) -> usize {
    (PIANO_MAX_PITCH - pitch) as usize
}

/// Convert vim row to pitch
fn row_to_pitch(row: usize) -> u8 {
    PIANO_MAX_PITCH.saturating_sub(row as u8)
}

/// Handle keyboard input for piano roll
/// Uses vim state machine - routes keys through vim and executes returned actions
fn handle_piano_roll_key(key: KeyEvent, app: &mut App) {
    // Handle Escape to return to channel rack (if not placing a note and not in visual mode)
    if key.code == KeyCode::Esc && app.placing_note.is_none() && !app.vim_piano_roll.is_visual() {
        app.set_view_mode(ViewMode::ChannelRack);
        return;
    }

    // Component-specific keys (not handled by vim)
    match key.code {
        // 's' to preview current pitch
        KeyCode::Char('s') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !app.is_previewing {
                app.preview_piano_note();
            }
            return;
        }
        // Ctrl+D - scroll down half page
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let new_pitch = app.piano_cursor_pitch.saturating_sub(10);
            app.piano_cursor_pitch = new_pitch.max(PIANO_MIN_PITCH);
            update_piano_viewport(app);
            return;
        }
        // Ctrl+U - scroll up half page
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let new_pitch = app.piano_cursor_pitch.saturating_add(10);
            app.piano_cursor_pitch = new_pitch.min(PIANO_MAX_PITCH);
            update_piano_viewport(app);
            return;
        }
        // Octave jumps (Shift+J/K)
        KeyCode::Char('J') => {
            let new_pitch = app.piano_cursor_pitch.saturating_sub(12);
            app.piano_cursor_pitch = new_pitch.max(PIANO_MIN_PITCH);
            update_piano_viewport(app);
            return;
        }
        KeyCode::Char('K') => {
            let new_pitch = app.piano_cursor_pitch.saturating_add(12);
            app.piano_cursor_pitch = new_pitch.min(PIANO_MAX_PITCH);
            update_piano_viewport(app);
            return;
        }
        // Nudge note left/right
        KeyCode::Char('<') => {
            nudge_note(app, -1);
            return;
        }
        KeyCode::Char('>') => {
            nudge_note(app, 1);
            return;
        }
        _ => {}
    }

    // Configure vim for piano roll: rows = pitches (inverted), cols = steps
    app.vim_piano_roll
        .update_dimensions(PIANO_PITCH_RANGE, PIANO_NUM_STEPS);
    let piano_zones = vim::GridSemantics::with_zones(vec![vim::Zone::new(0, PIANO_NUM_STEPS - 1)
        .main()
        .with_word_interval(4)]);
    app.vim_piano_roll.set_grid_semantics(piano_zones);

    // Convert key to char for vim
    let (ch, ctrl) = match key.code {
        KeyCode::Char(c) => (c, key.modifiers.contains(KeyModifiers::CONTROL)),
        KeyCode::Esc => ('\x1b', false),
        KeyCode::Up => ('k', false),
        KeyCode::Down => ('j', false),
        KeyCode::Left => ('h', false),
        KeyCode::Right => ('l', false),
        KeyCode::Enter => ('x', false), // Map Enter to toggle
        _ => return,
    };

    // Get cursor position in vim coordinates (row = inverted pitch, col = step)
    let cursor = vim::Position::new(pitch_to_row(app.piano_cursor_pitch), app.piano_cursor_step);

    // Let vim process the key
    let actions = app.vim_piano_roll.process_key(ch, ctrl, cursor);

    // Execute returned actions
    for action in actions {
        execute_piano_roll_vim_action(action, app);
    }
}

/// Execute a vim action for the piano roll
fn execute_piano_roll_vim_action(action: VimAction, app: &mut App) {
    match action {
        VimAction::None => {}

        VimAction::MoveCursor(pos) => {
            // Convert vim row back to pitch
            app.piano_cursor_pitch = row_to_pitch(pos.row).clamp(PIANO_MIN_PITCH, PIANO_MAX_PITCH);
            app.piano_cursor_step = pos.col.min(PIANO_NUM_STEPS - 1);
            update_piano_viewport(app);
        }

        VimAction::Toggle => {
            handle_piano_roll_toggle(app);
        }

        VimAction::Yank(range) => {
            let data = get_piano_roll_data(app, &range);
            app.vim_piano_roll.store_yank(data, range.range_type);
        }

        VimAction::Delete(range) => {
            let data = get_piano_roll_data(app, &range);
            app.vim_piano_roll.store_delete(data, range.range_type);
            delete_piano_roll_data(app, &range);
            app.mark_dirty();
        }

        VimAction::Paste | VimAction::PasteBefore => {
            paste_piano_roll_data(app);
            app.mark_dirty();
        }

        VimAction::Escape => {
            // Cancel placement mode on escape
            app.placing_note = None;
        }

        VimAction::SelectionChanged(_) | VimAction::ModeChanged(_) => {
            // UI handles these via vim.mode() and vim.get_selection()
        }
    }
}

/// Update piano roll viewport to keep cursor visible
fn update_piano_viewport(app: &mut App) {
    // Keep cursor in viewport (viewport_top is highest visible pitch)
    if app.piano_cursor_pitch > app.piano_viewport_top {
        app.piano_viewport_top = app.piano_cursor_pitch;
    }
    // Assume ~20 visible rows
    if app.piano_cursor_pitch < app.piano_viewport_top.saturating_sub(20) {
        app.piano_viewport_top = app.piano_cursor_pitch + 10;
    }
}

/// Handle note placement toggle in piano roll
fn handle_piano_roll_toggle(app: &mut App) {
    use crate::sequencer::Note;

    let pitch = app.piano_cursor_pitch;
    let step = app.piano_cursor_step;
    let channel = app.cursor_channel;

    if let Some(start_step) = app.placing_note {
        // Finish placing note
        let min_step = start_step.min(step);
        let max_step = start_step.max(step);
        let duration = max_step - min_step + 1;

        let note = Note::new(pitch, min_step, duration);
        if let Some(pattern) = app.get_current_pattern_mut() {
            pattern.add_note(channel, note);
        }
        app.placing_note = None;
        app.mark_dirty();
    } else {
        // Check for existing note at cursor
        let existing = app
            .get_current_pattern()
            .and_then(|p| p.get_note_at(channel, pitch, step))
            .map(|n| (n.id.clone(), n.start_step));

        if let Some((note_id, start)) = existing {
            // Remove existing note and start new placement from its position
            if let Some(pattern) = app.get_current_pattern_mut() {
                pattern.remove_note(channel, &note_id);
            }
            app.placing_note = Some(start);
            app.mark_dirty();
        } else {
            // Start new placement
            app.placing_note = Some(step);
        }
    }
}

/// Nudge a note at the current cursor position
fn nudge_note(app: &mut App, delta: i32) {
    let pitch = app.piano_cursor_pitch;
    let step = app.piano_cursor_step;
    let channel = app.cursor_channel;

    // Find note at cursor
    let note_info = app
        .get_current_pattern()
        .and_then(|p| p.get_note_at(channel, pitch, step))
        .map(|n| (n.id.clone(), n.start_step, n.duration));

    if let Some((note_id, start_step, duration)) = note_info {
        let new_start = (start_step as i32 + delta).clamp(0, 15 - duration as i32 + 1) as usize;
        if new_start != start_step {
            if let Some(pattern) = app.get_current_pattern_mut() {
                // Remove old note
                pattern.remove_note(channel, &note_id);
                // Add new note at nudged position
                let note = crate::sequencer::Note::new(pitch, new_start, duration);
                pattern.add_note(channel, note);
            }
            app.mark_dirty();
        }
    }
}

/// Get notes in range as YankedNote data (relative offsets from anchor)
fn get_piano_roll_data(app: &App, range: &vim::Range) -> Vec<crate::sequencer::YankedNote> {
    use crate::sequencer::YankedNote;

    let channel = app.cursor_channel;
    let (start, end) = range.normalized();

    // Convert vim rows to pitches
    let anchor_pitch = row_to_pitch(start.row);
    let min_pitch = row_to_pitch(end.row);
    let max_pitch = row_to_pitch(start.row);

    let mut yanked = Vec::new();

    if let Some(pattern) = app.get_current_pattern() {
        for note in pattern.get_notes(channel) {
            // Note must be within pitch range
            if note.pitch < min_pitch || note.pitch > max_pitch {
                continue;
            }

            let note_row = pitch_to_row(note.pitch);

            // Check if note is within step range based on selection type
            let in_step_range = if range.range_type == vim::RangeType::Block || start.row == end.row
            {
                // Block selection or single-row: rectangular bounds
                note.start_step >= start.col && note.start_step <= end.col
            } else {
                // Character-wise selection spanning multiple rows
                if note_row == start.row {
                    note.start_step >= start.col
                } else if note_row == end.row {
                    note.start_step <= end.col
                } else {
                    // Middle rows - include all steps
                    true
                }
            };

            if in_step_range {
                yanked.push(YankedNote {
                    pitch_offset: note.pitch as i32 - anchor_pitch as i32,
                    step_offset: note.start_step as i32 - start.col as i32,
                    duration: note.duration,
                });
            }
        }
    }

    yanked
}

/// Delete notes in range from pattern
fn delete_piano_roll_data(app: &mut App, range: &vim::Range) {
    let channel = app.cursor_channel;
    let (start, end) = range.normalized();

    let min_pitch = row_to_pitch(end.row);
    let max_pitch = row_to_pitch(start.row);

    // Collect IDs of notes to delete
    let to_delete: Vec<String> = app
        .get_current_pattern()
        .map(|p| {
            p.get_notes(channel)
                .iter()
                .filter(|n| {
                    // Note must be within pitch range
                    if n.pitch < min_pitch || n.pitch > max_pitch {
                        return false;
                    }

                    let note_row = pitch_to_row(n.pitch);

                    // For block selection or single-row selection, use rectangular bounds
                    if range.range_type == vim::RangeType::Block || start.row == end.row {
                        n.start_step >= start.col && n.start_step <= end.col
                    } else {
                        // Character-wise selection spanning multiple rows:
                        // - First row: from start.col to end of row
                        // - Middle rows: entire row
                        // - Last row: from start of row to end.col
                        if note_row == start.row {
                            n.start_step >= start.col
                        } else if note_row == end.row {
                            n.start_step <= end.col
                        } else {
                            // Middle rows - include all steps
                            true
                        }
                    }
                })
                .map(|n| n.id.clone())
                .collect()
        })
        .unwrap_or_default();

    // Delete collected notes
    if let Some(pattern) = app.get_current_pattern_mut() {
        for id in to_delete {
            pattern.remove_note(channel, &id);
        }
    }
}

/// Paste notes from register at cursor position
fn paste_piano_roll_data(app: &mut App) {
    use crate::sequencer::Note;

    let channel = app.cursor_channel;
    let cursor_pitch = app.piano_cursor_pitch;
    let cursor_step = app.piano_cursor_step;

    // Clone register data to avoid borrow issues
    let paste_data = app.vim_piano_roll.get_register().cloned();

    if let Some(register) = paste_data {
        if let Some(pattern) = app.get_current_pattern_mut() {
            for yanked in &register.data {
                let new_pitch = (cursor_pitch as i32 + yanked.pitch_offset)
                    .clamp(PIANO_MIN_PITCH as i32, PIANO_MAX_PITCH as i32)
                    as u8;
                let new_step = (cursor_step as i32 + yanked.step_offset)
                    .clamp(0, (PIANO_NUM_STEPS - yanked.duration) as i32)
                    as usize;

                let note = Note::new(new_pitch, new_step, yanked.duration);
                pattern.add_note(channel, note);
            }
        }
    }
}

/// Handle a mouse event
pub fn handle_mouse(mouse: MouseEvent, _app: &mut App) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // TODO: Determine which panel was clicked and focus it
            // TODO: Handle clicks within panels (toggle steps, select items, etc.)
            let _x = mouse.column;
            let _y = mouse.row;
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            // TODO: Handle drag for selection
        }
        MouseEventKind::ScrollUp => {
            // TODO: Scroll content up
        }
        MouseEventKind::ScrollDown => {
            // TODO: Scroll content down
        }
        _ => {}
    }
}
