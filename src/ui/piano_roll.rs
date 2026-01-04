//! Piano roll panel - note editor grid
//!
//! Layout:
//! - Pitch labels on left (5 chars): "C4  ", "C#4 "
//! - Step grid (16 steps, 3 chars each)
//! - Notes shown as filled blocks with continuation lines

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

/// Width of the pitch label column
const PITCH_WIDTH: u16 = 5;
/// Width of each step cell
const STEP_WIDTH: u16 = 3;
/// Number of header rows (step numbers + separator)
const HEADER_ROWS: u16 = 2;
/// Minimum visible pitch (C2)
const MIN_PITCH: u8 = 36;
/// Maximum visible pitch (C6)
const MAX_PITCH: u8 = 84;

/// Pitch names for each semitone
const PITCH_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Get the display name for a pitch (e.g., "C4", "F#3")
fn get_pitch_name(pitch: u8) -> String {
    let note = PITCH_NAMES[(pitch % 12) as usize];
    let octave = pitch / 12;
    format!("{}{}", note, octave)
}

/// Check if a pitch is a black key
fn is_black_key(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

/// Render the piano roll
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.mode.current_panel() == Panel::PianoRoll;

    // Get channel name for title
    let channel_name = app
        .channels
        .get(app.channel_rack.channel)
        .map(|c| c.name.as_str())
        .unwrap_or("Channel 1");
    let title = format!("Piano Roll - {}", channel_name);

    let inner = render_panel_frame(frame, area, &title, Panel::PianoRoll, app);

    if inner.height < HEADER_ROWS + 1 || inner.width < PITCH_WIDTH + STEP_WIDTH {
        return; // Not enough space
    }

    // Render header rows
    render_header(frame, inner, app);

    // Get notes from current pattern for current channel
    let notes = app
        .get_current_pattern()
        .map(|p| p.get_notes(app.channel_rack.channel))
        .unwrap_or(&[]);

    // Get current visual selection (if any)
    let cursor_row = MAX_PITCH.saturating_sub(app.piano_roll.pitch) as usize;
    let cursor = Position::new(cursor_row, app.piano_roll.step);
    let selection = app.vim_piano_roll.get_selection(cursor);

    // Calculate visible pitch range based on viewport
    let visible_rows = (inner.height - HEADER_ROWS) as usize;
    let viewport_top = app.piano_roll.viewport_top.min(MAX_PITCH);

    // Render pitch rows (from high to low)
    for row_idx in 0..visible_rows {
        let y = inner.y + HEADER_ROWS + row_idx as u16;
        if y >= inner.y + inner.height {
            break;
        }

        // Calculate pitch for this row (high pitches at top)
        let pitch = viewport_top.saturating_sub(row_idx as u8);
        if pitch < MIN_PITCH {
            break;
        }

        render_pitch_row(frame, inner, app, y, pitch, notes, focused, selection);
    }
}

