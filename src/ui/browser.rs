//! Browser panel for file/sample/plugin navigation

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

use crate::app::{App, FocusedPanel};
use crate::browser::BrowserMode;

/// Render the browser panel
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focused_panel == FocusedPanel::Browser;
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = if app.browser.selection_mode {
        if let Some(ch) = app.browser.target_channel {
            match app.browser.mode {
                BrowserMode::Samples => format!("Select Sample for CH{} *", ch + 1),
                BrowserMode::Plugins => format!("Select Plugin for CH{} *", ch + 1),
            }
        } else {
            match app.browser.mode {
                BrowserMode::Samples => "Select Sample *".to_string(),
                BrowserMode::Plugins => "Select Plugin *".to_string(),
            }
        }
    } else if focused {
        "Browser *".to_string()
    } else {
        "Browser".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split for tabs and content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    // Render mode tabs
    let tabs = Tabs::new(vec!["Samples", "Plugins"])
        .select(match app.browser.mode {
            BrowserMode::Samples => 0,
            BrowserMode::Plugins => 1,
        })
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider("|");
    frame.render_widget(tabs, chunks[0]);

    let content_area = chunks[1];

    // Check if we have entries
    if app.browser.visible_entries.is_empty() {
        let msg = match app.browser.mode {
            BrowserMode::Samples => "No samples found\n\nAdd .wav/.mp3/.flac files to:\nsamples/",
            BrowserMode::Plugins => "No plugins found\n\nAdd .clap files to:\nplugins/",
        };
        let paragraph = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
        frame.render_widget(paragraph, content_area);
        return;
    }

    // Calculate visible range for scrolling
    let visible_height = content_area.height as usize;
    let cursor = app.browser.cursor;
    let total = app.browser.visible_entries.len();

    // Center the cursor when possible
    let scroll_offset = if total <= visible_height || cursor < visible_height / 2 {
        0
    } else if cursor > total - visible_height / 2 {
        total.saturating_sub(visible_height)
    } else {
        cursor.saturating_sub(visible_height / 2)
    };

    let mode = app.browser.mode;

    // Build lines for visible entries
    let lines: Vec<Line> = app
        .browser
        .visible_entries
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|(idx, entry)| {
            let is_cursor = idx == cursor;
            let is_expanded = app.browser.expanded.contains(&entry.path);

            // Indentation based on depth
            let indent = "  ".repeat(entry.depth);

            // Icon for folder/file/plugin
            let icon = if entry.is_dir {
                if is_expanded {
                    "▼ "
                } else {
                    "▶ "
                }
            } else {
                match mode {
                    BrowserMode::Samples => "♪ ",
                    BrowserMode::Plugins => "P ",
                }
            };

            // Entry style
            let style = if is_cursor {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if entry.is_dir {
                Style::default().fg(Color::Blue)
            } else {
                Style::default().fg(Color::White)
            };

            // Build the line
            let mut spans = vec![Span::raw(indent)];
            spans.push(Span::styled(format!("{}{}", icon, entry.name), style));

            Line::from(spans)
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, content_area);
}
