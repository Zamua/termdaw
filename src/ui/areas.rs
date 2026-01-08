//! Screen areas for mouse hit testing
//!
//! This module provides a registry of clickable areas that is populated
//! during UI rendering and queried during mouse input handling.
//!
//! Design principles:
//! - Pure data structure, no business logic
//! - Cleared at start of each render, populated during render
//! - Queried during input handling for coordinate mapping

#![allow(dead_code)] // Will be used as mouse handling is wired up

use ratatui::layout::Rect;
use std::collections::HashMap;

// ============================================================================
// Area Identification
// ============================================================================

/// Identifies a clickable region in the UI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AreaId {
    // ========================================================================
    // Top-level panels
    // ========================================================================
    Transport,
    Browser,
    MainView, // Patterns, Piano Roll, or Playlist
    Mixer,

    // ========================================================================
    // Transport bar regions
    // ========================================================================
    TransportPlayStop,
    TransportBpm,
    TransportWaveform,
    TransportViewChannelRack,
    #[allow(dead_code)]
    TransportViewPianoRoll, // Piano Roll accessed via channel rack context menu
    TransportViewPlaylist,
    TransportPatternPrev,
    TransportPatternNext,
    TransportPatternLabel,
    TransportBrowserToggle,
    TransportMixerToggle,
    TransportEventLogToggle,

    // ========================================================================
    // Browser regions
    // ========================================================================
    BrowserTabs,
    BrowserContent,
    BrowserClose,

    // ========================================================================
    // Main view regions (generic - the actual view type determines behavior)
    // ========================================================================
    /// Tab bar at top of main view (Patterns | Playlist)
    MainViewTabBar,
    /// Patterns tab button (was Channel Rack)
    MainViewTabChannelRack,
    /// Playlist tab button
    MainViewTabPlaylist,
    /// The grid area of the current main view (channel rack steps, piano roll notes, playlist bars)
    MainViewGrid,
    /// Header row of the main view
    MainViewHeader,

    // ========================================================================
    // Channel Rack specific
    // ========================================================================
    ChannelRackPatternPrev,
    ChannelRackPatternNext,
    ChannelRackMuteColumn,
    ChannelRackSampleColumn,
    ChannelRackStepsGrid,

    // ========================================================================
    // Piano Roll specific
    // ========================================================================
    PianoRollPitchColumn,
    PianoRollGrid,

    // ========================================================================
    // Playlist specific
    // ========================================================================
    PlaylistPatternColumn,
    PlaylistMuteColumn,
    PlaylistGrid,

    // ========================================================================
    // Mixer regions
    // ========================================================================
    /// A mixer channel strip (index stored separately in cell lookup)
    MixerChannelStrip,
    /// Mixer close button
    MixerClose,

    // ========================================================================
    // Event Log regions
    // ========================================================================
    /// Event log panel
    EventLog,
    /// Event log close button
    EventLogClose,

    // ========================================================================
    // Modals
    // ========================================================================
    CommandPicker,
    PluginEditor,
    ContextMenu,
}

// ============================================================================
// Screen Areas Registry
// ============================================================================

/// Registry of all clickable areas from the last render
///
/// This is populated during UI rendering and queried during input handling.
/// It provides both coarse-grained area detection and fine-grained cell lookup
/// for grid-based components.
#[derive(Debug, Default)]
pub struct ScreenAreas {
    /// Map of area ID to its screen rect
    areas: HashMap<AreaId, Rect>,

    // ========================================================================
    // Cell-level lookups for grid components
    // ========================================================================
    /// Channel rack: maps (channel_idx, vim_col) to screen rect
    /// vim_col: 0=mute, 1=sample, 2-17=steps
    pub channel_rack_cells: HashMap<(usize, usize), Rect>,

    /// Piano roll: maps (vim_row, step) to screen rect
    /// vim_row: 0=highest pitch (C6), 48=lowest pitch (C2)
    pub piano_roll_cells: HashMap<(usize, usize), Rect>,

    /// Playlist: maps (pattern_row, bar_col) to screen rect
    /// bar_col: 0=mute, 1-16=bars
    pub playlist_cells: HashMap<(usize, usize), Rect>,

    /// Browser items: maps visible index to screen rect
    pub browser_items: Vec<Rect>,

    /// Mixer channels: maps channel index to fader rect (for drag calculations)
    pub mixer_faders: HashMap<usize, Rect>,

    /// Mixer mute buttons: maps channel index to rect
    pub mixer_mute_buttons: HashMap<usize, Rect>,

    /// Mixer solo buttons: maps channel index to rect
    pub mixer_solo_buttons: HashMap<usize, Rect>,

    /// Mixer channel strips: maps channel index to full strip rect
    pub mixer_channel_strips: HashMap<usize, Rect>,

    /// Plugin editor params: maps param index to slider rect
    pub plugin_editor_params: Vec<Rect>,

    /// Command picker items: maps item index to rect
    pub command_picker_items: Vec<Rect>,

    /// Context menu items: maps item index to rect
    pub context_menu_items: Vec<Rect>,
}

