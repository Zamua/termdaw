//! Mixer panel input handling

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;
use crate::command::AppCommand;
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
                adjust_volume(app, -0.02);
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
                adjust_volume(app, 0.02);
            }
        }
        // H/L for fine adjustment (pan/volume only)
        KeyCode::Char('H') => {
            if selected == PAN_ITEM {
                adjust_pan(app, -0.01);
            } else if selected == VOLUME_ITEM {
                adjust_volume(app, -0.005);
            }
        }
        KeyCode::Char('L') => {
            if selected == PAN_ITEM {
                adjust_pan(app, 0.01);
            } else if selected == VOLUME_ITEM {
                adjust_volume(app, 0.005);
            }
        }
        // c to center pan (when pan is selected)
        KeyCode::Char('c') => {
            if selected == PAN_ITEM {
                app.dispatch(AppCommand::ResetTrackPan(app.mixer.selected_track));
            }
        }
        // Enter = toggle bypass (on bypass column) or edit effect (on effect column)
        KeyCode::Enter => {
            if selected < EFFECT_SLOTS {
                if on_bypass {
                    app.dispatch(AppCommand::ToggleEffectBypass {
                        track: app.mixer.selected_track,
                        slot: app.mixer.selected_effect_slot,
                    });
                } else {
                    app.open_effect_editor();
                }
            }
        }
        // d = delete effect - only for effect slots, only on effect column
        KeyCode::Char('d') => {
            if selected < EFFECT_SLOTS && !on_bypass {
                app.dispatch(AppCommand::RemoveEffect {
                    track: app.mixer.selected_track,
                    slot: app.mixer.selected_effect_slot,
                });
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
            adjust_volume(app, -0.01);
        }
        KeyCode::Char('J') => {
            adjust_volume(app, -0.05);
        }
        KeyCode::Char('k') => {
            adjust_volume(app, 0.01);
        }
        KeyCode::Char('K') => {
            adjust_volume(app, 0.05);
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
            app.dispatch(AppCommand::ResetTrackPan(app.mixer.selected_track));
        }
        KeyCode::Char('m') => {
            // Don't allow muting master
            if app.mixer.selected_track != 0 {
                app.dispatch(AppCommand::ToggleTrackMute(app.mixer.selected_track));
            }
        }
        KeyCode::Char('s') => {
            // Don't allow soloing master
            if app.mixer.selected_track != 0 {
                app.dispatch(AppCommand::ToggleTrackSolo(app.mixer.selected_track));
            }
        }
        // 0-9 for direct volume set: 0 = 100%, 1 = 10%, 2 = 20%, etc.
        KeyCode::Char('0') => {
            app.dispatch(AppCommand::SetTrackVolume {
                track: app.mixer.selected_track,
                volume: 1.0,
            });
        }
        KeyCode::Char(c @ '1'..='9') => {
            let volume = (c as u8 - b'0') as f32 * 0.1;
            app.dispatch(AppCommand::SetTrackVolume {
                track: app.mixer.selected_track,
                volume,
            });
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
    let track = app.mixer.selected_track;
    let current = app.mixer.track(TrackId(track)).pan;
    let new_pan = (current + delta).clamp(-1.0, 1.0);
    app.dispatch(AppCommand::SetTrackPan {
        track,
        pan: new_pan,
    });
}

/// Adjust volume for the selected track
fn adjust_volume(app: &mut App, delta: f32) {
    let track = app.mixer.selected_track;
    let current = app.mixer.track(TrackId(track)).volume;
    let new_volume = (current + delta).clamp(0.0, 1.0);
    app.dispatch(AppCommand::SetTrackVolume {
        track,
        volume: new_volume,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioHandle;
    use crate::effects::{EffectSlot, EffectType};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tempfile::TempDir;

    /// Create a test App
    fn create_test_app() -> (App, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_path = temp_dir.path().join("test-project");
        std::fs::create_dir_all(&project_path).expect("Failed to create project dir");
        let audio = AudioHandle::dummy();
        let app = App::new(project_path.to_str().unwrap(), audio);
        (app, temp_dir)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    // ========================================================================
    // Effect delete/paste tests
    // ========================================================================

    #[test]
    fn test_delete_effect_stores_in_register() {
        let (mut app, _temp) = create_test_app();

        // Add an effect to track 1, slot 0
        let effect = EffectSlot::new(EffectType::Filter);
        app.mixer.tracks[1].effects[0] = Some(effect);

        // Focus on track 1's effects
        app.mixer.selected_track = 1;
        app.mixer.effects_focused = true;
        app.mixer.selected_effect_slot = 0;
        app.mixer.on_bypass_column = false;

        // Press 'd' to delete
        handle_key(key(KeyCode::Char('d')), &mut app);

        // Effect should be removed from slot
        assert!(
            app.mixer.tracks[1].effects[0].is_none(),
            "Effect should be removed from slot"
        );

        // Effect should be stored in yank register
        assert!(
            app.ui.effect_register.is_some(),
            "Deleted effect should be stored in register"
        );
        assert_eq!(
            app.ui.effect_register.as_ref().unwrap().effect_type,
            EffectType::Filter,
            "Register should contain the deleted Filter effect"
        );
    }

    #[test]
    fn test_paste_effect_from_register() {
        let (mut app, _temp) = create_test_app();

        // Pre-populate the register with a Delay effect
        app.ui.effect_register = Some(EffectSlot::new(EffectType::Delay));

        // Focus on track 1's effects, slot 2 (empty)
        app.mixer.selected_track = 1;
        app.mixer.effects_focused = true;
        app.mixer.selected_effect_slot = 2;
        app.mixer.on_bypass_column = false;

        // Press 'p' to paste
        handle_key(key(KeyCode::Char('p')), &mut app);

        // Effect should be added to slot
        assert!(
            app.mixer.tracks[1].effects[2].is_some(),
            "Effect should be pasted to slot"
        );
        assert_eq!(
            app.mixer.tracks[1].effects[2].as_ref().unwrap().effect_type,
            EffectType::Delay,
            "Pasted effect should be Delay"
        );
    }

    #[test]
    fn test_delete_then_paste_workflow() {
        let (mut app, _temp) = create_test_app();

        // Add a Reverb effect to track 1, slot 0
        let effect = EffectSlot::new(EffectType::Reverb);
        app.mixer.tracks[1].effects[0] = Some(effect);

        // Focus and delete from slot 0
        app.mixer.selected_track = 1;
        app.mixer.effects_focused = true;
        app.mixer.selected_effect_slot = 0;
        app.mixer.on_bypass_column = false;
        handle_key(key(KeyCode::Char('d')), &mut app);

        // Move to slot 3 and paste
        app.mixer.selected_effect_slot = 3;
        handle_key(key(KeyCode::Char('p')), &mut app);

        // Slot 0 should be empty, slot 3 should have the effect
        assert!(
            app.mixer.tracks[1].effects[0].is_none(),
            "Original slot should be empty"
        );
        assert!(
            app.mixer.tracks[1].effects[3].is_some(),
            "Target slot should have effect"
        );
        assert_eq!(
            app.mixer.tracks[1].effects[3].as_ref().unwrap().effect_type,
            EffectType::Reverb,
            "Pasted effect should be Reverb"
        );
    }

    #[test]
    fn test_paste_does_nothing_with_empty_register() {
        let (mut app, _temp) = create_test_app();

        // Register is empty (None)
        app.ui.effect_register = None;

        // Focus on an empty slot
        app.mixer.selected_track = 1;
        app.mixer.effects_focused = true;
        app.mixer.selected_effect_slot = 0;
        app.mixer.on_bypass_column = false;

        // Press 'p' to paste
        handle_key(key(KeyCode::Char('p')), &mut app);

        // Slot should still be empty
        assert!(
            app.mixer.tracks[1].effects[0].is_none(),
            "Slot should remain empty when pasting from empty register"
        );
    }
}
