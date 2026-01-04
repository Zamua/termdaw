//! Application mode state machine.
//!
//! The app is always in exactly one mode. Modes can be stacked (e.g., opening
//! command picker from any panel). This ensures proper return behavior and
//! prevents invalid state combinations.

// Allow dead code - many modal variants are defined for future use
#![allow(dead_code)]

/// The primary panels that can be focused in normal mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Panel {
    #[default]
    ChannelRack,
    PianoRoll,
    Playlist,
    Mixer,
    Browser,
}

impl Panel {
    /// Get the next panel in tab order
    pub fn next(self, show_browser: bool, show_mixer: bool, view_mode: ViewMode) -> Self {
        // Get the main panel for the current view mode
        let main_panel = match view_mode {
            ViewMode::ChannelRack => Self::ChannelRack,
            ViewMode::PianoRoll => Self::PianoRoll,
            ViewMode::Playlist => Self::Playlist,
        };

        match self {
            Self::Browser => main_panel,
            Self::ChannelRack | Self::PianoRoll | Self::Playlist => {
                // From main panel, go to mixer if visible, then browser if visible
                if show_mixer {
                    Self::Mixer
                } else if show_browser {
                    Self::Browser
                } else {
                    main_panel
                }
            }
            Self::Mixer => {
                if show_browser {
                    Self::Browser
                } else {
                    main_panel
                }
            }
        }
    }

    /// Get display name
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Browser => "BROWSER",
            Self::ChannelRack => "CHANNELRACK",
            Self::PianoRoll => "PIANOROLL",
            Self::Playlist => "PLAYLIST",
            Self::Mixer => "MIXER",
        }
    }
}

/// Which main view is currently shown (for the central area)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    ChannelRack,
    PianoRoll,
    Playlist,
}

/// Input target for text input mode
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputTarget {
    ChannelRename { channel_idx: usize },
    ProjectName,
    Tempo,
}

/// Application mode - exactly one is active at a time
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    /// Normal editing mode with a focused panel
    Normal { panel: Panel },

    /// Command picker overlay (can be opened from any mode)
    CommandPicker { return_to: Panel },

    /// Plugin editor for a specific channel
    PluginEditor {
        channel_idx: usize,
        return_to: Panel,
    },

    /// Browser in selection mode (choosing sample/plugin for a channel)
    BrowserSelection {
        channel_idx: usize,
        return_to: Panel,
    },

    /// Text input mode (tempo entry, renaming, etc.)
    TextInput {
        target: InputTarget,
        return_to: Panel,
    },
}

impl Default for AppMode {
    fn default() -> Self {
        Self::Normal {
            panel: Panel::default(),
        }
    }
}

impl AppMode {
    /// Get the current panel (for rendering highlights, etc.)
    pub fn current_panel(&self) -> Panel {
        match self {
            Self::Normal { panel } => *panel,
            Self::CommandPicker { return_to } => *return_to,
            Self::PluginEditor { return_to, .. } => *return_to,
            Self::BrowserSelection { .. } => Panel::Browser,
            Self::TextInput { return_to, .. } => *return_to,
        }
    }

    /// Check if we're in normal mode
    pub fn is_normal(&self) -> bool {
        matches!(self, Self::Normal { .. })
    }

    /// Check if command picker is open
    pub fn is_command_picker(&self) -> bool {
        matches!(self, Self::CommandPicker { .. })
    }

    /// Check if plugin editor is open
    pub fn is_plugin_editor(&self) -> bool {
        matches!(self, Self::PluginEditor { .. })
    }

    /// Get plugin editor channel if in that mode
    pub fn plugin_editor_channel(&self) -> Option<usize> {
        match self {
            Self::PluginEditor { channel_idx, .. } => Some(*channel_idx),
            _ => None,
        }
    }

    /// Check if in browser selection mode
    pub fn is_browser_selection(&self) -> bool {
        matches!(self, Self::BrowserSelection { .. })
    }