impl ScreenAreas {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all areas (called at start of each render)
    pub fn clear(&mut self) {
        self.areas.clear();
        self.channel_rack_cells.clear();
        self.piano_roll_cells.clear();
        self.playlist_cells.clear();
        self.browser_items.clear();
        self.mixer_faders.clear();
        self.mixer_mute_buttons.clear();
        self.mixer_solo_buttons.clear();
        self.mixer_channel_strips.clear();
        self.plugin_editor_params.clear();
        self.command_picker_items.clear();
        self.context_menu_items.clear();
    }

    // ========================================================================
    // Area registration (called during render)
    // ========================================================================

    /// Register an area
    pub fn register(&mut self, id: AreaId, rect: Rect) {
        self.areas.insert(id, rect);
    }

    /// Get an area's rect
    pub fn get(&self, id: AreaId) -> Option<Rect> {
        self.areas.get(&id).copied()
    }

    // ========================================================================
    // Hit testing (called during input handling)
    // ========================================================================

    /// Find which area contains the given screen coordinates
    ///
    /// Checks modals first (they're rendered on top), then other areas.
    pub fn hit_test(&self, x: u16, y: u16) -> Option<AreaId> {
        // Check modals first (they're on top)
        // Order matters: context menu > plugin editor > command picker
        for id in [
            AreaId::ContextMenu,
            AreaId::PluginEditor,
            AreaId::CommandPicker,
        ] {
            if let Some(rect) = self.areas.get(&id) {
                if Self::point_in_rect(x, y, *rect) {
                    return Some(id);
                }
            }
        }

        // Check specific sub-areas before their parents
        // This ensures clicking on a button inside a panel returns the button, not the panel
        let sub_areas = [
            // Transport sub-areas
            AreaId::TransportPlayStop,
            AreaId::TransportBpm,
            AreaId::TransportWaveform,
            AreaId::TransportViewChannelRack,
            AreaId::TransportViewPlaylist,
            AreaId::TransportPatternPrev,
            AreaId::TransportPatternNext,
            AreaId::TransportPatternLabel,
            AreaId::TransportBrowserToggle,
            AreaId::TransportMixerToggle,
            AreaId::TransportEventLogToggle,
            // Browser sub-areas
            AreaId::BrowserTabs,
            AreaId::BrowserContent,
            AreaId::BrowserClose,
            // Main view tabs (specific tabs before parent tab bar)
            AreaId::MainViewTabChannelRack,
            AreaId::MainViewTabPlaylist,
            AreaId::MainViewTabBar,
            // Channel rack sub-areas
            AreaId::ChannelRackPatternPrev,
            AreaId::ChannelRackPatternNext,
            AreaId::ChannelRackMuteColumn,
            AreaId::ChannelRackSampleColumn,
            AreaId::ChannelRackStepsGrid,
            // Piano roll sub-areas
            AreaId::PianoRollPitchColumn,
            AreaId::PianoRollGrid,
            // Playlist sub-areas
            AreaId::PlaylistPatternColumn,
            AreaId::PlaylistMuteColumn,
            AreaId::PlaylistGrid,
            // Mixer sub-areas
            AreaId::MixerChannelStrip,
            AreaId::MixerClose,
            // Event log sub-areas
            AreaId::EventLogClose,
        ];

        for id in sub_areas {
            if let Some(rect) = self.areas.get(&id) {
                if Self::point_in_rect(x, y, *rect) {
                    return Some(id);
                }
            }
        }

        // Check top-level panels
        for id in [
            AreaId::Transport,
            AreaId::Browser,
            AreaId::MainView,
            AreaId::Mixer,
        ] {
            if let Some(rect) = self.areas.get(&id) {
                if Self::point_in_rect(x, y, *rect) {
                    return Some(id);
                }
            }
        }

        None
    }

    /// Check if a point is inside a rect
    fn point_in_rect(x: u16, y: u16, rect: Rect) -> bool {
        x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
    }

    // ========================================================================
    // Cell-level lookups
    // ========================================================================

    /// Find channel rack cell at screen position
    /// Returns (channel_idx, vim_col) where vim_col: 0=mute, 1=sample, 2-17=steps
    pub fn channel_rack_cell_at(&self, x: u16, y: u16) -> Option<(usize, usize)> {
        for ((row, col), rect) in &self.channel_rack_cells {
            if Self::point_in_rect(x, y, *rect) {
                return Some((*row, *col));
            }
        }
        None
    }

    /// Find piano roll cell at screen position
    /// Returns (vim_row, step) where vim_row 0=highest pitch
    pub fn piano_roll_cell_at(&self, x: u16, y: u16) -> Option<(usize, usize)> {
        for ((row, col), rect) in &self.piano_roll_cells {
            if Self::point_in_rect(x, y, *rect) {
                return Some((*row, *col));
            }
        }
        None
    }

