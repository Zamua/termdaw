//! Piano roll panel input handling
//!
//! Uses vim state machine - routes keys through vim and executes returned actions

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::mode::ViewMode;

use super::common::key_to_vim_char;
use super::vim::{self, VimAction};

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
pub fn handle_key(key: KeyEvent, app: &mut App) {
    // Handle Escape to return to channel rack (if not placing a note and not in visual mode)
    if key.code == KeyCode::Esc
        && app.piano_roll.placing_note.is_none()
        && !app.vim_piano_roll.is_visual()
    {
        app.set_view_mode(ViewMode::ChannelRack);
        return;
    }

    // Component-specific keys (not handled by vim)
    match key.code {
        // 's' to preview the current pitch
        KeyCode::Char('s') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !app.is_previewing {
                app.preview_piano_note();
            }
            return;
        }
        // '<' and '>' to nudge note left/right
        KeyCode::Char('<') => {
            nudge_note(app, -1);
            return;
        }
        KeyCode::Char('>') => {
            nudge_note(app, 1);
            return;
        }
        // 'J' and 'K' (shift) to transpose note up/down
        KeyCode::Char('J') => {
            // Transpose down: pitch - 1
            transpose_note(app, -1);
            return;
        }
        KeyCode::Char('K') => {
            // Transpose up: pitch + 1
            transpose_note(app, 1);
            return;
        }
        _ => {}
    }

    // Convert key to char for vim
    let Some((ch, ctrl)) = key_to_vim_char(key) else {
        return;
    };

    // Get cursor position in vim coordinates (row 0 = highest pitch)
    let cursor_row = pitch_to_row(app.piano_roll.pitch);
    let cursor = vim::Position::new(cursor_row, app.piano_roll.step);

    // Update vim dimensions for current pitch range
    app.vim_piano_roll
        .update_dimensions(PIANO_PITCH_RANGE, PIANO_NUM_STEPS);

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
            app.piano_roll.pitch = row_to_pitch(pos.row).clamp(PIANO_MIN_PITCH, PIANO_MAX_PITCH);
            app.piano_roll.step = pos.col.min(PIANO_NUM_STEPS - 1);

            // Update viewport
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

        VimAction::SelectionChanged(_) | VimAction::ModeChanged(_) | VimAction::Escape => {
            // UI handles these via vim.mode() and vim.get_selection()
        }

        VimAction::ScrollViewport(delta) => {
            // Scroll viewport without moving cursor
            let visible_rows = 20u8;
            if delta > 0 {
                // Scroll down (lower pitches)
                app.piano_roll.viewport_top =
                    app.piano_roll.viewport_top.saturating_sub(delta as u8);
                app.piano_roll.viewport_top = app
                    .piano_roll
                    .viewport_top
                    .max(PIANO_MIN_PITCH + visible_rows);
            } else {
                // Scroll up (higher pitches)
                app.piano_roll.viewport_top =
                    (app.piano_roll.viewport_top + (-delta) as u8).min(PIANO_MAX_PITCH);
            }
            // Keep cursor visible
            if app.piano_roll.pitch > app.piano_roll.viewport_top {
                app.piano_roll.pitch = app.piano_roll.viewport_top;
            } else if app.piano_roll.pitch
                < app.piano_roll.viewport_top.saturating_sub(visible_rows)
            {
                app.piano_roll.pitch = app.piano_roll.viewport_top.saturating_sub(visible_rows);
            }
        }

        VimAction::NextTab => {
            // Switch to Playlist view and focus it
            app.view_mode = crate::mode::ViewMode::Playlist;
            app.mode.switch_panel(crate::app::Panel::Playlist);
        }

        VimAction::PrevTab => {
            // Switch to Playlist view (only 2 tabs, so same as next)
            app.view_mode = crate::mode::ViewMode::Playlist;
            app.mode.switch_panel(crate::app::Panel::Playlist);
        }
    }
}

