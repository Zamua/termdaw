//! Mixer panel with volume faders

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, Panel};

/// Render the mixer panel
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.mode.current_panel() == Panel::Mixer;
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = if focused { "Mixer *" } else { "Mixer" };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let channels = &app.channels;
    let channel_width = 8u16;

    for (i, channel) in channels.iter().enumerate() {
        let x = inner.x + (i as u16 * channel_width);
        if x + channel_width > inner.x + inner.width {
            break;
        }

        let is_selected = i == app.mixer_selected_channel && focused;

        // Channel name
        let name_style = if is_selected {
            Style::default().fg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::White)
        };
        let name = Paragraph::new(format!("{:^7}", &channel.name[..channel.name.len().min(7)]))
            .style(name_style);
        frame.render_widget(name, Rect::new(x, inner.y, channel_width - 1, 1));

        // Volume fader (vertical bar)
        let fader_height = inner.height.saturating_sub(4);
        let filled_height = ((channel.volume * fader_height as f32) as u16).min(fader_height);

        for row in 0..fader_height {
            let y = inner.y + 2 + row;
            let is_filled = row >= (fader_height - filled_height);
            let ch = if is_filled { "█" } else { "░" };
            let style = if is_selected {
                Style::default().fg(Color::Cyan)
            } else if channel.muted {
                Style::default().fg(Color::Red)
            } else if channel.solo {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Green)
            };
            let fader = Paragraph::new(format!(" {} ", ch)).style(style);
            frame.render_widget(fader, Rect::new(x + 2, y, 3, 1));
        }

        // Volume percentage
        let vol_text = format!("{:3}%", (channel.volume * 100.0) as i32);
        let vol_style = if is_selected {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let vol = Paragraph::new(vol_text).style(vol_style);
        frame.render_widget(vol, Rect::new(x + 1, inner.y + inner.height - 2, 5, 1));

        // Mute/Solo indicators
        let mute_style = if channel.muted {
            Style::default().fg(Color::Red).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let solo_style = if channel.solo {
            Style::default().fg(Color::Yellow).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let mute = Paragraph::new("M").style(mute_style);
        let solo = Paragraph::new("S").style(solo_style);
        frame.render_widget(mute, Rect::new(x + 1, inner.y + inner.height - 1, 1, 1));
        frame.render_widget(solo, Rect::new(x + 4, inner.y + inner.height - 1, 1, 1));
    }
}
