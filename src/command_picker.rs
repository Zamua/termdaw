//! Command picker - which-key style command palette
//!
//! Press Space to open, then press a key to execute a command.
//! Shows available commands grouped by category.

use tui_input::Input;

/// A command that can be executed from the picker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    // Views
    ShowPlaylist,
    ShowChannelRack,
    ShowPianoRoll,

    // Panels
    ToggleBrowser,
    ToggleMixer,

    // Transport
    PlayStop,
    SetTempo,

    // File
    Quit,
}

impl Command {
    /// Get the key that triggers this command
    pub fn key(&self) -> char {
        match self {
            Command::ShowPlaylist => 'p',
            Command::ShowChannelRack => 'c',
            Command::ShowPianoRoll => 'r',
            Command::ToggleBrowser => 'b',
            Command::ToggleMixer => 'm',
            Command::PlayStop => ' ',
            Command::SetTempo => 't',
            Command::Quit => 'q',
        }
    }

    /// Get the display label for this command
    pub fn label(&self) -> &'static str {
        match self {
            Command::ShowPlaylist => "Playlist",
            Command::ShowChannelRack => "Channel Rack",
            Command::ShowPianoRoll => "Piano Roll",
            Command::ToggleBrowser => "Toggle Browser",
            Command::ToggleMixer => "Toggle Mixer",
            Command::PlayStop => "Play/Stop",
            Command::SetTempo => "Set Tempo",
            Command::Quit => "Quit",
        }
    }
}

/// A group of commands in the same category
#[derive(Debug, Clone)]
pub struct CommandGroup {
    pub name: &'static str,
    pub commands: Vec<Command>,
}

/// Input mode for text entry (e.g., tempo)
pub struct InputMode {
    /// Whether input mode is active
    pub active: bool,
    /// The prompt to show
    pub prompt: &'static str,
    /// Current input (using tui-input crate)
    pub input: Input,
    /// What to do with the input
    pub target: InputTarget,
}

impl Default for InputMode {
    fn default() -> Self {
        Self {
            active: false,
            prompt: "",
            input: Input::default(),
            target: InputTarget::None,
        }
    }
}

impl std::fmt::Debug for InputMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputMode")
            .field("active", &self.active)
            .field("prompt", &self.prompt)
            .field("input", &self.input.value())
            .field("target", &self.target)
            .finish()
    }
}

impl Clone for InputMode {
    fn clone(&self) -> Self {
        Self {
            active: self.active,
            prompt: self.prompt,
            input: Input::new(self.input.value().to_string()),
            target: self.target,
        }
    }
}

/// What the input is for
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InputTarget {
    #[default]
    None,
    Tempo,
}

/// Command picker state
#[derive(Debug, Clone)]
pub struct CommandPicker {
    /// Whether the picker is currently visible
    pub visible: bool,
    /// All available commands grouped by category
    pub groups: Vec<CommandGroup>,
    /// Text input mode (for tempo, etc.)
    pub input: InputMode,
}

impl CommandPicker {
    /// Create a new command picker with default commands
    pub fn new() -> Self {
        let groups = vec![
            CommandGroup {
                name: "Views",
                commands: vec![
                    Command::ShowPlaylist,
                    Command::ShowChannelRack,
                    Command::ShowPianoRoll,
                ],
            },
            CommandGroup {
                name: "Panels",
                commands: vec![Command::ToggleBrowser, Command::ToggleMixer],
            },
            CommandGroup {
                name: "Transport",
                commands: vec![Command::PlayStop, Command::SetTempo],
            },
            CommandGroup {
                name: "File",
                commands: vec![Command::Quit],
            },
        ];

        Self {
            visible: false,
            groups,
            input: InputMode::default(),
        }
    }

    /// Start tempo input mode
    pub fn start_tempo_input(&mut self, current_bpm: f64) {
        self.visible = false;
        self.input = InputMode {
            active: true,
            prompt: "Tempo (BPM):",
            input: Input::new(format!("{:.0}", current_bpm)),
            target: InputTarget::Tempo,
        };
    }

    /// Cancel input mode
    pub fn cancel_input(&mut self) {
        self.input = InputMode::default();
    }

    /// Get the parsed tempo value, if valid
    pub fn get_tempo_value(&self) -> Option<f64> {
        if self.input.target == InputTarget::Tempo {
            self.input.input.value().parse::<f64>().ok()
        } else {
            None
        }
    }

    /// Show the picker
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the picker
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Find a command by key
    pub fn find_command(&self, key: char) -> Option<Command> {
        for group in &self.groups {
            for cmd in &group.commands {
                if cmd.key() == key {
                    return Some(*cmd);
                }
            }
        }
        None
    }

    /// Format a key for display
    pub fn format_key(key: char) -> String {
        match key {
            ' ' => "␣".to_string(),
            c => c.to_string(),
        }
    }

    /// Get a command by its flattened index (for mouse click handling)
    /// Commands are enumerated in order across all groups.
    pub fn get_command_at(&self, index: usize) -> Option<Command> {
        let mut current_idx = 0;
        for group in &self.groups {
            for cmd in &group.commands {
                if current_idx == index {
                    return Some(*cmd);
                }
                current_idx += 1;
            }
        }
        None
    }

    /// Get total number of commands (for area registration)
    #[allow(dead_code)]
    pub fn command_count(&self) -> usize {
        self.groups.iter().map(|g| g.commands.len()).sum()
    }
}

impl Default for CommandPicker {
    fn default() -> Self {
        Self::new()
    }
}
