//! Piano roll panel input handling
//!
//! Uses vim state machine - routes keys through vim and executes returned actions

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::mode::ViewMode;

use super::common::key_to_vim_char;
use super::vim::{self, VimAction};

/// Piano roll pitch range constants
pub const PIANO_MIN_PITCH: u8 = 36; // C2
pub const PIANO_MAX_PITCH: u8 = 84; // C6
pub const PIANO_PITCH_RANGE: usize = (PIANO_MAX_PITCH - PIANO_MIN_PITCH + 1) as usize; // 49 pitches
pub const PIANO_NUM_STEPS: usize = 16;

/// Convert pitch to vim row (row 0 = highest pitch)
///
/// In the piano roll, higher pitches appear at the top of the screen (lower row numbers),
/// while lower pitches appear at the bottom (higher row numbers).
///
/// # Arguments
/// * `pitch` - MIDI pitch value (typically 36-84 for C2-C6)
///
/// # Returns
/// Vim row index where row 0 = PIANO_MAX_PITCH
pub fn pitch_to_row(pitch: u8) -> usize {
    (PIANO_MAX_PITCH.saturating_sub(pitch)) as usize
}

/// Convert vim row to pitch
///
/// Inverse of `pitch_to_row`. Converts a vim row index back to MIDI pitch.
///
/// # Arguments
/// * `row` - Vim row index (0 = highest pitch)
///
/// # Returns
/// MIDI pitch value
pub fn row_to_pitch(row: usize) -> u8 {
    PIANO_MAX_PITCH.saturating_sub(row as u8)
}

/// Check if placing a note would collide with existing notes
///
/// A collision occurs when a note at the given pitch overlaps in time with
/// any existing note at the same pitch.
///
/// # Arguments
/// * `notes` - Slice of existing notes to check against
/// * `pitch` - MIDI pitch of the note being placed
/// * `start` - Starting step of the note being placed
/// * `end` - Ending step (inclusive) of the note being placed
///
/// # Returns
/// `true` if the note would collide with any existing note
#[allow(dead_code)] // Public for testing, will be used in future note placement validation
pub fn check_note_collision(
    notes: &[crate::sequencer::Note],
    pitch: u8,
    start: usize,
    end: usize,
) -> bool {
    notes.iter().any(|n| {
        if n.pitch != pitch {
            return false;
        }
        let note_end = n.start_step + n.duration - 1;
        // Check for overlap: not (end < n.start OR start > note_end)
        !(end < n.start_step || start > note_end)
    })
}