    /// Find playlist cell at screen position
    /// Returns (pattern_row, bar_col) where bar_col: 0=mute, 1-16=bars
    pub fn playlist_cell_at(&self, x: u16, y: u16) -> Option<(usize, usize)> {
        for ((row, col), rect) in &self.playlist_cells {
            if Self::point_in_rect(x, y, *rect) {
                return Some((*row, *col));
            }
        }
        None
    }

    /// Find browser item at screen position
    /// Returns the visible item index
    pub fn browser_item_at(&self, x: u16, y: u16) -> Option<usize> {
        for (idx, rect) in self.browser_items.iter().enumerate() {
            if Self::point_in_rect(x, y, *rect) {
                return Some(idx);
            }
        }
        None
    }

    /// Find mixer fader at screen position
    /// Returns (channel_idx, y_position_in_fader)
    pub fn mixer_fader_at(&self, x: u16, y: u16) -> Option<(usize, u16)> {
        for (ch_idx, rect) in &self.mixer_faders {
            if Self::point_in_rect(x, y, *rect) {
                // Return the relative y position within the fader for volume calculation
                let relative_y = y.saturating_sub(rect.y);
                return Some((*ch_idx, relative_y));
            }
        }
        None
    }

    /// Find mixer mute button at screen position
    pub fn mixer_mute_at(&self, x: u16, y: u16) -> Option<usize> {
        for (ch_idx, rect) in &self.mixer_mute_buttons {
            if Self::point_in_rect(x, y, *rect) {
                return Some(*ch_idx);
            }
        }
        None
    }

    /// Find mixer solo button at screen position
    pub fn mixer_solo_at(&self, x: u16, y: u16) -> Option<usize> {
        for (ch_idx, rect) in &self.mixer_solo_buttons {
            if Self::point_in_rect(x, y, *rect) {
                return Some(*ch_idx);
            }
        }
        None
    }

    /// Find mixer channel strip at screen position
    pub fn mixer_channel_strip_at(&self, x: u16, y: u16) -> Option<usize> {
        for (ch_idx, rect) in &self.mixer_channel_strips {
            if Self::point_in_rect(x, y, *rect) {
                return Some(*ch_idx);
            }
        }
        None
    }

    /// Find plugin editor param at screen position
    pub fn plugin_param_at(&self, x: u16, y: u16) -> Option<usize> {
        for (idx, rect) in self.plugin_editor_params.iter().enumerate() {
            if Self::point_in_rect(x, y, *rect) {
                return Some(idx);
            }
        }
        None
    }

    /// Find command picker item at screen position
    pub fn command_item_at(&self, x: u16, y: u16) -> Option<usize> {
        for (idx, rect) in self.command_picker_items.iter().enumerate() {
            if Self::point_in_rect(x, y, *rect) {
                return Some(idx);
            }
        }
        None
    }

    /// Find context menu item at screen position
    pub fn context_menu_item_at(&self, x: u16, y: u16) -> Option<usize> {
        for (idx, rect) in self.context_menu_items.iter().enumerate() {
            if Self::point_in_rect(x, y, *rect) {
                return Some(idx);
            }
        }
        None
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_test_basic() {
        let mut areas = ScreenAreas::new();

        areas.register(AreaId::Transport, Rect::new(0, 0, 100, 3));
        areas.register(AreaId::MainView, Rect::new(0, 3, 100, 20));
        areas.register(AreaId::Mixer, Rect::new(0, 23, 100, 16));

        assert_eq!(areas.hit_test(50, 1), Some(AreaId::Transport));
        assert_eq!(areas.hit_test(50, 10), Some(AreaId::MainView));
        assert_eq!(areas.hit_test(50, 30), Some(AreaId::Mixer));
        assert_eq!(areas.hit_test(50, 50), None); // Outside all areas
    }

    #[test]
    fn test_modal_priority() {
        let mut areas = ScreenAreas::new();

        // Register overlapping areas
        areas.register(AreaId::MainView, Rect::new(0, 0, 100, 40));
        areas.register(AreaId::CommandPicker, Rect::new(30, 10, 40, 20));

        // Modal should take priority
        assert_eq!(areas.hit_test(50, 15), Some(AreaId::CommandPicker));
        // Outside modal but inside main view
        assert_eq!(areas.hit_test(10, 15), Some(AreaId::MainView));
    }

    #[test]
    fn test_cell_lookup() {
        let mut areas = ScreenAreas::new();

        // Register some channel rack cells
        areas
            .channel_rack_cells
            .insert((0, 0), Rect::new(0, 0, 3, 1)); // mute
        areas
            .channel_rack_cells
            .insert((0, 1), Rect::new(3, 0, 10, 1)); // sample
        areas
            .channel_rack_cells
            .insert((0, 2), Rect::new(13, 0, 3, 1)); // step 0

        assert_eq!(areas.channel_rack_cell_at(1, 0), Some((0, 0)));
        assert_eq!(areas.channel_rack_cell_at(5, 0), Some((0, 1)));
        assert_eq!(areas.channel_rack_cell_at(14, 0), Some((0, 2)));
        assert_eq!(areas.channel_rack_cell_at(50, 0), None);
    }
}
