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
use crate::ui::areas::ScreenAreas;
use crate::ui::colors::{self, ColGroup};

use super::view_model::{ChannelRackViewModel, ChannelRowView};
use super::{render_header, HEADER_ROWS, MUTE_WIDTH, SAMPLE_WIDTH, STEP_WIDTH, TRACK_WIDTH};

// Note: TOTAL_CHANNEL_SLOTS is now in view_model.rs

/// Render the step grid (normal channel rack mode)
pub fn render(frame: &mut Frame, inner: Rect, app: &mut App, focused: bool) {
    // Render header rows (still uses app directly - shared with piano roll)
    render_header(frame, inner, app, false);

    // Calculate visible rows and build ViewModel
    let visible_rows = (inner.height - HEADER_ROWS) as usize;
    let view_model = ChannelRackViewModel::from_app(app, visible_rows, focused);

    // Render grid using ViewModel
    render_grid(frame, inner, &view_model, &mut app.screen_areas);
}

/// Render the step grid body from ViewModel
///
/// Takes ViewModel (read-only) and ScreenAreas (mutable for hit-test registration).
fn render_grid(
    frame: &mut Frame,
    inner: Rect,
    vm: &ChannelRackViewModel,
    screen_areas: &mut ScreenAreas,
) {
    for (row_idx, row) in vm.rows.iter().enumerate() {
        let y = inner.y + HEADER_ROWS + row_idx as u16;
        let mut x = inner.x;

        render_mute_zone(frame, &mut x, y, row, vm, screen_areas);
        render_track_zone(frame, &mut x, y, row, vm, screen_areas);
        render_sample_zone(frame, &mut x, y, row, vm, screen_areas);
        render_steps_zone(frame, x, y, inner, row, vm, screen_areas);
    }
}

/// Render mute zone (col -3, VimCol 0)
fn render_mute_zone(
    frame: &mut Frame,
    x: &mut u16,
    y: u16,
    row: &ChannelRowView,
    vm: &ChannelRackViewModel,
    screen_areas: &mut ScreenAreas,
) {
    let mute_rect = Rect::new(*x, y, MUTE_WIDTH, 1);
    screen_areas
        .channel_rack_cells
        .insert((row.slot, 0), mute_rect);

    let is_cursor = row.slot == vm.cursor_row && vm.cursor_col.is_mute_zone() && vm.is_focused;
    let (mute_char, mute_color) = if row.is_allocated {
        if row.is_solo {
            ("S", Color::Yellow)
        } else if row.is_muted {
            ("M", Color::Red)
        } else {
            ("○", Color::Green)
        }
    } else {
        ("·", Color::DarkGray)
    };
    let mute_style = if is_cursor {
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
    *x += MUTE_WIDTH;
}

/// Render track zone (col -2, VimCol 1)
fn render_track_zone(
    frame: &mut Frame,
    x: &mut u16,
    y: u16,
    row: &ChannelRowView,
    vm: &ChannelRackViewModel,
    screen_areas: &mut ScreenAreas,
) {
    let track_rect = Rect::new(*x, y, TRACK_WIDTH, 1);
    screen_areas
        .channel_rack_cells
        .insert((row.slot, 1), track_rect);

    let is_cursor = row.slot == vm.cursor_row && vm.cursor_col.is_track_zone() && vm.is_focused;
    let track_text = if row.is_allocated {
        format!("{:<width$}", row.mixer_track, width = TRACK_WIDTH as usize)
    } else {
        format!("{:<width$}", "·", width = TRACK_WIDTH as usize)
    };
    let track_style = if is_cursor {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if row.is_allocated {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let track_widget = Paragraph::new(track_text).style(track_style);
    frame.render_widget(track_widget, track_rect);
    *x += TRACK_WIDTH;
}

/// Render sample/channel name zone (col -1, VimCol 2)
fn render_sample_zone(
    frame: &mut Frame,
    x: &mut u16,
    y: u16,
    row: &ChannelRowView,
    vm: &ChannelRackViewModel,
    screen_areas: &mut ScreenAreas,
) {
    let sample_rect = Rect::new(*x, y, SAMPLE_WIDTH, 1);
    screen_areas
        .channel_rack_cells
        .insert((row.slot, 2), sample_rect);

    let is_cursor = row.slot == vm.cursor_row && vm.cursor_col.is_sample_zone() && vm.is_focused;
    let is_current_row = row.slot == vm.cursor_row && vm.is_focused;

    let sample_style = if is_cursor {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if is_current_row {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if !row.is_allocated {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let name_display = format!(
        "{:<width$}",
        &row.name[..row.name.len().min(SAMPLE_WIDTH as usize - 1)],
        width = SAMPLE_WIDTH as usize - 1
    );
    let sample_widget = Paragraph::new(name_display).style(sample_style);
    frame.render_widget(sample_widget, sample_rect);
    *x += SAMPLE_WIDTH;
}

/// Render steps zone (col 0-15, VimCol 3-18)
fn render_steps_zone(
    frame: &mut Frame,
    mut x: u16,
    y: u16,
    inner: Rect,
    row: &ChannelRowView,
    vm: &ChannelRackViewModel,
    screen_areas: &mut ScreenAreas,
) {
    let cursor_col = vm.cursor_col.0;

    for step in 0..16 {
        if x + STEP_WIDTH > inner.x + inner.width {
            break;
        }

        let step_idx = step as usize;
        let vim_col_for_step = step_idx + 3; // VimCol 3-18 for steps 0-15
        let pos = Position::new(row.slot, vim_col_for_step);

        let step_rect = Rect::new(x, y, STEP_WIDTH, 1);
        screen_areas
            .channel_rack_cells
            .insert((row.slot, vim_col_for_step), step_rect);

        let is_cursor = row.slot == vm.cursor_row && cursor_col == step && vm.is_focused;
        let is_selected = vm.selection.map(|r| r.contains(pos)).unwrap_or(false);
        let is_beat = step % 4 == 0;
        let is_active = row.steps[step_idx];
        let is_playhead = vm.is_playing && step_idx == vm.playhead_step;

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
