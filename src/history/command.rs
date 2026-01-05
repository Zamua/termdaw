//! Command trait and implementations for undo/redo operations
//!
//! Each command is self-contained, storing all data needed to undo
//! without querying external state.

use crate::app::App;
use crate::arrangement::PatternPlacement;
use crate::input::context::StepGridContext;
use crate::sequencer::Note;

/// A reversible command that mutates App state
///
/// Commands must be self-contained: they store the data needed to
/// reverse the operation (e.g., the previous value before a change).
pub trait Command: std::fmt::Debug + Send {
    /// Execute the command, modifying app state
    fn execute(&mut self, app: &mut App);

    /// Reverse the command (undo)
    fn undo(&mut self, app: &mut App);

    /// Re-execute after undo (redo)
    ///
    /// Default implementation just calls execute, but some commands
    /// may need special handling.
    fn redo(&mut self, app: &mut App) {
        self.execute(app);
    }

    /// Human-readable description for status display
    fn description(&self) -> &str;
}

// ============================================================================
// Channel Rack Commands
// ============================================================================

/// Toggle a single step in the channel rack
#[derive(Debug)]
pub struct ToggleStepCmd {
    pub pattern_id: usize,
    pub channel: usize,
    pub step: usize,
    /// The state before the toggle (captured on first execute)
    was_active: Option<bool>,
}

impl ToggleStepCmd {
    pub fn new(pattern_id: usize, channel: usize, step: usize) -> Self {
        Self {
            pattern_id,
            channel,
            step,
            was_active: None,
        }
    }
}

impl Command for ToggleStepCmd {
    fn execute(&mut self, app: &mut App) {
        let pattern_length = app.pattern_length();
        if let Some(ch) = app.channels.get_mut(self.channel) {
            let slice = ch.get_or_create_pattern(self.pattern_id, pattern_length);
            // Capture the current state before toggling
            if self.was_active.is_none() {
                self.was_active = Some(slice.get_step(self.step));
            }
            slice.toggle_step(self.step);
            app.mark_dirty();
        }
    }

    fn undo(&mut self, app: &mut App) {
        let pattern_length = app.pattern_length();
        if let Some(ch) = app.channels.get_mut(self.channel) {
            let slice = ch.get_or_create_pattern(self.pattern_id, pattern_length);
            if let Some(was_active) = self.was_active {
                slice.set_step(self.step, was_active);
                app.mark_dirty();
            }
        }
    }

    fn description(&self) -> &str {
        "Toggle step"
    }
}

/// Delete steps in a range (for vim delete operations)
#[derive(Debug)]
pub struct DeleteStepsCmd {
    pub pattern_id: usize,
    /// The deleted data: Vec<(channel, step, was_active)>
    deleted_steps: Vec<(usize, usize, bool)>,
}

impl DeleteStepsCmd {
    pub fn new(pattern_id: usize) -> Self {
        Self {
            pattern_id,
            deleted_steps: Vec::new(),
        }
    }

    /// Add a step to be deleted (call before execute)
    pub fn add_step(&mut self, channel: usize, step: usize, was_active: bool) {
        self.deleted_steps.push((channel, step, was_active));
    }
}

impl Command for DeleteStepsCmd {
    fn execute(&mut self, app: &mut App) {
        let pattern_length = app.pattern_length();
        for &(channel, step, _) in &self.deleted_steps {
            if let Some(ch) = app.channels.get_mut(channel) {
                let slice = ch.get_or_create_pattern(self.pattern_id, pattern_length);
                slice.set_step(step, false);
            }
        }
        app.mark_dirty();
    }

    fn undo(&mut self, app: &mut App) {
        let pattern_length = app.pattern_length();
        for &(channel, step, was_active) in &self.deleted_steps {
            if let Some(ch) = app.channels.get_mut(channel) {
                let slice = ch.get_or_create_pattern(self.pattern_id, pattern_length);
                slice.set_step(step, was_active);
            }
        }
        app.mark_dirty();
    }

    fn description(&self) -> &str {
        "Delete steps"
    }
}

