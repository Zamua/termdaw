//! Context traits for input handler testing
//!
//! These traits define the minimal interfaces needed by input handlers,
//! enabling unit testing with mock implementations.

use crate::sequencer::Note;

// ============================================================================
// Step Grid Context (for channel rack step sequencer)
// ============================================================================

/// Context for step grid operations in the channel rack
///
/// This trait abstracts the step sequencer functionality, enabling
/// unit testing of step manipulation logic without the full App.
pub trait StepGridContext {
    /// Get the number of channels
    fn channel_count(&self) -> usize;

    /// Get the current pattern length
    fn pattern_length(&self) -> usize;

    /// Get step state at (channel, step)
    fn get_step(&self, channel: usize, step: usize) -> bool;

    /// Set step state at (channel, step)
    fn set_step(&mut self, channel: usize, step: usize, active: bool);

    /// Toggle step at (channel, step)
    fn toggle_step(&mut self, channel: usize, step: usize) {
        let current = self.get_step(channel, step);
        self.set_step(channel, step, !current);
    }

    /// Get steps for a range of channels and steps
    fn get_step_range(
        &self,
        channels: std::ops::Range<usize>,
        steps: std::ops::Range<usize>,
    ) -> Vec<Vec<bool>> {
        channels
            .map(|ch| steps.clone().map(|st| self.get_step(ch, st)).collect())
            .collect()
    }

    /// Set steps from a 2D array starting at (channel, step)
    fn set_step_range(&mut self, start_channel: usize, start_step: usize, data: &[Vec<bool>]) {
        for (ch_offset, row) in data.iter().enumerate() {
            let ch = start_channel + ch_offset;
            if ch >= self.channel_count() {
                break;
            }
            for (st_offset, &active) in row.iter().enumerate() {
                let st = start_step + st_offset;
                if st < self.pattern_length() {
                    self.set_step(ch, st, active);
                }
            }
        }
    }

    /// Clear steps in a range
    fn clear_step_range(
        &mut self,
        channels: std::ops::Range<usize>,
        steps: std::ops::Range<usize>,
    ) {
        for ch in channels {
            if ch >= self.channel_count() {
                break;
            }
            for st in steps.clone() {
                if st < self.pattern_length() {
                    self.set_step(ch, st, false);
                }
            }
        }
    }
}

// ============================================================================
// Piano Roll Context (for note editing)
// ============================================================================

/// Context for piano roll note operations
///
/// This trait abstracts note manipulation, enabling unit testing
/// of note placement and selection logic.
pub trait PianoRollContext {
    /// Get all notes for the current channel/pattern
    fn notes(&self) -> &[Note];

    /// Add a note
    fn add_note(&mut self, note: Note);

    /// Remove a note by ID, returning it if found
    fn remove_note(&mut self, id: &str) -> Option<Note>;

    /// Find a note at a specific pitch and step
    fn get_note_at(&self, pitch: u8, step: usize) -> Option<&Note> {
        self.notes()
            .iter()
            .find(|n| n.pitch == pitch && n.covers_step(step))
    }

    /// Find all notes in a pitch/step range
    fn get_notes_in_range(
        &self,
        pitch_range: std::ops::RangeInclusive<u8>,
        step_range: std::ops::Range<usize>,
    ) -> Vec<&Note> {
        self.notes()
            .iter()
            .filter(|n| {
                pitch_range.contains(&n.pitch) && step_range.clone().any(|s| n.covers_step(s))
            })
            .collect()
    }

    /// Remove all notes in a range, returning them
    fn remove_notes_in_range(
        &mut self,
        pitch_range: std::ops::RangeInclusive<u8>,
        step_range: std::ops::Range<usize>,
    ) -> Vec<Note> {
        let ids: Vec<String> = self
            .notes()
            .iter()
            .filter(|n| {
                pitch_range.contains(&n.pitch) && step_range.clone().any(|s| n.covers_step(s))
            })
            .map(|n| n.id.clone())
            .collect();

        ids.into_iter()
            .filter_map(|id| self.remove_note(&id))
            .collect()
    }
}

