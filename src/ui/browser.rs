//! Browser panel for file/sample/plugin navigation

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

use super::areas::AreaId;
use crate::app::{App, Panel};
use crate::browser::BrowserMode;

/// Render the browser panel
pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.mode.current_panel() == Panel::Browser;
    let always_highlight = app.browser.selection_mode;

    // Selection mode always shows highlighted border
    let show_focused = focused || always_highlight;
    let border_color = if show_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    // Split area: tabs row | content box
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    // Split tabs row into tabs and close button
    let tabs_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(chunks[0]);

    // Register tabs area (left portion for Samples/Plugins)
    app.screen_areas.register(AreaId::BrowserTabs, tabs_row[0]);

    // Register close button area
    app.screen_areas.register(AreaId::BrowserClose, tabs_row[1]);

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
    frame.render_widget(tabs, tabs_row[0]);

    // Render close button
    let close_btn = Paragraph::new(" × ").style(Style::default().fg(Color::Red));
    frame.render_widget(close_btn, tabs_row[1]);

    // Content box with no title
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let content_area = content_block.inner(chunks[1]);
    frame.render_widget(content_block, chunks[1]);

    // Register content area
    app.screen_areas.register(AreaId::BrowserContent, content_area);

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

    // Clone entry data needed for rendering to avoid borrow conflicts
    let entries_to_render: Vec<_> = app
        .browser
        .visible_entries
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|(idx, entry)| {
            let is_cursor = idx == cursor;
            let is_expanded = app.browser.expanded.contains(&entry.path);
            (idx, entry.clone(), is_cursor, is_expanded)
        })
        .collect();

    // Register browser item areas
    for (visible_idx, (idx, _, _, _)) in entries_to_render.iter().enumerate() {
        let item_rect = Rect::new(
            content_area.x,
            content_area.y + visible_idx as u16,
            content_area.width,
            1,
        );
        // visible_idx is the index in the visible list (0-based from top of view)
        app.screen_areas.browser_items.push(item_rect);
        let _ = idx; // Suppress unused warning
    }

    // Build lines for visible entries
    let lines: Vec<Line> = entries_to_render
        .iter()
        .map(|(_idx, entry, is_cursor, is_expanded)| {
            // Indentation based on depth
            let indent = "  ".repeat(entry.depth);

            // Icon for folder/file/plugin
            let icon = if entry.is_dir {
                if *is_expanded {
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
            let style = if *is_cursor {
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
