//! File browser for sample and plugin selection
//!
//! Features:
//! - Directory tree navigation with expand/collapse
//! - Vim keybindings (j/k/h/l/gg/G)
//! - Sample preview with Space
//! - Selection mode for assigning samples/plugins to channels
//! - Toggle between Samples and Plugins mode with Tab

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Browser mode - what type of files to browse
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserMode {
    /// Browse audio samples
    #[default]
    Samples,
    /// Browse CLAP plugins
    Plugins,
}

#[allow(dead_code)]
impl BrowserMode {
    /// Toggle to the other mode
    pub fn toggle(&mut self) {
        *self = match self {
            BrowserMode::Samples => BrowserMode::Plugins,
            BrowserMode::Plugins => BrowserMode::Samples,
        };
    }

    /// Get the display name
    pub fn name(&self) -> &'static str {
        match self {
            BrowserMode::Samples => "Samples",
            BrowserMode::Plugins => "Plugins",
        }
    }
}

/// A file or directory entry in the browser
#[derive(Debug, Clone)]
pub struct BrowserEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
}

/// Browser state
#[derive(Debug, Clone)]
pub struct BrowserState {
    /// Current browser mode (samples or plugins)
    pub mode: BrowserMode,
    /// Root path for samples browsing
    pub samples_path: PathBuf,
    /// Root path for plugins browsing
    pub plugins_path: PathBuf,
    /// All entries (files and directories)
    all_entries: Vec<BrowserEntry>,
    /// Currently visible entries (respecting expand/collapse)
    pub visible_entries: Vec<BrowserEntry>,
    /// Current cursor position in visible entries
    pub cursor: usize,
    /// Expanded directory paths
    pub expanded: HashSet<PathBuf>,
    /// Whether we're in selection mode
    pub selection_mode: bool,
    /// Which channel we're selecting for
    pub target_channel: Option<usize>,
}

#[allow(dead_code)]
impl BrowserState {
    /// Create a new browser state for the given samples path
    pub fn new(samples_path: PathBuf) -> Self {
        // Plugin path is in project's plugins directory
        let plugins_path = samples_path
            .parent()
            .map(|p| p.join("plugins"))
            .unwrap_or_else(|| PathBuf::from("plugins"));

        let mut state = Self {
            mode: BrowserMode::Samples,
            samples_path,
            plugins_path,
            all_entries: Vec::new(),
            visible_entries: Vec::new(),
            cursor: 0,
            expanded: HashSet::new(),
            selection_mode: false,
            target_channel: None,
        };
        state.scan_directory();
        state.update_visible_entries();
        state
    }

    /// Get the current root path based on mode
    pub fn root_path(&self) -> &Path {
        match self.mode {
            BrowserMode::Samples => &self.samples_path,
            BrowserMode::Plugins => &self.plugins_path,
        }
    }

    /// Toggle between samples and plugins mode
    pub fn toggle_mode(&mut self) {
        self.mode.toggle();
        self.cursor = 0;
        self.expanded.clear();
        self.scan_directory();
        self.update_visible_entries();
    }

    /// Scan the directory for files based on current mode
    pub fn scan_directory(&mut self) {
        self.all_entries.clear();

        let root = self.root_path().to_path_buf();
        if !root.exists() {
            return;
        }

        // Use filter_entry to prevent descending into .clap bundles on macOS
        // (they are directories but should be treated as files)
        let walker = WalkDir::new(&root)
            .min_depth(1)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| {
                // Don't descend into .clap bundles - they are plugin bundles, not folders
                if e.file_type().is_dir() && is_plugin_bundle(e.path()) {
                    // We'll add this as a "file" entry, but don't recurse into it
                    return true; // Include in iteration but...
                }
                true
            });

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path().to_path_buf();
            let is_bundle = is_plugin_bundle(&path);
            let is_dir = entry.file_type().is_dir() && !is_bundle;

            // Skip contents inside .clap bundles
            if self.mode == BrowserMode::Plugins {
                // Check if any parent is a .clap bundle
                let mut in_bundle = false;
                for ancestor in path.ancestors().skip(1) {
                    if ancestor == root {
                        break;
                    }
                    if is_plugin_bundle(ancestor) {
                        in_bundle = true;
                        break;
                    }
                }
                if in_bundle {
                    continue;
                }
            }

            // Filter based on mode
            let include = match self.mode {
                BrowserMode::Samples => is_dir || is_audio_file(&path),
                BrowserMode::Plugins => is_dir || is_bundle || is_plugin_file(&path),
            };

