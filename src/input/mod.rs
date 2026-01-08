//! Input handling for keyboard and mouse events
//!
//! This module is split into submodules by panel:
//! - `channel_rack` - Channel rack step sequencer
//! - `piano_roll` - Piano roll note editor
//! - `playlist` - Arrangement/playlist view
//! - `mixer` - Mixer panel
//! - `browser` - File browser
//! - `common` - Shared utilities
//! - `mouse` - Encapsulated mouse state machine

pub mod context;
pub mod mouse;
pub mod vim;

mod browser;
mod channel_rack;
mod common;
mod mixer;
mod piano_roll;
mod playlist;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};
use tui_input::backend::crossterm::EventHandler;

use crate::app::{App, Panel};
use crate::command::AppCommand;

/// Handle a keyboard event
/// Returns true if the app should quit
pub fn handle_key(key: KeyEvent, app: &mut App) -> bool {
    // Handle key release events - only specific keys need release handling
    if key.kind == KeyEventKind::Release {
        // Only handle 's' release for stopping plugin preview
        if key.code == KeyCode::Char('s') && app.ui.is_previewing {
            if let Some(channel) = app.ui.preview_channel {
                app.stop_preview(channel);
            }
            // Trigger the release phase of the envelope animation
            app.ui.plugin_editor.stop_preview_animation();
        }
        // Ignore all other release events
        return false;
    }

    // Allow key repeat events for navigation/scroll keys only
    // Note: 'u' is NOT included - it's undo, not scroll/navigation
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
    if app.ui.command_picker.input.active {
        return handle_input_mode_key(key, app);
    }

    // Handle context menu (if visible)
    if app.ui.context_menu.visible {
        return handle_context_menu_key(key, app);
    }

    // Handle command picker (if visible)
    if app.ui.command_picker.visible {
        return handle_command_picker_key(key, app);
    }

    // Handle plugin editor modal (if visible)
    if app.ui.plugin_editor.visible {
        return handle_plugin_editor_key(key, app);
    }

    // Handle effect picker modal
    if app.ui.mode.is_effect_picker() {
        return handle_effect_picker_key(key, app);
    }

    // Handle effect editor modal
    if app.ui.mode.is_effect_editor() {
        return handle_effect_editor_key(key, app);
    }

    // Global keybindings (always active)
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match (key.code, ctrl) {
        // Tab to cycle focus
        (KeyCode::Tab, false) => {
            app.next_panel();
            return false;
        }

        // Space to show command picker
        (KeyCode::Char(' '), false) => {
            app.ui.command_picker.show();
            return false;
        }

        // Undo (u in normal mode)
        (KeyCode::Char('u'), false) if app.ui.mode.is_normal() => {
            // Take history out temporarily to avoid borrow conflict
            let mut history = std::mem::take(&mut app.history);
            history.undo(app);
            app.history = history;
            return false;
        }

        // Redo (Ctrl+R)
        (KeyCode::Char('r'), true) if app.ui.mode.is_normal() => {
            // Take history out temporarily to avoid borrow conflict
            let mut history = std::mem::take(&mut app.history);
            history.redo(app);
            app.history = history;
            return false;
        }

        // Jump back (Ctrl+O)
        (KeyCode::Char('o'), true) => {
            let current = app.current_jump_position();
            if let Some(pos) = app.ui.global_jumplist.go_back(current) {
                app.goto_jump_position(&pos);
            }
            return false;
        }

        // Jump forward (Ctrl+I)
        (KeyCode::Char('i'), true) => {
            if let Some(pos) = app.ui.global_jumplist.go_forward() {
                app.goto_jump_position(&pos);
            }
            return false;
        }

        _ => {}
    }

    // Panel-specific keybindings
    match app.ui.mode.current_panel() {
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
            app.ui.command_picker.cancel_input();
            false
        }
        // Enter confirms input
        KeyCode::Enter => {
            if let Some(bpm) = app.ui.command_picker.get_tempo_value() {
                app.transport.bpm = bpm.clamp(20.0, 999.0);
                app.mark_dirty();
            }
            app.ui.command_picker.cancel_input();
            false
        }
        // For tempo input, only allow digits and decimal point
        KeyCode::Char(c) if !(c.is_ascii_digit() || c == '.') => {
            false // Ignore non-numeric characters for tempo
        }
        // Let tui-input handle the rest (digits, backspace, delete, arrows, etc.)
        _ => {
            // Limit input length for tempo
            if app.ui.command_picker.input.input.value().len() < 6
                || key.code == KeyCode::Backspace
                || key.code == KeyCode::Delete
            {
                app.ui
                    .command_picker
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
            app.ui.command_picker.hide();
            false
        }
        // Any other key - try to find and execute a command
        KeyCode::Char(c) => {
            if let Some(cmd) = app.ui.command_picker.find_command(c) {
                app.ui.command_picker.hide();
                common::execute_command(cmd, app)
            } else {
                // Unknown key - just close picker
                app.ui.command_picker.hide();
                false
            }
        }
        _ => {
            // Other keys close picker
            app.ui.command_picker.hide();
            false
        }
    }
}