// ============================================================================
// Playlist Context (for arrangement editing)
// ============================================================================

/// Context for playlist arrangement operations
pub trait PlaylistContext {
    /// Get the number of patterns
    fn pattern_count(&self) -> usize;

    /// Check if a placement exists at (pattern_id, bar)
    fn has_placement(&self, pattern_id: usize, bar: usize) -> bool;

    /// Add a placement
    fn add_placement(&mut self, pattern_id: usize, bar: usize);

    /// Remove a placement
    fn remove_placement(&mut self, pattern_id: usize, bar: usize);

    /// Toggle a placement
    fn toggle_placement(&mut self, pattern_id: usize, bar: usize) {
        if self.has_placement(pattern_id, bar) {
            self.remove_placement(pattern_id, bar);
        } else {
            self.add_placement(pattern_id, bar);
        }
    }
}

// ============================================================================
// Cursor Context (for grid navigation)
// ============================================================================

/// Cursor state for a grid-based panel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridCursor {
    pub row: usize,
    pub col: usize,
    pub viewport_top: usize,
}

impl GridCursor {
    pub fn new(row: usize, col: usize) -> Self {
        Self {
            row,
            col,
            viewport_top: 0,
        }
    }

    /// Move cursor, clamping to bounds
    pub fn move_to(&mut self, row: usize, col: usize, max_row: usize, max_col: usize) {
        self.row = row.min(max_row.saturating_sub(1));
        self.col = col.min(max_col.saturating_sub(1));
    }

    /// Update viewport to keep cursor visible
    pub fn update_viewport(&mut self, visible_rows: usize) {
        if self.row >= self.viewport_top + visible_rows {
            self.viewport_top = self.row - visible_rows + 1;
        }
        if self.row < self.viewport_top {
            self.viewport_top = self.row;
        }
    }

    /// Scroll viewport by delta, keeping cursor visible
    pub fn scroll_viewport(&mut self, delta: i32, max_row: usize, visible_rows: usize) {
        if delta > 0 {
            let max_top = max_row.saturating_sub(visible_rows);
            self.viewport_top = (self.viewport_top + delta as usize).min(max_top);
        } else {
            self.viewport_top = self.viewport_top.saturating_sub((-delta) as usize);
        }
        // Keep cursor visible
        if self.row < self.viewport_top {
            self.row = self.viewport_top;
        } else if self.row >= self.viewport_top + visible_rows {
            self.row = self.viewport_top + visible_rows - 1;
        }
    }
}

// ============================================================================
// Mock Implementations for Testing
// ============================================================================

/// Mock implementation of StepGridContext for testing
#[cfg(test)]
pub struct MockStepGrid {
    pub steps: Vec<Vec<bool>>,
    pub pattern_length: usize,
}

#[cfg(test)]
impl MockStepGrid {
    pub fn new(channels: usize, steps: usize) -> Self {
        Self {
            steps: vec![vec![false; steps]; channels],
            pattern_length: steps,
        }
    }
}

#[cfg(test)]
impl StepGridContext for MockStepGrid {
    fn channel_count(&self) -> usize {
        self.steps.len()
    }

    fn pattern_length(&self) -> usize {
        self.pattern_length
    }

    fn get_step(&self, channel: usize, step: usize) -> bool {
        self.steps
            .get(channel)
            .and_then(|ch| ch.get(step))
            .copied()
            .unwrap_or(false)
    }

    fn set_step(&mut self, channel: usize, step: usize, active: bool) {
        if let Some(ch) = self.steps.get_mut(channel) {
            if let Some(st) = ch.get_mut(step) {
                *st = active;
            }
        }
    }
}

/// Mock implementation of PianoRollContext for testing
#[cfg(test)]
#[derive(Default)]
pub struct MockPianoRoll {
    pub notes: Vec<Note>,
}

