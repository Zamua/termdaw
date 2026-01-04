//! Unified color scheme for grid-based views
//!
//! All grid views (channel rack, piano roll, playlist) use this consistent scheme:
//! - Alternating row backgrounds (A/B pattern like FL Studio)
//! - Cursor highlighting
//! - Filled cell highlighting
//! - Selection highlighting
//! - Playhead highlighting

#![allow(dead_code)] // Constants are defined for future use across all grid views

use ratatui::style::{Color, Style};

/// Column group type for alternating colors (like FL Studio's grey/red pattern)
/// Alternates every 4 columns: cols 0-3 = A, cols 4-7 = B, cols 8-11 = A, etc.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColGroup {
    /// First group in each 8-column cycle (cols 0-3, 8-11, ...)
    A,
    /// Second group in each 8-column cycle (cols 4-7, 12-15, ...)
    B,
}

impl ColGroup {
    /// Get column group based on step/column index, alternating every 4 columns
    /// Cols 0-3 = A, Cols 4-7 = B, Cols 8-11 = A, etc.
    pub fn from_step(step: usize) -> Self {
        if (step / 4).is_multiple_of(2) {
            ColGroup::A
        } else {
            ColGroup::B
        }
    }
}

/// Cell state for determining visual style
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    /// Normal empty cell
    Empty,
    /// Cell has content (step, note, placement)
    Filled,
    /// Cursor is on this cell (empty)
    CursorEmpty,
    /// Cursor is on this cell (has content)
    CursorFilled,
    /// Cell is in visual selection (empty)
    SelectedEmpty,
    /// Cell is in visual selection (has content)
    SelectedFilled,
    /// Playhead is on this cell (empty)
    PlayheadEmpty,
    /// Playhead is on this cell (has content)
    PlayheadFilled,
}

/// Background colors for alternating column groups
pub mod bg {
    use super::*;

    /// Column group A background - dark gray
    pub const COL_A: Color = Color::Rgb(40, 40, 45);
    /// Column group B background - dark red/maroon
    pub const COL_B: Color = Color::Rgb(50, 35, 40);

    /// Cursor background
    pub const CURSOR: Color = Color::Cyan;
    /// Selection background
    pub const SELECTED: Color = Color::Yellow;
    /// Playhead background
    pub const PLAYHEAD: Color = Color::Green;
}

/// Foreground colors for filled cells
pub mod fg {
    use super::*;

    /// Filled cell in column group A - bright/white
    pub const FILLED_A: Color = Color::Rgb(200, 200, 220);
    /// Filled cell in column group B - pink/salmon
    pub const FILLED_B: Color = Color::Rgb(220, 150, 170);

    /// Content under cursor - dark for visibility
    pub const CURSOR_CONTENT: Color = Color::Rgb(30, 80, 90);
    /// Content in selection - contrasting
    pub const SELECTED_CONTENT: Color = Color::Red;
    /// Content at playhead
    pub const PLAYHEAD_CONTENT: Color = Color::Rgb(0, 60, 0);

    /// Muted content (for muted channels/patterns)
    pub const MUTED: Color = Color::DarkGray;
}

/// Get the style for a grid cell based on its state and column group
pub fn cell_style(state: CellState, col_group: ColGroup) -> Style {
    match state {
        CellState::Empty => Style::default().bg(col_bg(col_group)),

        CellState::Filled => Style::default()
            .fg(filled_fg(col_group))
            .bg(col_bg(col_group)),

        CellState::CursorEmpty => Style::default().bg(bg::CURSOR),

        CellState::CursorFilled => Style::default().fg(fg::CURSOR_CONTENT).bg(bg::CURSOR),

        CellState::SelectedEmpty => Style::default().bg(bg::SELECTED),

        CellState::SelectedFilled => Style::default().fg(fg::SELECTED_CONTENT).bg(bg::SELECTED),

        CellState::PlayheadEmpty => Style::default().bg(bg::PLAYHEAD),

        CellState::PlayheadFilled => Style::default().fg(fg::PLAYHEAD_CONTENT).bg(bg::PLAYHEAD),
    }
}

/// Get background color for a column group
pub fn col_bg(col_group: ColGroup) -> Color {
    match col_group {
        ColGroup::A => bg::COL_A,
        ColGroup::B => bg::COL_B,
    }
}

/// Get foreground color for filled cells in a column group
pub fn filled_fg(col_group: ColGroup) -> Color {
    match col_group {
        ColGroup::A => fg::FILLED_A,
        ColGroup::B => fg::FILLED_B,
    }
}

/// Determine cell state from boolean flags
/// Priority: cursor > selected > playhead > filled > empty
pub fn determine_cell_state(
    is_cursor: bool,
    is_selected: bool,
    is_playhead: bool,
    is_filled: bool,
) -> CellState {
    if is_cursor {
        if is_filled {
            CellState::CursorFilled
        } else {
            CellState::CursorEmpty
        }
    } else if is_selected {
        if is_filled {
            CellState::SelectedFilled
        } else {
            CellState::SelectedEmpty
        }
    } else if is_playhead {
        if is_filled {
            CellState::PlayheadFilled
        } else {
            CellState::PlayheadEmpty
        }
    } else if is_filled {
        CellState::Filled
    } else {
        CellState::Empty
    }
}

/// Cell content characters
pub mod chars {
    /// Filled cell (2 chars wide)
    pub const FILLED_2: &str = "██";
    /// Empty cell (2 chars wide)
    pub const EMPTY_2: &str = "  ";
    /// Filled cell (3 chars wide, for playlist)
    pub const FILLED_3: &str = "███";
    /// Empty cell (3 chars wide, for playlist)
    pub const EMPTY_3: &str = "   ";
    /// Note continuation (for piano roll)
    pub const NOTE_CONT_2: &str = "──";
}
