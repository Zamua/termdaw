//! Context menu overlay for right-click actions
//!
//! Design principles (matching mouse.rs):
//! - Pure data structure for menu state
//! - Actions are executed by the caller, not the menu
//! - Menu doesn't know about App internals

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::areas::{AreaId, ScreenAreas};

// ============================================================================
// Context Menu Actions
// ============================================================================

/// Actions that can be triggered from context menus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuAction {
    // Channel Rack actions
    DeleteChannel,
    DuplicateChannel,
    AssignSample,
    AssignPlugin,
    MuteChannel,
    SoloChannel,
    PreviewChannel,
    OpenPianoRoll,

    // Piano Roll actions
    DeleteNote,
    DuplicateNote,
    SetVelocity,

    // Playlist actions
    DeletePlacement,
    DuplicatePlacement,
    MutePattern,

    // Mixer actions (TODO: reimplement mouse handling for track-based mixer)
    #[allow(dead_code)]
    ResetVolume,
    #[allow(dead_code)]
    MuteTrack,
    #[allow(dead_code)]
    SoloTrack,

    // Browser actions
    PreviewFile,
    AssignToChannel,

    // Plugin Editor actions
    #[allow(dead_code)]
    ResetParameter,
}

impl ContextMenuAction {
    /// Get the display label for this action
    pub fn label(&self) -> &'static str {
        match self {
            ContextMenuAction::DeleteChannel => "Delete Channel",
            ContextMenuAction::DuplicateChannel => "Duplicate Channel",
            ContextMenuAction::AssignSample => "Assign Sample...",
            ContextMenuAction::AssignPlugin => "Assign Plugin...",
            ContextMenuAction::MuteChannel => "Mute",
            ContextMenuAction::SoloChannel => "Solo",
            ContextMenuAction::PreviewChannel => "Preview",
            ContextMenuAction::OpenPianoRoll => "Piano Roll",
            ContextMenuAction::DeleteNote => "Delete Note",
            ContextMenuAction::DuplicateNote => "Duplicate Note",
            ContextMenuAction::SetVelocity => "Set Velocity...",
            ContextMenuAction::DeletePlacement => "Delete Placement",
            ContextMenuAction::DuplicatePlacement => "Duplicate Placement",
            ContextMenuAction::MutePattern => "Mute Pattern",
            ContextMenuAction::ResetVolume => "Reset Volume",
            ContextMenuAction::MuteTrack => "Mute",
            ContextMenuAction::SoloTrack => "Solo",
            ContextMenuAction::PreviewFile => "Preview",
            ContextMenuAction::AssignToChannel => "Assign to Channel",
            ContextMenuAction::ResetParameter => "Reset to Default",
        }
    }

    /// Get keyboard shortcut hint (if any)
    pub fn shortcut(&self) -> Option<&'static str> {
        match self {
            ContextMenuAction::DeleteChannel => Some("d"),
            ContextMenuAction::MuteChannel => Some("m"),
            ContextMenuAction::SoloChannel => Some("S"),
            ContextMenuAction::PreviewChannel => Some("s"),
            ContextMenuAction::OpenPianoRoll => Some("p"),
            ContextMenuAction::DeleteNote => Some("x"),
            ContextMenuAction::MutePattern => Some("m"),
            ContextMenuAction::MuteTrack => Some("m"),
            ContextMenuAction::SoloTrack => Some("s"),
            ContextMenuAction::PreviewFile => Some("s"),
            _ => None,
        }
    }
}

// ============================================================================
// Context Menu Item
// ============================================================================

/// A single item in a context menu
#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    pub action: ContextMenuAction,
    pub enabled: bool,
}

impl ContextMenuItem {
    pub fn new(action: ContextMenuAction) -> Self {
        Self {
            action,
            enabled: true,
        }
    }

    pub fn disabled(action: ContextMenuAction) -> Self {
        Self {
            action,
            enabled: false,
        }
    }
}

