//! Browser panel input handling

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Panel};

use super::mouse::MouseAction;

/// Handle keyboard input for browser
pub fn handle_key(key: KeyEvent, app: &mut App) {
    // Handle Escape to cancel selection mode
    if key.code == KeyCode::Esc {
        if app.browser.selection_mode {
            app.browser.cancel_selection();
        }
        app.mode.switch_panel(Panel::ChannelRack);
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
                        app.mode.switch_panel(Panel::ChannelRack);
                    }
                } else {
                    // Just preview the file
                    let full_path = app.project_path.join("samples").join(
                        entry
                            .path
                            .strip_prefix(app.browser.root_path())
                            .unwrap_or(&entry.path),
                    );
                    let gen_idx = app
                        .browser
                        .target_channel
                        .unwrap_or(app.channel_rack.channel);
                    app.audio.preview_sample(&full_path, gen_idx);
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
                let gen_idx = app
                    .browser
                    .target_channel
                    .unwrap_or(app.channel_rack.channel);
                app.audio.preview_sample(&full_path, gen_idx);
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
            if let Some(visible_idx) = app.screen_areas.browser_item_at(*x, *y) {
                // visible_idx corresponds directly to visible_entries index
                // Move cursor to clicked item
                let prev_cursor = app.browser.cursor;
                app.browser.cursor = visible_idx.min(app.browser.visible_entries.len().saturating_sub(1));

                // Auto-preview on click (only for audio files in samples mode)
                if app.browser.cursor != prev_cursor {
                    if let Some(entry) = app.browser.current_entry() {
                        if !entry.is_dir && app.browser.mode == crate::browser::BrowserMode::Samples
                        {
                            let full_path = app.project_path.join("samples").join(
                                entry
                                    .path
                                    .strip_prefix(app.browser.root_path())
                                    .unwrap_or(&entry.path),
                            );
                            app.audio.preview_sample(&full_path, app.channel_rack.channel);
                        }
                    }
                }
            }
        }

        MouseAction::DoubleClick { x, y } => {
            // Double-click to expand folder or select file
            if let Some(visible_idx) = app.screen_areas.browser_item_at(*x, *y) {
                app.browser.cursor = visible_idx.min(app.browser.visible_entries.len().saturating_sub(1));

                if let Some(entry) = app.browser.current_entry().cloned() {
                    if entry.is_dir {
                        // Toggle folder expansion
                        app.browser.toggle_or_select();
                    } else if app.browser.selection_mode {
                        // Complete selection and assign sample/plugin to channel
                        let browser_mode = app.browser.mode;
                        if let Some((channel_idx, relative_path)) = app.browser.complete_selection()
                        {
                            match browser_mode {
                                crate::browser::BrowserMode::Samples => {
                                    app.set_channel_sample(channel_idx, relative_path);
                                }
                                crate::browser::BrowserMode::Plugins => {
                                    app.set_channel_plugin(channel_idx, relative_path);
                                }
                            }
                            app.mode.switch_panel(Panel::ChannelRack);
                        }
                    } else {
                        // Just preview the file
                        let full_path = app.project_path.join("samples").join(
                            entry
                                .path
                                .strip_prefix(app.browser.root_path())
                                .unwrap_or(&entry.path),
                        );
                        app.audio.preview_sample(&full_path, app.channel_rack.channel);
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
                app.browser.move_up(3);
            } else {
                // Scroll down
                app.browser.move_down(3);
            }
        }

        MouseAction::RightClick { x, y } => {
            // Show context menu for browser
            if let Some(visible_idx) = app.screen_areas.browser_item_at(*x, *y) {
                use crate::ui::context_menu::{browser_menu, MenuContext};

                app.browser.cursor = visible_idx.min(app.browser.visible_entries.len().saturating_sub(1));

                // Check if this is a file (not directory)
                let is_file = app
                    .browser
                    .current_entry()
                    .map(|e| !e.is_dir)
                    .unwrap_or(false);

                let items = browser_menu(is_file);
                if !items.is_empty() {
                    let context = MenuContext::Browser { item_idx: visible_idx };
                    app.context_menu.show(*x, *y, items, context);
                }
            }
        }
    }
}
