//! History module for undo/redo and jump list functionality
//!
//! This module provides:
//! - `History`: Undo/redo stack using the Command pattern
//! - `GlobalJumplist`: Cross-view position history for Ctrl+O/Ctrl+I navigation
//! - `Command` trait: Interface for reversible operations

// Allow dead code - these types define a complete API for future use
#![allow(dead_code)]

pub mod command;
pub mod jumplist;

#[cfg(test)]
mod tests;

pub use command::Command;
pub use jumplist::{GlobalJumplist, JumpPosition};

use crate::app::App;

/// Maximum number of commands to keep in history
const MAX_HISTORY_SIZE: usize = 100;

/// Undo/redo history stack
///
/// Maintains two stacks:
/// - `undo_stack`: Commands that can be undone
/// - `redo_stack`: Commands that have been undone and can be redone
///
/// When a new command is executed, it's pushed to the undo stack and
/// the redo stack is cleared (standard vim behavior).
pub struct History {
    /// Commands that can be undone (most recent at end)
    undo_stack: Vec<Box<dyn Command>>,
    /// Commands that can be redone (most recent at end)
    redo_stack: Vec<Box<dyn Command>>,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    /// Create a new empty history
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Execute a command and add it to the undo stack
    ///
    /// This clears the redo stack (can't redo after a new action).
    pub fn execute(&mut self, mut cmd: Box<dyn Command>, app: &mut App) {
        cmd.execute(app);
        self.undo_stack.push(cmd);
        self.redo_stack.clear();

        // Enforce max size
        if self.undo_stack.len() > MAX_HISTORY_SIZE {
            self.undo_stack.remove(0);
        }
    }

    /// Undo the last command
    ///
    /// Returns `true` if an action was undone, `false` if the stack was empty.
    pub fn undo(&mut self, app: &mut App) -> bool {
        if let Some(mut cmd) = self.undo_stack.pop() {
            cmd.undo(app);
            self.redo_stack.push(cmd);
            true
        } else {
            false
        }
    }

    /// Redo the last undone command
    ///
    /// Returns `true` if an action was redone, `false` if the redo stack was empty.
    pub fn redo(&mut self, app: &mut App) -> bool {
        if let Some(mut cmd) = self.redo_stack.pop() {
            cmd.redo(app);
            self.undo_stack.push(cmd);
            true
        } else {
            false
        }
    }

    /// Check if there are commands to undo
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if there are commands to redo
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Get the number of commands in the undo stack
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Get the number of commands in the redo stack
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }

    /// Get a description of the last undoable command
    pub fn last_undo_description(&self) -> Option<&str> {
        self.undo_stack.last().map(|cmd| cmd.description())
    }

    /// Get a description of the last redoable command
    pub fn last_redo_description(&self) -> Option<&str> {
        self.redo_stack.last().map(|cmd| cmd.description())
    }
}
