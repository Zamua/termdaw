//! Mixer panel input handling

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;

/// Handle keyboard input for mixer
pub fn handle_key(key: KeyEvent, app: &mut App) {
    let max_channel = app.channel_count().saturating_sub(1);

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => {
            app.mixer.selected_channel = app.mixer.selected_channel.saturating_sub(1);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.mixer.selected_channel = (app.mixer.selected_channel + 1).min(max_channel);
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
            if let Some(channel) = app.channels.get_mut(app.mixer.selected_channel) {
                channel.volume = 1.0;
                if channel.is_plugin() {
                    app.audio
                        .plugin_set_volume(app.mixer.selected_channel, channel.volume);
                }
                app.mark_dirty();
            }
        }
        KeyCode::Char(c @ '1'..='9') => {
            if let Some(channel) = app.channels.get_mut(app.mixer.selected_channel) {
                channel.volume = (c as u8 - b'0') as f32 * 0.1;
                if channel.is_plugin() {
                    app.audio
                        .plugin_set_volume(app.mixer.selected_channel, channel.volume);
                }
                app.mark_dirty();
            }
        }
        _ => {}
    }
}

// ============================================================================
// Mouse handling
// ============================================================================

use super::mouse::MouseAction;

/// Handle mouse actions for mixer
///
/// This mirrors the keyboard handler pattern - receives actions from MouseState
/// and executes component-specific behavior.
pub fn handle_mouse_action(action: &MouseAction, app: &mut App) {
    match action {
        MouseAction::Click { x, y, .. } => {
            // Check for mute button click
            if let Some(ch_idx) = app.screen_areas.mixer_mute_at(*x, *y) {
                app.mixer.selected_channel = ch_idx;
                app.toggle_mute();
                return;
            }

            // Check for solo button click
            if let Some(ch_idx) = app.screen_areas.mixer_solo_at(*x, *y) {
                app.mixer.selected_channel = ch_idx;
                app.toggle_solo();
                return;
            }

            // Check for fader click - select channel and set volume
            if let Some((ch_idx, y_pos)) = app.screen_areas.mixer_fader_at(*x, *y) {
                app.mixer.selected_channel = ch_idx;

                // Calculate volume from y position (inverted - top = 1.0, bottom = 0.0)
                if let Some(fader_rect) = app.screen_areas.mixer_faders.get(&ch_idx) {
                    let volume = 1.0 - (y_pos as f32 / fader_rect.height.max(1) as f32);
                    if let Some(channel) = app.channels.get_mut(ch_idx) {
                        channel.volume = volume.clamp(0.0, 1.0);
                        if channel.is_plugin() {
                            app.audio.plugin_set_volume(ch_idx, channel.volume);
                        }
                        app.mark_dirty();
                    }
                }
            }
        }

        MouseAction::DoubleClick { x, y } => {
            // Double-click on fader to reset to 100%
            if let Some((ch_idx, _)) = app.screen_areas.mixer_fader_at(*x, *y) {
                if let Some(channel) = app.channels.get_mut(ch_idx) {
                    channel.volume = 1.0;
                    if channel.is_plugin() {
                        app.audio.plugin_set_volume(ch_idx, channel.volume);
                    }
                    app.mark_dirty();
                }
            }
        }

        MouseAction::DragStart { x, y, .. } => {
            // Start fader drag
            if let Some((ch_idx, _)) = app.screen_areas.mixer_fader_at(*x, *y) {
                app.mixer.selected_channel = ch_idx;
            }
        }

        MouseAction::DragMove { y, .. } => {
            // Adjust fader while dragging
            let ch_idx = app.mixer.selected_channel;
            if let Some(fader_rect) = app.screen_areas.mixer_faders.get(&ch_idx) {
                // Calculate volume from absolute y position relative to fader
                let relative_y = (*y).saturating_sub(fader_rect.y);
                let volume = 1.0 - (relative_y as f32 / fader_rect.height.max(1) as f32);
                if let Some(channel) = app.channels.get_mut(ch_idx) {
                    channel.volume = volume.clamp(0.0, 1.0);
                    if channel.is_plugin() {
                        app.audio.plugin_set_volume(ch_idx, channel.volume);
                    }
                    app.mark_dirty();
                }
            }
        }

        MouseAction::DragEnd { .. } => {
            // Fader drag complete
        }

        MouseAction::Scroll { x, y, delta } => {
            // Scroll on fader to adjust volume
            if let Some((ch_idx, _)) = app.screen_areas.mixer_fader_at(*x, *y) {
                app.mixer.selected_channel = ch_idx;
                // Scroll up = increase volume, scroll down = decrease
                let adjustment = if *delta < 0 { 0.02 } else { -0.02 };
                app.adjust_mixer_volume(adjustment);
            }
        }

        MouseAction::RightClick { x, y } => {
            // Show context menu for mixer
            if let Some((ch_idx, _)) = app.screen_areas.mixer_fader_at(*x, *y) {
                use crate::ui::context_menu::{mixer_menu, MenuContext};

                app.mixer.selected_channel = ch_idx;
                let items = mixer_menu();
                let context = MenuContext::Mixer { channel: ch_idx };
                app.context_menu.show(*x, *y, items, context);
            }
        }
    }
}