/// Handle keyboard input for piano roll
pub fn handle_key(key: KeyEvent, app: &mut App) {
    // Handle Escape to return to channel rack (if not placing a note and not in visual mode)
    if key.code == KeyCode::Esc
        && app.ui.cursors.piano_roll.placing_note.is_none()
        && !app.ui.vim.piano_roll.is_visual()
    {
        app.set_view_mode(ViewMode::ChannelRack);
        return;
    }

    // Component-specific keys (not handled by vim)
    match key.code {
        // 's' to preview the current pitch
        KeyCode::Char('s') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !app.ui.is_previewing {
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
    let cursor_row = pitch_to_row(app.ui.cursors.piano_roll.pitch);
    let cursor = vim::Position::new(cursor_row, app.ui.cursors.piano_roll.step);

    // Update vim dimensions for current pitch range
    app.ui
        .vim
        .piano_roll
        .update_dimensions(PIANO_PITCH_RANGE, PIANO_NUM_STEPS);

    // Let vim process the key
    let actions = app.ui.vim.piano_roll.process_key(ch, ctrl, cursor);

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
            app.ui.cursors.piano_roll.pitch =
                row_to_pitch(pos.row).clamp(PIANO_MIN_PITCH, PIANO_MAX_PITCH);
            app.ui.cursors.piano_roll.step = pos.col.min(PIANO_NUM_STEPS - 1);

            // Update viewport
            update_piano_viewport(app);
        }

        VimAction::Toggle => {
            handle_piano_roll_toggle(app);
        }

        VimAction::Yank(range) => {
            let data = get_piano_roll_data(app, &range);
            app.ui.vim.piano_roll.store_yank(data, range.range_type);
        }

        VimAction::Delete(range) => {
            let data = get_piano_roll_data(app, &range);
            app.ui.vim.piano_roll.store_delete(data, range.range_type);
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
                app.ui.cursors.piano_roll.viewport_top = app
                    .ui
                    .cursors
                    .piano_roll
                    .viewport_top
                    .saturating_sub(delta as u8);
                app.ui.cursors.piano_roll.viewport_top = app
                    .ui
                    .cursors
                    .piano_roll
                    .viewport_top
                    .max(PIANO_MIN_PITCH + visible_rows);
            } else {
                // Scroll up (higher pitches)
                app.ui.cursors.piano_roll.viewport_top =
                    (app.ui.cursors.piano_roll.viewport_top + (-delta) as u8).min(PIANO_MAX_PITCH);
            }
            // Keep cursor visible
            if app.ui.cursors.piano_roll.pitch > app.ui.cursors.piano_roll.viewport_top {
                app.ui.cursors.piano_roll.pitch = app.ui.cursors.piano_roll.viewport_top;
            } else if app.ui.cursors.piano_roll.pitch
                < app
                    .ui
                    .cursors
                    .piano_roll
                    .viewport_top
                    .saturating_sub(visible_rows)
            {
                app.ui.cursors.piano_roll.pitch = app
                    .ui
                    .cursors
                    .piano_roll
                    .viewport_top
                    .saturating_sub(visible_rows);
            }
        }

        VimAction::NextTab => {
            // Switch to Playlist view and focus it
            // Use set_view_mode() to record position in global jumplist
            app.set_view_mode(crate::mode::ViewMode::Playlist);
            app.ui.mode.switch_panel(crate::app::Panel::Playlist);
        }

        VimAction::PrevTab => {
            // Switch to Playlist view (only 2 tabs, so same as next)
            // Use set_view_mode() to record position in global jumplist
            app.set_view_mode(crate::mode::ViewMode::Playlist);
            app.ui.mode.switch_panel(crate::app::Panel::Playlist);
        }

        VimAction::RecordJump => {
            // Record current position in global jumplist before a jump movement (G, gg)
            let current = app.current_jump_position();
            app.ui.global_jumplist.push(current);
        }
    }
}

/// Update piano roll viewport to keep cursor visible
fn update_piano_viewport(app: &mut App) {
    // Keep cursor in viewport (viewport_top is highest visible pitch)
    if app.ui.cursors.piano_roll.pitch > app.ui.cursors.piano_roll.viewport_top {
        app.ui.cursors.piano_roll.viewport_top = app.ui.cursors.piano_roll.pitch;
    }
    // Assume ~20 visible rows
    if app.ui.cursors.piano_roll.pitch < app.ui.cursors.piano_roll.viewport_top.saturating_sub(20) {
        app.ui.cursors.piano_roll.viewport_top = app.ui.cursors.piano_roll.pitch + 10;
    }
}

/// Handle note placement toggle in piano roll
fn handle_piano_roll_toggle(app: &mut App) {
    use crate::sequencer::Note;

    let pitch = app.ui.cursors.piano_roll.pitch;
    let step = app.ui.cursors.piano_roll.step;
    let channel_idx = app.ui.cursors.channel_rack.channel;
    let pattern_id = app.current_pattern;

    if let Some(start_step) = app.ui.cursors.piano_roll.placing_note {
        // Finish placing note
        let min_step = start_step.min(step);
        let max_step = start_step.max(step);
        let duration = max_step - min_step + 1;

        let note = Note::new(pitch, min_step, duration);
        // Use history-aware add for undo/redo support
        app.add_note_with_history(note);
        app.ui.cursors.piano_roll.placing_note = None;
    } else {
        // Check for existing note at cursor
        let existing = app
            .channels
            .get(channel_idx)
            .and_then(|c| c.get_pattern(pattern_id))
            .and_then(|s| s.get_note_at(pitch, step))
            .map(|n| (n.id.clone(), n.start_step));

        if let Some((note_id, start)) = existing {
            // Remove existing note with history and start new placement from its position
            app.remove_note_with_history(note_id);
            app.ui.cursors.piano_roll.placing_note = Some(start);
        } else {
            // Start new placement
            app.ui.cursors.piano_roll.placing_note = Some(step);
        }
    }
}

