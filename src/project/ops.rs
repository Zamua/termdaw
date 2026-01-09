//! Project operations - create, rename, copy, delete projects
//!
//! These operations work at the directory level, managing entire project folders.

use std::path::{Path, PathBuf};

use super::ProjectError;

/// Create a new project with optional name
///
/// If name is None, generates an untitled-N name.
/// Returns the path to the created project.
pub fn new_project(name: Option<&str>) -> Result<PathBuf, ProjectError> {
    new_project_in(name, &crate::templates::projects_dir())
}

/// Create a new project in a specific directory (for testing)
pub fn new_project_in(_name: Option<&str>, _projects_dir: &Path) -> Result<PathBuf, ProjectError> {
    todo!()
}

/// Copy a project to a new name (save as)
///
/// Creates a copy of the source project with the new name.
/// Returns the path to the new project.
pub fn save_project_as(from: &Path, new_name: &str) -> Result<PathBuf, ProjectError> {
    save_project_as_in(from, new_name, &crate::templates::projects_dir())
}

/// Copy a project to a new name in a specific directory (for testing)
pub fn save_project_as_in(
    _from: &Path,
    _new_name: &str,
    _projects_dir: &Path,
) -> Result<PathBuf, ProjectError> {
    todo!()
}

/// Rename a project
///
/// Renames the project directory and updates project.json.
/// Returns the new path.
pub fn rename_project(path: &Path, new_name: &str) -> Result<PathBuf, ProjectError> {
    rename_project_in(path, new_name, &crate::templates::projects_dir())
}

/// Rename a project in a specific directory (for testing)
pub fn rename_project_in(
    _path: &Path,
    _new_name: &str,
    _projects_dir: &Path,
) -> Result<PathBuf, ProjectError> {
    todo!()
}

/// Delete a project permanently
///
/// Removes the entire project directory.
pub fn delete_project(_path: &Path) -> Result<(), ProjectError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{load_project, save_project};
    use tempfile::TempDir;

    /// Helper to create a test project in a temp directory
    fn create_test_project(dir: &Path, name: &str) -> PathBuf {
        let project_path = dir.join(name);
        std::fs::create_dir_all(&project_path).unwrap();
        std::fs::create_dir_all(project_path.join("samples")).unwrap();

        let project = crate::project::ProjectFile::new(name);
        save_project(&project_path, &project).unwrap();

        project_path
    }

    #[test]
    fn test_new_project_creates_directory() {
        let temp_dir = TempDir::new().unwrap();

        let result = new_project_in(Some("test-project"), temp_dir.path());

        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.join("project.json").exists());
    }

    #[test]
    fn test_new_project_with_custom_name() {
        let temp_dir = TempDir::new().unwrap();

        let result = new_project_in(Some("my-custom-project"), temp_dir.path());

        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with("my-custom-project"));

        // Load and verify name in project.json
        let project = load_project(&path).unwrap();
        assert_eq!(project.name, "my-custom-project");
    }

    #[test]
    fn test_new_project_fails_if_exists() {
        let temp_dir = TempDir::new().unwrap();

        // Create a project first
        new_project_in(Some("existing"), temp_dir.path()).unwrap();

        // Try to create with same name
        let result = new_project_in(Some("existing"), temp_dir.path());

        assert!(result.is_err());
        assert!(matches!(result, Err(ProjectError::AlreadyExists(_))));
    }

    #[test]
    fn test_save_as_creates_copy() {
        let temp_dir = TempDir::new().unwrap();

        // Create original project
        let original = create_test_project(temp_dir.path(), "original");

        let result = save_project_as_in(&original, "copy", temp_dir.path());

        assert!(result.is_ok());
        let copy_path = result.unwrap();
        assert!(copy_path.exists());
        assert!(copy_path.join("project.json").exists());

        // Original should still exist
        assert!(original.exists());

        // Copy should have updated name
        let copy_project = load_project(&copy_path).unwrap();
        assert_eq!(copy_project.name, "copy");
    }

    #[test]
    fn test_save_as_fails_if_name_exists() {
        let temp_dir = TempDir::new().unwrap();

        let original = create_test_project(temp_dir.path(), "original");
        create_test_project(temp_dir.path(), "existing");

        let result = save_project_as_in(&original, "existing", temp_dir.path());

        assert!(result.is_err());
        assert!(matches!(result, Err(ProjectError::AlreadyExists(_))));
    }

    #[test]
    fn test_rename_moves_directory() {
        let temp_dir = TempDir::new().unwrap();

        let original = create_test_project(temp_dir.path(), "old-name");

        let result = rename_project_in(&original, "new-name", temp_dir.path());

        assert!(result.is_ok());
        let new_path = result.unwrap();

        // Old path should not exist
        assert!(!original.exists());

        // New path should exist
        assert!(new_path.exists());
        assert!(new_path.ends_with("new-name"));
    }

    #[test]
    fn test_rename_updates_project_json() {
        let temp_dir = TempDir::new().unwrap();

        let original = create_test_project(temp_dir.path(), "old-name");

        let new_path = rename_project_in(&original, "new-name", temp_dir.path()).unwrap();

        let project = load_project(&new_path).unwrap();
        assert_eq!(project.name, "new-name");
    }

    #[test]
    fn test_rename_fails_if_name_exists() {
        let temp_dir = TempDir::new().unwrap();

        let original = create_test_project(temp_dir.path(), "original");
        create_test_project(temp_dir.path(), "existing");

        let result = rename_project_in(&original, "existing", temp_dir.path());

        assert!(result.is_err());
        assert!(matches!(result, Err(ProjectError::AlreadyExists(_))));
    }

    #[test]
    fn test_delete_removes_directory() {
        let temp_dir = TempDir::new().unwrap();

        let project = create_test_project(temp_dir.path(), "to-delete");
        assert!(project.exists());

        let result = delete_project(&project);

        assert!(result.is_ok());
        assert!(!project.exists());
    }

    #[test]
    fn test_delete_fails_if_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let fake_path = temp_dir.path().join("nonexistent");

        let result = delete_project(&fake_path);

        assert!(result.is_err());
        assert!(matches!(result, Err(ProjectError::NotFound(_))));
    }
}
