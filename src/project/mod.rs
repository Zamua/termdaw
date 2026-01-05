//! Project persistence - save and load project files
//!
//! Project format:
//! - `project.json` at project root
//! - `samples/` directory for audio files

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::arrangement::Arrangement;
use crate::mixer::Mixer;
use crate::sequencer::{Channel, Pattern};

/// Current project file version
pub const PROJECT_VERSION: u32 = 2;

/// Project file name
pub const PROJECT_FILE_NAME: &str = "project.json";

/// Serializable project file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub bpm: f64,
    pub current_pattern: usize,
    /// Channels with all their data (source, routing, pattern data)
    pub channels: Vec<Channel>,
    /// Pattern metadata (just id, name, length)
    pub patterns: Vec<Pattern>,
    #[serde(default)]
    pub arrangement: Arrangement,
    /// Mixer state (tracks only - routing is now in channels)
    #[serde(default)]
    pub mixer: Option<Mixer>,
}

#[allow(dead_code)]
impl ProjectFile {
    /// Create a new project file with default values
    pub fn new(name: &str) -> Self {
        let now = Utc::now();
        Self {
            version: PROJECT_VERSION,
            name: name.to_string(),
            created_at: now,
            modified_at: now,
            bpm: 140.0,
            current_pattern: 0,
            channels: Vec::new(),
            patterns: Vec::new(),
            arrangement: Arrangement::default(),
            mixer: None,
        }
    }

    /// Create from app state
    #[allow(clippy::too_many_arguments)]
    pub fn from_state(
        name: &str,
        bpm: f64,
        current_pattern: usize,
        channels: &[Channel],
        patterns: &[Pattern],
        arrangement: &Arrangement,
        mixer: &Mixer,
        created_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            version: PROJECT_VERSION,
            name: name.to_string(),
            created_at: created_at.unwrap_or_else(Utc::now),
            modified_at: Utc::now(),
            bpm,
            current_pattern,
            channels: channels.to_vec(),
            patterns: patterns.to_vec(),
            arrangement: arrangement.clone(),
            mixer: Some(mixer.clone()),
        }
    }

    /// Convert to channels (for loading)
    pub fn into_channels(self) -> Vec<Channel> {
        self.channels
    }

    /// Convert to patterns (for loading)
    pub fn into_patterns(self) -> Vec<Pattern> {
        self.patterns
    }
}

/// Project-related errors
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Project not found: {0}")]
    NotFound(String),
    #[error("Invalid project version: {0}")]
    #[allow(dead_code)]
    InvalidVersion(u32),
}

/// Check if a directory contains a valid project
pub fn is_valid_project(path: &Path) -> bool {
    path.join(PROJECT_FILE_NAME).exists()
}

/// Load a project from disk
pub fn load_project(path: &Path) -> Result<ProjectFile, ProjectError> {
    let project_file = path.join(PROJECT_FILE_NAME);
    if !project_file.exists() {
        return Err(ProjectError::NotFound(path.display().to_string()));
    }

    let json = fs::read_to_string(&project_file)?;
    let project: ProjectFile = serde_json::from_str(&json)?;

    Ok(project)
}

/// Save a project to disk (atomic write)
pub fn save_project(path: &Path, project: &ProjectFile) -> Result<(), ProjectError> {
    // Ensure project directory exists
    fs::create_dir_all(path)?;

    // Ensure samples directory exists
    let samples_dir = path.join("samples");
    fs::create_dir_all(&samples_dir)?;

    let project_file = path.join(PROJECT_FILE_NAME);
    let temp_file = path.join(format!(".{}.tmp", PROJECT_FILE_NAME));

    // Write to temp file first
    let json = serde_json::to_string_pretty(project)?;
    fs::write(&temp_file, &json)?;

    // Atomic rename
    fs::rename(&temp_file, &project_file)?;

    Ok(())
}

/// Create a new project directory with default structure
#[allow(dead_code)]
pub fn create_project(path: &Path, name: &str) -> Result<ProjectFile, ProjectError> {
    // Create directory structure
    fs::create_dir_all(path)?;
    fs::create_dir_all(path.join("samples"))?;

    // Create project file
    let project = ProjectFile::new(name);
    save_project(path, &project)?;

    Ok(project)
}

/// Find the template directory (bundled with the app)
pub fn find_template_dir() -> Option<PathBuf> {
    // Try relative to current directory first (for development)
    let cwd_template = PathBuf::from("templates/default");
    if cwd_template.exists() && cwd_template.is_dir() {
        return Some(cwd_template);
    }

    // Try relative to executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_template = exe_dir.join("templates/default");
            if exe_template.exists() && exe_template.is_dir() {
                return Some(exe_template);
            }
            // Also try one level up (for target/debug/termdaw)
            if let Some(parent) = exe_dir.parent() {
                if let Some(grandparent) = parent.parent() {
                    let dev_template = grandparent.join("templates/default");
                    if dev_template.exists() && dev_template.is_dir() {
                        return Some(dev_template);
                    }
                }
            }
        }
    }

    None
}

/// Copy template to a new project directory
pub fn copy_template(project_path: &Path) -> Result<(), ProjectError> {
    let Some(template_dir) = find_template_dir() else {
        // No template found - not an error, just skip
        return Ok(());
    };

    copy_dir_recursive(&template_dir, project_path)?;

    Ok(())
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), ProjectError> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Generate a unique project name (untitled-1, untitled-2, etc.)
pub fn generate_project_name() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    for i in 1..1000 {
        let name = format!("untitled-{}", i);
        let path = cwd.join(&name);
        if !path.exists() {
            return name;
        }
    }

    // Fallback with timestamp
    format!("untitled-{}", chrono::Utc::now().timestamp())
}
