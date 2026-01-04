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
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, Panel};
use crate::input::vim::Position;

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

/// Render the playlist
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.mode.current_panel() == Panel::Playlist;
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = if focused { "Playlist *" } else { "Playlist" };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < HEADER_ROWS + 1 || inner.width < PATTERN_NAME_WIDTH + MUTE_WIDTH + BAR_WIDTH {
        return; // Not enough space
    }

    // Render header rows
    render_header(frame, inner, app);

    // Get non-empty patterns (patterns that have content)
    let patterns: Vec<_> = app
        .patterns
        .iter()
        .filter(|p| {
            // A pattern is non-empty if it has steps or notes
            p.steps.iter().any(|ch| ch.iter().any(|&s| s))
                || p.notes.iter().any(|ch| !ch.is_empty())
        })
        .collect();

    // If no patterns have content, show all patterns
    let patterns: Vec<_> = if patterns.is_empty() {
        app.patterns.iter().collect()
    } else {
        patterns
    };

    // Get current visual selection (if any)
    let cursor = Position::new(app.playlist_cursor_row, app.playlist_cursor_bar);
    let selection = app.vim_playlist.get_selection(cursor);

    // Calculate visible pattern range based on viewport
    let visible_rows = (inner.height - HEADER_ROWS) as usize;
    let viewport_top = app.playlist_viewport_top;

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
        let is_playhead =
            app.playback.is_playing_arrangement() && bar == app.playback.bar_or_zero();
        let is_cursor_col = focused && app.playlist_cursor_bar == bar;

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

    // Pattern name
    let is_cursor_row = focused && app.playlist_cursor_row == row_idx;
    let pattern_style = if is_cursor_row {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
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

    let mute_style = if is_cursor_row && app.playlist_cursor_bar == 0 {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else {
        Style::default().fg(mute_color)
    };

    spans.push(Span::styled(
        format!("{:<width$}", mute_char, width = MUTE_WIDTH as usize),
        mute_style,
    ));

    // Bar cells
    for bar in 0..NUM_BARS {
        let is_beat = bar % 4 == 0;
        let is_cursor =
            focused && app.playlist_cursor_row == row_idx && app.playlist_cursor_bar == bar + 1;
        let is_playhead =
            app.playback.is_playing_arrangement() && bar == app.playback.bar_or_zero();

        // Check if this cell is in visual selection (bar + 1 because bar 0 is mute column)
        let pos = Position::new(row_idx, bar + 1);
        let is_selected = selection.map(|r| r.contains(pos)).unwrap_or(false);

        // Check if there's a placement at this bar
        let has_placement = app.arrangement.get_placement_at(pattern.id, bar).is_some();

        // Separator
        let sep = if is_beat { "┃" } else { "│" };
        spans.push(Span::styled(sep, Style::default().fg(Color::DarkGray)));

        // Cell content
        let (cell, cell_style) = if is_cursor {
            if has_placement {
                ("███", Style::default().fg(Color::Cyan).bg(Color::Cyan))
            } else {
                ("   ", Style::default().bg(Color::Cyan))
            }
        } else if is_selected {
            if has_placement {
                ("███", Style::default().fg(Color::Yellow).bg(Color::Yellow))
            } else {
                ("   ", Style::default().bg(Color::Yellow))
            }
        } else if is_playhead {
            if has_placement {
                ("███", Style::default().fg(Color::Green).bg(Color::Green))
            } else {
                ("   ", Style::default().bg(Color::Green))
            }
        } else if has_placement {
            if is_muted {
                ("███", Style::default().fg(Color::DarkGray))
            } else {
                ("███", Style::default().fg(Color::Magenta))
            }
        } else {
            ("   ", Style::default())
        };

        spans.push(Span::styled(cell, cell_style));
    }

    let row_line = Line::from(spans);
    let row_widget = Paragraph::new(row_line);
    frame.render_widget(row_widget, Rect::new(inner.x, y, inner.width, 1));
}