/// Handle plugin editor modal keys
fn handle_plugin_editor_key(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        // Escape closes editor
        KeyCode::Esc => {
            app.ui.plugin_editor.close();
            false
        }
        // 's' to preview the synth sound
        KeyCode::Char('s') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !app.ui.is_previewing {
                app.start_preview(app.ui.plugin_editor.channel_idx);
                app.ui.plugin_editor.start_preview_animation();
            }
            false
        }
        // Navigation: j/k or down/up
        KeyCode::Char('j') | KeyCode::Down => {
            app.ui.plugin_editor.select_next();
            false
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.ui.plugin_editor.select_prev();
            false
        }
        // Adjust value: h/l or left/right
        KeyCode::Char('h') | KeyCode::Left => {
            app.ui.plugin_editor.adjust_value(-1.0);
            common::send_param_to_plugin(app);
            false
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.ui.plugin_editor.adjust_value(1.0);
            common::send_param_to_plugin(app);
            false
        }
        // Fine adjust with shift
        KeyCode::Char('H') => {
            app.ui.plugin_editor.adjust_value(-0.1);
            common::send_param_to_plugin(app);
            false
        }
        KeyCode::Char('L') => {
            app.ui.plugin_editor.adjust_value(0.1);
            common::send_param_to_plugin(app);
            false
        }
        _ => false,
    }
}

/// Handle keyboard input when context menu is visible
fn handle_context_menu_key(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        // Escape closes menu
        KeyCode::Esc => {
            app.ui.context_menu.hide();
            false
        }
        // j/Down moves selection down
        KeyCode::Char('j') | KeyCode::Down => {
            app.ui.context_menu.select_next();
            false
        }
        // k/Up moves selection up
        KeyCode::Char('k') | KeyCode::Up => {
            app.ui.context_menu.select_prev();
            false
        }
        // Enter executes selected action
        KeyCode::Enter => {
            if let Some(action) = app.ui.context_menu.get_selected_action() {
                let context = app.ui.context_menu.context;
                app.ui.context_menu.hide();
                execute_context_menu_action(action, context, app);
            }
            false
        }
        // Any other key closes menu
        _ => {
            app.ui.context_menu.hide();
            false
        }
    }
}

/// Handle effect picker modal keys
fn handle_effect_picker_key(key: KeyEvent, app: &mut App) -> bool {
    use crate::effects::EffectType;

    match key.code {
        // Escape closes picker
        KeyCode::Esc => {
            app.ui.mode.close_modal();
            false
        }
        // Navigate up/down
        KeyCode::Char('j') | KeyCode::Down => {
            let effect_count = EffectType::all().len();
            if app.ui.effect_picker_selection < effect_count - 1 {
                app.ui.effect_picker_selection += 1;
            }
            false
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.ui.effect_picker_selection > 0 {
                app.ui.effect_picker_selection -= 1;
            }
            false
        }
        // Enter to add the selected effect
        KeyCode::Enter => {
            let effect_types = EffectType::all();
            if app.ui.effect_picker_selection < effect_types.len() {
                let effect_type = effect_types[app.ui.effect_picker_selection];
                app.add_effect(effect_type);
            }
            app.ui.effect_picker_selection = 0;
            app.ui.mode.close_modal();
            false
        }
        _ => false,
    }
}

