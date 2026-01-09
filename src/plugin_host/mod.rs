//! Plugin hosting infrastructure for CLAP plugins
//!
//! Uses clack-host to load and run CLAP audio plugins.

use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, Sender};

#[allow(dead_code)]
mod host;
pub mod loader;
pub mod params;

#[cfg(test)]
pub use loader::mock::MockPluginLoader;
pub use loader::{ClapPluginLoader, LoadedPlugin, PluginLoadError, PluginLoader};
pub use params::PluginParamId;

#[allow(unused_imports)]
pub use host::{ActivePluginProcessor, MidiNote, ParamChange, PluginHost};

/// A loaded plugin's parameter info
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PluginParam {
    pub id: u32,
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

/// Info about a loaded plugin
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub params: Vec<PluginParam>,
}

/// Commands sent to the plugin host
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum PluginCommand {
    /// Load a plugin from path for a channel
    Load { channel: usize, path: PathBuf },
    /// Unload the plugin for a channel
    Unload { channel: usize },
    /// Set a parameter value
    SetParam {
        channel: usize,
        param_id: u32,
        value: f32,
    },
    /// Send note on
    NoteOn {
        channel: usize,
        note: u8,
        velocity: f32,
    },
    /// Send note off
    NoteOff { channel: usize, note: u8 },
    /// Get plugin info (sends response via callback)
    GetInfo { channel: usize },
}

/// Events sent back from the plugin host
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum PluginEvent {
    /// Plugin loaded successfully
    Loaded { channel: usize, info: PluginInfo },
    /// Plugin failed to load
    LoadFailed { channel: usize, error: String },
    /// Plugin unloaded
    Unloaded { channel: usize },
    /// Parameter changed (from plugin automation)
    ParamChanged {
        channel: usize,
        param_id: u32,
        value: f32,
    },
}

/// Handle for sending commands to the plugin host
#[derive(Clone)]
pub struct PluginHandle {
    tx: Sender<PluginCommand>,
    event_rx: Receiver<PluginEvent>,
}

#[allow(dead_code)]
impl PluginHandle {
    pub fn new(tx: Sender<PluginCommand>, event_rx: Receiver<PluginEvent>) -> Self {
        Self { tx, event_rx }
    }

    /// Load a plugin for a channel
    pub fn load(&self, channel: usize, path: &Path) {
        let _ = self.tx.send(PluginCommand::Load {
            channel,
            path: path.to_path_buf(),
        });
    }

    /// Unload the plugin for a channel
    pub fn unload(&self, channel: usize) {
        let _ = self.tx.send(PluginCommand::Unload { channel });
    }

    /// Set a parameter value
    pub fn set_param(&self, channel: usize, param_id: u32, value: f32) {
        let _ = self.tx.send(PluginCommand::SetParam {
            channel,
            param_id,
            value,
        });
    }

    /// Send note on
    pub fn note_on(&self, channel: usize, note: u8, velocity: f32) {
        let _ = self.tx.send(PluginCommand::NoteOn {
            channel,
            note,
            velocity,
        });
    }

    /// Send note off
    pub fn note_off(&self, channel: usize, note: u8) {
        let _ = self.tx.send(PluginCommand::NoteOff { channel, note });
    }

    /// Poll for events from plugins
    pub fn poll_events(&self) -> Vec<PluginEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            events.push(event);
        }
        events
    }
}

/// Scan for CLAP plugins in standard directories
#[allow(dead_code)]
pub fn scan_plugin_directories() -> Vec<PathBuf> {
    let mut plugins = Vec::new();

    // Standard CLAP plugin directories
    let dirs = get_plugin_directories();

    for dir in dirs {
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "clap").unwrap_or(false) {
                        plugins.push(path);
                    }
                }
            }
        }
    }

    plugins
}

/// Get standard plugin directories for the current platform
fn get_plugin_directories() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // User home directory plugins
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".clap"));
    }

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join("Library/Audio/Plug-Ins/CLAP"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".local/lib/clap"));
        }
        dirs.push(PathBuf::from("/usr/lib/clap"));
        dirs.push(PathBuf::from("/usr/local/lib/clap"));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(program_files) = std::env::var_os("PROGRAMFILES") {
            dirs.push(PathBuf::from(program_files).join("Common Files/CLAP"));
        }
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local_app_data).join("Programs/Common/CLAP"));
        }
    }

    dirs
}
