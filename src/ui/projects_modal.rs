//! Projects modal UI - list and open projects interactively

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::projects_modal::ModalMode;

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

    // Render based on mode
    match &app.ui.projects_modal.mode {
        ModalMode::Browse => render_browse_mode(frame, popup_area, app),
        ModalMode::TextInput { prompt, input, .. } => {
            let prompt = *prompt;
            let value = input.value().to_string();
            let cursor = input.visual_cursor();
            render_text_input_mode(frame, popup_area, app, prompt, &value, cursor);
        }
        ModalMode::Confirm { message, .. } => {
            let message = message.clone();
            render_confirm_mode(frame, popup_area, &message);
        }
    }
}

/// Render browse mode - project list
fn render_browse_mode(frame: &mut Frame, popup_area: Rect, app: &App) {
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

    render_project_list(frame, inner, app);
}

/// Render text input mode
fn render_text_input_mode(
    frame: &mut Frame,
    popup_area: Rect,
    app: &App,
    prompt: &str,
    value: &str,
    cursor: usize,
) {
    let block = Block::default()
        .title(" Projects ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Show project list in background (dimmed)
    render_project_list_dimmed(frame, inner, app);

    // Render input overlay at bottom of inner area
    let input_height = 3;
    let input_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(input_height),
        width: inner.width,
        height: input_height,
    };

    frame.render_widget(Clear, input_area);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let input_inner = input_block.inner(input_area);
    frame.render_widget(input_block, input_area);

    // Render prompt and input value
    let display_value = if value.is_empty() {
        String::new()
    } else {
        value.to_string()
    };

    let text = format!("{} {}", prompt, display_value);
    let paragraph = Paragraph::new(text).style(Style::default().fg(Color::White));
    frame.render_widget(paragraph, input_inner);

    // Position cursor
    let cursor_x = input_inner.x + prompt.len() as u16 + 1 + cursor as u16;
    let cursor_y = input_inner.y;
    frame.set_cursor_position((cursor_x, cursor_y));
}

/// Render confirmation mode
fn render_confirm_mode(frame: &mut Frame, popup_area: Rect, message: &str) {
    let block = Block::default()
        .title(" Confirm ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Center the confirmation dialog
    let dialog_height = 5;
    let dialog_y = inner.y + (inner.height.saturating_sub(dialog_height)) / 2;

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            message,
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

    let dialog_area = Rect {
        x: inner.x,
        y: dialog_y,
        width: inner.width,
        height: dialog_height,
    };

    frame.render_widget(paragraph, dialog_area);
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
            "Press 'n' to create a new project",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        // Calculate visible range based on selection and area height
        let visible_height = (area.height as usize).saturating_sub(2);
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
            let is_current = project == &app.state.project.name;

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if is_current {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if is_selected { "> " } else { "  " };
            let suffix = if is_current { " *" } else { "" };
            lines.push(Line::from(Span::styled(
                format!("{}{}{}", prefix, project, suffix),
                style,
            )));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Render the project list dimmed (for text input overlay)
fn render_project_list_dimmed(frame: &mut Frame, area: Rect, app: &App) {
    let modal = &app.ui.projects_modal;
    let mut lines: Vec<Line> = Vec::new();

    let visible_height = (area.height as usize).saturating_sub(5); // Leave room for input
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
        let prefix = if is_selected { "> " } else { "  " };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, project),
            Style::default().fg(Color::DarkGray),
        )));
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
