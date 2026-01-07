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

use super::piano_roll_view_model::{ChannelSidebarView, PianoRollViewModel};
use super::{render_header, HEADER_ROWS, MUTE_WIDTH, NOTE_WIDTH, SAMPLE_WIDTH, TRACK_WIDTH};

// ============================================================================
// Constants
// ============================================================================

/// Minimum visible pitch (C2)
const MIN_PITCH: u8 = 36;

// ============================================================================
// Rendering
// ============================================================================

/// Render piano roll mode: pitch rows with channel info and note grid
pub fn render(frame: &mut Frame, inner: Rect, app: &mut App, focused: bool) {
    // Render header rows (with piano roll mode headers)
    render_header(frame, inner, app, true);

    // Calculate visible rows and build ViewModel
    let visible_rows = (inner.height - HEADER_ROWS) as usize;
    let vm = PianoRollViewModel::from_app(app, visible_rows, focused);

    // Render grid using ViewModel
    render_grid(frame, inner, &vm, visible_rows);
}

/// Render the piano roll grid from ViewModel
fn render_grid(frame: &mut Frame, inner: Rect, vm: &PianoRollViewModel, visible_rows: usize) {
    // Render pitch rows (from high to low)
    for row_idx in 0..visible_rows {
        let y = inner.y + HEADER_ROWS + row_idx as u16;
        if y >= inner.y + inner.height {
            break;
        }

        // Calculate pitch for this row (high pitches at top)
        let pitch = vm.pitch_viewport_top.saturating_sub(row_idx as u8);
        if pitch < MIN_PITCH {
            break;
        }

        // Get sidebar channel for this row
        let sidebar = vm.sidebar_channels.get(row_idx);

        render_row(frame, inner, y, pitch, row_idx, vm, sidebar);
    }
}

/// Render a single row in piano roll mode
fn render_row(
    frame: &mut Frame,
    inner: Rect,
    y: u16,
    pitch: u8,
    row_idx: usize,
    vm: &PianoRollViewModel,
    sidebar: Option<&ChannelSidebarView>,
) {
    let mut spans = Vec::new();

    // Calculate vim row for this pitch (high pitches at top = low row numbers)
    let vim_row = PianoRollViewModel::pitch_to_vim_row(pitch);

    let is_cursor_row = vm.is_focused && vm.cursor_pitch == pitch;
    let is_black = PianoRollViewModel::is_black_key(pitch);

    // Get sidebar info (if available)
    let slot = vm.channel_viewport_top + row_idx;
    let is_selected_channel = sidebar.map(|s| s.is_selected).unwrap_or(false);

    // === MUTE ZONE ===
    let (mute_char, mute_color) = if let Some(ch) = sidebar {
        if ch.is_allocated {
            if ch.is_solo {
                ("S", Color::Yellow)
            } else if ch.is_muted {
                ("M", Color::Red)
            } else {
                ("○", Color::Green)
            }
        } else {
            ("·", Color::DarkGray)
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
    let track_text = if let Some(ch) = sidebar {
        if ch.is_allocated {
            format!("{:<width$}", ch.mixer_track, width = TRACK_WIDTH as usize)
        } else {
            format!("{:<width$}", "·", width = TRACK_WIDTH as usize)
        }
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
    let channel_display = if let Some(ch) = sidebar {
        format!(
            "{:<width$}",
            &ch.name[..ch.name.len().min(SAMPLE_WIDTH as usize - 1)],
            width = SAMPLE_WIDTH as usize - 1
        )
    } else {
        format!(
            "{:<width$}",
            format!("Slot {}", slot + 1),
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
    let pitch_name = PianoRollViewModel::pitch_name(pitch);
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
        let is_cursor = is_cursor_row && vm.cursor_step == step;
        let is_playhead = vm.is_playing && step == vm.playhead_step;
        let is_placing_preview = vm.is_placing_preview(pitch, step);

        // Check if this cell is in visual selection
        let pos = Position::new(vim_row, step);
        let is_selected = vm.selection.map(|r| r.contains(pos)).unwrap_or(false);

        // Check if there's a note at this position
        let has_note = vm.note_at(pitch, step).is_some();
        let is_note_start = vm.note_starts_at(pitch, step);

        // Separator
        let sep = if is_beat { "┃" } else { "│" };
        spans.push(Span::styled(sep, Style::default().fg(Color::DarkGray)));

        // Special handling for placement mode cursor
        let is_placement_mode = vm.placing_note_start.is_some();

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
