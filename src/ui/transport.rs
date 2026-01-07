//! Transport bar component

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::areas::AreaId;
use super::waveform;
use crate::app::App;

/// Render the transport bar
pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split into: waveform | spacer | transport info | spacer | browser | mixer
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25), // Waveform visualizer
            Constraint::Length(2),  // Spacer
            Constraint::Length(35), // Transport info (play, bpm, time sig, step)
            Constraint::Length(2),  // Spacer
            Constraint::Length(5),  // Browser toggle
            Constraint::Length(1),  // Spacer
            Constraint::Length(5),  // Mixer toggle
            Constraint::Min(0),     // Remaining space
        ])
        .split(inner);

    // Register waveform area
    app.ui
        .screen_areas
        .register(AreaId::TransportWaveform, chunks[0]);

    // Render waveform using Kitty graphics (direct to stdout)
    if chunks[0].width > 0 && chunks[0].height > 0 {
        waveform::render_waveform_direct(
            app.audio.waveform_buffer(),
            chunks[0].x,
            chunks[0].y,
            chunks[0].width,
            chunks[0].height,
        );
    }

    // Transport info area
    let info_area = chunks[2];
    render_transport_info(frame, info_area, app);

    // Browser toggle area
    let browser_area = chunks[4];
    render_browser_toggle(frame, browser_area, app);

    // Mixer toggle area
    let mixer_area = chunks[6];
    render_mixer_toggle(frame, mixer_area, app);
}

/// Render transport info (play/stop, BPM, time sig, step)
fn render_transport_info(frame: &mut Frame, area: Rect, app: &mut App) {
    let info_x = area.x;
    let info_y = area.y;

    // Play/Stop area: "▶ Playing" or "■ Stopped" = ~10 chars
    let play_stop_rect = Rect::new(info_x, info_y, 10, 1);
    app.ui
        .screen_areas
        .register(AreaId::TransportPlayStop, play_stop_rect);

    // BPM area: starts after play/stop, "  XXX BPM" = ~10 chars
    let bpm_rect = Rect::new(info_x + 10, info_y, 10, 1);
    app.ui.screen_areas.register(AreaId::TransportBpm, bpm_rect);

    // Play/Stop indicator
    let play_indicator = if app.is_playing() {
        Span::styled("▶ Playing", Style::default().fg(Color::Green))
    } else {
        Span::styled("■ Stopped", Style::default().fg(Color::Red))
    };

    // BPM display
    let bpm_display = Span::styled(
        format!("  {:.0} BPM", app.transport.bpm),
        Style::default().fg(Color::White),
    );

    // Time signature (static for now)
    let time_sig = Span::styled("  4/4", Style::default().fg(Color::DarkGray));

    // Playhead position
    let position = Span::styled(
        format!("  Step: {:02}", app.playhead_step() + 1),
        Style::default().fg(Color::DarkGray),
    );

    let line = Line::from(vec![play_indicator, bpm_display, time_sig, position]);
    let transport_info = Paragraph::new(line);
    frame.render_widget(transport_info, area);
}

/// Render browser toggle button
fn render_browser_toggle(frame: &mut Frame, area: Rect, app: &mut App) {
    let browser_rect = Rect::new(area.x, area.y, 3, 1);
    app.ui
        .screen_areas
        .register(AreaId::TransportBrowserToggle, browser_rect);

    let style = if app.ui.show_browser {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let line = Line::from(vec![Span::styled("BRO", style)]);
    let browser_btn = Paragraph::new(line);
    frame.render_widget(browser_btn, area);
}

/// Render mixer toggle button
fn render_mixer_toggle(frame: &mut Frame, area: Rect, app: &mut App) {
    let mixer_rect = Rect::new(area.x, area.y, 5, 1);
    app.ui
        .screen_areas
        .register(AreaId::TransportMixerToggle, mixer_rect);

    let style = if app.ui.show_mixer {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let line = Line::from(vec![Span::styled("MIX", style)]);
    let mixer_btn = Paragraph::new(line);
    frame.render_widget(mixer_btn, area);
}
