//! Piano roll component for channel rack
//!
//! Renders the piano roll note editor when in piano roll mode.
//! Shows the channel name, pitch labels (white/black key coloring), and note grid.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::input::vim::Position;

use super::{render_header, HEADER_ROWS, MUTE_WIDTH, NOTE_WIDTH, SAMPLE_WIDTH, TRACK_WIDTH};

// ============================================================================
// Constants
// ============================================================================

/// Minimum visible pitch (C2)
const MIN_PITCH: u8 = 36;
/// Maximum visible pitch (C6)
const MAX_PITCH: u8 = 84;

/// Pitch names for each semitone
const PITCH_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

// ============================================================================
// Helpers
// ============================================================================

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

// ============================================================================
// Rendering
// ============================================================================

/// Render piano roll mode: pitch rows with channel info and note grid
pub fn render(frame: &mut Frame, inner: Rect, app: &mut App, focused: bool) {
    // Render header rows (with piano roll mode headers)
    render_header(frame, inner, app, true);

    let selected_channel = app.channel_rack.channel;

    // Get notes from current pattern for selected channel
    let notes: Vec<crate::sequencer::Note> = app
        .get_current_pattern()
        .map(|p| p.get_notes(selected_channel).to_vec())
        .unwrap_or_default();

    // Get current visual selection (if any)
    let cursor_row = MAX_PITCH.saturating_sub(app.piano_roll.pitch) as usize;
    let cursor = Position::new(cursor_row, app.piano_roll.step);
    let selection = app.vim_piano_roll.get_selection(cursor);

    // Calculate visible pitch range based on viewport
    let visible_rows = (inner.height - HEADER_ROWS) as usize;
    let pitch_viewport_top = app.piano_roll.viewport_top.min(MAX_PITCH);
    let channel_viewport_top = app.channel_rack.viewport_top;

    // Render pitch rows (from high to low)
    for row_idx in 0..visible_rows {
        let y = inner.y + HEADER_ROWS + row_idx as u16;
        if y >= inner.y + inner.height {
            break;
        }

        // Calculate pitch for this row (high pitches at top)
        let pitch = pitch_viewport_top.saturating_sub(row_idx as u8);
        if pitch < MIN_PITCH {
            break;
        }

        render_row(
            frame,
            inner,
            app,
            y,
            pitch,
            row_idx,
            &notes,
            focused,
            selection,
            selected_channel,
            channel_viewport_top,
        );
    }
}

/// Render a single row in piano roll mode
#[allow(clippy::too_many_arguments)]
fn render_row(
    frame: &mut Frame,
    inner: Rect,
    app: &App,
    y: u16,
    pitch: u8,
    row_idx: usize,
    notes: &[crate::sequencer::Note],
    focused: bool,
    selection: Option<crate::input::vim::Range>,
    selected_channel: usize,
    viewport_top: usize,
) {
    let mut spans = Vec::new();

    // Calculate vim row for this pitch (high pitches at top = low row numbers)
    let vim_row = MAX_PITCH.saturating_sub(pitch) as usize;

    let is_cursor_row = focused && app.piano_roll.pitch == pitch;
    let is_black = is_black_key(pitch);

    // Map this row to a generator index (for displaying generator list alongside pitches)
    let channel_idx = viewport_top + row_idx;
    let generator = app.generators.get(channel_idx);
    let is_selected_channel = channel_idx == selected_channel;

    // Get mute/solo state from the mixer track this generator routes to
    let track_id = app.mixer.get_generator_track(channel_idx);
    let mixer_track = app.mixer.track(track_id);

    // === MUTE ZONE ===
    let (mute_char, mute_color) = if generator.is_some() {
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
    let mute_style = if is_selected_channel {
        Style::default().fg(mute_color)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    spans.push(Span::styled(
        format!("{:<width$}", mute_char, width = MUTE_WIDTH as usize),
        mute_style,
    ));

    // === TRACK ZONE ===
    let track_num = track_id.index();
    let track_text = if generator.is_some() {
        format!("{:<width$}", track_num, width = TRACK_WIDTH as usize)
    } else {
        format!("{:<width$}", "·", width = TRACK_WIDTH as usize)
    };
    let track_style = if is_selected_channel {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    spans.push(Span::styled(track_text, track_style));

    // === CHANNEL NAME ZONE ===
    // Show all generators, highlight selected one
    let channel_display = if let Some(gen) = generator {
        if gen.is_plugin() || gen.sample_path.is_some() {
            format!(
                "{:<width$}",
                &gen.name[..gen.name.len().min(SAMPLE_WIDTH as usize - 1)],
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
    let channel_style = if is_selected_channel {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    spans.push(Span::styled(channel_display, channel_style));

    // Add spacing between Channel and Note
    spans.push(Span::styled("  ", Style::default()));

    // === NOTE/PITCH ZONE ===
    // Show pitch name with white/black key coloring
    let pitch_name = get_pitch_name(pitch);
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
        format!("{:<width$}", pitch_name, width = NOTE_WIDTH as usize),
        pitch_style,
    ));

    // === STEP CELLS (piano roll notes) ===
    for step in 0..16usize {
        let is_beat = step % 4 == 0;
        let is_cursor = is_cursor_row && app.piano_roll.step == step;
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

        // Determine if there's a note/content at this position
        let has_note = note_at.is_some();

        // Special handling for placement mode cursor
        let is_placement_mode = app.piano_roll.placing_note.is_some();

        // Determine cell style and content
        let (cell, cell_style) = if is_cursor {
            if is_placement_mode {
                // Yellow cursor when placing notes
                if has_note || is_placing_preview {
                    ("██", Style::default().fg(Color::Black).bg(Color::Yellow))
                } else {
                    ("  ", Style::default().bg(Color::Yellow))
                }
            } else if has_note {
                ("██", Style::default().fg(Color::Cyan).bg(Color::Cyan))
            } else {
                ("  ", Style::default().bg(Color::Cyan))
            }
        } else if is_selected {
            if has_note {
                if is_note_start {
                    ("██", Style::default().fg(Color::Red).bg(Color::Yellow))
                } else {
                    ("──", Style::default().fg(Color::Red).bg(Color::Yellow))
                }
            } else {
                ("  ", Style::default().bg(Color::Yellow))
            }
        } else if is_playhead {
            if has_note {
                ("██", Style::default().fg(Color::Green).bg(Color::Green))
            } else {
                ("  ", Style::default().bg(Color::Green))
            }
        } else if is_placing_preview {
            ("░░", Style::default().fg(Color::Yellow))
        } else if has_note {
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
