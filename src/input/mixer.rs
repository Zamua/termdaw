//! Mixer panel input handling

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;

/// Handle keyboard input for mixer
pub fn handle_key(key: KeyEvent, app: &mut App) {
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