/// Update piano roll viewport to keep cursor visible
fn update_piano_viewport(app: &mut App) {
    // Keep cursor in viewport (viewport_top is highest visible pitch)
    if app.piano_roll.pitch > app.piano_roll.viewport_top {
        app.piano_roll.viewport_top = app.piano_roll.pitch;
    }
    // Assume ~20 visible rows
    if app.piano_roll.pitch < app.piano_roll.viewport_top.saturating_sub(20) {
        app.piano_roll.viewport_top = app.piano_roll.pitch + 10;
    }
}

/// Handle note placement toggle in piano roll
fn handle_piano_roll_toggle(app: &mut App) {
    use crate::sequencer::Note;

    let pitch = app.piano_roll.pitch;
    let step = app.piano_roll.step;
    let channel = app.channel_rack.channel;

    if let Some(start_step) = app.piano_roll.placing_note {
        // Finish placing note
        let min_step = start_step.min(step);
        let max_step = start_step.max(step);
        let duration = max_step - min_step + 1;

        let note = Note::new(pitch, min_step, duration);
        if let Some(pattern) = app.get_current_pattern_mut() {
            pattern.add_note(channel, note);
        }
        app.piano_roll.placing_note = None;
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
            app.piano_roll.placing_note = Some(start);
            app.mark_dirty();
        } else {
            // Start new placement
            app.piano_roll.placing_note = Some(step);
        }
    }
}