// ============================================================================
// Piano Roll Commands
// ============================================================================

/// Add a note to the piano roll
#[derive(Debug)]
pub struct AddNoteCmd {
    pub pattern_id: usize,
    pub channel: usize,
    pub note: Note,
}

impl AddNoteCmd {
    pub fn new(pattern_id: usize, channel: usize, note: Note) -> Self {
        Self {
            pattern_id,
            channel,
            note,
        }
    }
}

impl Command for AddNoteCmd {
    fn execute(&mut self, app: &mut App) {
        let pattern_length = app.pattern_length();
        if let Some(ch) = app.channels.get_mut(self.channel) {
            let slice = ch.get_or_create_pattern(self.pattern_id, pattern_length);
            slice.add_note(self.note.clone());
            app.mark_dirty();
        }
    }

    fn undo(&mut self, app: &mut App) {
        if let Some(ch) = app.channels.get_mut(self.channel) {
            if let Some(slice) = ch.get_pattern_mut(self.pattern_id) {
                slice.remove_note(&self.note.id);
                app.mark_dirty();
            }
        }
    }

    fn description(&self) -> &str {
        "Add note"
    }
}

/// Remove a note from the piano roll
#[derive(Debug)]
pub struct RemoveNoteCmd {
    pub pattern_id: usize,
    pub channel: usize,
    /// The removed note (captured during execute)
    removed_note: Option<Note>,
    /// The note ID to remove
    note_id: String,
}

impl RemoveNoteCmd {
    pub fn new(pattern_id: usize, channel: usize, note_id: String) -> Self {
        Self {
            pattern_id,
            channel,
            removed_note: None,
            note_id,
        }
    }

    /// Create from an existing note (when we already have the note data)
    pub fn from_note(pattern_id: usize, channel: usize, note: Note) -> Self {
        Self {
            pattern_id,
            channel,
            note_id: note.id.clone(),
            removed_note: Some(note),
        }
    }
}

impl Command for RemoveNoteCmd {
    fn execute(&mut self, app: &mut App) {
        if let Some(ch) = app.channels.get_mut(self.channel) {
            if let Some(slice) = ch.get_pattern_mut(self.pattern_id) {
                if let Some(note) = slice.remove_note(&self.note_id) {
                    self.removed_note = Some(note);
                    app.mark_dirty();
                }
            }
        }
    }

    fn undo(&mut self, app: &mut App) {
        let pattern_length = app.pattern_length();
        if let Some(ch) = app.channels.get_mut(self.channel) {
            let slice = ch.get_or_create_pattern(self.pattern_id, pattern_length);
            if let Some(note) = self.removed_note.take() {
                slice.add_note(note.clone());
                self.removed_note = Some(note);
                app.mark_dirty();
            }
        }
    }

    fn description(&self) -> &str {
        "Remove note"
    }
}

/// Delete multiple notes (for vim delete operations)
#[derive(Debug)]
pub struct DeleteNotesCmd {
    pub pattern_id: usize,
    pub channel: usize,
    /// The deleted notes
    deleted_notes: Vec<Note>,
}

impl DeleteNotesCmd {
    pub fn new(pattern_id: usize, channel: usize, notes: Vec<Note>) -> Self {
        Self {
            pattern_id,
            channel,
            deleted_notes: notes,
        }
    }
}

impl Command for DeleteNotesCmd {
    fn execute(&mut self, app: &mut App) {
        if let Some(ch) = app.channels.get_mut(self.channel) {
            if let Some(slice) = ch.get_pattern_mut(self.pattern_id) {
                for note in &self.deleted_notes {
                    slice.remove_note(&note.id);
                }
                app.mark_dirty();
            }
        }
    }

    fn undo(&mut self, app: &mut App) {
        let pattern_length = app.pattern_length();
        if let Some(ch) = app.channels.get_mut(self.channel) {
            let slice = ch.get_or_create_pattern(self.pattern_id, pattern_length);
            for note in &self.deleted_notes {
                slice.add_note(note.clone());
            }
            app.mark_dirty();
        }
    }