/// Handle effect editor modal keys
fn handle_effect_editor_key(key: KeyEvent, app: &mut App) -> bool {
    use crate::effects::get_param_defs;
    use crate::mode::AppMode;

    // Get current track/slot/param from mode
    let (track_idx, slot_idx, selected_param) = match &app.ui.mode {
        AppMode::EffectEditor {
            track_idx,
            slot_idx,
            selected_param,
            ..
        } => (*track_idx, *slot_idx, *selected_param),
        _ => return false,
    };

    // Get the effect slot
    let effect_slot = match &app.mixer.tracks[track_idx].effects[slot_idx] {
        Some(slot) => slot.clone(),
        None => {
            app.ui.mode.close_modal();
            return false;
        }
    };

    let param_defs = get_param_defs(effect_slot.effect_type);

    match key.code {
        // Escape closes editor
        KeyCode::Esc => {
            app.ui.mode.close_modal();
            false
        }
        // Navigate up/down
        KeyCode::Char('j') | KeyCode::Down => {
            if selected_param < param_defs.len() - 1 {
                if let AppMode::EffectEditor {
                    selected_param: ref mut p,
                    ..
                } = app.ui.mode
                {
                    *p += 1;
                }
            }
            false
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if selected_param > 0 {
                if let AppMode::EffectEditor {
                    selected_param: ref mut p,
                    ..
                } = app.ui.mode
                {
                    *p -= 1;
                }
            }
            false
        }
        // Adjust value: h/l = coarse, H/L = fine
        KeyCode::Char('h') | KeyCode::Left => {
            if selected_param < param_defs.len() {
                let def = &param_defs[selected_param];
                let current = effect_slot.get_param(def.id);
                // For discrete params, step by 1; for continuous, step by 5%
                let step = if def.is_discrete() {
                    1.0
                } else {
                    (def.max - def.min) * 0.05
                };
                let new_value = (current - step).max(def.min);
                app.set_effect_param(def.id, new_value);
            }
            false
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if selected_param < param_defs.len() {
                let def = &param_defs[selected_param];
                let current = effect_slot.get_param(def.id);
                // For discrete params, step by 1; for continuous, step by 5%
                let step = if def.is_discrete() {
                    1.0
                } else {
                    (def.max - def.min) * 0.05
                };
                let new_value = (current + step).min(def.max);
                app.set_effect_param(def.id, new_value);
            }
            false
        }
        KeyCode::Char('H') => {
            if selected_param < param_defs.len() {
                let def = &param_defs[selected_param];
                let current = effect_slot.get_param(def.id);
                // For discrete params, H/L also step by 1; for continuous, step by 1%
                let step = if def.is_discrete() {
                    1.0
                } else {
                    (def.max - def.min) * 0.01
                };
                let new_value = (current - step).max(def.min);
                app.set_effect_param(def.id, new_value);
            }
            false
        }
        KeyCode::Char('L') => {
            if selected_param < param_defs.len() {
                let def = &param_defs[selected_param];
                let current = effect_slot.get_param(def.id);
                // For discrete params, H/L also step by 1; for continuous, step by 1%
                let step = if def.is_discrete() {
                    1.0
                } else {
                    (def.max - def.min) * 0.01
                };
                let new_value = (current + step).min(def.max);
                app.set_effect_param(def.id, new_value);
            }
            false
        }
        _ => false,
    }
}

