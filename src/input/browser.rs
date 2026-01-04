//! Browser panel input handling

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Panel};

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
