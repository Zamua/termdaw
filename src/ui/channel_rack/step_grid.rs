//! Step grid component for channel rack
//!
//! Renders the 16-step sequencer grid for all channels.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::input::vim::Position;
use crate::mixer::TrackId;
use crate::ui::colors::{self, ColGroup};

use super::{render_header, HEADER_ROWS, MUTE_WIDTH, SAMPLE_WIDTH, STEP_WIDTH, TRACK_WIDTH};

/// Total number of channel slots
const TOTAL_CHANNEL_SLOTS: usize = 99;

/// Render the step grid (normal channel rack mode)
pub fn render(frame: &mut Frame, inner: Rect, app: &mut App, focused: bool) {
    // Render header rows
    render_header(frame, inner, app, false);

    // Calculate visible rows
    let visible_rows = (inner.height - HEADER_ROWS) as usize;
    let viewport_top = app.channel_rack.viewport_top;

    // Get current visual selection (if any)
    let vim_col: crate::coords::VimCol = app.channel_rack.col.into();
    let cursor = Position::new(app.channel_rack.channel, vim_col.0);
    let selection = app.vim.channel_rack.get_selection(cursor);

    // Render all 99 channel slots (within viewport)
    for row_idx in 0..visible_rows {
        let channel_idx = viewport_top + row_idx;
        if channel_idx >= TOTAL_CHANNEL_SLOTS {
            break;
        }

        let y = inner.y + HEADER_ROWS + row_idx as u16;
        let mut x = inner.x;

        // Check if this slot has an allocated channel (lookup by slot, not Vec index)
        // Extract all needed data upfront to avoid borrow conflicts
        let channel_data = app.get_channel_at_slot(channel_idx).map(|c| {
            (
                c.mixer_track,
                c.name.clone(),
                c.is_plugin(),
                c.sample_path().map(|s| s.to_string()),
            )
        });
        let is_allocated = channel_data.is_some();

        // Get mute/solo state from the mixer track this channel routes to
        let track_id = channel_data
            .as_ref()
            .map(|(mt, _, _, _)| TrackId(*mt))
            .unwrap_or(TrackId(1));
        let mixer_track = app.mixer.track(track_id);

        // === MUTE ZONE (col -2) - now comes first ===
        let mute_rect = Rect::new(x, y, MUTE_WIDTH, 1);
        app.screen_areas
            .channel_rack_cells
            .insert((channel_idx, 0), mute_rect);

        let is_mute_cursor = channel_idx == app.channel_rack.channel
            && app.channel_rack.col.is_mute_zone()
            && focused;
        let (mute_char, mute_color) = if is_allocated {
            if mixer_track.solo {
                ("S", Color::Yellow)
            } else if mixer_track.muted {
                ("M", Color::Red)
            } else {
                ("○", Color::Green)
            }
        } else {
            ("·", Color::DarkGray)
        };
        let mute_style = if is_mute_cursor {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(mute_color)
        };
        let mute_widget = Paragraph::new(format!(
            "{:<width$}",
            mute_char,
            width = MUTE_WIDTH as usize
        ))
        .style(mute_style);
        frame.render_widget(mute_widget, mute_rect);
        x += MUTE_WIDTH;

        // === TRACK ZONE (col -2, VimCol 1) ===
        let track_rect = Rect::new(x, y, TRACK_WIDTH, 1);
        app.screen_areas
            .channel_rack_cells
            .insert((channel_idx, 1), track_rect);

        let is_track_cursor = channel_idx == app.channel_rack.channel
            && app.channel_rack.col.is_track_zone()
            && focused;
        let track_num = track_id.index();
        let track_text = if is_allocated {
            format!("{:<width$}", track_num, width = TRACK_WIDTH as usize)
        } else {
            format!("{:<width$}", "·", width = TRACK_WIDTH as usize)
        };
        let track_style = if is_track_cursor {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if is_allocated {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let track_widget = Paragraph::new(track_text).style(track_style);
        frame.render_widget(track_widget, track_rect);
        x += TRACK_WIDTH;

        // === SAMPLE ZONE (col -1, VimCol 2) ===
        let sample_rect = Rect::new(x, y, SAMPLE_WIDTH, 1);
        app.screen_areas
            .channel_rack_cells
            .insert((channel_idx, 2), sample_rect);

        let is_sample_cursor = channel_idx == app.channel_rack.channel
            && app.channel_rack.col.is_sample_zone()
            && focused;
        let sample_style = if is_sample_cursor {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if channel_idx == app.channel_rack.channel && focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if !is_allocated {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        // Display channel name or slot indicator
        // channel_data is (mixer_track, name, is_plugin, sample_path)
        let name_display = if let Some((_, ref name, is_plugin, ref sample_path)) = channel_data {
            if is_plugin || sample_path.is_some() {
                format!(
                    "{:<width$}",
                    &name[..name.len().min(SAMPLE_WIDTH as usize - 1)],
                    width = SAMPLE_WIDTH as usize - 1
                )
            } else {
                format!(
                    "{:<width$}",
                    format!("Slot {}", channel_idx + 1),
                    width = SAMPLE_WIDTH as usize - 1
                )
            }
        } else {
            format!(
                "{:<width$}",
                format!("Slot {}", channel_idx + 1),
                width = SAMPLE_WIDTH as usize - 1
            )
        };
        let sample_widget = Paragraph::new(name_display).style(sample_style);
        frame.render_widget(sample_widget, sample_rect);
        x += SAMPLE_WIDTH;

        // === STEPS ZONE (col 0-15) ===
        let pattern_id = app.current_pattern;
        let steps_data: Vec<bool> = app
            .get_channel_at_slot(channel_idx)
            .and_then(|c| c.get_pattern(pattern_id))
            .map(|s| s.steps.clone())
            .unwrap_or_else(|| vec![false; 16]);
        let is_playing = app.is_playing();
        let playhead = app.playhead_step();
        let cursor_channel = app.channel_rack.channel;
        let cursor_col = app.channel_rack.col.0;

        for step in 0..16 {
            if x + STEP_WIDTH > inner.x + inner.width {
                break;
            }

            let step_idx = step as usize;
            let vim_col_for_step = step_idx + 3; // VimCol 3-18 for steps 0-15
            let pos = Position::new(channel_idx, vim_col_for_step);

            let step_rect = Rect::new(x, y, STEP_WIDTH, 1);
            app.screen_areas
                .channel_rack_cells
                .insert((channel_idx, vim_col_for_step), step_rect);

            let is_cursor = channel_idx == cursor_channel && cursor_col == step && focused;
            let is_selected = selection.map(|r| r.contains(pos)).unwrap_or(false);
            let is_beat = step % 4 == 0;
            let is_active = *steps_data.get(step_idx).unwrap_or(&false);
            let is_playhead = is_playing && step_idx == playhead;

            let sep = if is_beat { "┃" } else { "│" };
            let sep_color = Color::DarkGray;

            // Get column group for alternating colors
            let col_group = ColGroup::from_step(step_idx);

            // Determine cell state and get style from colors module
            let cell_state =
                colors::determine_cell_state(is_cursor, is_selected, is_playhead, is_active);

            let cell_style = colors::cell_style(cell_state, col_group);
            let cell = if is_active {
                colors::chars::FILLED_2
            } else {
                colors::chars::EMPTY_2
            };

            let step_line = Line::from(vec![
                Span::styled(sep, Style::default().fg(sep_color)),
                Span::styled(cell, cell_style),
            ]);
            let step_widget = Paragraph::new(step_line);
            frame.render_widget(step_widget, step_rect);
            x += STEP_WIDTH;
        }
    }
}