/// Render the header rows (step numbers + separator)
fn render_header(frame: &mut Frame, inner: Rect, app: &App) {
    let focused = app.mode.current_panel() == Panel::PianoRoll;

    // Row 1: Step number headers
    let mut spans = Vec::new();

    // Pitch column header
    spans.push(Span::styled(
        format!("{:<width$}", "Note", width = PITCH_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Step number headers (1-16)
    for step in 0..16i32 {
        let step_num = step + 1;
        let is_beat = step % 4 == 0;
        let is_playhead = app.is_playing() && step as usize == app.playhead_step();
        let is_cursor_col = focused && app.piano_roll.step == step as usize;

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
        spans.push(Span::styled(format!("{:<2}", step_num), style));
    }

    let header_line = Line::from(spans);
    let header_widget = Paragraph::new(header_line);
    frame.render_widget(header_widget, Rect::new(inner.x, inner.y, inner.width, 1));

    // Row 2: Horizontal separator line
    let mut sep_spans = Vec::new();

    // Pitch column separator
    sep_spans.push(Span::styled(
        "─".repeat(PITCH_WIDTH as usize),
        Style::default().fg(Color::DarkGray),
    ));

    // Step column separators with crossings
    for step in 0..16 {
        let is_beat = step % 4 == 0;
        let cross = if is_beat { "╂" } else { "┼" };
        sep_spans.push(Span::styled(cross, Style::default().fg(Color::DarkGray)));
        sep_spans.push(Span::styled("──", Style::default().fg(Color::DarkGray)));
    }

    let sep_line = Line::from(sep_spans);
    let sep_widget = Paragraph::new(sep_line);
    frame.render_widget(sep_widget, Rect::new(inner.x, inner.y + 1, inner.width, 1));
}

/// Render a single pitch row
#[allow(clippy::too_many_arguments)]
fn render_pitch_row(
    frame: &mut Frame,
    inner: Rect,
    app: &App,
    y: u16,
    pitch: u8,
    notes: &[crate::sequencer::Note],
    focused: bool,
    selection: Option<crate::input::vim::Range>,
) {
    let mut spans = Vec::new();

    // Pitch label
    let pitch_name = get_pitch_name(pitch);
    let is_cursor_row = focused && app.piano_roll.pitch == pitch;
    let is_black = is_black_key(pitch);

    let pitch_style = if is_cursor_row {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if is_black {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    spans.push(Span::styled(
        format!("{:<width$}", pitch_name, width = PITCH_WIDTH as usize),
        pitch_style,
    ));

    // Calculate vim row for this pitch (high pitches at top = low row numbers)
    let vim_row = MAX_PITCH.saturating_sub(pitch) as usize;

    // Step cells
    for step in 0..16usize {
        let is_beat = step % 4 == 0;
        let is_cursor = focused && app.piano_roll.pitch == pitch && app.piano_roll.step == step;
        let is_playhead = app.is_playing() && step == app.playhead_step();
        let is_placing_preview = app.piano_roll.placing_note.is_some_and(|start| {
            let min = start.min(app.piano_roll.step);
            let max = start.max(app.piano_roll.step);
            pitch == app.piano_roll.pitch && step >= min && step <= max
        });

        // Check if this cell is in visual selection
        let pos = Position::new(vim_row, step);
        let is_selected = selection.map(|r| r.contains(pos)).unwrap_or(false);

        // Check if there's a note at this position
        let note_at = notes
            .iter()
            .find(|n| n.pitch == pitch && n.covers_step(step));
        let is_note_start = notes
            .iter()
            .any(|n| n.pitch == pitch && n.start_step == step);

        // Separator
        let sep = if is_beat { "┃" } else { "│" };
        spans.push(Span::styled(sep, Style::default().fg(Color::DarkGray)));

        // Cell content - use yellow cursor when in placement mode
        let cursor_color = if app.piano_roll.placing_note.is_some() {
            Color::Yellow
        } else {
            Color::Cyan
        };

        let (cell, cell_style) = if is_cursor {
            if note_at.is_some() {
                ("██", Style::default().fg(cursor_color).bg(cursor_color))
            } else if is_placing_preview {
                ("░░", Style::default().fg(cursor_color).bg(cursor_color))
            } else {
                ("  ", Style::default().bg(cursor_color))
            }
        } else if is_selected {
            if let Some(_note) = note_at {
                // Selected notes: show note block with contrasting colors
                if is_note_start {
                    ("██", Style::default().fg(Color::Red).bg(Color::Yellow))
                } else {
                    ("──", Style::default().fg(Color::Red).bg(Color::Yellow))
                }
            } else {
                // Empty selected cell
                ("  ", Style::default().bg(Color::Yellow))
            }
        } else if is_playhead {
            if note_at.is_some() {
                ("██", Style::default().fg(Color::Green).bg(Color::Green))
            } else {
                ("  ", Style::default().bg(Color::Green))
            }
        } else if is_placing_preview {
            ("░░", Style::default().fg(Color::Yellow))
        } else if let Some(_note) = note_at {
            if is_note_start {
                ("██", Style::default().fg(Color::Magenta))
            } else {
                ("──", Style::default().fg(Color::Magenta))
            }
        } else if is_black {
            ("  ", Style::default().fg(Color::DarkGray))
        } else {
            ("  ", Style::default())
        };

        spans.push(Span::styled(cell, cell_style));
    }

    let row_line = Line::from(spans);
    let row_widget = Paragraph::new(row_line);
    frame.render_widget(row_widget, Rect::new(inner.x, y, inner.width, 1));
}