            if include {
                let depth = entry.depth() - 1;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                self.all_entries.push(BrowserEntry {
                    path,
                    name,
                    is_dir,
                    depth,
                });
            }
        }
    }

    /// Update visible entries based on expanded directories
    pub fn update_visible_entries(&mut self) {
        self.visible_entries.clear();

        let root = self.root_path().to_path_buf();
        for entry in &self.all_entries {
            // Check if all parent directories are expanded
            let mut visible = true;
            let mut current = entry.path.parent();

            while let Some(parent) = current {
                if parent == root {
                    break;
                }
                if !self.expanded.contains(parent) {
                    visible = false;
                    break;
                }
                current = parent.parent();
            }
            if visible {
                self.visible_entries.push(entry.clone());
            }
        }

        // Clamp cursor to valid range
        if !self.visible_entries.is_empty() {
            self.cursor = self.cursor.min(self.visible_entries.len() - 1);
        } else {
            self.cursor = 0;
        }
    }

    /// Get the currently selected entry
    pub fn current_entry(&self) -> Option<&BrowserEntry> {
        self.visible_entries.get(self.cursor)
    }

    /// Move cursor down
    pub fn move_down(&mut self, count: usize) {
        if !self.visible_entries.is_empty() {
            self.cursor = (self.cursor + count).min(self.visible_entries.len() - 1);
        }
    }

    /// Move cursor up
    pub fn move_up(&mut self, count: usize) {
        self.cursor = self.cursor.saturating_sub(count);
    }

    /// Go to first entry
    pub fn go_to_top(&mut self) {
        self.cursor = 0;
    }

    /// Go to last entry
    pub fn go_to_bottom(&mut self) {
        if !self.visible_entries.is_empty() {
            self.cursor = self.visible_entries.len() - 1;
        }
    }

    /// Toggle folder expansion or select file
    pub fn toggle_or_select(&mut self) -> Option<PathBuf> {
        if let Some(entry) = self.current_entry().cloned() {
            if entry.is_dir {
                // Toggle folder expansion
                if self.expanded.contains(&entry.path) {
                    self.expanded.remove(&entry.path);
                } else {
                    self.expanded.insert(entry.path);
                }
                self.update_visible_entries();
                None
            } else {
                // Return selected file path
                Some(entry.path)
            }
        } else {
            None
        }
    }

    /// Expand current folder (l key)
    pub fn expand(&mut self) {
        if let Some(entry) = self.current_entry() {
            if entry.is_dir && !self.expanded.contains(&entry.path) {
                self.expanded.insert(entry.path.clone());
                self.update_visible_entries();
            }
        }
    }

    /// Collapse current folder or go to parent (h key)
    pub fn collapse_or_parent(&mut self) {
        let root = self.root_path().to_path_buf();
        if let Some(entry) = self.current_entry().cloned() {
            if entry.is_dir && self.expanded.contains(&entry.path) {
                // Collapse this folder
                self.expanded.remove(&entry.path);
                self.update_visible_entries();
            } else if let Some(parent) = entry.path.parent() {
                // Go to parent folder
                if parent != root {
                    // Find parent in visible entries and move cursor there
                    if let Some(idx) = self.visible_entries.iter().position(|e| e.path == parent) {
                        self.cursor = idx;
                    }
                }
            }
        }
    }

    /// Start sample selection mode for a channel
    pub fn start_selection(&mut self, channel_idx: usize) {
        self.selection_mode = true;
        self.target_channel = Some(channel_idx);
    }

    /// Cancel sample selection mode
    pub fn cancel_selection(&mut self) {
        self.selection_mode = false;
        self.target_channel = None;
    }

    /// Complete selection and return (channel_idx, relative_path)
    pub fn complete_selection(&mut self) -> Option<(usize, String)> {
        if !self.selection_mode {
            return None;
        }

        let channel_idx = self.target_channel?;
        let entry = self.current_entry()?;

        if entry.is_dir {
            return None;
        }

        let root = self.root_path().to_path_buf();

        // Get path relative to root
        let relative_path = entry
            .path
            .strip_prefix(&root)
            .ok()?
            .to_string_lossy()
            .to_string();

        self.selection_mode = false;
        self.target_channel = None;

        Some((channel_idx, relative_path))
    }

    /// Get the currently selected file's full path (for plugin loading)
    pub fn selected_file_path(&self) -> Option<PathBuf> {
        let entry = self.current_entry()?;
        if entry.is_dir {
            None
        } else {
            Some(entry.path.clone())
        }
    }

    /// Refresh the directory listing
    pub fn refresh(&mut self) {
        self.scan_directory();
        self.update_visible_entries();
    }
}

/// Check if a file is a supported audio format
fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_lowercase().as_str(), "wav" | "mp3" | "flac" | "ogg"))
        .unwrap_or(false)
}

/// Check if a path is a CLAP plugin
/// On macOS, .clap plugins are bundles (directories with .clap extension)
fn is_plugin_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase() == "clap")
        .unwrap_or(false)
}

/// Check if a path is a plugin bundle (directory with .clap extension)
fn is_plugin_bundle(path: &Path) -> bool {
    path.is_dir()
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase() == "clap")
            .unwrap_or(false)
}
