//! Command picker UI overlay - which-key style

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::command_picker::CommandPicker;

/// Render the command picker overlay (or input mode)
pub fn render(frame: &mut Frame, app: &App) {
    // Render input mode if active
    if app.command_picker.input.active {
        render_input_mode(frame, app);
        return;
    }

    // Render command picker if visible
    if !app.command_picker.visible {
        return;
    }

    let area = frame.area();

    // Calculate centered popup size
    let popup_width = 24;
    let popup_height = 18;

    let popup_area = centered_rect(popup_width, popup_height, area);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Render the popup
    let block = Block::default()
        .title(" Commands ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Render command groups
    render_commands(frame, inner, &app.command_picker);
}

/// Render the command groups
fn render_commands(frame: &mut Frame, area: Rect, picker: &CommandPicker) {
    let mut lines: Vec<Line> = Vec::new();

    for group in &picker.groups {
        // Group header
        lines.push(Line::from(Span::styled(
            group.name,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));

        // Commands in single column
        for cmd in &group.commands {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    CommandPicker::format_key(cmd.key()),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(cmd.label(), Style::default().fg(Color::White)),
            ]));
        }

        // Blank line between groups
        lines.push(Line::from(""));
    }

    // Footer
    lines.push(Line::from(vec![
        Span::styled("[Esc]", Style::default().fg(Color::DarkGray)),
        Span::styled(" Cancel", Style::default().fg(Color::DarkGray)),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Render the input mode overlay (tempo entry, etc.)
fn render_input_mode(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Small centered popup
    let popup_width = 30;
    let popup_height = 5;
    let popup_area = centered_rect(popup_width, popup_height, area);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Render the popup
    let block = Block::default()
        .title(format!(" {} ", app.command_picker.input.prompt))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Center the input box, but left-align text within it
    let text_width: u16 = 10; // Enough for "999.99" + cursor
    let input_area = Rect {
        x: inner.x + (inner.width.saturating_sub(text_width)) / 2,
        y: inner.y + inner.height / 2,
        width: text_width,
        height: 1,
    };

    // Get scroll offset for the input
    let input = &app.command_picker.input.input;
    let scroll = input.visual_scroll(text_width as usize);

    // Render the input value
    let input_widget = Paragraph::new(input.value())
        .style(Style::default().fg(Color::White))
        .scroll((0, scroll as u16));
    frame.render_widget(input_widget, input_area);

    // Set cursor position
    let cursor_x = input_area.x + (input.visual_cursor().saturating_sub(scroll)) as u16;
    frame.set_cursor_position((cursor_x, input_area.y));

    // Help text at bottom
    let help = Paragraph::new("[Enter] Confirm  [Esc] Cancel")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    let help_area = Rect {
        x: inner.x,
        y: inner.y + inner.height - 1,
        width: inner.width,
        height: 1,
    };
    frame.render_widget(help, help_area);
}

/// Create a centered rect of given size within the parent area
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;

    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
