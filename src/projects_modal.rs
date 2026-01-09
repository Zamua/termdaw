//! Projects modal - list and open projects interactively
//!
//! Shows a list of available projects that can be selected with j/k and opened with Enter.
//! Supports operations: new (n), save (s), save as (a), rename (r), delete (d).

use std::fs;
use std::path::PathBuf;

use tui_input::Input;

/// Action to perform after text input is confirmed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    /// Create a new project with the entered name
    NewProject,
    /// Save current project as a new name
    SaveAs,
    /// Rename the selected project
    RenameProject,
}

/// Action to perform after confirmation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    /// Delete the selected project
    DeleteProject { name: String },
}

/// Modal mode - determines what the modal is currently doing
#[derive(Debug, Clone, Default)]
pub enum ModalMode {
    /// Browsing project list (j/k to navigate, Enter to open)
    #[default]
    Browse,
    /// Entering text for an operation
    TextInput {
        /// Prompt to display
        prompt: &'static str,
        /// The input field state
        input: Input,
        /// What to do when confirmed
        action: InputAction,
    },
    /// Confirmation dialog (y/n)
    Confirm {
        /// Message to display
        message: String,
        /// What to do when confirmed
        action: ConfirmAction,
    },
}

/// Projects modal state
#[derive(Debug, Clone, Default)]
pub struct ProjectsModal {
    /// Whether the modal is visible
    pub visible: bool,
    /// Current mode (browse, text input, or confirm)
    pub mode: ModalMode,
    /// List of available project names
    pub projects: Vec<String>,
    /// Currently selected index
    pub selected: usize,
    /// The projects directory
    pub projects_dir: PathBuf,
}

impl ProjectsModal {
    /// Create a new projects modal
    pub fn new() -> Self {
        Self::default()
    }

    /// Show the modal and refresh the project list
    /// If current_project is provided, it will be selected by default
    pub fn show(&mut self, current_project: Option<&str>) {
        self.visible = true;
        self.refresh_projects();

        // Find and select the current project, or default to 0
        self.selected = current_project
            .and_then(|name| self.projects.iter().position(|p| p == name))
            .unwrap_or(0);
    }

    /// Hide the modal and reset to browse mode
    pub fn hide(&mut self) {
        self.visible = false;
        self.mode = ModalMode::Browse;
    }

    /// Enter text input mode for creating a new project
    pub fn start_new_project(&mut self) {
        self.mode = ModalMode::TextInput {
            prompt: "New project name:",
            input: Input::default(),
            action: InputAction::NewProject,
        };
    }

    /// Enter text input mode for save as
    pub fn start_save_as(&mut self) {
        self.mode = ModalMode::TextInput {
            prompt: "Save as:",
            input: Input::default(),
            action: InputAction::SaveAs,
        };
    }

    /// Enter text input mode for renaming the selected project
    pub fn start_rename(&mut self) {
        if let Some(current_name) = self.selected_project() {
            self.mode = ModalMode::TextInput {
                prompt: "Rename to:",
                input: Input::default().with_value(current_name.to_string()),
                action: InputAction::RenameProject,
            };
        }
    }

    /// Enter confirmation mode for deleting the selected project
    pub fn start_delete(&mut self) {
        if let Some(name) = self.selected_project() {
            let name = name.to_string();
            self.mode = ModalMode::Confirm {
                message: format!("Delete '{}'?", name),
                action: ConfirmAction::DeleteProject { name },
            };
        }
    }

    /// Cancel current mode and return to browse
    pub fn cancel(&mut self) {
        self.mode = ModalMode::Browse;
    }

    /// Check if currently in browse mode
    pub fn is_browse_mode(&self) -> bool {
        matches!(self.mode, ModalMode::Browse)
    }

    /// Check if currently in text input mode
    pub fn is_text_input_mode(&self) -> bool {
        matches!(self.mode, ModalMode::TextInput { .. })
    }

    /// Check if currently in confirm mode
    pub fn is_confirm_mode(&self) -> bool {
        matches!(self.mode, ModalMode::Confirm { .. })
    }

    /// Get mutable reference to input if in text input mode
    pub fn input_mut(&mut self) -> Option<&mut Input> {
        match &mut self.mode {
            ModalMode::TextInput { input, .. } => Some(input),
            _ => None,
        }
    }

    /// Get the current input value if in text input mode
    pub fn input_value(&self) -> Option<&str> {
        match &self.mode {
            ModalMode::TextInput { input, .. } => Some(input.value()),
            _ => None,
        }
    }

    /// Get the current action if in text input mode
    pub fn input_action(&self) -> Option<&InputAction> {
        match &self.mode {
            ModalMode::TextInput { action, .. } => Some(action),
            _ => None,
        }
    }

    /// Get the confirm action if in confirm mode
    pub fn confirm_action(&self) -> Option<&ConfirmAction> {
        match &self.mode {
            ModalMode::Confirm { action, .. } => Some(action),
            _ => None,
        }
    }

    /// Refresh the list of projects from the projects directory
    pub fn refresh_projects(&mut self) {
        self.projects_dir = crate::templates::projects_dir();

        self.projects = if self.projects_dir.exists() {
            let mut projects: Vec<_> = fs::read_dir(&self.projects_dir)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().join("project.json").exists())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            projects.sort();
            projects
        } else {
            Vec::new()
        };
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if !self.projects.is_empty() {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if !self.projects.is_empty() {
            self.selected = (self.selected + 1).min(self.projects.len() - 1);
        }
    }

    /// Get the currently selected project name
    pub fn selected_project(&self) -> Option<&str> {
        self.projects.get(self.selected).map(|s| s.as_str())
    }

    /// Get the full path to the selected project
    pub fn selected_project_path(&self) -> Option<PathBuf> {
        self.selected_project()
            .map(|name| self.projects_dir.join(name))
    }
}