// ============================================================================
// Context Menu State
// ============================================================================

/// Context for where the menu was opened
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuContext {
    ChannelRack {
        channel: usize,
    },
    PianoRoll {
        pitch: u8,
        step: usize,
    },
    Playlist {
        row: usize,
        bar: usize,
    },
    #[allow(dead_code)]
    Mixer {
        channel: usize,
    },
    Browser {
        item_idx: usize,
    },
    #[allow(dead_code)]
    PluginEditor {
        param_idx: usize,
    },
}

/// State for the context menu overlay
#[derive(Debug, Clone, Default)]
pub struct ContextMenu {
    /// Whether the menu is visible
    pub visible: bool,
    /// Screen position where menu was opened
    pub x: u16,
    pub y: u16,
    /// Menu items
    pub items: Vec<ContextMenuItem>,
    /// Currently highlighted item index
    pub selected: usize,
    /// Context where menu was opened (for action execution)
    pub context: Option<MenuContext>,
}

impl ContextMenu {
    pub fn new() -> Self {
        Self::default()
    }

    /// Show menu at position with given items
    pub fn show(&mut self, x: u16, y: u16, items: Vec<ContextMenuItem>, context: MenuContext) {
        self.visible = true;
        self.x = x;
        self.y = y;
        self.items = items;
        self.selected = 0;
        self.context = Some(context);
    }

    /// Hide the menu
    pub fn hide(&mut self) {
        self.visible = false;
        self.items.clear();
        self.context = None;
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
    }

    /// Get the currently selected action (if enabled)
    pub fn get_selected_action(&self) -> Option<ContextMenuAction> {
        self.items.get(self.selected).and_then(|item| {
            if item.enabled {
                Some(item.action)
            } else {
                None
            }
        })
    }

    /// Get item at screen position
    pub fn item_at(&self, _x: u16, y: u16, menu_rect: Rect) -> Option<usize> {
        if y < menu_rect.y + 1 || y >= menu_rect.y + menu_rect.height - 1 {
            return None;
        }
        let item_y = (y - menu_rect.y - 1) as usize;
        if item_y < self.items.len() {
            Some(item_y)
        } else {
            None
        }
    }

    /// Calculate menu rect for rendering
    pub fn menu_rect(&self, screen_width: u16, screen_height: u16) -> Rect {
        let width = 20u16;
        let height = (self.items.len() as u16 + 2).min(screen_height); // +2 for border

        // Adjust position to stay on screen
        let x = if self.x + width > screen_width {
            screen_width.saturating_sub(width)
        } else {
            self.x
        };
        let y = if self.y + height > screen_height {
            screen_height.saturating_sub(height)
        } else {
            self.y
        };

        Rect::new(x, y, width, height)
    }
}

// ============================================================================
// Menu Builders for Each Component
// ============================================================================

/// Build context menu items for channel rack
pub fn channel_rack_menu(has_sample: bool, is_plugin: bool) -> Vec<ContextMenuItem> {
    let mut items = vec![
        ContextMenuItem::new(ContextMenuAction::PreviewChannel),
        ContextMenuItem::new(ContextMenuAction::OpenPianoRoll),
    ];

    if is_plugin {
        items.push(ContextMenuItem::new(ContextMenuAction::AssignPlugin));
    } else {
        items.push(ContextMenuItem::new(ContextMenuAction::AssignSample));
    }

    items.extend([
        ContextMenuItem::new(ContextMenuAction::MuteChannel),
        ContextMenuItem::new(ContextMenuAction::SoloChannel),
        ContextMenuItem::new(ContextMenuAction::DuplicateChannel),
    ]);

    // Only allow delete if channel has content
    if has_sample || is_plugin {
        items.push(ContextMenuItem::new(ContextMenuAction::DeleteChannel));
    } else {
        items.push(ContextMenuItem::disabled(ContextMenuAction::DeleteChannel));
    }

    items
}

