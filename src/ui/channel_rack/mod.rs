//! Channel rack panel - step sequencer grid with zones
//!
//! Zones:
//! - Sample zone (col -2): Channel name, press x to assign sample
//! - Mute zone (col -1): M/S/○ indicator, press x to cycle mute state
//! - Steps zone (col 0-15): Step grid, press x to toggle
//!
//! When in Piano Roll mode, the step grid is replaced by the piano roll
//! for the selected channel, while other channels are greyed out.

mod piano_roll;
mod step_grid;

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, Panel};
use crate::mode::ViewMode;
use crate::ui::areas::AreaId;
use crate::ui::render_panel_frame;

// ============================================================================
// Shared Constants
// ============================================================================

/// Width of the sample name column
pub const SAMPLE_WIDTH: u16 = 10;
/// Width of the mute indicator column
pub const MUTE_WIDTH: u16 = 3;
/// Width of the note/pitch column (in piano roll mode)
pub const NOTE_WIDTH: u16 = 5;
/// Width of each step cell (separator + cell + space)
pub const STEP_WIDTH: u16 = 3;
/// Number of header rows (pattern hint + column headers + separator line)
pub const HEADER_ROWS: u16 = 3;

// ============================================================================
// Main Render Function
// ============================================================================

/// Render the channel rack (or channel rack + piano roll in piano roll mode)
pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    // Determine if we're in piano roll mode
    let in_piano_roll_mode = matches!(app.view_mode, ViewMode::PianoRoll);

    // Determine focus based on mode
    let focused = if in_piano_roll_mode {
        app.mode.current_panel() == Panel::PianoRoll
    } else {
        app.mode.current_panel() == Panel::ChannelRack
    };

    // Title changes based on mode (empty for channel rack, generator name for piano roll)
    let title = if in_piano_roll_mode {
        let generator_name = app
            .generators
            .get(app.channel_rack.channel)
            .map(|g| g.name.as_str())
            .unwrap_or("Generator 1");
        generator_name.to_string()
    } else {
        String::new()
    };

    // Use PianoRoll panel for focus styling in piano roll mode
    let panel = if in_piano_roll_mode {
        Panel::PianoRoll
    } else {
        Panel::ChannelRack
    };
    let inner = render_panel_frame(frame, area, &title, panel, app);

    // Register the grid area (entire inner area minus header)
    let grid_area = Rect::new(
        inner.x,
        inner.y + HEADER_ROWS,
        inner.width,
        inner.height.saturating_sub(HEADER_ROWS),
    );

    if in_piano_roll_mode {
        app.screen_areas.register(AreaId::PianoRollGrid, grid_area);
        piano_roll::render(frame, inner, app, focused);
    } else {
        app.screen_areas
            .register(AreaId::ChannelRackStepsGrid, grid_area);
        step_grid::render(frame, inner, app, focused);
    }
}

// ============================================================================
// Shared Header Rendering
// ============================================================================

/// Render the header rows (pattern hint + column headers)
pub fn render_header(frame: &mut Frame, inner: Rect, app: &mut App, piano_roll_mode: bool) {
    let focused = if piano_roll_mode {
        app.mode.current_panel() == Panel::PianoRoll
    } else {
        app.mode.current_panel() == Panel::ChannelRack
    };

    // Row 1: Pattern selector or piano roll hint
    let hint = if piano_roll_mode {
        Line::from(vec![
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled(" to exit piano roll", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        // Show pattern selector: < P01 >
        // Register click areas for prev/next - make them larger for easier clicking
        // Text: "< " (0-1) + "P01" (2-4) + " >" (5-6)
        let prev_rect = Rect::new(inner.x, inner.y, 3, 1);  // Covers "< " and "P"
        let next_rect = Rect::new(inner.x + 4, inner.y, 3, 1);  // Covers "1 >"
        app.screen_areas
            .register(AreaId::ChannelRackPatternPrev, prev_rect);
        app.screen_areas
            .register(AreaId::ChannelRackPatternNext, next_rect);

        let pattern_num = format!("P{:02}", app.current_pattern + 1);
        Line::from(vec![
            Span::styled("< ", Style::default().fg(Color::DarkGray)),
            Span::styled(pattern_num, Style::default().fg(Color::Cyan)),
            Span::styled(" >", Style::default().fg(Color::DarkGray)),
        ])
    };
    let hint_widget = Paragraph::new(hint);
    frame.render_widget(hint_widget, Rect::new(inner.x, inner.y, inner.width, 1));

    // Row 2: Column headers
    let mut spans = Vec::new();

    // Mute column header (matches MUTE_WIDTH) - now comes first
    spans.push(Span::styled(
        format!("{:<width$}", "M", width = MUTE_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Channel column header
    spans.push(Span::styled(
        format!("{:<width$}", "Channel", width = SAMPLE_WIDTH as usize - 1),
        Style::default().fg(Color::DarkGray),
    ));

    // Spacing and Note column header (only in piano roll mode)
    if piano_roll_mode {
        // Match the 2-space gap used in the body
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(
            format!("{:<width$}", "Note", width = NOTE_WIDTH as usize),
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        // In step grid mode, add 1 char to match SAMPLE_WIDTH
        spans.push(Span::styled(" ", Style::default()));
    }

    // Step number headers (1-16)
    for step in 0..16i32 {
        let step_num = step + 1;
        let is_beat = step % 4 == 0;
        let is_playhead = app.is_playing() && step as usize == app.playhead_step();
        let is_cursor_col = if piano_roll_mode {
            focused && app.piano_roll.step == step as usize
        } else {
            focused && app.channel_rack.col.0 == step && app.channel_rack.col.is_step_zone()
        };

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

        let sep = if is_beat { "┃" } else { "│" };
        spans.push(Span::styled(sep, Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(format!("{:<2}", step_num), style));
    }

    let header_line = Line::from(spans);
    let header_widget = Paragraph::new(header_line);
    frame.render_widget(
        header_widget,
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );

    // Row 3: Horizontal separator line
    let mut sep_spans = Vec::new();

    sep_spans.push(Span::styled(
        "─".repeat(MUTE_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Channel column separator
    sep_spans.push(Span::styled(
        "─".repeat(SAMPLE_WIDTH as usize - 1),
        Style::default().fg(Color::DarkGray),
    ));

    // Spacing and Note column separator (only in piano roll mode)
    if piano_roll_mode {
        sep_spans.push(Span::styled("──", Style::default().fg(Color::DarkGray)));
        sep_spans.push(Span::styled(
            "─".repeat(NOTE_WIDTH as usize),
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        sep_spans.push(Span::styled("─", Style::default().fg(Color::DarkGray)));
    }

    for step in 0..16 {
        let is_beat = step % 4 == 0;
        let cross = if is_beat { "╂" } else { "┼" };
        sep_spans.push(Span::styled(cross, Style::default().fg(Color::DarkGray)));
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
