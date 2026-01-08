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
        // d = delete effect (and yank) - only for effect slots, only on effect column
        KeyCode::Char('d') => {
            if selected < EFFECT_SLOTS && !on_bypass {
                let track = app.mixer.selected_track;
                let slot = app.mixer.selected_effect_slot;
                // Store effect in register before deleting (vim-like yank on delete)
                if let Some(effect) = app.mixer.tracks[track].effects[slot].clone() {
                    app.ui.effect_register = Some(effect);
                }
                app.dispatch(AppCommand::RemoveEffect { track, slot });
            }
        }
        // p = paste effect from register - only for effect slots, only on effect column
        KeyCode::Char('p') => {
            if selected < EFFECT_SLOTS && !on_bypass {
                if let Some(effect) = app.ui.effect_register.clone() {
                    let track = app.mixer.selected_track;
                    let slot = app.mixer.selected_effect_slot;
                    app.dispatch(AppCommand::AddEffect {
                        track,
                        slot,
                        effect_type: effect.effect_type,
                    });
                    // Copy parameters from register to the new effect
                    if let Some(new_effect) = &mut app.mixer.tracks[track].effects[slot] {
                        new_effect.params = effect.params;
                    }
                }
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

// ============================================================================
// Mouse handling
// ============================================================================

use super::mouse::MouseAction;

/// Handle mouse actions for mixer
pub fn handle_mouse_action(action: &MouseAction, app: &mut App) {
    match action {
        MouseAction::Click { x, y, .. } => {
            // Check mute button first
            if let Some(track) = app.ui.screen_areas.mixer_mute_at(*x, *y) {
                app.dispatch(AppCommand::ToggleTrackMute(track));
                return;
            }

            // Check solo button
            if let Some(track) = app.ui.screen_areas.mixer_solo_at(*x, *y) {
                // Can't solo master track (track 0)
                if track > 0 {
                    app.dispatch(AppCommand::ToggleTrackSolo(track));
                }
                return;
            }

            // Check fader - click to set volume
            if let Some((track, y_in_fader)) = app.ui.screen_areas.mixer_fader_at(*x, *y) {
                // Get fader height from the stored rect
                if let Some(rect) = app.ui.screen_areas.mixer_faders.get(&track) {
                    let fader_height = rect.height as f32;
                    // Fader is inverted: top = 100%, bottom = 0%
                    let volume = 1.0 - (y_in_fader as f32 / fader_height);
                    let volume = volume.clamp(0.0, 1.0);
                    app.dispatch(AppCommand::SetTrackVolume { track, volume });
                }
                // Select this track
                app.mixer.selected_track = track;
                return;
            }

            // Fallback: click anywhere on channel strip to select track and show effects
            if let Some(track) = app.ui.screen_areas.mixer_channel_strip_at(*x, *y) {
                app.mixer.selected_track = track;
                app.mixer.effects_focused = true;
            }
        }

        MouseAction::DragStart { x, y, .. } => {
            // Start fader drag - select the track
            if let Some((track, _)) = app.ui.screen_areas.mixer_fader_at(*x, *y) {
                app.mixer.selected_track = track;
            }
        }

        MouseAction::DragMove { x, y, .. } => {
            // Update volume during drag
            if let Some((track, y_in_fader)) = app.ui.screen_areas.mixer_fader_at(*x, *y) {
                if let Some(rect) = app.ui.screen_areas.mixer_faders.get(&track) {
                    let fader_height = rect.height as f32;
                    let volume = 1.0 - (y_in_fader as f32 / fader_height);
                    let volume = volume.clamp(0.0, 1.0);
                    app.dispatch(AppCommand::SetTrackVolume { track, volume });
                }
            }
        }

        MouseAction::DragEnd { .. } => {
            // Drag complete, nothing special needed
        }

        MouseAction::DoubleClick { x, y } => {
            // Double-click on fader to reset volume to 100%
            if let Some((track, _)) = app.ui.screen_areas.mixer_fader_at(*x, *y) {
                app.dispatch(AppCommand::SetTrackVolume { track, volume: 1.0 });
            }
        }

        MouseAction::Scroll { x, y, delta } => {
            // Scroll on fader to adjust volume
            if let Some((track, _)) = app.ui.screen_areas.mixer_fader_at(*x, *y) {
                let current_volume = app.mixer.tracks[track].volume;
                let step = 0.05; // 5% per scroll tick
                let new_volume = if *delta < 0 {
                    // Scroll down = decrease volume
                    (current_volume - step).max(0.0)
                } else {
                    // Scroll up = increase volume
                    (current_volume + step).min(1.0)
                };
                app.dispatch(AppCommand::SetTrackVolume {
                    track,
                    volume: new_volume,
                });
            }
        }

        MouseAction::RightClick { x, y } => {
            // Show context menu for mixer track
            if let Some((track, _)) = app.ui.screen_areas.mixer_fader_at(*x, *y) {
                use crate::ui::context_menu::{mixer_menu, MenuContext};

                let items = mixer_menu();
                let context = MenuContext::Mixer { channel: track };
                app.ui.context_menu.show(*x, *y, items, context);
            }
        }
    }
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

        // Create a minimal empty project.json so template is not copied
        let empty_project = crate::project::ProjectFile::new("test-project");
        crate::project::save_project(&project_path, &empty_project)
            .expect("Failed to create test project");

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

    // ========================================================================
    // Effect undo/redo tests
    // ========================================================================

    #[test]
    fn test_add_effect_is_undoable() {
        let (mut app, _temp) = create_test_app();

        // Add an effect via dispatch
        app.dispatch(AppCommand::AddEffect {
            track: 1,
            slot: 0,
            effect_type: EffectType::Filter,
        });

        // Effect should be present
        assert!(
            app.mixer.tracks[1].effects[0].is_some(),
            "Effect should be added"
        );

        // Undo should remove the effect
        let mut history = std::mem::take(&mut app.history);
        history.undo(&mut app);
        app.history = history;

        assert!(
            app.mixer.tracks[1].effects[0].is_none(),
            "Effect should be removed after undo"
        );

        // Redo should restore the effect
        let mut history = std::mem::take(&mut app.history);
        history.redo(&mut app);
        app.history = history;

        assert!(
            app.mixer.tracks[1].effects[0].is_some(),
            "Effect should be restored after redo"
        );
    }

    #[test]
    fn test_remove_effect_is_undoable() {
        let (mut app, _temp) = create_test_app();

        // First add an effect directly (not through dispatch)
        let effect = EffectSlot::new(EffectType::Delay);
        app.mixer.tracks[1].effects[0] = Some(effect);

        // Remove via dispatch
        app.dispatch(AppCommand::RemoveEffect { track: 1, slot: 0 });

        // Effect should be gone
        assert!(
            app.mixer.tracks[1].effects[0].is_none(),
            "Effect should be removed"
        );

        // Undo should restore the effect
        let mut history = std::mem::take(&mut app.history);
        history.undo(&mut app);
        app.history = history;

        assert!(
            app.mixer.tracks[1].effects[0].is_some(),
            "Effect should be restored after undo"
        );
        assert_eq!(
            app.mixer.tracks[1].effects[0].as_ref().unwrap().effect_type,
            EffectType::Delay,
            "Restored effect should be Delay"
        );
    }

    // ========================================================================
    // Mouse handling tests
    // ========================================================================

    use ratatui::layout::Rect;

    /// Register a fake fader area for testing
    fn register_fader(app: &mut App, track: usize, rect: Rect) {
        app.ui.screen_areas.mixer_faders.insert(track, rect);
    }

    /// Register a fake mute button area for testing
    fn register_mute_button(app: &mut App, track: usize, rect: Rect) {
        app.ui.screen_areas.mixer_mute_buttons.insert(track, rect);
    }

    /// Register a fake solo button area for testing
    fn register_solo_button(app: &mut App, track: usize, rect: Rect) {
        app.ui.screen_areas.mixer_solo_buttons.insert(track, rect);
    }

    /// Register a fake channel strip area for testing
    fn register_channel_strip(app: &mut App, track: usize, rect: Rect) {
        app.ui.screen_areas.mixer_channel_strips.insert(track, rect);
    }

    #[test]
    fn test_mouse_click_mute_button_toggles_mute() {
        let (mut app, _temp) = create_test_app();

        // Register a mute button at (10, 10) with size 3x1
        register_mute_button(&mut app, 1, Rect::new(10, 10, 3, 1));

        // Track 1 should start unmuted
        assert!(!app.mixer.tracks[1].muted);

        // Click on mute button
        let action = MouseAction::Click {
            x: 11,
            y: 10,
            button: crossterm::event::MouseButton::Left,
        };
        handle_mouse_action(&action, &mut app);

        // Track should now be muted
        assert!(
            app.mixer.tracks[1].muted,
            "Track should be muted after click"
        );
    }

    #[test]
    fn test_mouse_click_solo_button_toggles_solo() {
        let (mut app, _temp) = create_test_app();

        // Register a solo button at (15, 10) with size 3x1
        register_solo_button(&mut app, 1, Rect::new(15, 10, 3, 1));

        // Track 1 should start not soloed
        assert!(!app.mixer.tracks[1].solo);

        // Click on solo button
        let action = MouseAction::Click {
            x: 16,
            y: 10,
            button: crossterm::event::MouseButton::Left,
        };
        handle_mouse_action(&action, &mut app);

        // Track should now be soloed
        assert!(
            app.mixer.tracks[1].solo,
            "Track should be soloed after click"
        );
    }

    #[test]
    fn test_mouse_click_solo_on_master_does_nothing() {
        let (mut app, _temp) = create_test_app();

        // Register a solo button for master track (track 0)
        register_solo_button(&mut app, 0, Rect::new(15, 10, 3, 1));

        // Master track should start not soloed
        assert!(!app.mixer.tracks[0].solo);

        // Click on solo button
        let action = MouseAction::Click {
            x: 16,
            y: 10,
            button: crossterm::event::MouseButton::Left,
        };
        handle_mouse_action(&action, &mut app);

        // Master should still not be soloed (can't solo master)
        assert!(
            !app.mixer.tracks[0].solo,
            "Master track should not be soloable"
        );
    }

    #[test]
    fn test_mouse_click_fader_sets_volume() {
        let (mut app, _temp) = create_test_app();

        // Register a fader at (20, 10) with height 20 (y: 10-29)
        register_fader(&mut app, 1, Rect::new(20, 10, 5, 20));

        // Set initial volume
        app.mixer.tracks[1].volume = 0.5;

        // Click at top of fader (y=10) should set volume to ~100%
        let action = MouseAction::Click {
            x: 22,
            y: 10,
            button: crossterm::event::MouseButton::Left,
        };
        handle_mouse_action(&action, &mut app);

        // Volume should be close to 1.0 (top of fader)
        assert!(
            app.mixer.tracks[1].volume > 0.9,
            "Click at top should set high volume, got {}",
            app.mixer.tracks[1].volume
        );

        // Click at bottom of fader (y=29) should set volume to ~0%
        let action = MouseAction::Click {
            x: 22,
            y: 29,
            button: crossterm::event::MouseButton::Left,
        };
        handle_mouse_action(&action, &mut app);

        // Volume should be close to 0.0 (bottom of fader)
        assert!(
            app.mixer.tracks[1].volume < 0.1,
            "Click at bottom should set low volume, got {}",
            app.mixer.tracks[1].volume
        );
    }

    #[test]
    fn test_mouse_click_fader_selects_track() {
        let (mut app, _temp) = create_test_app();

        // Register faders for tracks 1 and 2
        register_fader(&mut app, 1, Rect::new(20, 10, 5, 20));
        register_fader(&mut app, 2, Rect::new(30, 10, 5, 20));

        // Start with track 1 selected
        app.mixer.selected_track = 1;

        // Click on track 2's fader
        let action = MouseAction::Click {
            x: 32,
            y: 15,
            button: crossterm::event::MouseButton::Left,
        };
        handle_mouse_action(&action, &mut app);

        // Track 2 should now be selected
        assert_eq!(
            app.mixer.selected_track, 2,
            "Clicking fader should select that track"
        );
    }

    #[test]
    fn test_mouse_scroll_adjusts_volume() {
        let (mut app, _temp) = create_test_app();

        // Register a fader
        register_fader(&mut app, 1, Rect::new(20, 10, 5, 20));

        // Set initial volume to 50%
        app.mixer.tracks[1].volume = 0.5;

        // Scroll up should increase volume
        let action = MouseAction::Scroll {
            x: 22,
            y: 15,
            delta: 1,
        };
        handle_mouse_action(&action, &mut app);

        assert!(
            app.mixer.tracks[1].volume > 0.5,
            "Scroll up should increase volume, got {}",
            app.mixer.tracks[1].volume
        );

        // Reset to 50%
        app.mixer.tracks[1].volume = 0.5;

        // Scroll down should decrease volume
        let action = MouseAction::Scroll {
            x: 22,
            y: 15,
            delta: -1,
        };
        handle_mouse_action(&action, &mut app);

        assert!(
            app.mixer.tracks[1].volume < 0.5,
            "Scroll down should decrease volume, got {}",
            app.mixer.tracks[1].volume
        );
    }

    #[test]
    fn test_mouse_double_click_resets_volume() {
        let (mut app, _temp) = create_test_app();

        // Register a fader
        register_fader(&mut app, 1, Rect::new(20, 10, 5, 20));

        // Set volume to something other than 100%
        app.mixer.tracks[1].volume = 0.3;

        // Double-click should reset to 100%
        let action = MouseAction::DoubleClick { x: 22, y: 15 };
        handle_mouse_action(&action, &mut app);

        assert_eq!(
            app.mixer.tracks[1].volume, 1.0,
            "Double-click should reset volume to 100%"
        );
    }

    #[test]
    fn test_mouse_drag_updates_volume() {
        let (mut app, _temp) = create_test_app();

        // Register a fader at (20, 10) with height 20
        register_fader(&mut app, 1, Rect::new(20, 10, 5, 20));

        // Set initial volume
        app.mixer.tracks[1].volume = 0.5;

        // Drag start
        let action = MouseAction::DragStart {
            x: 22,
            y: 20,
            button: crossterm::event::MouseButton::Left,
        };
        handle_mouse_action(&action, &mut app);

        // Track should be selected
        assert_eq!(app.mixer.selected_track, 1);

        // Drag move to top of fader
        let action = MouseAction::DragMove {
            x: 22,
            y: 10,
            start_x: 22,
            start_y: 20,
        };
        handle_mouse_action(&action, &mut app);

        // Volume should be high (near top)
        assert!(
            app.mixer.tracks[1].volume > 0.9,
            "Drag to top should set high volume"
        );

        // Drag move to bottom
        let action = MouseAction::DragMove {
            x: 22,
            y: 29,
            start_x: 22,
            start_y: 20,
        };
        handle_mouse_action(&action, &mut app);

        // Volume should be low (near bottom)
        assert!(
            app.mixer.tracks[1].volume < 0.1,
            "Drag to bottom should set low volume"
        );
    }

    #[test]
    fn test_mouse_right_click_shows_context_menu() {
        let (mut app, _temp) = create_test_app();

        // Register a fader
        register_fader(&mut app, 1, Rect::new(20, 10, 5, 20));

        // Context menu should not be visible initially
        assert!(!app.ui.context_menu.visible);

        // Right-click on fader
        let action = MouseAction::RightClick { x: 22, y: 15 };
        handle_mouse_action(&action, &mut app);

        // Context menu should now be visible
        assert!(
            app.ui.context_menu.visible,
            "Right-click should show context menu"
        );
    }

    #[test]
    fn test_mouse_click_channel_strip_selects_track() {
        let (mut app, _temp) = create_test_app();

        // Register channel strips for tracks 1 and 2
        // Track 1: x=10-19, y=0-30
        // Track 2: x=20-29, y=0-30
        register_channel_strip(&mut app, 1, Rect::new(10, 0, 10, 30));
        register_channel_strip(&mut app, 2, Rect::new(20, 0, 10, 30));

        // Start with track 1 selected
        app.mixer.selected_track = 1;

        // Click on track 2's channel strip (not on any control)
        let action = MouseAction::Click {
            x: 25,
            y: 5,
            button: crossterm::event::MouseButton::Left,
        };
        handle_mouse_action(&action, &mut app);

        // Track 2 should now be selected
        assert_eq!(
            app.mixer.selected_track, 2,
            "Clicking channel strip should select that track"
        );

        // Effects panel should be focused
        assert!(
            app.mixer.effects_focused,
            "Clicking channel strip should focus effects panel"
        );
    }
}
