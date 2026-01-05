//! Mixer panel with volume faders

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::areas::AreaId;
use crate::app::{App, Panel};

/// Render the mixer panel
pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.mode.current_panel() == Panel::Mixer;

    // Custom panel frame to add close button
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = "Mixer";

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Add close button in top-right corner (inside border)
    let close_x = area.x + area.width - 4; // Position inside the right border
    let close_rect = Rect::new(close_x, area.y, 3, 1);
    app.screen_areas.register(AreaId::MixerClose, close_rect);

    let close_style = Style::default()
        .fg(Color::Red)
        .add_modifier(Modifier::BOLD);
    let close_btn = Paragraph::new(Line::from(Span::styled(" × ", close_style)));
    frame.render_widget(close_btn, close_rect);

    // Clone channel data to avoid borrow conflicts
    let channels_data: Vec<_> = app
        .channels
        .iter()
        .map(|c| (c.name.clone(), c.volume, c.muted, c.solo))
        .collect();
    let selected_channel = app.mixer.selected_channel;
    let channel_width = 8u16;

    for (i, (name, volume, muted, solo)) in channels_data.iter().enumerate() {
        let x = inner.x + (i as u16 * channel_width);
        if x + channel_width > inner.x + inner.width {
            break;
        }

        let is_selected = i == selected_channel && focused;

        // Channel name
        let name_style = if is_selected {
            Style::default().fg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::White)
        };
        let name_display = Paragraph::new(format!("{:^7}", &name[..name.len().min(7)]))
            .style(name_style);
        frame.render_widget(name_display, Rect::new(x, inner.y, channel_width - 1, 1));

        // Volume fader (vertical bar)
        let fader_height = inner.height.saturating_sub(4);
        let filled_height = ((volume * fader_height as f32) as u16).min(fader_height);

        // Register fader area
        let fader_rect = Rect::new(x + 2, inner.y + 2, 3, fader_height);
        app.screen_areas.mixer_faders.insert(i, fader_rect);

        for row in 0..fader_height {
            let y = inner.y + 2 + row;
            let is_filled = row >= (fader_height - filled_height);
            let ch = if is_filled { "█" } else { "░" };
            let style = if is_selected {
                Style::default().fg(Color::Cyan)
            } else if *muted {
                Style::default().fg(Color::Red)
            } else if *solo {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Green)
            };
            let fader = Paragraph::new(format!(" {} ", ch)).style(style);
            frame.render_widget(fader, Rect::new(x + 2, y, 3, 1));
        }

        // Volume percentage
        let vol_text = format!("{:3}%", (volume * 100.0) as i32);
        let vol_style = if is_selected {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let vol = Paragraph::new(vol_text).style(vol_style);
        frame.render_widget(vol, Rect::new(x + 1, inner.y + inner.height - 2, 5, 1));

        // Mute/Solo indicators
        let mute_style = if *muted {
            Style::default().fg(Color::Red).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let solo_style = if *solo {
            Style::default().fg(Color::Yellow).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Register mute button area
        let mute_rect = Rect::new(x + 1, inner.y + inner.height - 1, 1, 1);
        app.screen_areas.mixer_mute_buttons.insert(i, mute_rect);

        // Register solo button area
        let solo_rect = Rect::new(x + 4, inner.y + inner.height - 1, 1, 1);
        app.screen_areas.mixer_solo_buttons.insert(i, solo_rect);

        let mute_widget = Paragraph::new("M").style(mute_style);
        let solo_widget = Paragraph::new("S").style(solo_style);
        frame.render_widget(mute_widget, mute_rect);
        frame.render_widget(solo_widget, solo_rect);
    }
}