/// Nudge a note at the current cursor position
fn nudge_note(app: &mut App, delta: i32) {
    let pitch = app.ui.cursors.piano_roll.pitch;
    let step = app.ui.cursors.piano_roll.step;
    let channel_idx = app.ui.cursors.channel_rack.channel;
    let pattern_id = app.current_pattern;

    // Find note at cursor
    let note_info = app
        .channels
        .get(channel_idx)
        .and_then(|c| c.get_pattern(pattern_id))
        .and_then(|s| s.get_note_at(pitch, step))
        .map(|n| (n.id.clone(), n.start_step, n.duration));

    if let Some((note_id, start_step, duration)) = note_info {
        let new_start = (start_step as i32 + delta).clamp(0, 15 - duration as i32 + 1) as usize;
        if new_start != start_step {
            if let Some(channel) = app.channels.get_mut(channel_idx) {
                if let Some(slice) = channel.get_pattern_mut(pattern_id) {
                    // Remove old note
                    slice.remove_note(&note_id);
                    // Add new note at nudged position
                    let note = crate::sequencer::Note::new(pitch, new_start, duration);
                    slice.add_note(note);
                }
            }
            app.mark_dirty();
        }
    }
}

/// Transpose a note at the current cursor position
fn transpose_note(app: &mut App, delta: i32) {
    let pitch = app.ui.cursors.piano_roll.pitch;
    let step = app.ui.cursors.piano_roll.step;
    let channel_idx = app.ui.cursors.channel_rack.channel;
    let pattern_id = app.current_pattern;

    // Find note at cursor
    let note_info = app
        .channels
        .get(channel_idx)
        .and_then(|c| c.get_pattern(pattern_id))
        .and_then(|s| s.get_note_at(pitch, step))
        .map(|n| (n.id.clone(), n.pitch, n.start_step, n.duration));

    if let Some((note_id, old_pitch, start_step, duration)) = note_info {
        let new_pitch =
            (old_pitch as i32 + delta).clamp(PIANO_MIN_PITCH as i32, PIANO_MAX_PITCH as i32) as u8;
        if new_pitch != old_pitch {
            if let Some(channel) = app.channels.get_mut(channel_idx) {
                if let Some(slice) = channel.get_pattern_mut(pattern_id) {
                    // Remove old note
                    slice.remove_note(&note_id);
                    // Add new note at transposed pitch
                    let note = crate::sequencer::Note::new(new_pitch, start_step, duration);
                    slice.add_note(note);
                }
            }
            app.mark_dirty();
        }
    }
}

