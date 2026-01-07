//! Browser panel input handling

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Panel};

use super::mouse::MouseAction;

/// Handle keyboard input for browser
pub fn handle_key(key: KeyEvent, app: &mut App) {
    // Handle Escape to cancel selection mode
    if key.code == KeyCode::Esc {
        if app.ui.browser.selection_mode {
            app.ui.browser.cancel_selection();
        }
        app.ui.mode.switch_panel(Panel::ChannelRack);
        return;
    }

    // Track previous cursor for auto-preview
    let prev_cursor = app.ui.browser.cursor;

    match key.code {
        // Toggle between Samples and Plugins mode with Shift+Tab or 't'
        KeyCode::BackTab | KeyCode::Char('t') => {
            app.ui.browser.toggle_mode();
            return; // Don't trigger auto-preview after mode switch
        }

        // Navigation
        KeyCode::Char('j') | KeyCode::Down => {
            app.ui.browser.move_down(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.ui.browser.move_up(1);
        }

        // Expand/collapse folders
        KeyCode::Char('l') | KeyCode::Right => {
            app.ui.browser.expand();
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.ui.browser.collapse_or_parent();
        }

        // Jump to first/last (vim style)
        KeyCode::Char('0') => {
            app.ui.browser.go_to_top();
        }
        KeyCode::Char('$') => {
            app.ui.browser.go_to_bottom();
        }
        KeyCode::Char('g') => {
            // gg to go to top (simplified - single g works)
            app.ui.browser.go_to_top();
        }
        KeyCode::Char('G') => {
            app.ui.browser.go_to_bottom();
        }

        // Enter or 'o' to select sample or toggle folder
        KeyCode::Enter | KeyCode::Char('o') => {
            if let Some(entry) = app.ui.browser.current_entry().cloned() {
                if entry.is_dir {
                    // Toggle folder expansion
                    app.ui.browser.toggle_or_select();
                } else if app.ui.browser.selection_mode {
                    // Complete selection and assign sample/plugin to channel
                    let browser_mode = app.ui.browser.mode;
                    if let Some((channel_idx, relative_path)) = app.ui.browser.complete_selection()
                    {
                        match browser_mode {
                            crate::browser::BrowserMode::Samples => {
                                app.set_channel_sample(channel_idx, relative_path);
                            }
                            crate::browser::BrowserMode::Plugins => {
                                app.set_channel_plugin(channel_idx, relative_path);
                            }
                        }
                        app.ui.mode.switch_panel(Panel::ChannelRack);
                    }
                } else {
                    // Just preview the file
                    let full_path = app.project.samples_path().join(
                        entry
                            .path
                            .strip_prefix(app.ui.browser.root_path())
                            .unwrap_or(&entry.path),
                    );
                    // Browser previews go directly to master, not through mixer tracks
                    app.audio.preview_sample_to_master(&full_path);
                }
            }
            return; // Don't trigger auto-preview
        }

        _ => {
            return;
        } // Don't trigger auto-preview for unhandled keys
    }

    // Auto-preview on cursor move (only for audio files in samples mode)
    if app.ui.browser.cursor != prev_cursor {
        if let Some(entry) = app.ui.browser.current_entry() {
            if !entry.is_dir && app.ui.browser.mode == crate::browser::BrowserMode::Samples {
                let full_path = app.project.samples_path().join(
                    entry
                        .path
                        .strip_prefix(app.ui.browser.root_path())
                        .unwrap_or(&entry.path),
                );
                // Browser previews go directly to master, not through mixer tracks
                app.audio.preview_sample_to_master(&full_path);
            }
        }
    }
}

// ============================================================================
// Mouse handling
// ============================================================================

/// Handle mouse actions for browser
///
/// This mirrors the keyboard handler pattern - receives actions from MouseState
/// and executes component-specific behavior.
pub fn handle_mouse_action(action: &MouseAction, app: &mut App) {
    match action {
        MouseAction::Click { x, y, .. } => {
            // Check if clicking on a browser item
            if let Some(visible_idx) = app.ui.screen_areas.browser_item_at(*x, *y) {
                // visible_idx corresponds directly to visible_entries index
                // Move cursor to clicked item
                let prev_cursor = app.ui.browser.cursor;
                app.ui.browser.cursor =
                    visible_idx.min(app.ui.browser.visible_entries.len().saturating_sub(1));

                // Auto-preview on click (only for audio files in samples mode)
                if app.ui.browser.cursor != prev_cursor {
                    if let Some(entry) = app.ui.browser.current_entry() {
                        if !entry.is_dir
                            && app.ui.browser.mode == crate::browser::BrowserMode::Samples
                        {
                            let full_path = app.project.samples_path().join(
                                entry
                                    .path
                                    .strip_prefix(app.ui.browser.root_path())
                                    .unwrap_or(&entry.path),
                            );
                            // Browser previews go directly to master
                            app.audio.preview_sample_to_master(&full_path);
                        }
                    }
                }
            }
        }

        MouseAction::DoubleClick { x, y } => {
            // Double-click to expand folder or select file
            if let Some(visible_idx) = app.ui.screen_areas.browser_item_at(*x, *y) {
                app.ui.browser.cursor =
                    visible_idx.min(app.ui.browser.visible_entries.len().saturating_sub(1));

                if let Some(entry) = app.ui.browser.current_entry().cloned() {
                    if entry.is_dir {
                        // Toggle folder expansion
                        app.ui.browser.toggle_or_select();
                    } else if app.ui.browser.selection_mode {
                        // Complete selection and assign sample/plugin to channel
                        let browser_mode = app.ui.browser.mode;
                        if let Some((channel_idx, relative_path)) =
                            app.ui.browser.complete_selection()
                        {
                            match browser_mode {
                                crate::browser::BrowserMode::Samples => {
                                    app.set_channel_sample(channel_idx, relative_path);
                                }
                                crate::browser::BrowserMode::Plugins => {
                                    app.set_channel_plugin(channel_idx, relative_path);
                                }
                            }
                            app.ui.mode.switch_panel(Panel::ChannelRack);
                        }
                    } else {
                        // Just preview the file - browser previews go directly to master
                        let full_path = app.project.samples_path().join(
                            entry
                                .path
                                .strip_prefix(app.ui.browser.root_path())
                                .unwrap_or(&entry.path),
                        );
                        app.audio.preview_sample_to_master(&full_path);
                    }
                }
            }
        }

        MouseAction::DragStart { .. } => {
            // Browser doesn't support drag operations
        }

        MouseAction::DragMove { .. } => {
            // Browser doesn't support drag operations
        }

        MouseAction::DragEnd { .. } => {
            // Browser doesn't support drag operations
        }

        MouseAction::Scroll { delta, .. } => {
            // Scroll the browser list
            if *delta < 0 {
                // Scroll up
                app.ui.browser.move_up(3);
            } else {
                // Scroll down
                app.ui.browser.move_down(3);
            }
        }

        MouseAction::RightClick { x, y } => {
            // Show context menu for browser
            if let Some(visible_idx) = app.ui.screen_areas.browser_item_at(*x, *y) {
                use crate::ui::context_menu::{browser_menu, MenuContext};

                app.ui.browser.cursor =
                    visible_idx.min(app.ui.browser.visible_entries.len().saturating_sub(1));

                // Check if this is a file (not directory)
                let is_file = app
                    .ui
                    .browser
                    .current_entry()
                    .map(|e| !e.is_dir)
                    .unwrap_or(false);

                let items = browser_menu(is_file);
                if !items.is_empty() {
                    let context = MenuContext::Browser {
                        item_idx: visible_idx,
                    };
                    app.ui.context_menu.show(*x, *y, items, context);
                }
            }
        }
    }
}
