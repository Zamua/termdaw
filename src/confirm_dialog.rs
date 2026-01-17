//! Reusable confirmation dialog
//!
//! A simple modal dialog that asks for confirmation before performing an action.
//! Used for destructive operations like clearing patterns, deleting items, etc.

use crate::command::AppCommand;

/// Action to perform when the user confirms
#[derive(Debug, Clone)]
pub enum ConfirmAction {
    /// Clear all data from a pattern
    ClearPattern(usize),
}

impl ConfirmAction {
    /// Convert to the corresponding AppCommand
    pub fn to_command(&self) -> AppCommand {
        match self {
            ConfirmAction::ClearPattern(id) => AppCommand::ClearPattern(*id),
        }
    }
}

/// Reusable confirmation dialog state
#[derive(Debug, Clone, Default)]
pub struct ConfirmDialog {
    /// Whether the dialog is visible
    pub visible: bool,
    /// Message to display
    pub message: String,
    /// Action to perform on confirmation
    pub action: Option<ConfirmAction>,
}

impl ConfirmDialog {
    /// Create a new confirm dialog
    pub fn new() -> Self {
        Self::default()
    }

    /// Show the dialog with a message and action
    pub fn show(&mut self, message: impl Into<String>, action: ConfirmAction) {
        self.visible = true;
        self.message = message.into();
        self.action = Some(action);
    }

    /// Hide the dialog
    pub fn hide(&mut self) {
        self.visible = false;
        self.message.clear();
        self.action = None;
    }

    /// Confirm the action and return the command to execute
    pub fn confirm(&mut self) -> Option<AppCommand> {
        let cmd = self.action.as_ref().map(|a| a.to_command());
        self.hide();
        cmd
    }

    /// Cancel without executing
    pub fn cancel(&mut self) {
        self.hide();
    }
}