/// Nudge a note at the current cursor position
fn nudge_note(app: &mut App, delta: i32) {
    let pitch = app.piano_roll.pitch;
    let step = app.piano_roll.step;
    let channel = app.channel_rack.channel;

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

/// Transpose a note at the current cursor position
fn transpose_note(app: &mut App, delta: i32) {
    let pitch = app.piano_roll.pitch;
    let step = app.piano_roll.step;
    let channel = app.channel_rack.channel;

    // Find note at cursor
    let note_info = app
        .get_current_pattern()
        .and_then(|p| p.get_note_at(channel, pitch, step))
        .map(|n| (n.id.clone(), n.pitch, n.start_step, n.duration));

    if let Some((note_id, old_pitch, start_step, duration)) = note_info {
        let new_pitch =
            (old_pitch as i32 + delta).clamp(PIANO_MIN_PITCH as i32, PIANO_MAX_PITCH as i32) as u8;
        if new_pitch != old_pitch {
            if let Some(pattern) = app.get_current_pattern_mut() {
                // Remove old note
                pattern.remove_note(channel, &note_id);
                // Add new note at transposed pitch
                let note = crate::sequencer::Note::new(new_pitch, start_step, duration);
                pattern.add_note(channel, note);
            }
            app.mark_dirty();
        }
    }
}

/// Get notes in range as YankedNote data (relative offsets from anchor)
fn get_piano_roll_data(app: &App, range: &vim::Range) -> Vec<crate::sequencer::YankedNote> {
    use crate::sequencer::YankedNote;

    let channel = app.channel_rack.channel;
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
    let channel = app.channel_rack.channel;
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

    let channel = app.channel_rack.channel;
    let cursor_pitch = app.piano_roll.pitch;
    let cursor_step = app.piano_roll.step;

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

// ============================================================================
// Mouse handling
// ============================================================================

use super::mouse::MouseAction;

/// Handle mouse actions for piano roll
///
/// This mirrors the keyboard handler pattern - receives actions from MouseState
/// and executes component-specific behavior.
pub fn handle_mouse_action(action: &MouseAction, app: &mut App) {
    match action {
        MouseAction::Click { x, y, .. } => {
            // Look up which cell was clicked
            if let Some((vim_row, step)) = app.screen_areas.piano_roll_cell_at(*x, *y) {
                // Exit visual mode if active
                if app.vim_piano_roll.is_visual() {
                    let cursor_row = pitch_to_row(app.piano_roll.pitch);
                    let cursor = vim::Position::new(cursor_row, app.piano_roll.step);
                    let actions = app.vim_piano_roll.process_key('\x1b', false, cursor);
                    for action in actions {
                        execute_piano_roll_vim_action(action, app);
                    }
                }

                // Convert vim row to pitch and move cursor
                let pitch = row_to_pitch(vim_row).clamp(PIANO_MIN_PITCH, PIANO_MAX_PITCH);
                app.piano_roll.pitch = pitch;
                app.piano_roll.step = step.min(PIANO_NUM_STEPS - 1);
                update_piano_viewport(app);

                // Toggle note placement (like pressing x/Enter)
                handle_piano_roll_toggle(app);
            }
        }

        MouseAction::DoubleClick { x, y } => {
            // Double-click to delete note at position
            if let Some((vim_row, step)) = app.screen_areas.piano_roll_cell_at(*x, *y) {
                let pitch = row_to_pitch(vim_row).clamp(PIANO_MIN_PITCH, PIANO_MAX_PITCH);
                let channel = app.channel_rack.channel;

                // Find and delete note at this position
                let note_id = app
                    .get_current_pattern()
                    .and_then(|p| p.get_note_at(channel, pitch, step))
                    .map(|n| n.id.clone());

                if let Some(id) = note_id {
                    if let Some(pattern) = app.get_current_pattern_mut() {
                        pattern.remove_note(channel, &id);
                        app.mark_dirty();
                    }
                }
            }
        }

        MouseAction::DragStart { x, y, .. } => {
            // Start note placement or selection
            if let Some((vim_row, step)) = app.screen_areas.piano_roll_cell_at(*x, *y) {
                let pitch = row_to_pitch(vim_row).clamp(PIANO_MIN_PITCH, PIANO_MAX_PITCH);
                app.piano_roll.pitch = pitch;
                app.piano_roll.step = step.min(PIANO_NUM_STEPS - 1);
                update_piano_viewport(app);

                // Start note placement
                app.piano_roll.placing_note = Some(step);
            }
        }

        MouseAction::DragMove { x, y, .. } => {
            // Update end position for note being placed
            if app.piano_roll.placing_note.is_some() {
                if let Some((_vim_row, step)) = app.screen_areas.piano_roll_cell_at(*x, *y) {
                    // Update step for note end (pitch stays at start)
                    app.piano_roll.step = step.min(PIANO_NUM_STEPS - 1);
                }
            }
        }

        MouseAction::DragEnd { x, y, .. } => {
            // Finish note placement
            if let Some(start_step) = app.piano_roll.placing_note {
                if let Some((_vim_row, end_step)) = app.screen_areas.piano_roll_cell_at(*x, *y) {
                    let end_step = end_step.min(PIANO_NUM_STEPS - 1);
                    let min_step = start_step.min(end_step);
                    let max_step = start_step.max(end_step);
                    let duration = max_step - min_step + 1;

                    // Copy values before mutable borrow
                    let pitch = app.piano_roll.pitch;
                    let channel = app.channel_rack.channel;
                    let note = crate::sequencer::Note::new(pitch, min_step, duration);
                    if let Some(pattern) = app.get_current_pattern_mut() {
                        pattern.add_note(channel, note);
                    }
                    app.mark_dirty();
                }
                app.piano_roll.placing_note = None;
            }
        }

        MouseAction::Scroll { delta, .. } => {
            // Scroll pitch viewport
            if *delta < 0 {
                // Scroll up (higher pitches)
                app.piano_roll.viewport_top =
                    (app.piano_roll.viewport_top + 3).min(PIANO_MAX_PITCH);
            } else {
                // Scroll down (lower pitches)
                app.piano_roll.viewport_top = app
                    .piano_roll
                    .viewport_top
                    .saturating_sub(3)
                    .max(PIANO_MIN_PITCH + 20);
            }
        }

        MouseAction::RightClick { x, y } => {
            // Show context menu for piano roll
            if let Some((vim_row, step)) = app.screen_areas.piano_roll_cell_at(*x, *y) {
                use crate::ui::context_menu::{piano_roll_menu, MenuContext};

                let pitch = row_to_pitch(vim_row).clamp(PIANO_MIN_PITCH, PIANO_MAX_PITCH);
                let channel = app.channel_rack.channel;

                // Check if there's a note at this position
                let has_note = app
                    .get_current_pattern()
                    .and_then(|p| p.get_note_at(channel, pitch, step))
                    .is_some();

                let items = piano_roll_menu(has_note);
                let context = MenuContext::PianoRoll { pitch, step };
                app.context_menu.show(*x, *y, items, context);
            }
        }
    }
}
