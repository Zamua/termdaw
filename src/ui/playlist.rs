//! Playlist panel - arrangement view
//!
//! Layout:
//! - Pattern name column (12 chars)
//! - Mute indicator column (3 chars)
//! - Bar grid (16 bars, 4 chars each)
//! - Placements shown as filled blocks

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, Panel};
use crate::input::vim::Position;
use crate::sequencer::Pattern;
use crate::ui::colors::{self, ColGroup};
use crate::ui::render_panel_frame;

/// Width of the pattern name column
const PATTERN_NAME_WIDTH: u16 = 12;
/// Width of the mute indicator column
const MUTE_WIDTH: u16 = 3;
/// Width of each bar cell
const BAR_WIDTH: u16 = 4;
/// Number of header rows (bar numbers + separator)
const HEADER_ROWS: u16 = 2;
/// Number of bars in the arrangement
const NUM_BARS: usize = 16;

/// Check if a pattern has any data (steps or notes) across all channels
fn pattern_has_data(app: &App, pattern: &Pattern) -> bool {
    app.channels.iter().any(|channel| {
        channel
            .get_pattern(pattern.id)
            .map(|slice| slice.steps.iter().any(|&s| s) || !slice.notes.is_empty())
            .unwrap_or(false)
    })
}

/// Render the playlist
pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.mode.current_panel() == Panel::Playlist;
    // No title needed (tab already says "Playlist")
    let inner = render_panel_frame(frame, area, "", Panel::Playlist, app);

    if inner.height < HEADER_ROWS + 1 || inner.width < PATTERN_NAME_WIDTH + MUTE_WIDTH + BAR_WIDTH {
        return; // Not enough space
    }

    // Render header rows
    render_header(frame, inner, app);

    // Get non-empty patterns (patterns that have content)
    let non_empty: Vec<_> = app
        .patterns
        .iter()
        .filter(|p| pattern_has_data(app, p))
        .collect();

    // If no patterns have content, show all patterns
    let patterns: Vec<_> = if non_empty.is_empty() {
        app.patterns.iter().collect()
    } else {
        non_empty
    };

    // Get current visual selection (if any)
    let cursor = Position::new(app.playlist.row, app.playlist.bar);
    let selection = app.vim.playlist.get_selection(cursor);

    // Calculate visible pattern range based on viewport
    let visible_rows = (inner.height - HEADER_ROWS) as usize;
    let viewport_top = app.playlist.viewport_top;

    // Render pattern rows
    for row_idx in 0..visible_rows {
        let pattern_idx = viewport_top + row_idx;
        if pattern_idx >= patterns.len() {
            break;
        }

        let y = inner.y + HEADER_ROWS + row_idx as u16;
        if y >= inner.y + inner.height {
            break;
        }

        let pattern = patterns[pattern_idx];
        render_pattern_row(
            frame,
            inner,
            app,
            y,
            pattern_idx,
            pattern,
            focused,
            selection,
        );
    }
}

