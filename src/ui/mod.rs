//! UI rendering using ratatui

pub mod areas;
mod browser;
mod channel_rack;
pub mod colors;
mod command_picker;
pub mod context_menu;
mod effect_editor;
mod envelope;
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
    let focused = app.mode.current_panel() == panel;
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
    app.screen_areas.clear();

    // Main vertical layout: Transport | Content | Mixer? | Status
    let mut constraints = vec![
        Constraint::Length(3), // Transport bar
        Constraint::Min(10),   // Main content area
    ];

    if app.show_mixer {
        constraints.push(Constraint::Length(16)); // Mixer panel
    }

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // Register transport area
    app.screen_areas.register(AreaId::Transport, main_layout[0]);

    // Transport bar
    transport::render(frame, main_layout[0], app);

    // Main content area (Browser? | Main View)
    render_content_area(frame, main_layout[1], app);

    // Mixer (if visible)
    if app.show_mixer {
        app.screen_areas.register(AreaId::Mixer, main_layout[2]);
        mixer::render(frame, main_layout[2], app);
    }

    // Command picker overlay (rendered last, on top of everything)
    command_picker::render(frame, app);

    // Plugin editor modal (rendered on top of command picker)
    plugin_editor::render(frame, app);

    // Context menu (rendered on top of everything else)
    context_menu::render(frame, &app.context_menu, &mut app.screen_areas);

    // Effect picker/editor modal
    effect_editor::render(frame, app);
}

/// Render the main content area (browser + main view)
fn render_content_area(frame: &mut Frame, area: ratatui::layout::Rect, app: &mut App) {
    use areas::AreaId;

    if app.show_browser {
        // Split horizontally: Browser | Main View
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(30), // Browser width
                Constraint::Min(40),    // Main view
            ])
            .split(area);

        app.screen_areas.register(AreaId::Browser, chunks[0]);
        browser::render(frame, chunks[0], app);

        app.screen_areas.register(AreaId::MainView, chunks[1]);
        render_main_view(frame, chunks[1], app);
    } else {
        app.screen_areas.register(AreaId::MainView, area);
        render_main_view(frame, area, app);
    }
}

/// Render the main view with tabs and content (no outer border)
fn render_main_view(frame: &mut Frame, area: ratatui::layout::Rect, app: &mut App) {
    use areas::AreaId;
    use ratatui::{
        style::Modifier,
        widgets::Tabs,
    };

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
    app.screen_areas.register(AreaId::MainViewTabBar, tab_bar);

    // Piano roll is considered part of channel rack view
    let selected_tab = match app.view_mode {
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
    app.screen_areas
        .register(AreaId::MainViewTabChannelRack, cr_rect);
    app.screen_areas
        .register(AreaId::MainViewTabPlaylist, pl_rect);

    // Render mode tabs using Tabs widget (like browser)
    let tabs = Tabs::new(vec![cr_text, pl_text])
        .select(selected_tab)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider("|");
    frame.render_widget(tabs, tab_bar);

    // Render content based on view mode
    // Note: Piano roll is embedded in channel rack view (not a separate view)
    match app.view_mode {
        ViewMode::ChannelRack | ViewMode::PianoRoll => {
            channel_rack::render(frame, content, app)
        }
        ViewMode::Playlist => playlist::render(frame, content, app),
    }
}