/// Build context menu items for piano roll
pub fn piano_roll_menu(has_note: bool) -> Vec<ContextMenuItem> {
    if has_note {
        vec![
            ContextMenuItem::new(ContextMenuAction::DeleteNote),
            ContextMenuItem::new(ContextMenuAction::DuplicateNote),
            ContextMenuItem::new(ContextMenuAction::SetVelocity),
        ]
    } else {
        vec![
            ContextMenuItem::disabled(ContextMenuAction::DeleteNote),
            ContextMenuItem::disabled(ContextMenuAction::DuplicateNote),
            ContextMenuItem::disabled(ContextMenuAction::SetVelocity),
        ]
    }
}

/// Build context menu items for playlist
pub fn playlist_menu(has_placement: bool) -> Vec<ContextMenuItem> {
    vec![
        if has_placement {
            ContextMenuItem::new(ContextMenuAction::DeletePlacement)
        } else {
            ContextMenuItem::disabled(ContextMenuAction::DeletePlacement)
        },
        if has_placement {
            ContextMenuItem::new(ContextMenuAction::DuplicatePlacement)
        } else {
            ContextMenuItem::disabled(ContextMenuAction::DuplicatePlacement)
        },
        ContextMenuItem::new(ContextMenuAction::MutePattern),
    ]
}

/// Build context menu items for mixer
#[allow(dead_code)]
pub fn mixer_menu() -> Vec<ContextMenuItem> {
    vec![
        ContextMenuItem::new(ContextMenuAction::ResetVolume),
        ContextMenuItem::new(ContextMenuAction::MuteTrack),
        ContextMenuItem::new(ContextMenuAction::SoloTrack),
    ]
}

/// Build context menu items for browser
pub fn browser_menu(is_file: bool) -> Vec<ContextMenuItem> {
    if is_file {
        vec![
            ContextMenuItem::new(ContextMenuAction::PreviewFile),
            ContextMenuItem::new(ContextMenuAction::AssignToChannel),
        ]
    } else {
        // Directory - no useful actions
        vec![]
    }
}

/// Build context menu items for plugin editor
#[allow(dead_code)]
pub fn plugin_editor_menu() -> Vec<ContextMenuItem> {
    vec![ContextMenuItem::new(ContextMenuAction::ResetParameter)]
}

// ============================================================================
// Rendering
// ============================================================================

/// Render the context menu overlay
pub fn render(frame: &mut Frame, menu: &ContextMenu, screen_areas: &mut ScreenAreas) {
    if !menu.visible || menu.items.is_empty() {
        return;
    }

    let area = frame.area();
    let menu_rect = menu.menu_rect(area.width, area.height);

    // Register context menu area
    screen_areas.register(AreaId::ContextMenu, menu_rect);

    // Register individual menu item areas
    for (idx, _item) in menu.items.iter().enumerate() {
        let item_rect = Rect::new(
            menu_rect.x + 1,
            menu_rect.y + 1 + idx as u16,
            menu_rect.width - 2,
            1,
        );
        screen_areas.context_menu_items.push(item_rect);
    }

    // Clear area behind menu
    frame.render_widget(Clear, menu_rect);

    // Render border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(menu_rect);
    frame.render_widget(block, menu_rect);

    // Render menu items
    let lines: Vec<Line> = menu
        .items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let is_selected = idx == menu.selected;

            let label = item.action.label();
            let shortcut = item.action.shortcut();

            let style = if !item.enabled {
                Style::default().fg(Color::DarkGray)
            } else if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let mut spans = vec![Span::styled(format!(" {:<14}", label), style)];

            if let Some(key) = shortcut {
                spans.push(Span::styled(
                    format!("{:>3}", key),
                    if item.enabled && is_selected {
                        style
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ));
            } else {
                spans.push(Span::raw("   "));
            }

            Line::from(spans)
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