#[cfg(test)]
impl MockPianoRoll {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
impl PianoRollContext for MockPianoRoll {
    fn notes(&self) -> &[Note] {
        &self.notes
    }

    fn add_note(&mut self, note: Note) {
        self.notes.push(note);
    }

    fn remove_note(&mut self, id: &str) -> Option<Note> {
        if let Some(idx) = self.notes.iter().position(|n| n.id == id) {
            Some(self.notes.remove(idx))
        } else {
            None
        }
    }
}

/// Mock implementation of PlaylistContext for testing
#[cfg(test)]
pub struct MockPlaylist {
    pub placements: std::collections::HashSet<(usize, usize)>, // (pattern_id, bar)
    pub pattern_count: usize,
}

#[cfg(test)]
impl MockPlaylist {
    pub fn new(pattern_count: usize) -> Self {
        Self {
            placements: std::collections::HashSet::new(),
            pattern_count,
        }
    }
}

#[cfg(test)]
impl PlaylistContext for MockPlaylist {
    fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    fn has_placement(&self, pattern_id: usize, bar: usize) -> bool {
        self.placements.contains(&(pattern_id, bar))
    }

    fn add_placement(&mut self, pattern_id: usize, bar: usize) {
        self.placements.insert((pattern_id, bar));
    }

