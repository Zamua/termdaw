//! Project operations - create, rename, copy, delete projects
//!
//! These operations work at the directory level, managing entire project folders.

use std::path::{Path, PathBuf};

use super::{load_project, save_project, ProjectError};

/// Create a new project with optional name
///
/// If name is None, generates an untitled-N name.
/// Returns the path to the created project.
pub fn new_project(name: Option<&str>) -> Result<PathBuf, ProjectError> {
    new_project_in(name, &crate::templates::projects_dir())
}

/// Create a new project in a specific directory (for testing)
pub fn new_project_in(name: Option<&str>, projects_dir: &Path) -> Result<PathBuf, ProjectError> {
    let project_name = name
        .map(|s| s.to_string())
        .unwrap_or_else(super::generate_project_name);

    let project_path = projects_dir.join(&project_name);

    if project_path.exists() {
        return Err(ProjectError::AlreadyExists(project_name));
    }

    // Copy template if available, otherwise create empty project
    super::copy_template(&project_path)?;

    // Update project name in project.json if it exists
    if let Ok(mut project) = load_project(&project_path) {
        project.name = project_name.clone();
        save_project(&project_path, &project)?;
    } else {
        // No template, create new project file
        super::create_project(&project_path, &project_name)?;
    }

    Ok(project_path)
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
    from: &Path,
    new_name: &str,
    projects_dir: &Path,
) -> Result<PathBuf, ProjectError> {
    if !from.exists() {
        return Err(ProjectError::NotFound(from.display().to_string()));
    }

    let new_path = projects_dir.join(new_name);

    if new_path.exists() {
        return Err(ProjectError::AlreadyExists(new_name.to_string()));
    }

    // Copy the entire directory
    copy_dir_recursive(from, &new_path)?;

    // Update the name in project.json
    if let Ok(mut project) = load_project(&new_path) {
        project.name = new_name.to_string();
        save_project(&new_path, &project)?;
    }

    Ok(new_path)
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
    path: &Path,
    new_name: &str,
    projects_dir: &Path,
) -> Result<PathBuf, ProjectError> {
    if !path.exists() {
        return Err(ProjectError::NotFound(path.display().to_string()));
    }

    let new_path = projects_dir.join(new_name);

    if new_path.exists() {
        return Err(ProjectError::AlreadyExists(new_name.to_string()));
    }

    // Rename the directory
    std::fs::rename(path, &new_path)?;

    // Update the name in project.json
    if let Ok(mut project) = load_project(&new_path) {
        project.name = new_name.to_string();
        save_project(&new_path, &project)?;
    }

    Ok(new_path)
}

/// Delete a project permanently
///
/// Removes the entire project directory.
pub fn delete_project(path: &Path) -> Result<(), ProjectError> {
    if !path.exists() {
        return Err(ProjectError::NotFound(path.display().to_string()));
    }

    std::fs::remove_dir_all(path)?;
    Ok(())
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), ProjectError> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper to create a test project in a temp directory
    fn create_test_project(dir: &Path, name: &str) -> PathBuf {
        let project_path = dir.join(name);
        std::fs::create_dir_all(&project_path).unwrap();
        std::fs::create_dir_all(project_path.join("samples")).unwrap();

        let project = super::super::ProjectFile::new(name);
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
