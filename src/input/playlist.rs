//! Playlist panel input handling
//!
//! Uses vim state machine - routes keys through vim and executes returned actions

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;

use super::common::key_to_vim_char;
use super::vim::{self, VimAction};

/// Number of bars in the arrangement
const NUM_BARS: usize = 16;

/// Handle keyboard input for playlist
pub fn handle_key(key: KeyEvent, app: &mut App) {
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
    let Some((ch, ctrl)) = key_to_vim_char(key) else {
        return;
    };

    // Get cursor position in vim coordinates
    let cursor = vim::Position::new(app.playlist.row, app.playlist.bar);

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
            app.playlist.row = pos.row.min(pattern_count.saturating_sub(1));
            app.playlist.bar = pos.col.min(NUM_BARS);

            // Auto-scroll viewport
            let visible_rows = 10;
            if app.playlist.row >= app.playlist.viewport_top + visible_rows {
                app.playlist.viewport_top = app.playlist.row - visible_rows + 1;
            }
            if app.playlist.row < app.playlist.viewport_top {
                app.playlist.viewport_top = app.playlist.row;
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

        VimAction::ScrollViewport(delta) => {
            // Scroll viewport without moving cursor
            let visible_rows = 10usize;
            let pattern_count = get_playlist_pattern_count(app);
            if delta > 0 {
                // Scroll down
                let max_top = pattern_count.saturating_sub(visible_rows);
                app.playlist.viewport_top =
                    (app.playlist.viewport_top + delta as usize).min(max_top);
            } else {
                // Scroll up
                app.playlist.viewport_top =
                    app.playlist.viewport_top.saturating_sub((-delta) as usize);
            }
            // Keep cursor visible
            if app.playlist.row < app.playlist.viewport_top {
                app.playlist.row = app.playlist.viewport_top;
            } else if app.playlist.row >= app.playlist.viewport_top + visible_rows {
                app.playlist.row = app.playlist.viewport_top + visible_rows - 1;
            }
        }

        VimAction::NextTab => {
            // Switch to Channel Rack view and focus it
            app.view_mode = crate::mode::ViewMode::ChannelRack;
            app.mode.switch_panel(crate::app::Panel::ChannelRack);
        }

        VimAction::PrevTab => {
            // Switch to Channel Rack view (only 2 tabs, so same as next)
            app.view_mode = crate::mode::ViewMode::ChannelRack;
            app.mode.switch_panel(crate::app::Panel::ChannelRack);
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
    if app.playlist.bar == 0 {
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

    if let Some(pattern) = patterns.get(app.playlist.row) {
        let pattern_id = pattern.id;
        // Convert cursor_bar (1-16) to bar index (0-15)
        let bar = app.playlist.bar - 1;
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

    if let Some(pattern) = patterns.get(app.playlist.row) {
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

    let cursor_bar = app.playlist.bar.saturating_sub(1); // cursor_bar 1-16 -> bar 0-15

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

// ============================================================================
// Mouse handling
// ============================================================================

use super::mouse::MouseAction;

/// Handle mouse actions for playlist
///
/// This mirrors the keyboard handler pattern - receives actions from MouseState
/// and executes component-specific behavior.
pub fn handle_mouse_action(action: &MouseAction, app: &mut App) {
    match action {
        MouseAction::Click { x, y, .. } => {
            // Look up which cell was clicked
            if let Some((row, bar_col)) = app.screen_areas.playlist_cell_at(*x, *y) {
                let pattern_count = get_playlist_pattern_count(app);

                // Exit visual mode if active
                if app.vim_playlist.is_visual() {
                    let cursor = vim::Position::new(app.playlist.row, app.playlist.bar);
                    let actions = app.vim_playlist.process_key('\x1b', false, cursor);
                    for action in actions {
                        execute_playlist_vim_action(action, app);
                    }
                }

                // Move cursor
                app.playlist.row = row.min(pattern_count.saturating_sub(1));
                app.playlist.bar = bar_col.min(NUM_BARS);
                update_playlist_viewport(app);

                // Handle zone-specific click behavior
                if bar_col == 0 {
                    // Click on mute column
                    handle_playlist_mute(app);
                } else {
                    // Click on bar - toggle placement
                    handle_playlist_toggle(app);
                }
            }
        }

        MouseAction::DoubleClick { .. } => {
            // Double-click doesn't have special behavior in playlist
        }

        MouseAction::DragStart { x, y, .. } => {
            // Start selection drag
            if let Some((row, bar_col)) = app.screen_areas.playlist_cell_at(*x, *y) {
                let pattern_count = get_playlist_pattern_count(app);

                // Move cursor to start position (skip mute column for selection)
                if bar_col > 0 {
                    app.playlist.row = row.min(pattern_count.saturating_sub(1));
                    app.playlist.bar = bar_col.min(NUM_BARS);
                    update_playlist_viewport(app);

                    // Enter visual block mode
                    let cursor = vim::Position::new(row, bar_col);
                    let actions = app.vim_playlist.process_key('v', true, cursor); // Ctrl+v for block
                    for action in actions {
                        execute_playlist_vim_action(action, app);
                    }
                }
            }
        }

        MouseAction::DragMove { x, y, .. } => {
            // Extend selection
            if app.vim_playlist.is_visual() {
                if let Some((row, bar_col)) = app.screen_areas.playlist_cell_at(*x, *y) {
                    let pattern_count = get_playlist_pattern_count(app);
                    app.playlist.row = row.min(pattern_count.saturating_sub(1));
                    app.playlist.bar = bar_col.min(NUM_BARS);
                    update_playlist_viewport(app);
                }
            }
        }

        MouseAction::DragEnd { .. } => {
            // Selection is complete, vim stays in visual mode
        }

        MouseAction::Scroll { delta, .. } => {
            // Scroll viewport
            let pattern_count = get_playlist_pattern_count(app);
            if *delta < 0 {
                // Scroll up
                app.playlist.viewport_top = app.playlist.viewport_top.saturating_sub(3);
            } else {
                // Scroll down
                app.playlist.viewport_top =
                    (app.playlist.viewport_top + 3).min(pattern_count.saturating_sub(1));
            }
        }

        MouseAction::RightClick { x, y } => {
            // Show context menu for playlist
            if let Some((row, bar_col)) = app.screen_areas.playlist_cell_at(*x, *y) {
                use crate::ui::context_menu::{playlist_menu, MenuContext};

                // Check if there's a placement at this position
                let bar = bar_col.saturating_sub(1); // bar_col 0 is mute column
                let has_placement = if bar_col > 0 {
                    if let Some(pattern_id) = get_pattern_id_at_row(app, row) {
                        app.arrangement.get_placement_at(pattern_id, bar).is_some()
                    } else {
                        false
                    }
                } else {
                    false
                };

                let items = playlist_menu(has_placement);
                let context = MenuContext::Playlist { row, bar };
                app.context_menu.show(*x, *y, items, context);
            }
        }
    }
}

/// Update playlist viewport to keep cursor visible
fn update_playlist_viewport(app: &mut App) {
    let visible_rows = 10; // Approximate
    if app.playlist.row >= app.playlist.viewport_top + visible_rows {
        app.playlist.viewport_top = app.playlist.row - visible_rows + 1;
    }
    if app.playlist.row < app.playlist.viewport_top {
        app.playlist.viewport_top = app.playlist.row;
    }
}