    fn description(&self) -> &str {
        "Delete notes"
    }
}

// ============================================================================
// Playlist Commands
// ============================================================================

/// Toggle a placement in the playlist
#[derive(Debug)]
pub struct TogglePlacementCmd {
    pub pattern_id: usize,
    pub bar: usize,
    /// If we removed a placement, store it here for undo
    removed_placement: Option<PatternPlacement>,
    /// If we added a placement, store its ID for undo
    added_placement_id: Option<String>,
}

impl TogglePlacementCmd {
    pub fn new(pattern_id: usize, bar: usize) -> Self {
        Self {
            pattern_id,
            bar,
            removed_placement: None,
            added_placement_id: None,
        }
    }
}

impl Command for TogglePlacementCmd {
    fn execute(&mut self, app: &mut App) {
        // Check if there's an existing placement at this position
        let existing = app
            .arrangement
            .placements
            .iter()
            .position(|p| p.pattern_id == self.pattern_id && p.start_bar == self.bar);

        if let Some(idx) = existing {
            // Remove existing placement
            self.removed_placement = Some(app.arrangement.placements.remove(idx));
            self.added_placement_id = None;
        } else {
            // Add new placement
            let placement = PatternPlacement::new(self.pattern_id, self.bar);
            self.added_placement_id = Some(placement.id.clone());
            app.arrangement.placements.push(placement);
            self.removed_placement = None;
        }
        app.mark_dirty();
    }

    fn undo(&mut self, app: &mut App) {
        if let Some(ref placement) = self.removed_placement {
            // We removed a placement, so add it back
            app.arrangement.placements.push(placement.clone());
        } else if let Some(ref id) = self.added_placement_id {
            // We added a placement, so remove it
            app.arrangement.remove_placement(id);
        }
        app.mark_dirty();
    }

    fn description(&self) -> &str {
        "Toggle placement"
    }
}

/// Delete placements in a range
#[derive(Debug)]
pub struct DeletePlacementsCmd {
    pub pattern_id: usize,
    pub start_bar: usize,
    pub end_bar: usize,
    /// The deleted placements
    deleted_placements: Vec<PatternPlacement>,
}

impl DeletePlacementsCmd {
    pub fn new(pattern_id: usize, start_bar: usize, end_bar: usize) -> Self {
        Self {
            pattern_id,
            start_bar,
            end_bar,
            deleted_placements: Vec::new(),
        }
    }
}

impl Command for DeletePlacementsCmd {
    fn execute(&mut self, app: &mut App) {
        self.deleted_placements =
            app.arrangement
                .remove_placements_in_range(self.pattern_id, self.start_bar, self.end_bar);
        app.mark_dirty();
    }

    fn undo(&mut self, app: &mut App) {
        for placement in &self.deleted_placements {
            app.arrangement.placements.push(placement.clone());
        }
        app.mark_dirty();
    }

    fn description(&self) -> &str {
        "Delete placements"
    }
}

// ============================================================================
// Batch Command (for grouping operations)
// ============================================================================

/// A command that groups multiple commands into one undo step
#[derive(Debug)]
pub struct BatchCmd {
    pub commands: Vec<Box<dyn Command>>,
    pub desc: String,
}

impl BatchCmd {
    pub fn new(desc: impl Into<String>) -> Self {
        Self {
            commands: Vec::new(),
            desc: desc.into(),
        }
    }

    /// Add a command to the batch
    pub fn push(&mut self, cmd: Box<dyn Command>) {
        self.commands.push(cmd);
    }

    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl Command for BatchCmd {
    fn execute(&mut self, app: &mut App) {
        for cmd in &mut self.commands {
            cmd.execute(app);
        }
    }

    fn undo(&mut self, app: &mut App) {
        // Undo in reverse order
        for cmd in self.commands.iter_mut().rev() {
            cmd.undo(app);
        }
    }

    fn redo(&mut self, app: &mut App) {
        for cmd in &mut self.commands {
            cmd.redo(app);
        }
    }

    fn description(&self) -> &str {
        &self.desc
    }
}
