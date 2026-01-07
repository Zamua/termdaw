//! Mixer panel with stereo meters - FL Studio style
//!
//! Layout:
//! - Left: Master track (always visible, fixed)
//! - Center: Scrollable track list with stereo L/R meters
//! - Right: Effects panel for selected track (when focused)
//!
//! Each track shows:
//! - Name
//! - Route indicator
//! - Stereo L/R peak meters
//! - Volume percentage
//! - Mute/Solo indicators

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::areas::AreaId;
use crate::app::{App, Panel};
use crate::effects::EffectType;
use crate::mixer::{RouteDestination, TrackId, NUM_TRACKS};
use crate::ui::render_panel_frame;

/// Width of each track column (including separator)
const TRACK_WIDTH: u16 = 10;
/// Width of the effects panel
const EFFECTS_PANEL_WIDTH: u16 = 24;
/// Number of effect slots
const EFFECT_SLOTS: usize = 8;
/// Item indices for selection
const PAN_ITEM: usize = EFFECT_SLOTS; // 8
const VOLUME_ITEM: usize = EFFECT_SLOTS + 1; // 9

/// Render the mixer panel
pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.mode.current_panel() == Panel::Mixer;

    let inner = render_panel_frame(frame, area, "Mixer", Panel::Mixer, app);

    // Add close button in top-right corner (inside border)
    let close_x = area.x + area.width - 4;
    let close_rect = Rect::new(close_x, area.y, 3, 1);
    app.screen_areas.register(AreaId::MixerClose, close_rect);

    let close_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
    let close_btn = Paragraph::new(Line::from(Span::styled(" × ", close_style)));
    frame.render_widget(close_btn, close_rect);

    if inner.width < TRACK_WIDTH * 2 || inner.height < 6 {
        return; // Not enough space
    }

    // Calculate available width for tracks (reserve space for effects panel when effects are focused)
    let effects_width = if focused && app.mixer.effects_focused {
        EFFECTS_PANEL_WIDTH
    } else {
        0
    };
    let tracks_area_width = inner.width.saturating_sub(effects_width);

    // Always show Master first, then scrollable tracks
    let mut x = inner.x;

    // Render Master track (always visible)
    render_track(frame, x, inner.y, inner.height, app, 0, focused);
    x += TRACK_WIDTH;

    // Calculate how many tracks can fit after Master
    let remaining_width = tracks_area_width.saturating_sub(TRACK_WIDTH);
    let visible_tracks = (remaining_width / TRACK_WIDTH) as usize;

    // Render visible tracks (with scrolling)
    let viewport_start = app.mixer.viewport_offset + 1; // Skip master
    for i in 0..visible_tracks {
        let track_idx = viewport_start + i;
        if track_idx >= NUM_TRACKS {
            break;
        }
        if x + TRACK_WIDTH > inner.x + tracks_area_width {
            break;
        }
        render_track(frame, x, inner.y, inner.height, app, track_idx, focused);
        x += TRACK_WIDTH;
    }

    // Render effects panel on the right when effects are focused
    if focused && app.mixer.effects_focused && effects_width > 0 {
        let effects_x = inner.x + inner.width - effects_width;
        render_effects_panel(frame, effects_x, inner.y, effects_width, inner.height, app);
    }
}

