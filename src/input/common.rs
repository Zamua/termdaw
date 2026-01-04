//! Common utilities for input handlers

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::command_picker::Command;
use crate::mode::ViewMode;
use crate::plugin_host::params::{ParamDef, PluginParamId};

/// Convert a KeyEvent to vim-compatible (char, is_ctrl) tuple.
/// Returns None for keys that shouldn't be passed to vim.
///
/// Mappings:
/// - Char(c) -> (c, ctrl_pressed)
/// - Esc -> ('\x1b', false)
/// - Arrow keys -> hjkl
/// - Enter -> 'x' (toggle action)
pub fn key_to_vim_char(key: KeyEvent) -> Option<(char, bool)> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let ch = match key.code {
        KeyCode::Char(c) => c,
        KeyCode::Esc => '\x1b',
        KeyCode::Enter => 'x',
        KeyCode::Up => 'k',
        KeyCode::Down => 'j',
        KeyCode::Left => 'h',
        KeyCode::Right => 'l',
        _ => return None,
    };
    Some((ch, ctrl))
}

/// Send the currently selected parameter to the plugin and save to channel
pub fn send_param_to_plugin(app: &mut App) {
    let channel_idx = app.plugin_editor.channel_idx;
    if let Some(param) = app.plugin_editor.selected_param() {
        let param_value = param.value;

        // Get the param ID from the registry
        let param_id = PluginParamId::ALL
            .get(app.plugin_editor.selected_param)
            .copied();

        if let Some(param_id) = param_id {
            // Save to channel's plugin_params for persistence
            if let Some(channel) = app.channels.get_mut(channel_idx) {
                channel.plugin_params.insert(param_id, param_value);
            }

            // Get the CLAP param ID and normalized value from registry
            let def = ParamDef::get(param_id);
            if let Some(def) = def {
                let normalized = def.normalize(param_value);
                app.audio
                    .plugin_set_param(channel_idx, def.clap_id, normalized);
            }

            app.mark_dirty();
        }
    }
}

/// Execute a command from the picker
pub fn execute_command(cmd: Command, app: &mut App) -> bool {
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