/// Handle a mouse event
///
/// This follows the same pattern as handle_key:
/// 1. MouseState processes raw event → MouseAction(s)
/// 2. Hit test to find which area/component
/// 3. Delegate to component handler
pub fn handle_mouse(event: MouseEvent, app: &mut App) {
    use crate::ui::areas::AreaId;
    use mouse::MouseAction;

    // 1. Process event through MouseState to get high-level actions
    let actions = app.ui.mouse.process_event(event);

    // 2. For each action, determine target and delegate
    for action in actions {
        let (x, y) = action.position();

        // Handle context menu first (rendered on top of everything)
        if app.ui.context_menu.visible {
            handle_context_menu_mouse(&action, app);
            continue;
        }

        // Handle modals (they're on top of regular content)
        if app.ui.command_picker.visible {
            handle_command_picker_mouse(&action, app);
            continue;
        }

        if app.ui.plugin_editor.visible {
            handle_plugin_editor_mouse(&action, app);
            continue;
        }

        // 3. Hit test to find which area/component
        let area = app.ui.screen_areas.hit_test(x, y);

        // Focus the panel if clicking a focusable area
        if matches!(&action, MouseAction::Click { .. }) {
            if let Some(ref area_id) = area {
                focus_panel_for_area(area_id, app);
            }
        }

        // 4. Delegate to component handler based on area
        match area {
            // Transport
            Some(AreaId::TransportPlayStop) => {
                if matches!(action, MouseAction::Click { .. }) {
                    app.toggle_play();
                }
            }
            Some(AreaId::TransportBpm) => {
                if matches!(action, MouseAction::Click { .. }) {
                    app.ui.command_picker.start_tempo_input(app.transport.bpm);
                }
            }

            // View switcher buttons
            Some(AreaId::TransportViewChannelRack) => {
                if matches!(action, MouseAction::Click { .. }) {
                    app.ui.view_mode = crate::mode::ViewMode::ChannelRack;
                }
            }
            Some(AreaId::TransportViewPlaylist) => {
                if matches!(action, MouseAction::Click { .. }) {
                    app.ui.view_mode = crate::mode::ViewMode::Playlist;
                }
            }

            // Pattern selector (transport - legacy, may be removed)
            Some(AreaId::TransportPatternPrev) | Some(AreaId::ChannelRackPatternPrev) => {
                if matches!(action, MouseAction::Click { .. }) && app.current_pattern > 0 {
                    app.current_pattern -= 1;
                }
            }
            Some(AreaId::TransportPatternNext) | Some(AreaId::ChannelRackPatternNext) => {
                if matches!(action, MouseAction::Click { .. }) {
                    // Same behavior as ']' key - go to next or create new pattern
                    if app.current_pattern + 1 < app.patterns.len() {
                        app.current_pattern += 1;
                    } else {
                        // Create a new pattern (now metadata-only)
                        let new_id = app.patterns.len();
                        app.patterns
                            .push(crate::sequencer::Pattern::new(new_id, 16));
                        app.current_pattern = new_id;
                    }
                    app.mark_dirty();
                }
            }
            Some(AreaId::TransportPatternLabel) => {
                // Could open a pattern picker in the future
            }

            // Browser toggle
            Some(AreaId::TransportBrowserToggle) => {
                if matches!(action, MouseAction::Click { .. }) {
                    app.ui.show_browser = !app.ui.show_browser;
                }
            }

            // Mixer toggle
            Some(AreaId::TransportMixerToggle) => {
                if matches!(action, MouseAction::Click { .. }) {
                    app.ui.show_mixer = !app.ui.show_mixer;
                }
            }

            // Browser
            Some(AreaId::BrowserClose) => {
                if matches!(action, MouseAction::Click { .. }) {
                    app.ui.show_browser = false;
                    // Return focus to main view
                    match app.ui.view_mode {
                        crate::mode::ViewMode::ChannelRack => {
                            app.ui.mode.switch_panel(Panel::ChannelRack)
                        }
                        crate::mode::ViewMode::PianoRoll => {
                            app.ui.mode.switch_panel(Panel::PianoRoll)
                        }
                        crate::mode::ViewMode::Playlist => {
                            app.ui.mode.switch_panel(Panel::Playlist)
                        }
                    }
                }
            }
            Some(AreaId::BrowserTabs) => {
                if matches!(action, MouseAction::Click { .. }) {
                    // Toggle between Samples and Plugins
                    app.ui.browser.toggle_mode();
                }
            }
            Some(AreaId::Browser) | Some(AreaId::BrowserContent) => {
                browser::handle_mouse_action(&action, app);
            }

            // Main view tabs (the tabbed pane at top of main view)
            // Use set_view_mode() to record position in global jumplist
            Some(AreaId::MainViewTabChannelRack) => {
                if matches!(action, MouseAction::Click { .. }) {
                    app.set_view_mode(crate::mode::ViewMode::ChannelRack);
                }
            }
            Some(AreaId::MainViewTabPlaylist) => {
                if matches!(action, MouseAction::Click { .. }) {
                    app.set_view_mode(crate::mode::ViewMode::Playlist);
                }
            }

            // Channel Rack
            Some(AreaId::ChannelRackMuteColumn)
            | Some(AreaId::ChannelRackSampleColumn)
            | Some(AreaId::ChannelRackStepsGrid) => {
                channel_rack::handle_mouse_action(&action, app);
            }

            // Piano Roll
            Some(AreaId::PianoRollPitchColumn) | Some(AreaId::PianoRollGrid) => {
                piano_roll::handle_mouse_action(&action, app);
            }

            // Playlist
            Some(AreaId::PlaylistPatternColumn)
            | Some(AreaId::PlaylistMuteColumn)
            | Some(AreaId::PlaylistGrid) => {
                playlist::handle_mouse_action(&action, app);
            }

            // Mixer
            Some(AreaId::MixerClose) => {
                if matches!(action, MouseAction::Click { .. }) {
                    app.ui.show_mixer = false;
                }
            }
            Some(AreaId::Mixer) | Some(AreaId::MixerChannelStrip) => {
                // TODO: Mixer mouse handling needs to be reimplemented for track-based architecture
            }

            // Main view (fallback for general area clicks)
            Some(AreaId::MainView) | Some(AreaId::MainViewGrid) => {
                // Delegate based on current view mode
                match app.ui.view_mode {
                    crate::mode::ViewMode::ChannelRack => {
                        channel_rack::handle_mouse_action(&action, app);
                    }
                    crate::mode::ViewMode::PianoRoll => {
                        piano_roll::handle_mouse_action(&action, app);
                    }
                    crate::mode::ViewMode::Playlist => {
                        playlist::handle_mouse_action(&action, app);
                    }
                }
            }

            _ => {}
        }
    }
}