/// Render a single mixer track with stereo meters
fn render_track(
    frame: &mut Frame,
    x: u16,
    y: u16,
    height: u16,
    app: &App,
    track_idx: usize,
    focused: bool,
) {
    let track = &app.mixer.tracks[track_idx];
    let is_selected = track_idx == app.mixer.selected_track && focused;
    let is_master = track_idx == 0;

    // Track name (row 0)
    let name_style = if is_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if is_master {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };
    let display_name = if track.name.is_empty() {
        if is_master {
            "Master".to_string()
        } else {
            format!("Track {}", track_idx)
        }
    } else {
        track.name[..track.name.len().min(TRACK_WIDTH as usize - 1)].to_string()
    };
    let name = Paragraph::new(format!(
        "{:^width$}",
        display_name,
        width = TRACK_WIDTH as usize - 1
    ))
    .style(name_style);
    frame.render_widget(name, Rect::new(x, y, TRACK_WIDTH - 1, 1));

    // Route indicator (row 1)
    let route_text = match app.mixer.routing.get_route(TrackId(track_idx)) {
        RouteDestination::Master if is_master => "OUT".to_string(),
        RouteDestination::Master => "→M".to_string(),
        RouteDestination::Track(t) => format!("→{}", t.index()),
    };
    let route_style = Style::default().fg(Color::DarkGray);
    let route = Paragraph::new(format!(
        "{:^width$}",
        route_text,
        width = TRACK_WIDTH as usize - 1
    ))
    .style(route_style);
    frame.render_widget(route, Rect::new(x, y + 1, TRACK_WIDTH - 1, 1));

    // Stereo meters (rows 2 to height-3)
    let meter_height = height.saturating_sub(5);
    let levels = app.mixer.peak_levels[track_idx];

    // Calculate filled heights based on peak levels
    // Use logarithmic scaling for better visibility of low-level audio
    // Map 0.001 (-60dB) to 0.1 and 1.0 (0dB) to 1.0
    fn log_scale(level: f32) -> f32 {
        if level < 0.001 {
            0.0
        } else {
            // Map log scale: -60dB (0.001) -> 0.1, 0dB (1.0) -> 1.0
            let db = 20.0 * level.log10(); // -60 to 0
            ((db + 60.0) / 60.0).clamp(0.0, 1.0)
        }
    }
    let left_filled = ((log_scale(levels.left) * meter_height as f32) as u16).min(meter_height);
    let right_filled = ((log_scale(levels.right) * meter_height as f32) as u16).min(meter_height);

    // Also show volume fader position
    let fader_height = ((track.volume * meter_height as f32) as u16).min(meter_height);

    for row in 0..meter_height {
        let row_y = y + 2 + row;
        let from_bottom = meter_height - row - 1;

        // Left meter (column 0-1)
        let left_active = from_bottom < left_filled;
        let left_fader = from_bottom < fader_height;

        // Right meter (column 2-3)
        let right_active = from_bottom < right_filled;
        let right_fader = from_bottom < fader_height;

        // Choose meter characters
        // Active meter (peak level): solid
        // Fader position: dim
        // Empty: very dim
        let (left_char, left_color) = if left_active {
            if is_master {
                ("█", Color::Yellow)
            } else {
                ("█", Color::Green)
            }
        } else if left_fader {
            (
                "▓",
                if is_selected {
                    Color::Cyan
                } else {
                    Color::DarkGray
                },
            )
        } else {
            ("░", Color::DarkGray)
        };

        let (right_char, right_color) = if right_active {
            if is_master {
                ("█", Color::Yellow)
            } else {
                ("█", Color::Green)
            }
        } else if right_fader {
            (
                "▓",
                if is_selected {
                    Color::Cyan
                } else {
                    Color::DarkGray
                },
            )
        } else {
            ("░", Color::DarkGray)
        };

        // Apply mute/solo colors
        let left_style = if track.muted {
            Style::default().fg(Color::Red)
        } else if track.solo {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(left_color)
        };

        let right_style = if track.muted {
            Style::default().fg(Color::Red)
        } else if track.solo {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(right_color)
        };

        // Render L R meters centered
        // With TRACK_WIDTH=10 (9 usable), center "L R" (3 chars) with padding
        let padding = (TRACK_WIDTH as usize - 1 - 3) / 2; // 3 chars on each side
        let meter_line = Line::from(vec![
            Span::raw(" ".repeat(padding)),
            Span::styled(left_char, left_style),
            Span::raw(" "),
            Span::styled(right_char, right_style),
            Span::raw(" ".repeat(padding)),
        ]);
        let meter_widget = Paragraph::new(meter_line);
        frame.render_widget(meter_widget, Rect::new(x, row_y, TRACK_WIDTH - 1, 1));
    }

    // Volume percentage (row height-3)
    let vol_text = format!("{:3}%", (track.volume * 100.0) as i32);
    let vol_style = if is_selected {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let vol = Paragraph::new(format!(
        "{:^width$}",
        vol_text,
        width = TRACK_WIDTH as usize - 1
    ))
    .style(vol_style);
    frame.render_widget(vol, Rect::new(x, y + height - 3, TRACK_WIDTH - 1, 1));

    // Pan indicator (row height-2) - show as position on line
    let pan_pos = ((track.pan + 1.0) / 2.0 * 4.0) as usize; // 0-4 position
    let pan_chars: Vec<char> = (0..5)
        .map(|i| if i == pan_pos { '●' } else { '─' })
        .collect();
    let pan_text: String = pan_chars.iter().collect();
    let pan_style = if is_selected {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let pan = Paragraph::new(format!(
        "{:^width$}",
        pan_text,
        width = TRACK_WIDTH as usize - 1
    ))
    .style(pan_style);
    frame.render_widget(pan, Rect::new(x, y + height - 2, TRACK_WIDTH - 1, 1));

    // Mute/Solo indicators (row height-1) - skip for Master
    if !is_master {
        let mute_style = if track.muted {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let solo_style = if track.solo {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        // Center "M   S" (5 chars) within track width
        let ms_padding = (TRACK_WIDTH as usize - 1 - 5) / 2;
        let ms_line = Line::from(vec![
            Span::raw(" ".repeat(ms_padding)),
            Span::styled("M", mute_style),
            Span::raw("   "),
            Span::styled("S", solo_style),
            Span::raw(" ".repeat(ms_padding)),
        ]);
        let ms_widget = Paragraph::new(ms_line);
        frame.render_widget(ms_widget, Rect::new(x, y + height - 1, TRACK_WIDTH - 1, 1));
    }

    // Draw separator line after track (except last visible)
    let sep_style = Style::default().fg(Color::DarkGray);
    for row in 0..height {
        let sep = Paragraph::new("│").style(sep_style);
        frame.render_widget(sep, Rect::new(x + TRACK_WIDTH - 1, y + row, 1, 1));
    }
}

/// Render the effects panel on the right side
fn render_effects_panel(frame: &mut Frame, x: u16, y: u16, width: u16, height: u16, app: &App) {
    let selected = app.mixer.selected_track;
    let track = &app.mixer.tracks[selected];

    // Panel border (left edge)
    let border_style = Style::default().fg(Color::Cyan);
    for row in 0..height {
        let border = Paragraph::new("│").style(border_style);
        frame.render_widget(border, Rect::new(x, y + row, 1, 1));
    }

    let content_x = x + 2;
    let content_width = width - 3;

    // Track name header
    let name = if track.name.is_empty() {
        if selected == 0 {
            "Master".to_string()
        } else {
            format!("Track {}", selected)
        }
    } else {
        track.name.clone()
    };
    let header = Paragraph::new(format!("┤ {} ├", name)).style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(header, Rect::new(content_x, y, content_width, 1));

    // Subheader
    let subheader = Paragraph::new("Effects Chain").style(Style::default().fg(Color::DarkGray));
    frame.render_widget(subheader, Rect::new(content_x, y + 1, content_width, 1));

    // Separator
    let sep = Paragraph::new("─".repeat(content_width as usize))
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(sep, Rect::new(content_x, y + 2, content_width, 1));

    // Effect slots with scrolling
    let selected_slot = app.mixer.selected_effect_slot;
    let on_bypass_column = app.mixer.on_bypass_column;

    // Calculate how many effect slots can be visible (leave room for separator, pan, volume)
    let effects_area_height = height.saturating_sub(6) as usize; // header(1) + subheader(1) + sep(1) + sep(1) + pan(1) + vol(1)
    let visible_slots = effects_area_height.min(EFFECT_SLOTS);

    // Calculate scroll offset to keep selected slot visible (only for effect slots, not pan/volume)
    let scroll_offset = if selected_slot < EFFECT_SLOTS {
        if selected_slot >= visible_slots {
            (selected_slot - visible_slots + 1).min(EFFECT_SLOTS - visible_slots)
        } else {
            0
        }
    } else {
        // When pan/volume selected, show last effects
        EFFECT_SLOTS.saturating_sub(visible_slots)
    };

    for display_idx in 0..visible_slots {
        let i = scroll_offset + display_idx;
        if i >= EFFECT_SLOTS {
            break;
        }

        let slot_y = y + 3 + display_idx as u16;
        let is_selected = i == selected_slot;
        let effect_slot = &track.effects[i];

        // Build slot display
        let (effect_name, bypassed, has_effect) = match effect_slot {
            Some(slot) => {
                let name = match slot.effect_type {
                    EffectType::Filter => {
                        let mode = slot.get_param(crate::effects::EffectParamId::FilterMode) as u32;
                        match mode {
                            0 => "Filter LP",
                            1 => "Filter HP",
                            _ => "Filter BP",
                        }
                    }
                    EffectType::Delay => {
                        let time = slot.get_param(crate::effects::EffectParamId::DelayTime) as u32;
                        match time {
                            0 => "Dly 1/32",
                            1 => "Dly 1/16",
                            2 => "Dly 1/8",
                            3 => "Dly 1/4",
                            4 => "Dly 1/2",
                            5 => "Dly 1",
                            6 => "Dly 2",
                            _ => "Dly 4",
                        }
                    }
                };
                (name, slot.bypassed, true)
            }
            None => ("·", false, false),
        };

        // Determine if bypass column or effect column is selected
        let bypass_col_selected = is_selected && on_bypass_column;
        let effect_col_selected = is_selected && !on_bypass_column;

        // Style for effect name
        let name_style = if effect_col_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if has_effect && !bypassed {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Bypass indicator: green ● = active, grey ○ = bypassed, · = empty
        // Highlight with cyan when bypass column is selected
        let (bypass_char, bypass_color) = if has_effect {
            if bypass_col_selected {
                if bypassed {
                    ("○", Color::Cyan)
                } else {
                    ("●", Color::Cyan)
                }
            } else if bypassed {
                ("○", Color::DarkGray)
            } else {
                ("●", Color::Green)
            }
        } else if bypass_col_selected {
            ("·", Color::Cyan)
        } else {
            ("·", Color::DarkGray)
        };

        let line = Line::from(vec![
            Span::styled(bypass_char, Style::default().fg(bypass_color)),
            Span::styled("  ", Style::default()), // spacing between columns
            Span::styled(format!("{}", i + 1), Style::default().fg(Color::DarkGray)),
            Span::styled(" ", Style::default()),
            Span::styled(effect_name, name_style),
        ]);
        let slot_widget = Paragraph::new(line);
        frame.render_widget(slot_widget, Rect::new(content_x, slot_y, content_width, 1));
    }

    // Scroll indicators
    let can_scroll_up = scroll_offset > 0;
    let can_scroll_down = scroll_offset + visible_slots < EFFECT_SLOTS;

    // Show scroll-up indicator if needed (replace the separator line after header)
    if can_scroll_up {
        let scroll_up =
            Paragraph::new("  ▲ more above").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(scroll_up, Rect::new(content_x, y + 2, content_width, 1));
    }

    // Separator before pan/volume (with scroll-down indicator if needed)
    let sep_y = y + height - 3;
    let sep_text = if can_scroll_down {
        "─▼ more──────────────".to_string()
    } else {
        "─".repeat(content_width as usize)
    };
    let sep = Paragraph::new(sep_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(sep, Rect::new(content_x, sep_y, content_width, 1));

    // Pan control
    let pan_y = y + height - 2;
    let pan_selected = selected_slot == PAN_ITEM;
    let pan_selector = if pan_selected { ">" } else { " " };
    let pan_style = if pan_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    // Pan bar: center marker with position indicator
    let pan = track.pan;
    let bar_width = 11;
    let pan_pos = ((pan + 1.0) / 2.0 * (bar_width - 1) as f32) as usize;
    let bar_color = if pan_selected {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let mut pan_chars: Vec<Span> = Vec::new();
    pan_chars.push(Span::styled(pan_selector, Style::default().fg(Color::Cyan)));
    pan_chars.push(Span::styled("Pan ", pan_style));
    pan_chars.push(Span::styled("[", Style::default().fg(bar_color)));
    for i in 0..bar_width {
        if i == pan_pos {
            pan_chars.push(Span::styled(
                "●",
                Style::default().fg(if pan_selected {
                    Color::Cyan
                } else {
                    Color::White
                }),
            ));
        } else if i == bar_width / 2 {
            pan_chars.push(Span::styled("│", Style::default().fg(bar_color)));
        } else {
            pan_chars.push(Span::styled("─", Style::default().fg(bar_color)));
        }
    }
    pan_chars.push(Span::styled("]", Style::default().fg(bar_color)));
    let pan_widget = Paragraph::new(Line::from(pan_chars));
    frame.render_widget(pan_widget, Rect::new(content_x, pan_y, content_width, 1));

    // Volume control
    let vol_y = y + height - 1;
    let vol_selected = selected_slot == VOLUME_ITEM;
    let vol_selector = if vol_selected { ">" } else { " " };
    let vol_style = if vol_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    // Volume bar: filled from left
    let volume = track.volume;
    let vol_filled = (volume * bar_width as f32) as usize;
    let vol_bar_color = if vol_selected {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let mut vol_chars: Vec<Span> = Vec::new();
    vol_chars.push(Span::styled(vol_selector, Style::default().fg(Color::Cyan)));
    vol_chars.push(Span::styled("Vol ", vol_style));
    vol_chars.push(Span::styled("[", Style::default().fg(vol_bar_color)));
    for i in 0..bar_width {
        if i < vol_filled {
            vol_chars.push(Span::styled(
                "█",
                Style::default().fg(if vol_selected {
                    Color::Cyan
                } else {
                    Color::Green
                }),
            ));
        } else {
            vol_chars.push(Span::styled("░", Style::default().fg(vol_bar_color)));
        }
    }
    vol_chars.push(Span::styled("]", Style::default().fg(vol_bar_color)));
    // Add percentage
    vol_chars.push(Span::styled(
        format!(" {:3}%", (volume * 100.0) as i32),
        vol_style,
    ));
    let vol_widget = Paragraph::new(Line::from(vol_chars));
    frame.render_widget(vol_widget, Rect::new(content_x, vol_y, content_width, 1));
}
