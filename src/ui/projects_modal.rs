//! Projects modal UI - list and open projects interactively

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;

/// Render the projects modal overlay
pub fn render(frame: &mut Frame, app: &mut App) {
    if !app.ui.projects_modal.visible {
        return;
    }

    let area = frame.area();

    // Calculate centered popup size
    let popup_width = 40;
    let popup_height = 16;
    let popup_area = centered_rect(popup_width, popup_height, area);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Render the popup
    let title = format!(
        " Projects ({}) ",
        app.ui.projects_modal.projects_dir.display()
    );
    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Render project list
    render_project_list(frame, inner, app);
}

/// Render the list of projects
fn render_project_list(frame: &mut Frame, area: Rect, app: &App) {
    let modal = &app.ui.projects_modal;
    let mut lines: Vec<Line> = Vec::new();

    if modal.projects.is_empty() {
        lines.push(Line::from(Span::styled(
            "No projects found",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Run termdaw to create a new project",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        // Calculate visible range based on selection and area height
        let visible_height = (area.height as usize).saturating_sub(2); // Leave room for footer
        let start = if modal.selected >= visible_height {
            modal.selected - visible_height + 1
        } else {
            0
        };
        let end = (start + visible_height).min(modal.projects.len());

        for (i, project) in modal
            .projects
            .iter()
            .enumerate()
            .skip(start)
            .take(end - start)
        {
            let is_selected = i == modal.selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if is_selected { "> " } else { "  " };
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, project),
                style,
            )));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
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