/// Focus the appropriate panel based on which area was clicked
fn focus_panel_for_area(area_id: &crate::ui::areas::AreaId, app: &mut App) {
    use crate::ui::areas::AreaId;

    let panel = match area_id {
        AreaId::Browser | AreaId::BrowserTabs | AreaId::BrowserContent => Some(Panel::Browser),
        AreaId::Mixer | AreaId::MixerChannelStrip => Some(Panel::Mixer),
        AreaId::ChannelRackPatternPrev
        | AreaId::ChannelRackPatternNext
        | AreaId::ChannelRackMuteColumn
        | AreaId::ChannelRackSampleColumn
        | AreaId::ChannelRackStepsGrid => Some(Panel::ChannelRack),
        AreaId::PianoRollPitchColumn | AreaId::PianoRollGrid => Some(Panel::PianoRoll),
        AreaId::PlaylistPatternColumn | AreaId::PlaylistMuteColumn | AreaId::PlaylistGrid => {
            Some(Panel::Playlist)
        }
        AreaId::MainView | AreaId::MainViewGrid => {
            // Use current view mode
            match app.ui.view_mode {
                crate::mode::ViewMode::ChannelRack => Some(Panel::ChannelRack),
                crate::mode::ViewMode::PianoRoll => Some(Panel::PianoRoll),
                crate::mode::ViewMode::Playlist => Some(Panel::Playlist),
            }
        }
        _ => None,
    };

    if let Some(p) = panel {
        app.ui.mode.switch_panel(p);
    }
}

