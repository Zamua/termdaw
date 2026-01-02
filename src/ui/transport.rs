//! Transport bar component

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::waveform;
use crate::app::App;

/// Render the transport bar
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split into waveform area, spacer, and info area (waveform on left)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25), // Waveform visualizer
            Constraint::Length(2),  // Spacer
            Constraint::Min(30),    // Transport info
        ])
        .split(inner);

    // Render waveform using Kitty graphics (direct to stdout)
    // This is rendered first (left side)
    if chunks[0].width > 0 && chunks[0].height > 0 {
        waveform::render_waveform_direct(
            app.audio.waveform_buffer(),
            chunks[0].x,
            chunks[0].y,
            chunks[0].width,
            chunks[0].height,
        );
    }

    // Play/Stop indicator
    let play_indicator = if app.is_playing {
        Span::styled("▶ Playing", Style::default().fg(Color::Green))
    } else {
        Span::styled("■ Stopped", Style::default().fg(Color::Red))
    };

    // BPM display
    let bpm_display = Span::styled(
        format!("  {:.0} BPM", app.bpm),
        Style::default().fg(Color::White),
    );

    // Time signature (static for now)
    let time_sig = Span::styled("  4/4", Style::default().fg(Color::DarkGray));

    // Playhead position
    let position = Span::styled(
        format!("  Step: {:02}", app.playhead_step + 1),
        Style::default().fg(Color::DarkGray),
    );

    let line = Line::from(vec![play_indicator, bpm_display, time_sig, position]);
    let transport_info = Paragraph::new(line);

    frame.render_widget(transport_info, chunks[2]);
}
