//! Input handling for keyboard and mouse events
//!
//! This module is split into submodules by panel:
//! - `channel_rack` - Channel rack step sequencer
//! - `piano_roll` - Piano roll note editor
//! - `playlist` - Arrangement/playlist view
//! - `mixer` - Mixer panel
//! - `browser` - File browser
//! - `common` - Shared utilities

pub mod vim;

mod browser;
mod channel_rack;
mod common;
mod mixer;
mod piano_roll;
mod playlist;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use tui_input::backend::crossterm::EventHandler;

use crate::app::{App, Panel};

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

    // Allow key repeat events for navigation/scroll keys only
    if key.kind == KeyEventKind::Repeat {
        let is_nav_key = matches!(
            key.code,
            KeyCode::Char('h')
                | KeyCode::Char('j')
                | KeyCode::Char('k')
                | KeyCode::Char('l')
                | KeyCode::Char('e')
                | KeyCode::Char('y')
                | KeyCode::Char('d')
                | KeyCode::Char('u')
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Left
                | KeyCode::Right
        );
        if !is_nav_key {
            return false;
        }
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
    match app.mode.current_panel() {
        Panel::ChannelRack => channel_rack::handle_key(key, app),
        Panel::Browser => browser::handle_key(key, app),
        Panel::Mixer => mixer::handle_key(key, app),
        Panel::Playlist => playlist::handle_key(key, app),
        Panel::PianoRoll => piano_roll::handle_key(key, app),
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
                common::execute_command(cmd, app)
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
            common::send_param_to_plugin(app);
            false
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.plugin_editor.adjust_value(1.0);
            common::send_param_to_plugin(app);
            false
        }
        // Fine adjust with shift
        KeyCode::Char('H') => {
            app.plugin_editor.adjust_value(-0.1);
            common::send_param_to_plugin(app);
            false
        }
        KeyCode::Char('L') => {
            app.plugin_editor.adjust_value(0.1);
            common::send_param_to_plugin(app);
            false
        }
        _ => false,
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