/// Handle mouse actions for command picker modal
fn handle_command_picker_mouse(action: &mouse::MouseAction, app: &mut App) {
    use mouse::MouseAction;

    if let MouseAction::Click { x, y, .. } = action {
        // Check if click is inside command picker
        if let Some(picker_rect) = app
            .ui
            .screen_areas
            .get(crate::ui::areas::AreaId::CommandPicker)
        {
            if *x < picker_rect.x
                || *x >= picker_rect.x + picker_rect.width
                || *y < picker_rect.y
                || *y >= picker_rect.y + picker_rect.height
            {
                // Click outside - dismiss
                app.ui.command_picker.hide();
                return;
            }

            // Check if clicking on a command item
            if let Some(idx) = app.ui.screen_areas.command_item_at(*x, *y) {
                // Execute the command at this index
                if let Some(cmd) = app.ui.command_picker.get_command_at(idx) {
                    app.ui.command_picker.hide();
                    common::execute_command(cmd, app);
                }
            }
        } else {
            // No picker rect registered, just dismiss
            app.ui.command_picker.hide();
        }
    }
}

/// Handle mouse actions for plugin editor modal
fn handle_plugin_editor_mouse(action: &mouse::MouseAction, app: &mut App) {
    use mouse::MouseAction;

    match action {
        MouseAction::Click { x, y, .. } => {
            // Check if click is inside plugin editor
            if let Some(editor_rect) = app
                .ui
                .screen_areas
                .get(crate::ui::areas::AreaId::PluginEditor)
            {
                if *x < editor_rect.x
                    || *x >= editor_rect.x + editor_rect.width
                    || *y < editor_rect.y
                    || *y >= editor_rect.y + editor_rect.height
                {
                    // Click outside - dismiss
                    app.ui.plugin_editor.close();
                    return;
                }

                // Check if clicking on a parameter
                if let Some(idx) = app.ui.screen_areas.plugin_param_at(*x, *y) {
                    app.ui.plugin_editor.selected_param = idx;
                }
            } else {
                app.ui.plugin_editor.close();
            }
        }
        MouseAction::DragStart { x, y, .. } => {
            // Check if starting drag on a parameter slider
            if let Some(idx) = app.ui.screen_areas.plugin_param_at(*x, *y) {
                app.ui.plugin_editor.selected_param = idx;
                // Store drag start for later calculation
            }
        }
        MouseAction::DragMove { start_x, x, .. } => {
            // Adjust parameter value based on horizontal drag distance
            if let Some(param_rect) = app
                .ui
                .screen_areas
                .plugin_editor_params
                .get(app.ui.plugin_editor.selected_param)
            {
                let delta_x = (*x as f32 - *start_x as f32) / param_rect.width as f32;
                // Scale delta to parameter range
                let delta_value = delta_x * 100.0; // Assuming 0-100 range
                app.ui.plugin_editor.adjust_value(delta_value);
                common::send_param_to_plugin(app);
            }
        }
        MouseAction::Scroll { delta, .. } => {
            // Scroll adjusts selected parameter
            if *delta < 0 {
                app.ui.plugin_editor.adjust_value(1.0);
            } else {
                app.ui.plugin_editor.adjust_value(-1.0);
            }
            common::send_param_to_plugin(app);
        }
        _ => {}
    }
}

