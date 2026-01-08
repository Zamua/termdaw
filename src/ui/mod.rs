//! UI rendering using ratatui

pub mod areas;
mod browser;
mod channel_rack;
pub mod colors;
mod command_picker;
pub mod context_menu;
mod effect_editor;
mod envelope;
mod event_log;
mod mixer;
mod playlist;
pub mod plugin_editor;
mod transport;
pub mod waveform;
mod widgets;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders},
    Frame,
};

use crate::app::{App, Panel};
use crate::mode::ViewMode;

/// Render a panel frame with focus-aware styling.
/// Returns the inner area for content rendering.
pub fn render_panel_frame(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    panel: Panel,
    app: &App,
) -> Rect {
    let focused = app.ui.mode.current_panel() == panel;
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let display_title = title.to_string();

    let block = Block::default()
        .title(display_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// Main render function - draws the entire UI
pub fn render(frame: &mut Frame, app: &mut App) {
    use areas::AreaId;

    let area = frame.area();

    // Clear all screen areas from previous frame
    app.ui.screen_areas.clear();

    // Main vertical layout: Transport | Content | Mixer? | Status
    let mut constraints = vec![
        Constraint::Length(3), // Transport bar
        Constraint::Min(10),   // Main content area
    ];

    if app.ui.show_mixer {
        constraints.push(Constraint::Length(16)); // Mixer panel
    }

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // Register transport area
    app.ui
        .screen_areas
        .register(AreaId::Transport, main_layout[0]);

    // Transport bar
    transport::render(frame, main_layout[0], app);

    // Main content area (Browser? | Main View)
    render_content_area(frame, main_layout[1], app);

    // Mixer (if visible)
    if app.ui.show_mixer {
        app.ui.screen_areas.register(AreaId::Mixer, main_layout[2]);
        mixer::render(frame, main_layout[2], app);
    }

    // Command picker overlay (rendered last, on top of everything)
    command_picker::render(frame, app);

    // Plugin editor modal (rendered on top of command picker)
    plugin_editor::render(frame, app);

    // Context menu (rendered on top of everything else)
    context_menu::render(frame, &app.ui.context_menu, &mut app.ui.screen_areas);

    // Effect picker/editor modal
    effect_editor::render(frame, app);
}

/// Render the main content area (browser + main view + event log)
fn render_content_area(frame: &mut Frame, area: ratatui::layout::Rect, app: &mut App) {
    use areas::AreaId;

    // Build constraints based on visible panels
    let mut constraints = Vec::new();

    if app.ui.show_browser {
        constraints.push(Constraint::Length(30)); // Browser width
    }

    constraints.push(Constraint::Min(40)); // Main view (always present)

    if app.ui.show_event_log {
        constraints.push(Constraint::Length(35)); // Event log width
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    let mut chunk_idx = 0;

    // Browser (optional, left side)
    if app.ui.show_browser {
        app.ui
            .screen_areas
            .register(AreaId::Browser, chunks[chunk_idx]);
        browser::render(frame, chunks[chunk_idx], app);
        chunk_idx += 1;
    }

    // Main view (always present, center)
    app.ui
        .screen_areas
        .register(AreaId::MainView, chunks[chunk_idx]);
    render_main_view(frame, chunks[chunk_idx], app);
    chunk_idx += 1;

    // Event log (optional, right side)
    if app.ui.show_event_log {
        app.ui
            .screen_areas
            .register(AreaId::EventLog, chunks[chunk_idx]);
        event_log::render(frame, chunks[chunk_idx], app);
    }
}

/// Render the main view with tabs and content (no outer border)
fn render_main_view(frame: &mut Frame, area: ratatui::layout::Rect, app: &mut App) {
    use areas::AreaId;
    use ratatui::{style::Modifier, widgets::Tabs};

    // Use a block with no borders (hidden)
    let block = Block::default().borders(Borders::NONE);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner area: tabs row | content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let tab_bar = chunks[0];
    let content = chunks[1];

    // Register tab bar area
    app.ui
        .screen_areas
        .register(AreaId::MainViewTabBar, tab_bar);

    // Piano roll is considered part of channel rack view
    let selected_tab = match app.ui.view_mode {
        ViewMode::ChannelRack | ViewMode::PianoRoll => 0,
        ViewMode::Playlist => 1,
    };

    // Calculate tab positions for click detection
    // Tab format: "Channel Rack | Playlist" with padding from Tabs widget
    let cr_text = "Channel Rack";
    let pl_text = "Playlist";

    // Tabs widget adds space padding, approximate positions
    let cr_rect = Rect::new(tab_bar.x, tab_bar.y, cr_text.len() as u16 + 2, 1);
    let pl_rect = Rect::new(
        tab_bar.x + cr_text.len() as u16 + 4,
        tab_bar.y,
        pl_text.len() as u16 + 2,
        1,
    );
    app.ui
        .screen_areas
        .register(AreaId::MainViewTabChannelRack, cr_rect);
    app.ui
        .screen_areas
        .register(AreaId::MainViewTabPlaylist, pl_rect);

    // Check if main view is focused (channel rack, piano roll, or playlist)
    let main_view_focused = matches!(
        app.ui.mode.current_panel(),
        Panel::ChannelRack | Panel::PianoRoll | Panel::Playlist
    );

    // Render mode tabs - only highlight in cyan when focused
    let highlight_color = if main_view_focused {
        Color::Cyan
    } else {
        Color::White
    };
    let tabs = Tabs::new(vec![cr_text, pl_text])
        .select(selected_tab)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(highlight_color)
                .add_modifier(Modifier::BOLD),
        )
        .divider("|");
    frame.render_widget(tabs, tab_bar);

    // Render content based on view mode
    // Note: Piano roll is embedded in channel rack view (not a separate view)
    match app.ui.view_mode {
        ViewMode::ChannelRack | ViewMode::PianoRoll => channel_rack::render(frame, content, app),
        ViewMode::Playlist => playlist::render(frame, content, app),
    }
}