    fn remove_placement(&mut self, pattern_id: usize, bar: usize) {
        self.placements.remove(&(pattern_id, bar));
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Step Grid tests
    #[test]
    fn test_step_grid_toggle() {
        let mut grid = MockStepGrid::new(4, 16);
        assert!(!grid.get_step(0, 0));

        grid.toggle_step(0, 0);
        assert!(grid.get_step(0, 0));

        grid.toggle_step(0, 0);
        assert!(!grid.get_step(0, 0));
    }

    #[test]
    fn test_step_grid_range() {
        let mut grid = MockStepGrid::new(4, 16);
        grid.set_step(0, 0, true);
        grid.set_step(0, 1, true);
        grid.set_step(1, 0, true);

        let data = grid.get_step_range(0..2, 0..2);
        assert_eq!(data, vec![vec![true, true], vec![true, false]]);
    }

    #[test]
    fn test_step_grid_set_range() {
        let mut grid = MockStepGrid::new(4, 16);
        let data = vec![vec![true, false, true], vec![false, true, false]];

        grid.set_step_range(1, 4, &data);

        assert!(grid.get_step(1, 4));
        assert!(!grid.get_step(1, 5));
        assert!(grid.get_step(1, 6));
        assert!(!grid.get_step(2, 4));
        assert!(grid.get_step(2, 5));
    }

    #[test]
    fn test_step_grid_clear_range() {
        let mut grid = MockStepGrid::new(4, 16);
        for ch in 0..4 {
            for st in 0..16 {
                grid.set_step(ch, st, true);
            }
        }

        grid.clear_step_range(1..3, 4..8);

        // Check cleared area
        assert!(!grid.get_step(1, 4));
        assert!(!grid.get_step(2, 7));

        // Check untouched areas
        assert!(grid.get_step(0, 4));
        assert!(grid.get_step(1, 3));
        assert!(grid.get_step(1, 8));
    }

    // Piano Roll tests
    #[test]
    fn test_piano_roll_add_remove() {
        let mut pr = MockPianoRoll::new();
        let note = Note::new(60, 0, 4);
        let id = note.id.clone();

        pr.add_note(note);
        assert_eq!(pr.notes().len(), 1);

        let removed = pr.remove_note(&id);
        assert!(removed.is_some());
        assert_eq!(pr.notes().len(), 0);
    }

    #[test]
    fn test_piano_roll_get_note_at() {
        let mut pr = MockPianoRoll::new();
        pr.add_note(Note::new(60, 4, 4)); // Steps 4-7

        assert!(pr.get_note_at(60, 5).is_some());
        assert!(pr.get_note_at(60, 8).is_none()); // After note
        assert!(pr.get_note_at(61, 5).is_none()); // Different pitch
    }

    #[test]
    fn test_piano_roll_notes_in_range() {
        let mut pr = MockPianoRoll::new();
        pr.add_note(Note::new(60, 0, 4));
        pr.add_note(Note::new(62, 4, 4));
        pr.add_note(Note::new(64, 8, 4));

        let notes = pr.get_notes_in_range(60..=62, 0..8);
        assert_eq!(notes.len(), 2); // Notes at 60 and 62
    }

    // Playlist tests
    #[test]
    fn test_playlist_toggle() {
        let mut pl = MockPlaylist::new(4);

        assert!(!pl.has_placement(0, 0));
        pl.toggle_placement(0, 0);
        assert!(pl.has_placement(0, 0));
        pl.toggle_placement(0, 0);
        assert!(!pl.has_placement(0, 0));
    }

    // Cursor tests
    #[test]
    fn test_cursor_move_clamped() {
        let mut cursor = GridCursor::new(0, 0);
        cursor.move_to(100, 100, 10, 16);
        assert_eq!(cursor.row, 9);
        assert_eq!(cursor.col, 15);
    }

    #[test]
    fn test_cursor_viewport_update() {
        let mut cursor = GridCursor::new(0, 0);
        cursor.row = 20;
        cursor.update_viewport(10);
        assert_eq!(cursor.viewport_top, 11); // 20 - 10 + 1

        cursor.row = 5;
        cursor.update_viewport(10);
        assert_eq!(cursor.viewport_top, 5);
    }

    #[test]
    fn test_cursor_scroll_viewport() {
        let mut cursor = GridCursor::new(5, 0);
        cursor.viewport_top = 0;

        // Scroll down
        cursor.scroll_viewport(3, 100, 10);
        assert_eq!(cursor.viewport_top, 3);
        assert_eq!(cursor.row, 5); // Still visible

        // Scroll down past cursor
        cursor.scroll_viewport(10, 100, 10);
        assert_eq!(cursor.viewport_top, 13);
        assert_eq!(cursor.row, 13); // Moved to stay visible
    }

    // Additional Step Grid tests
    #[test]
    fn test_step_grid_out_of_bounds() {
        let mut grid = MockStepGrid::new(4, 16);
        // Out of bounds reads should return false
        assert!(!grid.get_step(99, 0));
        assert!(!grid.get_step(0, 99));
        // Out of bounds writes should be no-ops (not panic)
        grid.set_step(99, 0, true);
        grid.set_step(0, 99, true);
    }

    #[test]
    fn test_step_grid_copy_paste_pattern() {
        let mut grid = MockStepGrid::new(4, 16);
        // Create a drum pattern
        grid.set_step(0, 0, true); // Kick on 1
        grid.set_step(0, 4, true); // Kick on 2
        grid.set_step(0, 8, true); // Kick on 3
        grid.set_step(0, 12, true); // Kick on 4
        grid.set_step(1, 4, true); // Snare on 2
        grid.set_step(1, 12, true); // Snare on 4

        // Copy steps 0-8 from channels 0-2
        let copied = grid.get_step_range(0..2, 0..8);

        // Paste at channel 2, step 8
        grid.set_step_range(2, 8, &copied);

        // Verify the paste
        assert!(grid.get_step(2, 8)); // Kick pattern copied
        assert!(grid.get_step(2, 12)); // Kick on beat 2 of copied region
        assert!(grid.get_step(3, 12)); // Snare pattern copied
    }

    // Additional Piano Roll tests
    #[test]
    fn test_piano_roll_overlapping_notes() {
        let mut pr = MockPianoRoll::new();
        pr.add_note(Note::new(60, 0, 8)); // Long note covering steps 0-7
        pr.add_note(Note::new(60, 4, 8)); // Overlapping note covering steps 4-11

        // Both notes exist
        assert_eq!(pr.notes().len(), 2);

        // Query at step 5 should find at least one note
        assert!(pr.get_note_at(60, 5).is_some());
    }

    #[test]
    fn test_piano_roll_remove_notes_in_range() {
        let mut pr = MockPianoRoll::new();
        pr.add_note(Note::new(60, 0, 4));
        pr.add_note(Note::new(62, 0, 4));
        pr.add_note(Note::new(64, 0, 4));
        pr.add_note(Note::new(66, 0, 4));

        // Remove notes in middle pitch range
        let removed = pr.remove_notes_in_range(61..=65, 0..4);
        assert_eq!(removed.len(), 2); // Notes at 62 and 64

        // Only notes at 60 and 66 remain
        assert_eq!(pr.notes().len(), 2);
        assert!(pr.get_note_at(60, 0).is_some());
        assert!(pr.get_note_at(66, 0).is_some());
        assert!(pr.get_note_at(62, 0).is_none());
    }

    #[test]
    fn test_piano_roll_chord() {
        let mut pr = MockPianoRoll::new();
        // C major chord: C4 (60), E4 (64), G4 (67)
        pr.add_note(Note::new(60, 0, 4));
        pr.add_note(Note::new(64, 0, 4));
        pr.add_note(Note::new(67, 0, 4));

        // All chord tones should be found at step 2
        let chord_notes = pr.get_notes_in_range(60..=67, 0..4);
        assert_eq!(chord_notes.len(), 3);
    }

    // Additional Playlist tests
    #[test]
    fn test_playlist_multiple_placements() {
        let mut pl = MockPlaylist::new(4);
        pl.add_placement(0, 0); // Pattern 0 at bar 0
        pl.add_placement(0, 4); // Pattern 0 at bar 4
        pl.add_placement(1, 0); // Pattern 1 at bar 0

        assert!(pl.has_placement(0, 0));
        assert!(pl.has_placement(0, 4));
        assert!(pl.has_placement(1, 0));
        assert!(!pl.has_placement(0, 8)); // Not placed
    }

    #[test]
    fn test_playlist_remove_preserves_others() {
        let mut pl = MockPlaylist::new(4);
        pl.add_placement(0, 0);
        pl.add_placement(0, 4);
        pl.add_placement(1, 0);

        pl.remove_placement(0, 0);

        assert!(!pl.has_placement(0, 0)); // Removed
        assert!(pl.has_placement(0, 4)); // Still there
        assert!(pl.has_placement(1, 0)); // Still there
    }

    // Additional Cursor tests
    #[test]
    fn test_cursor_scroll_up_clamps() {
        let mut cursor = GridCursor::new(5, 0);
        cursor.viewport_top = 5;
        cursor.row = 5;

        // Scroll up past beginning
        cursor.scroll_viewport(-10, 100, 10);
        assert_eq!(cursor.viewport_top, 0);
        assert_eq!(cursor.row, 5); // Row still valid, above viewport
    }

    #[test]
    fn test_cursor_scroll_down_clamps() {
        let mut cursor = GridCursor::new(90, 0);
        cursor.viewport_top = 85;
        cursor.row = 90;

        // Scroll down past end (max_row=100, visible=10, max_top=90)
        cursor.scroll_viewport(10, 100, 10);
        assert_eq!(cursor.viewport_top, 90);
        assert_eq!(cursor.row, 90);
    }

    #[test]
    fn test_cursor_keeps_cursor_visible_on_scroll_up() {
        let mut cursor = GridCursor::new(15, 0);
        cursor.viewport_top = 10;
        cursor.row = 15;

        // Scroll up - cursor moves to stay in viewport (5..14)
        cursor.scroll_viewport(-5, 100, 10);
        assert_eq!(cursor.viewport_top, 5);
        assert_eq!(cursor.row, 14); // Moved to bottom of viewport
    }
}
