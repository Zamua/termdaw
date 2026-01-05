//! Effect picker and editor modal UI

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::effects::{get_param_defs, EffectType};
use crate::mode::AppMode;

/// Render effect-related modals
pub fn render(frame: &mut Frame, app: &App) {
    match &app.mode {
        AppMode::EffectPicker { .. } => render_effect_picker(frame, app),
        AppMode::EffectEditor {
            track_idx,
            slot_idx,
            selected_param,
            ..
        } => {
            render_effect_editor(frame, app, *track_idx, *slot_idx, *selected_param);
        }
        _ => {}
    }
}

/// Render the effect type picker modal
fn render_effect_picker(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Center the modal
    let modal_width = 30;
    let modal_height = 8;
    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    // Clear the area behind the modal
    frame.render_widget(Clear, modal_area);

    // Modal border
    let block = Block::default()
        .title("Add Effect")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    // Get effect picker selection (stored in app state)
    let selected = app.effect_picker_selection;

    // Effect types list
    let effect_types = EffectType::all();
    for (i, effect_type) in effect_types.iter().enumerate() {
        let style = if i == selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let selector = if i == selected { ">" } else { " " };
        let text = format!("{} {}", selector, effect_type.name());
        let para = Paragraph::new(text).style(style);
        frame.render_widget(
            para,
            Rect::new(inner.x + 1, inner.y + i as u16, inner.width - 2, 1),
        );
    }

    // Help text
    let help = Paragraph::new("j/k: select  Enter: add  Esc: cancel")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(
        help,
        Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
    );
}

/// Render the effect editor modal
fn render_effect_editor(
    frame: &mut Frame,
    app: &App,
    track_idx: usize,
    slot_idx: usize,
    selected_param: usize,
) {
    let area = frame.area();

    // Get the effect slot
    let effect_slot = match &app.mixer.tracks[track_idx].effects[slot_idx] {
        Some(slot) => slot,
        None => return,
    };

    let param_defs = get_param_defs(effect_slot.effect_type);

    // Modal size based on number of parameters
    let modal_width = 40;
    let modal_height = (param_defs.len() + 4).min(15) as u16;
    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    // Clear the area behind the modal
    frame.render_widget(Clear, modal_area);

    // Modal border with effect name
    let title = format!("{}", effect_slot.effect_type.name());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    // Render each parameter
    for (i, def) in param_defs.iter().enumerate() {
        let row_y = inner.y + i as u16;
        if row_y >= inner.y + inner.height - 1 {
            break;
        }

        let is_selected = i == selected_param;
        let value = effect_slot.get_param(def.id);
        let normalized = def.normalize(value);

        // Parameter name
        let name_style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let selector = if is_selected { ">" } else { " " };

        // Value bar (10 chars wide)
        let bar_width = 10;
        let filled = (normalized * bar_width as f32) as usize;
        let bar: String = (0..bar_width)
            .map(|i| if i < filled { '█' } else { '░' })
            .collect();

        // Value display
        let value_str = def.format_value(value);

        let line = Line::from(vec![
            Span::styled(selector, Style::default().fg(Color::Cyan)),
            Span::styled(format!("{:12}", def.id.name()), name_style),
            Span::styled(
                bar,
                Style::default().fg(if is_selected {
                    Color::Cyan
                } else {
                    Color::DarkGray
                }),
            ),
            Span::styled(format!(" {:>8}", value_str), name_style),
        ]);

        let para = Paragraph::new(line);
        frame.render_widget(para, Rect::new(inner.x, row_y, inner.width, 1));
    }

    // Help text
    let help = Paragraph::new("j/k: param  h/l: adjust  Esc: close")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(
        help,
        Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
    );
}
