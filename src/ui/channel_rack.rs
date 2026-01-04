//! Channel rack panel - step sequencer grid with zones
//!
//! Zones:
//! - Sample zone (col -2): Channel name, press x to assign sample
//! - Mute zone (col -1): M/S/○ indicator, press x to cycle mute state
//! - Steps zone (col 0-15): Step grid, press x to toggle

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, Panel};
use crate::input::vim::Position;
use crate::ui::render_panel_frame;

/// Width of the sample name column
const SAMPLE_WIDTH: u16 = 10;
/// Width of the mute indicator column
const MUTE_WIDTH: u16 = 3;
/// Width of each step cell (separator + cell + space)
const STEP_WIDTH: u16 = 3;
/// Number of header rows (pattern hint + column headers + separator line)
const HEADER_ROWS: u16 = 3;
/// Total number of channel slots
const TOTAL_CHANNEL_SLOTS: usize = 99;

/// Render the channel rack
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.mode.current_panel() == Panel::ChannelRack;

    // Title shows "Channel Rack - Pattern N"
    let pattern_name = app
        .get_current_pattern()
        .map(|p| p.name.as_str())
        .unwrap_or("Pattern 1");
    let title = format!("Channel Rack - {}", pattern_name);

    let inner = render_panel_frame(frame, area, &title, Panel::ChannelRack, app);

    // Render header rows
    render_header(frame, inner, app);

    // Get channels from app state
    let channels = &app.channels;

    // Calculate visible rows
    let visible_rows = (inner.height - HEADER_ROWS) as usize;
    let viewport_top = app.channel_rack.viewport_top;

    // Get current visual selection (if any)
    // Convert cursor_col to vim space for selection check
    let vim_col: crate::coords::VimCol = app.channel_rack.col.into();
    let cursor = Position::new(app.channel_rack.channel, vim_col.0);
    let selection = app.vim_channel_rack.get_selection(cursor);

    // Render all 99 channel slots (within viewport)
    for row_idx in 0..visible_rows {
        let channel_idx = viewport_top + row_idx;
        if channel_idx >= TOTAL_CHANNEL_SLOTS {
            break;
        }

        let y = inner.y + HEADER_ROWS + row_idx as u16;
        let mut x = inner.x;

        // Check if this slot has an allocated channel
        let channel = channels.get(channel_idx);
        let is_allocated = channel.is_some();

        // === MUTE ZONE (col -2) - now comes first ===
        let is_mute_cursor = channel_idx == app.channel_rack.channel
            && app.channel_rack.col.is_mute_zone()
            && focused;
        let (mute_char, mute_color) = if let Some(ch) = channel {
            if ch.solo {
                ("S", Color::Yellow)
            } else if ch.muted {
                ("M", Color::Red)
            } else {
                ("○", Color::Green)
            }
        } else {
            ("·", Color::DarkGray) // Unallocated slot
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
        frame.render_widget(mute_widget, Rect::new(x, y, MUTE_WIDTH, 1));
        x += MUTE_WIDTH;

        // === SAMPLE ZONE (col -1) ===
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
            Style::default().fg(Color::DarkGray) // Greyed out for unallocated
        } else {
            Style::default().fg(Color::White)
        };

        // Display channel name or empty slot indicator
        let name_display = if let Some(ch) = channel {
            if ch.is_plugin() || ch.sample_path.is_some() {
                format!(
                    "{:<width$}",
                    &ch.name[..ch.name.len().min(SAMPLE_WIDTH as usize - 1)],
                    width = SAMPLE_WIDTH as usize - 1
                )
            } else {
                format!("{:<width$}", "(empty)", width = SAMPLE_WIDTH as usize - 1)
            }
        } else {
            // Unallocated slot - show slot number greyed out
            format!(
                "{:<width$}",
                format!("Slot {}", channel_idx + 1),
                width = SAMPLE_WIDTH as usize - 1
            )
        };
        let sample_widget = Paragraph::new(name_display).style(sample_style);
        frame.render_widget(sample_widget, Rect::new(x, y, SAMPLE_WIDTH, 1));
        x += SAMPLE_WIDTH;

        // === STEPS ZONE (col 0-15) ===
        let pattern = app.get_current_pattern();
        let steps = pattern
            .and_then(|p| p.steps.get(channel_idx))
            .map(|s| s.as_slice());

        for step in 0..16 {
            if x + STEP_WIDTH > inner.x + inner.width {
                break;
            }

            let step_idx = step as usize;
            // For selection, use vim coordinates (step + 2)
            let pos = Position::new(channel_idx, step_idx + 2);

            let is_cursor = channel_idx == app.channel_rack.channel
                && app.channel_rack.col.0 == step
                && focused;
            let is_selected = selection.map(|r| r.contains(pos)).unwrap_or(false);
            let is_beat = step % 4 == 0;
            let is_active = steps
                .map(|s| s.get(step_idx).copied().unwrap_or(false))
                .unwrap_or(false);
            let is_playhead = app.is_playing() && step_idx == app.playhead_step();

            // Separator character (always shown)
            let sep = if is_beat { "┃" } else { "│" };
            let sep_color = Color::DarkGray;

            // Cell content (2 chars: empty or filled based on active state)
            let (cell, cell_style) = if is_cursor {
                if is_active {
                    ("██", Style::default().fg(Color::Cyan).bg(Color::Cyan))
                } else {
                    ("  ", Style::default().bg(Color::Cyan))
                }
            } else if is_selected {
                if is_active {
                    // Selected active step: show with contrasting colors
                    ("██", Style::default().fg(Color::Red).bg(Color::Yellow))
                } else {
                    // Empty selected cell
                    ("  ", Style::default().bg(Color::Yellow))
                }
            } else if is_playhead {
                if is_active {
                    ("██", Style::default().fg(Color::Green).bg(Color::Green))
                } else {
                    ("  ", Style::default().bg(Color::Green))
                }
            } else if is_active {
                ("██", Style::default().fg(Color::Yellow))
            } else if !is_allocated {
                // Greyed out step cells for unallocated slots
                ("  ", Style::default().fg(Color::DarkGray))
            } else {
                ("  ", Style::default())
            };

            // Render separator + cell (2 chars) as a combined line
            let step_line = Line::from(vec![
                Span::styled(sep, Style::default().fg(sep_color)),
                Span::styled(cell, cell_style),
            ]);
            let step_widget = Paragraph::new(step_line);
            frame.render_widget(step_widget, Rect::new(x, y, STEP_WIDTH, 1));
            x += STEP_WIDTH;
        }
    }
}