    /// Get browser selection channel if in that mode
    pub fn browser_selection_channel(&self) -> Option<usize> {
        match self {
            Self::BrowserSelection { channel_idx, .. } => Some(*channel_idx),
            _ => None,
        }
    }

    /// Check if in text input mode
    pub fn is_text_input(&self) -> bool {
        matches!(self, Self::TextInput { .. })
    }

    /// Open command picker from current mode
    pub fn open_command_picker(&mut self) {
        let return_to = self.current_panel();
        *self = Self::CommandPicker { return_to };
    }

    /// Open plugin editor for a channel
    pub fn open_plugin_editor(&mut self, channel_idx: usize) {
        let return_to = self.current_panel();
        *self = Self::PluginEditor {
            channel_idx,
            return_to,
        };
    }

    /// Enter browser selection mode
    pub fn enter_browser_selection(&mut self, channel_idx: usize) {
        let return_to = self.current_panel();
        *self = Self::BrowserSelection {
            channel_idx,
            return_to,
        };
    }

    /// Start text input
    pub fn start_text_input(&mut self, target: InputTarget) {
        let return_to = self.current_panel();
        *self = Self::TextInput { target, return_to };
    }

    /// Close current modal and return to previous mode
    pub fn close_modal(&mut self) {
        *self = match self {
            Self::CommandPicker { return_to } => Self::Normal { panel: *return_to },
            Self::PluginEditor { return_to, .. } => Self::Normal { panel: *return_to },
            Self::BrowserSelection { return_to, .. } => Self::Normal { panel: *return_to },
            Self::TextInput { return_to, .. } => Self::Normal { panel: *return_to },
            Self::Normal { panel } => Self::Normal { panel: *panel },
        };
    }

    /// Switch to a different panel (only valid in normal mode)
    pub fn switch_panel(&mut self, panel: Panel) {
        if let Self::Normal { panel: p } = self {
            *p = panel;
        }
    }

    /// Get the focused panel, adjusting for view mode if in normal mode
    pub fn get_panel_for_view(&self, view_mode: ViewMode) -> Panel {
        if let Self::Normal { panel } = self {
            // If focused on a main view panel, sync with view mode
            match panel {
                Panel::ChannelRack | Panel::PianoRoll | Panel::Playlist => match view_mode {
                    ViewMode::ChannelRack => Panel::ChannelRack,
                    ViewMode::PianoRoll => Panel::PianoRoll,
                    ViewMode::Playlist => Panel::Playlist,
                },
                other => *other,
            }
        } else {
            self.current_panel()
        }
    }

    /// Cycle to next panel (only in normal mode)
    pub fn next_panel(&mut self, show_browser: bool, show_mixer: bool, view_mode: ViewMode) {
        if let Self::Normal { panel } = self {
            *panel = panel.next(show_browser, show_mixer, view_mode);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modal_roundtrip() {
        let mut mode = AppMode::Normal {
            panel: Panel::ChannelRack,
        };

        mode.open_command_picker();
        assert!(mode.is_command_picker());
        assert_eq!(mode.current_panel(), Panel::ChannelRack);

        mode.close_modal();
        assert!(mode.is_normal());
        assert_eq!(mode.current_panel(), Panel::ChannelRack);
    }

    #[test]
    fn test_plugin_editor_preserves_panel() {
        let mut mode = AppMode::Normal {
            panel: Panel::PianoRoll,
        };

        mode.open_plugin_editor(5);
        assert!(mode.is_plugin_editor());
        assert_eq!(mode.plugin_editor_channel(), Some(5));
        assert_eq!(mode.current_panel(), Panel::PianoRoll);

        mode.close_modal();
        assert!(mode.is_normal());
        assert_eq!(mode.current_panel(), Panel::PianoRoll);
    }

    #[test]
    fn test_browser_selection() {
        let mut mode = AppMode::Normal {
            panel: Panel::ChannelRack,
        };

        mode.enter_browser_selection(3);
        assert!(mode.is_browser_selection());
        assert_eq!(mode.browser_selection_channel(), Some(3));
        // Browser selection should show browser panel
        assert_eq!(mode.current_panel(), Panel::Browser);
    }
}
