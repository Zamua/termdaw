//! UI rendering using ratatui

mod browser;
mod channel_rack;
pub mod colors;
mod command_picker;
mod envelope;
mod mixer;
mod piano_roll;
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

    let display_title = if focused {
        format!("{} *", title)
    } else {
        title.to_string()
    };

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
    let area = frame.area();

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

    // Transport bar
    transport::render(frame, main_layout[0], app);

    // Main content area (Browser? | Main View)
    render_content_area(frame, main_layout[1], app);

    // Mixer (if visible)
    if app.show_mixer {
        mixer::render(frame, main_layout[2], app);
    }

    // Command picker overlay (rendered last, on top of everything)
    command_picker::render(frame, app);

    // Plugin editor modal (rendered on top of command picker)
    plugin_editor::render(frame, app);
}

/// Render the main content area (browser + main view)
fn render_content_area(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    if app.show_browser {
        // Split horizontally: Browser | Main View
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(30), // Browser width
                Constraint::Min(40),    // Main view
            ])
            .split(area);

        browser::render(frame, chunks[0], app);
        render_main_view(frame, chunks[1], app);
    } else {
        render_main_view(frame, area, app);
    }
}

/// Render the main view (channel rack, piano roll, or playlist)
fn render_main_view(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    match app.view_mode {
        ViewMode::ChannelRack => channel_rack::render(frame, area, app),
        ViewMode::PianoRoll => piano_roll::render(frame, area, app),
        ViewMode::Playlist => playlist::render(frame, area, app),
    }
}