/// Render the header rows (bar numbers + separator)
fn render_header(frame: &mut Frame, inner: Rect, app: &App) {
    let focused = app.mode.current_panel() == Panel::Playlist;

    // Row 1: Bar number headers
    let mut spans = Vec::new();

    // Pattern column header
    spans.push(Span::styled(
        format!("{:<width$}", "Pattern", width = PATTERN_NAME_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Mute column header
    spans.push(Span::styled(
        format!("{:<width$}", "M", width = MUTE_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Bar number headers (1-16)
    for bar in 0..NUM_BARS {
        let bar_num = bar + 1;
        let is_beat = bar % 4 == 0;
        let is_playhead = app.is_playing_arrangement() && bar == app.arrangement_bar();
        let is_cursor_col = focused && app.playlist.bar == bar;

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

        // Separator + number
        let sep = if is_beat { "┃" } else { "│" };
        spans.push(Span::styled(sep, Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(format!("{:<3}", bar_num), style));
    }

    let header_line = Line::from(spans);
    let header_widget = Paragraph::new(header_line);
    frame.render_widget(header_widget, Rect::new(inner.x, inner.y, inner.width, 1));

    // Row 2: Horizontal separator line
    let mut sep_spans = Vec::new();

    // Pattern column separator
    sep_spans.push(Span::styled(
        "─".repeat(PATTERN_NAME_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Mute column separator
    sep_spans.push(Span::styled(
        "─".repeat(MUTE_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Bar column separators with crossings
    for bar in 0..NUM_BARS {
        let is_beat = bar % 4 == 0;
        let cross = if is_beat { "╂" } else { "┼" };
        sep_spans.push(Span::styled(cross, Style::default().fg(Color::DarkGray)));
        sep_spans.push(Span::styled("───", Style::default().fg(Color::DarkGray)));
    }

    let sep_line = Line::from(sep_spans);
    let sep_widget = Paragraph::new(sep_line);
    frame.render_widget(sep_widget, Rect::new(inner.x, inner.y + 1, inner.width, 1));
}

/// Render a single pattern row
#[allow(clippy::too_many_arguments)]
fn render_pattern_row(
    frame: &mut Frame,
    inner: Rect,
    app: &App,
    y: u16,
    row_idx: usize,
    pattern: &crate::sequencer::Pattern,
    focused: bool,
    selection: Option<crate::input::vim::Range>,
) {
    let mut spans = Vec::new();

    // Default background for non-bar zones
    let zone_bg = colors::bg::COL_A;

    // Pattern name
    let is_cursor_row = focused && app.playlist.row == row_idx;
    let pattern_style = if is_cursor_row {
        Style::default()
            .fg(Color::Cyan)
            .bg(zone_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).bg(zone_bg)
    };

    let name = if pattern.name.len() > PATTERN_NAME_WIDTH as usize - 1 {
        format!(
            "{:<width$}",
            &pattern.name[..PATTERN_NAME_WIDTH as usize - 1],
            width = PATTERN_NAME_WIDTH as usize
        )
    } else {
        format!(
            "{:<width$}",
            pattern.name,
            width = PATTERN_NAME_WIDTH as usize
        )
    };
    spans.push(Span::styled(name, pattern_style));

    // Mute/Solo indicator - same as channel rack: ○ (normal), M (muted), S (solo)
    let is_muted = app.arrangement.is_pattern_muted(pattern.id);
    let is_soloed = app.arrangement.is_pattern_soloed(pattern.id);

    let (mute_char, mute_color) = if is_soloed {
        ("S", Color::Yellow)
    } else if is_muted {
        ("M", Color::Red)
    } else {
        ("○", Color::Green)
    };

    let is_mute_cursor = is_cursor_row && app.playlist.bar == 0;
    let mute_style = if is_mute_cursor {
        Style::default()
            .fg(colors::fg::CURSOR_CONTENT)
            .bg(colors::bg::CURSOR)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(mute_color).bg(zone_bg)
    };

    spans.push(Span::styled(
        format!("{:<width$}", mute_char, width = MUTE_WIDTH as usize),
        mute_style,
    ));

    // Bar cells
    for bar in 0..NUM_BARS {
        let is_beat = bar % 4 == 0;
        let is_cursor = focused && app.playlist.row == row_idx && app.playlist.bar == bar + 1;
        let is_playhead = app.is_playing_arrangement() && bar == app.arrangement_bar();

        // Check if this cell is in visual selection (bar + 1 because bar 0 is mute column)
        let pos = Position::new(row_idx, bar + 1);
        let is_selected = selection.map(|r| r.contains(pos)).unwrap_or(false);

        // Check if there's a placement at this bar
        let has_placement = app.arrangement.get_placement_at(pattern.id, bar).is_some();

        // Separator
        let sep = if is_beat { "┃" } else { "│" };
        spans.push(Span::styled(sep, Style::default().fg(Color::DarkGray)));

        // Get column group for this bar (alternates every 4 bars)
        let col_group = ColGroup::from_step(bar);

        // Determine cell state using unified color logic
        let cell_state =
            colors::determine_cell_state(is_cursor, is_selected, is_playhead, has_placement);

        // Get style from unified color scheme
        let cell_style = if has_placement && is_muted && !is_cursor && !is_selected && !is_playhead
        {
            // Muted placements get dimmed
            Style::default()
                .fg(colors::fg::MUTED)
                .bg(colors::col_bg(col_group))
        } else {
            colors::cell_style(cell_state, col_group)
        };

        // Cell content (3 chars for playlist)
        let cell = if has_placement {
            colors::chars::FILLED_3
        } else {
            colors::chars::EMPTY_3
        };

        spans.push(Span::styled(cell, cell_style));
    }

    let row_line = Line::from(spans);
    let row_widget = Paragraph::new(row_line);
    frame.render_widget(row_widget, Rect::new(inner.x, y, inner.width, 1));
}
