//! Mixer panel input handling

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;
use crate::effects::EFFECT_SLOTS;
use crate::mixer::{TrackId, NUM_TRACKS};

/// Approximate number of visible tracks (will be recalculated at render time)
const VISIBLE_TRACKS_ESTIMATE: usize = 10;

/// Item indices for effects panel focus
const PAN_ITEM: usize = EFFECT_SLOTS; // 8
const VOLUME_ITEM: usize = EFFECT_SLOTS + 1; // 9
const TOTAL_ITEMS: usize = EFFECT_SLOTS + 2; // 10

/// Handle keyboard input for mixer
pub fn handle_key(key: KeyEvent, app: &mut App) {
    if app.mixer.effects_focused {
        handle_effects_key(key, app);
    } else {
        handle_tracks_key(key, app);
    }
}

/// Handle keys when effects chain is focused
fn handle_effects_key(key: KeyEvent, app: &mut App) {
    let selected = app.mixer.selected_effect_slot;
    let on_bypass = app.mixer.on_bypass_column;

    match key.code {
        // Escape exits effects focus (or exits bypass column first)
        KeyCode::Esc => {
            if on_bypass {
                app.mixer.on_bypass_column = false;
            } else {
                app.mixer.effects_focused = false;
            }
        }
        // j/k navigate items (effects, pan, volume)
        KeyCode::Char('j') | KeyCode::Down => {
            if app.mixer.selected_effect_slot < TOTAL_ITEMS - 1 {
                app.mixer.selected_effect_slot += 1;
                // Reset to effect column when moving to pan/volume
                if app.mixer.selected_effect_slot >= EFFECT_SLOTS {
                    app.mixer.on_bypass_column = false;
                }
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.mixer.selected_effect_slot > 0 {
                app.mixer.selected_effect_slot -= 1;
                // Reset to effect column when coming from pan/volume
                if selected >= EFFECT_SLOTS && app.mixer.selected_effect_slot < EFFECT_SLOTS {
                    app.mixer.on_bypass_column = false;
                }
            }
        }
        // gg = first slot, G = last item (volume)
        KeyCode::Char('g') => {
            app.mixer.selected_effect_slot = 0;
            app.mixer.on_bypass_column = false;
        }
        KeyCode::Char('G') => {
            app.mixer.selected_effect_slot = TOTAL_ITEMS - 1;
            app.mixer.on_bypass_column = false;
        }
        // h = move to bypass column (for effect slots) or adjust pan/volume
        KeyCode::Char('h') | KeyCode::Left => {
            if selected < EFFECT_SLOTS {
                // Move to bypass column
                app.mixer.on_bypass_column = true;
            } else if selected == PAN_ITEM {
                adjust_pan(app, -0.05);
            } else if selected == VOLUME_ITEM {
                app.adjust_mixer_volume(-0.02);
            }
        }
        // l = move to effect column (for effect slots) or adjust pan/volume
        KeyCode::Char('l') | KeyCode::Right => {
            if selected < EFFECT_SLOTS {
                // Move to effect column
                app.mixer.on_bypass_column = false;
            } else if selected == PAN_ITEM {
                adjust_pan(app, 0.05);
            } else if selected == VOLUME_ITEM {
                app.adjust_mixer_volume(0.02);
            }
        }
        // H/L for fine adjustment (pan/volume only)
        KeyCode::Char('H') => {
            if selected == PAN_ITEM {
                adjust_pan(app, -0.01);
            } else if selected == VOLUME_ITEM {
                app.adjust_mixer_volume(-0.005);
            }
        }
        KeyCode::Char('L') => {
            if selected == PAN_ITEM {
                adjust_pan(app, 0.01);
            } else if selected == VOLUME_ITEM {
                app.adjust_mixer_volume(0.005);
            }
        }
        // c to center pan (when pan is selected)
        KeyCode::Char('c') => {
            if selected == PAN_ITEM {
                let track_id = TrackId(app.mixer.selected_track);
                app.mixer.set_pan(track_id, 0.0);
                app.sync_mixer_to_audio();
                app.mark_dirty();
            }
        }
        // Enter = toggle bypass (on bypass column) or edit effect (on effect column)
        KeyCode::Enter => {
            if selected < EFFECT_SLOTS {
                if on_bypass {
                    app.toggle_effect_bypass();
                } else {
                    app.open_effect_editor();
                }
            }
        }
        // d = delete effect - only for effect slots, only on effect column
        KeyCode::Char('d') => {
            if selected < EFFECT_SLOTS && !on_bypass {
                app.delete_effect();
            }
        }
        _ => {}
    }
}

/// Handle keys when tracks are focused (default mode)
fn handle_tracks_key(key: KeyEvent, app: &mut App) {
    let max_track = NUM_TRACKS - 1;

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => {
            if app.mixer.selected_track > 0 {
                app.mixer.selected_track -= 1;
                update_viewport(app);
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if app.mixer.selected_track < max_track {
                app.mixer.selected_track += 1;
                update_viewport(app);
            }
        }
        // gg = first track (Master), G = last track
        KeyCode::Char('g') => {
            app.mixer.selected_track = 0;
            app.mixer.viewport_offset = 0;
        }
        KeyCode::Char('G') => {
            app.mixer.selected_track = max_track;
            update_viewport(app);
        }
        // Enter = focus effects chain
        KeyCode::Enter => {
            app.mixer.effects_focused = true;
            app.mixer.selected_effect_slot = 0;
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
        // < > for pan (±5%), { } for pan (±1%)
        KeyCode::Char('<') => {
            adjust_pan(app, -0.1);
        }
        KeyCode::Char('>') => {
            adjust_pan(app, 0.1);
        }
        KeyCode::Char('{') => {
            adjust_pan(app, -0.02);
        }
        KeyCode::Char('}') => {
            adjust_pan(app, 0.02);
        }
        // c to center pan
        KeyCode::Char('c') => {
            let track_id = TrackId(app.mixer.selected_track);
            app.mixer.set_pan(track_id, 0.0);
            app.sync_mixer_to_audio();
            app.mark_dirty();
        }
        KeyCode::Char('m') => {
            // Don't allow muting master
            if app.mixer.selected_track != 0 {
                app.toggle_mute();
            }
        }
        KeyCode::Char('s') => {
            // Don't allow soloing master
            if app.mixer.selected_track != 0 {
                app.toggle_solo();
            }
        }
        // 0-9 for direct volume set: 0 = 100%, 1 = 10%, 2 = 20%, etc.
        KeyCode::Char('0') => {
            let track_id = TrackId(app.mixer.selected_track);
            app.mixer.set_volume(track_id, 1.0);
            app.sync_mixer_to_audio();
            app.mark_dirty();
        }
        KeyCode::Char(c @ '1'..='9') => {
            let track_id = TrackId(app.mixer.selected_track);
            let volume = (c as u8 - b'0') as f32 * 0.1;
            app.mixer.set_volume(track_id, volume);
            app.sync_mixer_to_audio();
            app.mark_dirty();
        }
        _ => {}
    }
}

/// Update viewport to keep selected track visible
fn update_viewport(app: &mut App) {
    let selected = app.mixer.selected_track;

    // Master (track 0) is always visible, so viewport starts at track 1
    if selected == 0 {
        // Viewing master, no viewport adjustment needed
        return;
    }

    // Calculate visible range (track indices 1..=N that are visible)
    let viewport_start = app.mixer.viewport_offset + 1;
    let viewport_end = viewport_start + VISIBLE_TRACKS_ESTIMATE - 1;

    if selected < viewport_start {
        // Scroll left
        app.mixer.viewport_offset = (selected - 1).max(0);
    } else if selected > viewport_end {
        // Scroll right
        app.mixer.viewport_offset = selected - VISIBLE_TRACKS_ESTIMATE;
    }
}

/// Adjust pan for the selected track
fn adjust_pan(app: &mut App, delta: f32) {
    let track_id = TrackId(app.mixer.selected_track);
    let current = app.mixer.track(track_id).pan;
    let new_pan = (current + delta).clamp(-1.0, 1.0);
    app.mixer.set_pan(track_id, new_pan);
    app.sync_mixer_to_audio();
    app.mark_dirty();
}
