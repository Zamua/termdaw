//! Projects modal - list and open projects interactively
//!
//! Shows a list of available projects that can be selected with j/k and opened with Enter.

use std::fs;
use std::path::PathBuf;

/// Projects modal state
#[derive(Debug, Clone, Default)]
pub struct ProjectsModal {
    /// Whether the modal is visible
    pub visible: bool,
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

    /// Hide the modal
    pub fn hide(&mut self) {
        self.visible = false;
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