/// Render the header rows (pattern hint + column headers)
fn render_header(frame: &mut Frame, inner: Rect, app: &App) {
    let focused = app.mode.current_panel() == Panel::ChannelRack;

    // Row 1: Pattern hint
    let hint = Line::from(vec![
        Span::styled("[ ]", Style::default().fg(Color::DarkGray)),
        Span::styled(" to switch pattern", Style::default().fg(Color::DarkGray)),
    ]);
    let hint_widget = Paragraph::new(hint);
    frame.render_widget(hint_widget, Rect::new(inner.x, inner.y, inner.width, 1));

    // Row 2: Column headers
    let mut spans = Vec::new();

    // Mute column header (matches MUTE_WIDTH) - now comes first
    spans.push(Span::styled(
        format!("{:<width$}", "M", width = MUTE_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Channel column header (matches SAMPLE_WIDTH)
    spans.push(Span::styled(
        format!("{:<width$}", "Channel", width = SAMPLE_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Step number headers (1-16)
    // Format: separator + number (right-aligned in remaining space)
    // This matches the step cell format: separator + cell + space
    for step in 0..16i32 {
        let step_num = step + 1; // 1-indexed
        let is_beat = step % 4 == 0;
        let is_playhead = app.is_playing() && step as usize == app.playhead_step();
        let is_cursor_col =
            focused && app.channel_rack.col.0 == step && app.channel_rack.col.is_step_zone();

        let color = if is_playhead {
            Color::Green
        } else if is_beat {
            Color::Yellow
        } else {
            Color::DarkGray
        };

        let style = if is_cursor_col {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };

        // Separator character (matches step cells)
        let sep = if is_beat { "┃" } else { "│" };
        spans.push(Span::styled(sep, Style::default().fg(Color::DarkGray)));
        // Step number + space (2 chars to fill remaining STEP_WIDTH)
        spans.push(Span::styled(format!("{:<2}", step_num), style));
    }

    let header_line = Line::from(spans);
    let header_widget = Paragraph::new(header_line);
    frame.render_widget(
        header_widget,
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );

    // Row 3: Horizontal separator line with crossing characters at step separators
    let mut sep_spans = Vec::new();

    // Mute column: horizontal line - now comes first
    sep_spans.push(Span::styled(
        "─".repeat(MUTE_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Sample column: horizontal line
    sep_spans.push(Span::styled(
        "─".repeat(SAMPLE_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Step columns: crossing character + horizontal line
    for step in 0..16 {
        let is_beat = step % 4 == 0;
        // Match crossing to vertical line weight: ┃ (heavy) or │ (light)
        let cross = if is_beat { "╂" } else { "┼" };
        sep_spans.push(Span::styled(cross, Style::default().fg(Color::DarkGray)));
        // Horizontal line for remaining step width (STEP_WIDTH - 1)
        sep_spans.push(Span::styled(
            "─".repeat(STEP_WIDTH as usize - 1),
            Style::default().fg(Color::DarkGray),
        ));
    }

    let sep_line = Line::from(sep_spans);
    let separator_widget = Paragraph::new(sep_line);
    frame.render_widget(
        separator_widget,
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
    );
}
