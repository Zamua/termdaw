//! Event log panel for displaying recent commands
//!
//! Displays a scrollable list of recently dispatched commands with timestamps,
//! useful for debugging and understanding application state.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::areas::AreaId;
use crate::app::App;
use crate::event_log::LogEntry;

/// Render the event log panel
pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let border_color = Color::Yellow; // Match toggle button color

    // Split area: title row | content box (like browser)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    // Split title row into title and close button
    let title_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(chunks[0]);

    // Register close button area
    app.ui
        .screen_areas
        .register(AreaId::EventLogClose, title_row[1]);

    // Render title above the box
    let title = Paragraph::new("Event Log").style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(title, title_row[0]);

    // Render close button
    let close_btn = Paragraph::new(" × ").style(Style::default().fg(Color::Red));
    frame.render_widget(close_btn, title_row[1]);

    // Content box with no title
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = content_block.inner(chunks[1]);
    frame.render_widget(content_block, chunks[1]);

    if app.event_log().is_empty() {
        let msg =
            Paragraph::new("No events logged yet").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(msg, inner);
        return;
    }

    let now = std::time::Instant::now();
    let visible_height = inner.height as usize;

    let lines: Vec<Line> = app
        .event_log()
        .entries_recent_first()
        .take(visible_height)
        .map(|entry| format_entry(entry, now))
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

/// Format a log entry for display
fn format_entry(entry: &LogEntry, now: std::time::Instant) -> Line<'static> {
    let elapsed = now.duration_since(entry.timestamp);
    let time_str = format_elapsed(elapsed);

    let desc_color = if entry.is_undoable {
        Color::White
    } else {
        Color::DarkGray // Non-undoable commands dimmed
    };

    let desc_style = if entry.is_undoable {
        Style::default().fg(desc_color)
    } else {
        Style::default().fg(desc_color).add_modifier(Modifier::DIM)
    };

    Line::from(vec![
        Span::styled(time_str, Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        Span::styled(entry.description.to_string(), desc_style),
    ])
}

/// Format elapsed time for display
fn format_elapsed(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        format!("{:>3}s", secs)
    } else if secs < 3600 {
        format!("{:>2}m", secs / 60)
    } else {
        format!("{:>2}h", secs / 3600)
    }
}
