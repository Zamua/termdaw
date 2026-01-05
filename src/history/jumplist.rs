//! Global cross-view jump list for Ctrl+O/Ctrl+I navigation
//!
//! Unlike the per-VimState jumplist, this tracks positions across all views
//! (channel rack, piano roll, playlist) enabling cross-view jump navigation.

use crate::mode::ViewMode;

/// Maximum number of positions to keep in the jump list
const MAX_JUMPLIST_SIZE: usize = 100;

/// A position in the app that can be jumped to
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpPosition {
    /// Which view this position is in
    pub view: ViewMode,
    /// Row coordinate (channel/pitch-row/pattern-row depending on view)
    pub row: usize,
    /// Column coordinate (step/step/bar depending on view)
    pub col: usize,
}

impl JumpPosition {
    /// Create a new jump position
    pub fn new(view: ViewMode, row: usize, col: usize) -> Self {
        Self { view, row, col }
    }

    /// Create a position for channel rack
    pub fn channel_rack(channel: usize, step: usize) -> Self {
        Self::new(ViewMode::ChannelRack, channel, step)
    }

    /// Create a position for piano roll
    pub fn piano_roll(pitch_row: usize, step: usize) -> Self {
        Self::new(ViewMode::PianoRoll, pitch_row, step)
    }

    /// Create a position for playlist
    pub fn playlist(pattern_row: usize, bar: usize) -> Self {
        Self::new(ViewMode::Playlist, pattern_row, bar)
    }
}

/// Global jump list for cross-view navigation
///
/// Tracks cursor positions across all views, enabling Ctrl+O (back)
/// and Ctrl+I (forward) navigation similar to vim's jumplist.
///
/// ## How it works:
/// - When you make a "jump" (view switch, gg, G, etc.), the current position
///   is pushed to the jumplist
/// - Ctrl+O navigates backward through the history
/// - Ctrl+I navigates forward through positions you've jumped back from
/// - Making a new jump after going back truncates the forward history
#[derive(Debug, Default)]
pub struct GlobalJumplist {
    /// Stack of positions (oldest at index 0, newest at end)
    positions: Vec<JumpPosition>,
    /// Current index in the jumplist
    /// -1 means we're at the current position (beyond the end of the list)
    /// 0+ means we're at that index in the positions vec
    index: isize,
}

impl GlobalJumplist {
    /// Create a new empty jumplist
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a jump from the current position
    ///
    /// Call this when:
    /// - Switching views (gt/gT or clicking tabs)
    /// - Making large motions (gg, G)
    /// - Any other action that should be a "jump point"
    pub fn push(&mut self, pos: JumpPosition) {
        // Avoid duplicate adjacent entries
        if self.positions.last() == Some(&pos) {
            return;
        }

        // If we've navigated back and are now pushing a new position,
        // truncate the forward history
        if self.index >= 0 {
            let keep = (self.index + 1) as usize;
            self.positions.truncate(keep);
        }

        self.positions.push(pos);
        self.index = -1; // Reset to "current" position

        // Enforce max size
        if self.positions.len() > MAX_JUMPLIST_SIZE {
            self.positions.remove(0);
        }
    }

    /// Go back to the previous position (Ctrl+O)
    ///
    /// Returns the position to jump to, or None if at the beginning.
    /// The `current` parameter is the current position before jumping,
    /// which gets saved if we're jumping back for the first time.
    pub fn go_back(&mut self, current: JumpPosition) -> Option<JumpPosition> {
        if self.positions.is_empty() {
            return None;
        }

        if self.index == -1 {
            // First time going back - save current position
            // but only if it's different from the last saved position
            if self.positions.last() != Some(&current) {
                self.positions.push(current);
            }
            // Go to the second-to-last position (last is where we just were)
            self.index = (self.positions.len() as isize) - 2;
        } else if self.index > 0 {
            self.index -= 1;
        } else {
            // Already at the beginning
            return None;
        }

        if self.index >= 0 {
            self.positions.get(self.index as usize).cloned()
        } else {
            None
        }
    }

    /// Go forward to the next position (Ctrl+I)
    ///
    /// Returns the position to jump to, or None if at the end.
    pub fn go_forward(&mut self) -> Option<JumpPosition> {
        if self.index < 0 || self.positions.is_empty() {
            return None;
        }

        let max_index = (self.positions.len() as isize) - 1;
        if self.index < max_index {
            self.index += 1;
            let pos = self.positions.get(self.index as usize).cloned();

            // If we've returned to the end, reset index to -1
            if self.index == max_index {
                self.index = -1;
            }

            pos
        } else {
            None
        }
    }

    /// Check if we can go back
    pub fn can_go_back(&self) -> bool {
        !self.positions.is_empty() && (self.index == -1 || self.index > 0)
    }

    /// Check if we can go forward
    pub fn can_go_forward(&self) -> bool {
        self.index >= 0 && (self.index as usize) < self.positions.len().saturating_sub(1)
    }

    /// Get the number of positions in the jumplist
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Check if the jumplist is empty
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Clear all positions
    pub fn clear(&mut self) {
        self.positions.clear();
        self.index = -1;
    }
}