/// Handle mouse actions for context menu
fn handle_context_menu_mouse(action: &mouse::MouseAction, app: &mut App) {
    use crate::ui::areas::AreaId;
    use mouse::MouseAction;

    match action {
        MouseAction::Click { x, y, .. } => {
            // Check if click is inside context menu
            if let Some(menu_rect) = app.ui.screen_areas.get(AreaId::ContextMenu) {
                if *x >= menu_rect.x
                    && *x < menu_rect.x + menu_rect.width
                    && *y >= menu_rect.y
                    && *y < menu_rect.y + menu_rect.height
                {
                    // Click inside menu - check if on an item
                    if let Some(idx) = app.ui.context_menu.item_at(*x, *y, menu_rect) {
                        app.ui.context_menu.selected = idx;
                        if let Some(action) = app.ui.context_menu.get_selected_action() {
                            let context = app.ui.context_menu.context;
                            app.ui.context_menu.hide();
                            execute_context_menu_action(action, context, app);
                        }
                    }
                    return;
                }
            }
            // Click outside - dismiss
            app.ui.context_menu.hide();
        }
        MouseAction::RightClick { .. } => {
            // Right-click while menu is open just dismisses it
            app.ui.context_menu.hide();
        }
        _ => {}
    }
}

/// Execute a context menu action
fn execute_context_menu_action(
    action: crate::ui::context_menu::ContextMenuAction,
    context: Option<crate::ui::context_menu::MenuContext>,
    app: &mut App,
) {
    use crate::ui::context_menu::{ContextMenuAction, MenuContext};

    match action {
        // Channel Rack actions
        ContextMenuAction::DeleteChannel => {
            if let Some(MenuContext::ChannelRack { channel: slot }) = context {
                app.dispatch(AppCommand::DeleteChannel(slot));
            }
        }
        ContextMenuAction::MuteChannel => {
            if let Some(MenuContext::ChannelRack { channel: slot }) = context {
                // Toggle mute on the mixer track this channel routes to
                if let Some(ch) = app.get_channel_at_slot(slot) {
                    app.dispatch(AppCommand::ToggleTrackMute(ch.mixer_track));
                }
            }
        }
        ContextMenuAction::SoloChannel => {
            if let Some(MenuContext::ChannelRack { channel: slot }) = context {
                // Toggle solo on the mixer track this channel routes to
                if let Some(ch) = app.get_channel_at_slot(slot) {
                    app.dispatch(AppCommand::ToggleTrackSolo(ch.mixer_track));
                }
            }
        }
        ContextMenuAction::PreviewChannel => {
            if let Some(MenuContext::ChannelRack { channel: slot }) = context {
                app.start_preview(slot);
            }
        }
        ContextMenuAction::DuplicateChannel => {
            if let Some(MenuContext::ChannelRack { channel: slot }) = context {
                if let Some(ch) = app.get_channel_at_slot(slot) {
                    let mut new_channel = ch.clone();
                    // Find first unused slot number
                    let used_slots: std::collections::HashSet<usize> =
                        app.channels.iter().map(|c| c.slot).collect();
                    let new_slot = (0..99).find(|s| !used_slots.contains(s)).unwrap_or(98);
                    new_channel.slot = new_slot;
                    new_channel.mixer_track = app.find_available_mixer_track();
                    app.channels.push(new_channel);
                    app.mark_dirty();
                }
            }
        }
        ContextMenuAction::AssignSample => {
            // Start selection mode and switch to browser - record position for Ctrl+O
            let current = app.current_jump_position();
            app.ui.global_jumplist.push(current);
            if let Some(MenuContext::ChannelRack { channel }) = context {
                app.ui.browser.start_selection(channel);
            }
            app.ui.mode.switch_panel(Panel::Browser);
            app.ui.show_browser = true;
        }
        ContextMenuAction::AssignPlugin => {
            // Start plugin selection mode and switch to browser - record position for Ctrl+O
            let current = app.current_jump_position();
            app.ui.global_jumplist.push(current);
            if let Some(MenuContext::ChannelRack { channel }) = context {
                app.ui.browser.start_selection(channel);
                app.ui.browser.mode = crate::browser::BrowserMode::Plugins;
            }
            app.ui.mode.switch_panel(Panel::Browser);
            app.ui.show_browser = true;
        }
        ContextMenuAction::OpenPianoRoll => {
            // Switch to piano roll view for the channel - use set_view_mode for jumplist
            if let Some(MenuContext::ChannelRack { channel }) = context {
                app.ui.cursors.channel_rack.channel = channel;
            }
            app.set_view_mode(crate::mode::ViewMode::PianoRoll);
        }

        // Piano Roll actions
        ContextMenuAction::DeleteNote => {
            if let Some(MenuContext::PianoRoll { pitch, step }) = context {
                let channel = app.ui.cursors.channel_rack.channel;
                let pattern = app.current_pattern;
                // Find note position first
                let note_info = app
                    .channels
                    .get(channel)
                    .and_then(|c| c.get_pattern(pattern))
                    .and_then(|s| s.get_note_at(pitch, step))
                    .map(|n| (n.pitch, n.start_step));

                if let Some((note_pitch, start_step)) = note_info {
                    app.dispatch(AppCommand::DeleteNote {
                        channel,
                        pattern,
                        pitch: note_pitch,
                        start_step,
                    });
                }
            }
        }
        ContextMenuAction::DuplicateNote => {
            // TODO: implement note duplication
        }
        ContextMenuAction::SetVelocity => {
            // TODO: implement velocity dialog
        }

        // Playlist actions
        ContextMenuAction::DeletePlacement => {
            if let Some(MenuContext::Playlist { row, bar }) = context {
                // Capture pattern.id before mutable borrow
                let pattern_id = app.patterns.get(row).map(|p| p.id);
                if let Some(id) = pattern_id {
                    app.dispatch(AppCommand::RemovePlacement {
                        pattern_id: id,
                        bar,
                    });
                }
            }
        }
        ContextMenuAction::DuplicatePlacement => {
            // TODO: implement placement duplication
        }
        ContextMenuAction::MutePattern => {
            if let Some(MenuContext::Playlist { row, .. }) = context {
                // Capture pattern.id before mutable borrow
                let pattern_id = app.patterns.get(row).map(|p| p.id);
                if let Some(id) = pattern_id {
                    app.dispatch(AppCommand::TogglePatternMute(id));
                }
            }
        }

        // Mixer actions - now operate on mixer tracks, not generators
        ContextMenuAction::ResetVolume => {
            if let Some(MenuContext::Mixer { channel }) = context {
                // channel here is a track index
                app.dispatch(AppCommand::ResetTrackVolume(channel));
            }
        }
        ContextMenuAction::MuteTrack => {
            if let Some(MenuContext::Mixer { channel }) = context {
                app.dispatch(AppCommand::ToggleTrackMute(channel));
            }
        }
        ContextMenuAction::SoloTrack => {
            if let Some(MenuContext::Mixer { channel }) = context {
                app.dispatch(AppCommand::ToggleTrackSolo(channel));
            }
        }

        // Browser actions
        ContextMenuAction::PreviewFile => {
            if let Some(MenuContext::Browser { item_idx }) = context {
                if let Some(entry) = app.ui.browser.visible_entries.get(item_idx) {
                    if !entry.is_dir {
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
        ContextMenuAction::AssignToChannel => {
            if let Some(MenuContext::Browser { item_idx }) = context {
                if let Some(entry) = app.ui.browser.visible_entries.get(item_idx).cloned() {
                    if !entry.is_dir {
                        if let Some(file_name) = entry.path.file_name() {
                            let sample_path = file_name.to_string_lossy().to_string();
                            app.set_channel_sample(
                                app.ui.cursors.channel_rack.channel,
                                sample_path,
                            );
                        }
                    }
                }
            }
        }

        // Plugin Editor actions
        ContextMenuAction::ResetParameter => {
            if let Some(MenuContext::PluginEditor { param_idx }) = context {
                // Reset parameter to default value
                if let Some(param) = app.ui.plugin_editor.params.get_mut(param_idx) {
                    param.value = param.default;
                }
                common::send_param_to_plugin(app);
            }
        }
    }
}