/// Get notes in range as YankedNote data (relative offsets from anchor)
fn get_piano_roll_data(app: &App, range: &vim::Range) -> Vec<crate::sequencer::YankedNote> {
    use crate::sequencer::YankedNote;

    let channel_idx = app.ui.cursors.channel_rack.channel;
    let pattern_id = app.current_pattern;
    let (start, end) = range.normalized();

    // Convert vim rows to pitches
    let anchor_pitch = row_to_pitch(start.row);
    let min_pitch = row_to_pitch(end.row);
    let max_pitch = row_to_pitch(start.row);

    let mut yanked = Vec::new();

    let notes = app
        .channels
        .get(channel_idx)
        .and_then(|c| c.get_pattern(pattern_id))
        .map(|s| &s.notes);

    if let Some(notes) = notes {
        for note in notes {
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
    let channel_idx = app.ui.cursors.channel_rack.channel;
    let pattern_id = app.current_pattern;
    let (start, end) = range.normalized();

    let min_pitch = row_to_pitch(end.row);
    let max_pitch = row_to_pitch(start.row);

    // Collect IDs of notes to delete
    let to_delete: Vec<String> = app
        .channels
        .get(channel_idx)
        .and_then(|c| c.get_pattern(pattern_id))
        .map(|slice| {
            slice
                .notes
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
    if let Some(channel) = app.channels.get_mut(channel_idx) {
        if let Some(slice) = channel.get_pattern_mut(pattern_id) {
            for id in to_delete {
                slice.remove_note(&id);
            }
        }
    }
}

/// Paste notes from register at cursor position
fn paste_piano_roll_data(app: &mut App) {
    use crate::sequencer::Note;

    let channel_idx = app.ui.cursors.channel_rack.channel;
    let pattern_id = app.current_pattern;
    let pattern_length = app.get_current_pattern().map(|p| p.length).unwrap_or(16);
    let cursor_pitch = app.ui.cursors.piano_roll.pitch;
    let cursor_step = app.ui.cursors.piano_roll.step;

    // Clone register data to avoid borrow issues
    let paste_data = app.ui.vim.piano_roll.get_register().cloned();

    if let Some(register) = paste_data {
        if let Some(channel) = app.channels.get_mut(channel_idx) {
            let slice = channel.get_or_create_pattern(pattern_id, pattern_length);
            for yanked in &register.data {
                let new_pitch = (cursor_pitch as i32 + yanked.pitch_offset)
                    .clamp(PIANO_MIN_PITCH as i32, PIANO_MAX_PITCH as i32)
                    as u8;
                let new_step = (cursor_step as i32 + yanked.step_offset)
                    .clamp(0, (PIANO_NUM_STEPS - yanked.duration) as i32)
                    as usize;

                let note = Note::new(new_pitch, new_step, yanked.duration);
                slice.add_note(note);
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
            if let Some((vim_row, step)) = app.ui.screen_areas.piano_roll_cell_at(*x, *y) {
                // Exit visual mode if active
                if app.ui.vim.piano_roll.is_visual() {
                    let cursor_row = pitch_to_row(app.ui.cursors.piano_roll.pitch);
                    let cursor = vim::Position::new(cursor_row, app.ui.cursors.piano_roll.step);
                    let actions = app.ui.vim.piano_roll.process_key('\x1b', false, cursor);
                    for action in actions {
                        execute_piano_roll_vim_action(action, app);
                    }
                }

                // Convert vim row to pitch and move cursor
                let pitch = row_to_pitch(vim_row).clamp(PIANO_MIN_PITCH, PIANO_MAX_PITCH);
                app.ui.cursors.piano_roll.pitch = pitch;
                app.ui.cursors.piano_roll.step = step.min(PIANO_NUM_STEPS - 1);
                update_piano_viewport(app);

                // Toggle note placement (like pressing x/Enter)
                handle_piano_roll_toggle(app);
            }
        }

        MouseAction::DoubleClick { x, y } => {
            // Double-click to delete note at position
            if let Some((vim_row, step)) = app.ui.screen_areas.piano_roll_cell_at(*x, *y) {
                let pitch = row_to_pitch(vim_row).clamp(PIANO_MIN_PITCH, PIANO_MAX_PITCH);
                let channel_idx = app.ui.cursors.channel_rack.channel;
                let pattern_id = app.current_pattern;

                // Find and delete note at this position
                let note_id = app
                    .channels
                    .get(channel_idx)
                    .and_then(|c| c.get_pattern(pattern_id))
                    .and_then(|s| s.get_note_at(pitch, step))
                    .map(|n| n.id.clone());

                if let Some(id) = note_id {
                    if let Some(channel) = app.channels.get_mut(channel_idx) {
                        if let Some(slice) = channel.get_pattern_mut(pattern_id) {
                            slice.remove_note(&id);
                            app.mark_dirty();
                        }
                    }
                }
            }
        }

        MouseAction::DragStart { x, y, .. } => {
            // Start note placement or selection
            if let Some((vim_row, step)) = app.ui.screen_areas.piano_roll_cell_at(*x, *y) {
                let pitch = row_to_pitch(vim_row).clamp(PIANO_MIN_PITCH, PIANO_MAX_PITCH);
                app.ui.cursors.piano_roll.pitch = pitch;
                app.ui.cursors.piano_roll.step = step.min(PIANO_NUM_STEPS - 1);
                update_piano_viewport(app);

                // Start note placement
                app.ui.cursors.piano_roll.placing_note = Some(step);
            }
        }

        MouseAction::DragMove { x, y, .. } => {
            // Update end position for note being placed
            if app.ui.cursors.piano_roll.placing_note.is_some() {
                if let Some((_vim_row, step)) = app.ui.screen_areas.piano_roll_cell_at(*x, *y) {
                    // Update step for note end (pitch stays at start)
                    app.ui.cursors.piano_roll.step = step.min(PIANO_NUM_STEPS - 1);
                }
            }
        }

        MouseAction::DragEnd { x, y, .. } => {
            // Finish note placement
            if let Some(start_step) = app.ui.cursors.piano_roll.placing_note {
                if let Some((_vim_row, end_step)) = app.ui.screen_areas.piano_roll_cell_at(*x, *y) {
                    let end_step = end_step.min(PIANO_NUM_STEPS - 1);
                    let min_step = start_step.min(end_step);
                    let max_step = start_step.max(end_step);
                    let duration = max_step - min_step + 1;

                    // Copy values before mutable borrow
                    let pitch = app.ui.cursors.piano_roll.pitch;
                    let channel_idx = app.ui.cursors.channel_rack.channel;
                    let pattern_id = app.current_pattern;
                    let pattern_length = app.get_current_pattern().map(|p| p.length).unwrap_or(16);
                    let note = crate::sequencer::Note::new(pitch, min_step, duration);
                    if let Some(channel) = app.channels.get_mut(channel_idx) {
                        let slice = channel.get_or_create_pattern(pattern_id, pattern_length);
                        slice.add_note(note);
                    }
                    app.mark_dirty();
                }
                app.ui.cursors.piano_roll.placing_note = None;
            }
        }

        MouseAction::Scroll { delta, .. } => {
            // Scroll pitch viewport
            if *delta < 0 {
                // Scroll up (higher pitches)
                app.ui.cursors.piano_roll.viewport_top =
                    (app.ui.cursors.piano_roll.viewport_top + 3).min(PIANO_MAX_PITCH);
            } else {
                // Scroll down (lower pitches)
                app.ui.cursors.piano_roll.viewport_top = app
                    .ui
                    .cursors
                    .piano_roll
                    .viewport_top
                    .saturating_sub(3)
                    .max(PIANO_MIN_PITCH + 20);
            }
        }

        MouseAction::RightClick { x, y } => {
            // Show context menu for piano roll
            if let Some((vim_row, step)) = app.ui.screen_areas.piano_roll_cell_at(*x, *y) {
                use crate::ui::context_menu::{piano_roll_menu, MenuContext};

                let pitch = row_to_pitch(vim_row).clamp(PIANO_MIN_PITCH, PIANO_MAX_PITCH);
                let channel_idx = app.ui.cursors.channel_rack.channel;
                let pattern_id = app.current_pattern;

                // Check if there's a note at this position
                let has_note = app
                    .channels
                    .get(channel_idx)
                    .and_then(|c| c.get_pattern(pattern_id))
                    .and_then(|s| s.get_note_at(pitch, step))
                    .is_some();

                let items = piano_roll_menu(has_note);
                let context = MenuContext::PianoRoll { pitch, step };
                app.ui.context_menu.show(*x, *y, items, context);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::Note;

    // ========================================================================
    // Pitch conversion tests
    // ========================================================================

    #[test]
    fn test_pitch_to_row_max_pitch() {
        // MAX_PITCH (84 = C6) should be row 0
        assert_eq!(pitch_to_row(PIANO_MAX_PITCH), 0);
    }

    #[test]
    fn test_pitch_to_row_min_pitch() {
        // MIN_PITCH (36 = C2) should be row 48
        assert_eq!(pitch_to_row(PIANO_MIN_PITCH), 48);
    }

    #[test]
    fn test_pitch_to_row_middle_c() {
        // Middle C (60 = C4) should be row 24
        assert_eq!(pitch_to_row(60), 24);
    }

    #[test]
    fn test_row_to_pitch_zero() {
        // Row 0 should be MAX_PITCH
        assert_eq!(row_to_pitch(0), PIANO_MAX_PITCH);
    }

    #[test]
    fn test_row_to_pitch_last() {
        // Row 48 should be MIN_PITCH
        assert_eq!(row_to_pitch(48), PIANO_MIN_PITCH);
    }

    #[test]
    fn test_pitch_row_roundtrip() {
        // Converting pitch -> row -> pitch should be identity
        for pitch in PIANO_MIN_PITCH..=PIANO_MAX_PITCH {
            let row = pitch_to_row(pitch);
            let back = row_to_pitch(row);
            assert_eq!(back, pitch, "Roundtrip failed for pitch {}", pitch);
        }
    }

    // ========================================================================
    // Note collision tests
    // ========================================================================

    #[test]
    fn test_collision_no_notes() {
        let notes: Vec<Note> = vec![];
        assert!(!check_note_collision(&notes, 60, 0, 3));
    }

    #[test]
    fn test_collision_different_pitch() {
        let notes = vec![Note::new(60, 4, 4)]; // C4 at steps 4-7
                                               // Check for collision at different pitch (C5)
        assert!(!check_note_collision(&notes, 72, 4, 7));
    }

    #[test]
    fn test_collision_before_note() {
        let notes = vec![Note::new(60, 4, 4)]; // C4 at steps 4-7
                                               // Place note at steps 0-3 (just before)
        assert!(!check_note_collision(&notes, 60, 0, 3));
    }

    #[test]
    fn test_collision_after_note() {
        let notes = vec![Note::new(60, 4, 4)]; // C4 at steps 4-7
                                               // Place note at steps 8-11 (just after)
        assert!(!check_note_collision(&notes, 60, 8, 11));
    }

    #[test]
    fn test_collision_overlap_start() {
        let notes = vec![Note::new(60, 4, 4)]; // C4 at steps 4-7
                                               // Place note at steps 2-5 (overlaps start)
        assert!(check_note_collision(&notes, 60, 2, 5));
    }

    #[test]
    fn test_collision_overlap_end() {
        let notes = vec![Note::new(60, 4, 4)]; // C4 at steps 4-7
                                               // Place note at steps 6-9 (overlaps end)
        assert!(check_note_collision(&notes, 60, 6, 9));
    }

    #[test]
    fn test_collision_contained() {
        let notes = vec![Note::new(60, 4, 4)]; // C4 at steps 4-7
                                               // Place note at steps 5-6 (inside existing)
        assert!(check_note_collision(&notes, 60, 5, 6));
    }

    #[test]
    fn test_collision_contains() {
        let notes = vec![Note::new(60, 4, 4)]; // C4 at steps 4-7
                                               // Place note at steps 2-10 (contains existing)
        assert!(check_note_collision(&notes, 60, 2, 10));
    }

    #[test]
    fn test_collision_exact_overlap() {
        let notes = vec![Note::new(60, 4, 4)]; // C4 at steps 4-7
                                               // Place note at exact same position
        assert!(check_note_collision(&notes, 60, 4, 7));
    }

    #[test]
    fn test_collision_touching_boundary() {
        let notes = vec![Note::new(60, 4, 4)]; // C4 at steps 4-7
                                               // Place note touching at boundary (end of new = start of existing)
                                               // Step 3 ends just before step 4 starts, so no collision
        assert!(!check_note_collision(&notes, 60, 0, 3));
        // Step 8 starts just after step 7 ends, so no collision
        assert!(!check_note_collision(&notes, 60, 8, 10));
    }
}
