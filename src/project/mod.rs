//! Project persistence - save and load project files
//!
//! Project format:
//! - `project.json` at project root
//! - `samples/` directory for audio files

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::arrangement::Arrangement;
use crate::mixer::Mixer;
use crate::plugin_host::PluginParamId;
use crate::sequencer::{Generator, GeneratorType, Note, Pattern};

/// Current project file version
pub const PROJECT_VERSION: u32 = 1;

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
    pub channels: Vec<ChannelData>,
    pub patterns: Vec<PatternData>,
    #[serde(default)]
    pub arrangement: Arrangement,
    /// Mixer state (tracks, routing, generator assignments)
    /// Optional for backward compatibility with old project files
    #[serde(default)]
    pub mixer: Option<Mixer>,
}

/// Serializable channel/generator data
/// Keeps volume/muted/solo for backward compatibility with old project files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelData {
    pub name: String,
    #[serde(default, alias = "channel_type")]
    pub generator_type: GeneratorType,
    pub sample_path: Option<String>,
    /// Legacy field - volume now lives in mixer. Kept for backward compat.
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// Legacy field - muted now lives in mixer. Kept for backward compat.
    #[serde(default)]
    pub muted: bool,
    /// Legacy field - solo now lives in mixer. Kept for backward compat.
    #[serde(default)]
    pub solo: bool,
    /// Plugin parameter values (param_name -> value)
    #[serde(default)]
    pub plugin_params: HashMap<String, f32>,
}

fn default_volume() -> f32 {
    0.8
}

/// Serializable pattern data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternData {
    pub id: usize,
    pub name: String,
    pub steps: Vec<Vec<bool>>,
    #[serde(default)]
    pub notes: Vec<Vec<NoteData>>,
}

/// Serializable note data for piano roll
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteData {
    pub id: String,
    pub pitch: u8,
    pub start_step: usize,
    pub duration: usize,
    #[serde(default = "default_velocity")]
    pub velocity: f32,
}

fn default_velocity() -> f32 {
    0.8
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
    pub fn from_state(
        name: &str,
        bpm: f64,
        current_pattern: usize,
        generators: &[Generator],
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
            channels: generators.iter().map(ChannelData::from).collect(),
            patterns: patterns.iter().map(PatternData::from).collect(),
            arrangement: arrangement.clone(),
            mixer: Some(mixer.clone()),
        }
    }
}

impl From<&Generator> for ChannelData {
    fn from(generator: &Generator) -> Self {
        Self {
            name: generator.name.clone(),
            generator_type: generator.generator_type.clone(),
            sample_path: generator.sample_path.clone(),
            // Legacy fields - write defaults since Generator no longer has these
            volume: default_volume(),
            muted: false,
            solo: false,
            // Convert PluginParamId -> String for serialization
            plugin_params: generator
                .plugin_params
                .iter()
                .map(|(k, v)| (k.as_str().to_string(), *v))
                .collect(),
        }
    }
}

impl From<&Pattern> for PatternData {
    fn from(pattern: &Pattern) -> Self {
        Self {
            id: pattern.id,
            name: pattern.name.clone(),
            steps: pattern.steps.clone(),
            notes: pattern
                .notes
                .iter()
                .map(|channel_notes| channel_notes.iter().map(NoteData::from).collect())
                .collect(),
        }
    }
}

impl From<&Note> for NoteData {
    fn from(note: &Note) -> Self {
        Self {
            id: note.id.clone(),
            pitch: note.pitch,
            start_step: note.start_step,
            duration: note.duration,
            velocity: note.velocity,
        }
    }
}

impl From<&NoteData> for Note {
    fn from(data: &NoteData) -> Self {
        Self {
            id: data.id.clone(),
            pitch: data.pitch,
            start_step: data.start_step,
            duration: data.duration,
            velocity: data.velocity,
        }
    }
}

impl From<&ChannelData> for Generator {
    fn from(data: &ChannelData) -> Self {
        Self {
            name: data.name.clone(),
            generator_type: data.generator_type.clone(),
            sample_path: data.sample_path.clone(),
            // NOTE: volume, muted, solo from ChannelData are ignored
            // They will be loaded from mixer state separately (TODO)
            // Convert String -> PluginParamId for deserialization
            plugin_params: data
                .plugin_params
                .iter()
                .filter_map(|(k, v)| {
                    PluginParamId::ALL
                        .iter()
                        .find(|id| id.as_str() == k)
                        .map(|id| (*id, *v))
                })
                .collect(),
        }
    }
}

impl From<&PatternData> for Pattern {
    fn from(data: &PatternData) -> Self {
        let num_channels = data.steps.len();

        // Ensure notes vector has correct number of channels
        // (may be empty if loaded from old project file)
        let notes = if data.notes.len() == num_channels {
            data.notes
                .iter()
                .map(|channel_notes| channel_notes.iter().map(Note::from).collect())
                .collect()
        } else {
            // Initialize with empty vectors for each channel
            vec![Vec::new(); num_channels]
        };

        Self {
            id: data.id,
            name: data.name.clone(),
            steps: data.steps.clone(),
            notes,
            length: data.steps.first().map(|s| s.len()).unwrap_or(16),
        }
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

    // Version check (for future migrations)
    if project.version > PROJECT_VERSION {
        return Err(ProjectError::InvalidVersion(project.version));
    }

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
