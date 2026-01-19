//! Confirm dialog UI rendering

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Render the confirm dialog if visible
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if !app.ui.confirm_dialog.visible {
        return;
    }

    // Calculate centered popup dimensions
    let popup_width = 40.min(area.width.saturating_sub(4));
    let popup_height = 7;
    let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Clear the background
    frame.render_widget(Clear, popup_area);

    // Render the dialog
    let block = Block::default()
        .title(" Confirm ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            &app.ui.confirm_dialog.message,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y]", Style::default().fg(Color::Green)),
            Span::raw(" Yes  "),
            Span::styled("[n]", Style::default().fg(Color::Red)),
            Span::raw(" No"),
        ]),
    ];

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, inner);
}
