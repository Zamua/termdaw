//! Application state and core logic

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::arrangement::Arrangement;
use crate::audio_sync::AudioSync;

// ============================================================================
// Extracted State Structs (Phase 1 Refactoring)
// ============================================================================

/// Project metadata and paths
#[derive(Debug, Clone)]
pub struct ProjectState {
    /// Project name (directory name)
    pub name: String,
    /// Project root path
    pub path: PathBuf,
    /// When the project was created
    pub created_at: DateTime<Utc>,
}

impl ProjectState {
    /// Create a new project state
    pub fn new(name: &str, path: PathBuf, created_at: DateTime<Utc>) -> Self {
        Self {
            name: name.to_string(),
            path,
            created_at,
        }
    }

    /// Get the samples directory path
    pub fn samples_path(&self) -> PathBuf {
        self.path.join("samples")
    }

    /// Get the plugins directory path
    pub fn plugins_path(&self) -> PathBuf {
        self.path.join("plugins")
    }
}

/// Transport/playback state (bpm, timing, play state)
#[derive(Debug, Clone)]
pub struct TransportState {
    /// Playback state machine (stopped, playing pattern, playing arrangement)
    pub playback: PlaybackState,
    /// Tempo in beats per minute
    pub bpm: f64,
    /// Time accumulator for step timing (private)
    step_accumulator: Duration,
}

impl TransportState {
    /// Create a new transport state with the given BPM
    pub fn new(bpm: f64) -> Self {
        Self {
            playback: PlaybackState::default(),
            bpm,
            step_accumulator: Duration::ZERO,
        }
    }

    /// Calculate the duration of one step at current BPM
    pub fn step_duration(&self) -> Duration {
        Duration::from_secs_f64(60.0 / self.bpm / 4.0)
    }

    /// Add time to the accumulator
    pub fn add_time(&mut self, delta: Duration) {
        self.step_accumulator += delta;
    }

    /// Check if enough time has accumulated for a step
    pub fn should_advance(&self) -> bool {
        self.step_accumulator >= self.step_duration()
    }

    /// Consume one step worth of accumulated time
    pub fn consume_step(&mut self) {
        self.step_accumulator -= self.step_duration();
    }

    /// Reset the accumulator (called when playback stops/starts)
    pub fn reset_accumulator(&mut self) {
        self.step_accumulator = Duration::ZERO;
    }
}
use crate::audio::AudioHandle;
use crate::browser::BrowserState;
use crate::command_picker::CommandPicker;
use crate::coords::AppCol;
use crate::cursor::CursorStates;
use crate::effects::{EffectSlot, EffectType, EFFECT_SLOTS};
use crate::history::{Command, GlobalJumplist, History, JumpPosition};
use crate::input::context::{PianoRollContext, PlaylistContext, StepGridContext};
use crate::input::mouse::MouseState;
use crate::input::vim::{GridSemantics, VimStates, Zone};
use crate::mixer::{Mixer, TrackId};
use crate::playback::{PlaybackEvent, PlaybackState};
use crate::plugin_host::params::build_init_params;
use crate::plugin_host::{ClapPluginLoader, PluginLoader};
use crate::project::{self, ProjectFile};
use crate::projects_modal::ProjectsModal;
use crate::sequencer::{default_channels, Channel, ChannelSource, Note, Pattern};
use crate::ui::areas::ScreenAreas;
use crate::ui::context_menu::ContextMenu;
use crate::ui::plugin_editor::PluginEditorState;

// Re-export types from mode module for external use
pub use crate::mode::{AppMode, Panel, ViewMode};

// ============================================================================
// Phase 3: AppState + UiState Split
// ============================================================================

/// Application domain state (business logic, audio, data)
///
/// Contains all state related to the music project, audio processing,
/// and undo/redo history. This is the "model" in an MVC sense.
pub struct AppState {
    /// Project metadata (name, path, created_at)
    pub project: ProjectState,

    /// Channels (sound sources - samplers, plugins)
    /// Use `channels()` for reads, `channels_mut()` for writes in commands
    pub(crate) channels: Vec<Channel>,

    /// Patterns
    /// Use `patterns()` for reads, `patterns_mut()` for writes in commands
    pub(crate) patterns: Vec<Pattern>,

    /// Currently selected pattern
    /// Use `current_pattern()` and `set_current_pattern()` accessors
    pub(crate) current_pattern: usize,

    /// Arrangement data
    pub arrangement: Arrangement,

    /// Mixer (FL Studio-style with routing)
    pub mixer: Mixer,

    /// Audio handle for playback
    pub audio: AudioHandle,

    /// Plugin loader for loading CLAP/VST plugins
    pub(crate) plugin_loader: Box<dyn PluginLoader>,

    /// Audio sync coordinator for batched updates
    pub audio_sync: AudioSync,

    /// Transport state (playback, bpm, timing)
    pub transport: TransportState,

    /// Undo/redo history
    pub history: History,

    /// Dirty flag for auto-save
    pub(crate) dirty: bool,

    /// Last change time for debounced auto-save
    pub(crate) last_change: Instant,
}

/// User interface state (view, interaction, navigation)
///
/// Contains all state related to the UI: modes, cursors, vim states,
/// panel visibility, and modals. This is the "view" state in an MVC sense.
pub struct UiState {
    /// Whether the app should quit
    pub should_quit: bool,

    /// Application mode (normal, command picker, plugin editor, etc.)
    pub mode: AppMode,

    /// Current view mode (what's in the main area)
    pub view_mode: ViewMode,

    /// Whether the browser panel is visible
    pub show_browser: bool,

    /// Whether the mixer panel is visible
    pub show_mixer: bool,

    /// Whether the event log panel is visible
    pub show_event_log: bool,

    /// Terminal dimensions
    pub terminal_width: u16,
    pub terminal_height: u16,

    /// Cursor states for all panels
    pub cursors: CursorStates,

    /// Vim state machines for grid-based views
    pub vim: VimStates,

    /// Global cross-view jump list for Ctrl+O/Ctrl+I
    pub global_jumplist: GlobalJumplist,

    /// File browser state
    pub browser: BrowserState,

    /// Command picker (which-key style)
    pub command_picker: CommandPicker,

    /// Projects modal (list and open projects)
    pub projects_modal: ProjectsModal,

    /// Plugin editor modal
    pub plugin_editor: PluginEditorState,

    /// Screen areas for mouse hit testing (populated during render)
    pub screen_areas: ScreenAreas,

    /// Mouse state machine (gesture tracking)
    pub mouse: MouseState,

    /// Context menu state
    pub context_menu: ContextMenu,

    /// Effect picker selection index
    pub effect_picker_selection: usize,

    /// Effect register for yank/paste operations (stores last deleted/yanked effect)
    pub effect_register: Option<crate::effects::EffectSlot>,

    /// Channel register for yank/paste operations (stores last deleted/yanked channel)
    pub channel_register: Option<crate::sequencer::Channel>,

    /// Whether we're currently previewing a channel (for hold-to-preview)
    pub is_previewing: bool,

    /// Which channel is being previewed (for sending note_off)
    pub preview_channel: Option<usize>,

    /// Which note is being previewed (for plugins)
    pub(crate) preview_note: Option<u8>,
}

/// Main application - combines domain state and UI state
///
/// App is a facade that provides convenient access to both `AppState` (domain data)
/// and `UiState` (UI state). Most code should access these through the
/// convenience accessors on App for backwards compatibility.
#[allow(dead_code)]
pub struct App {
    /// Domain state (project, channels, patterns, mixer, audio)
    pub state: AppState,

    /// UI state (mode, cursors, vim, panels, modals)
    pub ui: UiState,

    /// Event log for command tracking (private - only dispatch() writes)
    event_log: crate::event_log::EventLog,
}

// ============================================================================
// Convenience Accessors for Backwards Compatibility
// ============================================================================

impl App {
    // --- AppState accessors ---

    /// Project metadata
    pub fn project(&self) -> &ProjectState {
        &self.state.project
    }

    /// Channels (read-only)
    pub fn channels(&self) -> &[Channel] {
        &self.state.channels
    }

    /// Patterns (read-only)
    pub fn patterns(&self) -> &[Pattern] {
        &self.state.patterns
    }

    /// Mixer (read-only)
    pub fn mixer(&self) -> &Mixer {
        &self.state.mixer
    }

    /// Transport (read-only)
    pub fn transport(&self) -> &TransportState {
        &self.state.transport
    }

    /// Event log (read-only - for UI rendering)
    pub fn event_log(&self) -> &crate::event_log::EventLog {
        &self.event_log
    }

    /// Log an event directly (for operations that bypass dispatch like undo/redo)
    pub fn log_event(&mut self, description: &'static str, is_undoable: bool) {
        self.event_log.log(description, is_undoable);
    }

    // === Internal Mutable Accessors (for commands only) ===
    // These bypass history and should only be used by history/command.rs

    /// Mutable access to channels (for history commands only)
    pub(crate) fn channels_mut(&mut self) -> &mut Vec<Channel> {
        &mut self.state.channels
    }

    /// Mutable access to patterns (for history commands only)
    pub(crate) fn patterns_mut(&mut self) -> &mut Vec<Pattern> {
        &mut self.state.patterns
    }

    /// Set current pattern (for history commands only)
    pub(crate) fn set_current_pattern(&mut self, pattern: usize) {
        self.state.current_pattern = pattern;
    }

    /// Get current pattern
    pub fn current_pattern(&self) -> usize {
        self.state.current_pattern
    }
}

// Legacy field accessors (for gradual migration)
// These allow existing code to work with `app.field` syntax
impl std::ops::Deref for App {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl std::ops::DerefMut for App {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl App {
    /// Create a new App instance
    pub fn new(project_name: &str, audio: AudioHandle) -> Self {
        let project_path = crate::templates::projects_dir().join(project_name);

        // If project doesn't exist, create from template
        if !project::is_valid_project(&project_path) {
            if let Err(e) = project::copy_template(&project_path) {
                eprintln!("Warning: Failed to copy template: {}", e);
            }
        }

        // Load project (either existing or newly created from template)
        let (channels, patterns, bpm, current_pattern, arrangement, created_at, mixer) =
            if project::is_valid_project(&project_path) {
                match project::load_project(&project_path) {
                    Ok(project) => {
                        let channels = project.channels;
                        let patterns = project.patterns;
                        // Load mixer from project file, or create default
                        let mixer = project
                            .mixer
                            .unwrap_or_else(|| Self::create_default_mixer(&channels));
                        (
                            channels,
                            patterns,
                            project.bpm,
                            project.current_pattern,
                            project.arrangement,
                            Some(project.created_at),
                            mixer,
                        )
                    }
                    Err(_) => Self::default_state(),
                }
            } else {
                Self::default_state()
            };

        // Create project state
        let project = ProjectState::new(
            project_name,
            project_path,
            created_at.unwrap_or_else(Utc::now),
        );

        // Pre-compute paths needed before moving project into struct
        let samples_path = project.samples_path();

        let num_channels = channels.len();

        // Channel rack zones in vim coordinate space:
        // - Mute zone (col 0): mute/solo indicator
        // - Track zone (col 1): mixer track assignment
        // - Sample zone (col 2): channel name
        // - Steps zone (cols 3-18): the 16-step sequencer grid (main zone)
        let channel_rack_zones = GridSemantics::with_zones(vec![
            Zone::new(0, 0),                               // Mute
            Zone::new(1, 1),                               // Track
            Zone::new(2, 2),                               // Sample
            Zone::new(3, 18).main().with_word_interval(4), // Steps
        ]);

        // Create domain state (business logic, audio, data)
        let state = AppState {
            project,
            channels,
            patterns,
            current_pattern,
            arrangement,
            mixer,
            audio,
            plugin_loader: Box::new(ClapPluginLoader),
            audio_sync: AudioSync::new(),
            transport: TransportState::new(bpm),
            history: History::new(),
            dirty: false,
            last_change: Instant::now(),
        };

        // Create UI state (view, interaction, navigation)
        let ui = UiState {
            should_quit: false,
            mode: AppMode::default(),
            view_mode: ViewMode::default(),
            show_browser: true,
            show_mixer: false,
            show_event_log: false,
            terminal_width: 80,
            terminal_height: 24,
            cursors: CursorStates::default(),
            vim: VimStates::new(
                99,
                19,
                channel_rack_zones, // 99 channel slots, 19 cols (3 metadata + 16 steps)
                49,
                16, // Piano roll: 49 pitches (C2-C6), 16 steps
                num_channels,
                17, // Playlist: rows = patterns, 16 bars + mute col
            ),
            global_jumplist: GlobalJumplist::new(),
            browser: BrowserState::new(samples_path),
            command_picker: CommandPicker::new(),
            projects_modal: ProjectsModal::new(),
            plugin_editor: PluginEditorState::new(),
            screen_areas: ScreenAreas::new(),
            mouse: MouseState::new(),
            context_menu: ContextMenu::new(),
            effect_picker_selection: 0,
            effect_register: None,
            channel_register: None,
            is_previewing: false,
            preview_channel: None,
            preview_note: None,
        };

        let mut app = Self {
            state,
            ui,
            event_log: crate::event_log::EventLog::new(),
        };

        // Note: Plugins are loaded by AudioEngine::new() via setup_engine(),
        // so we don't need to call load_plugins() here anymore.

        // Mark initial mixer and routing state as dirty, then flush immediately
        app.state.audio_sync.mark_mixer_dirty();
        // Collect routing info first to avoid borrow conflict
        let routing_info: Vec<_> = app
            .state
            .channels
            .iter()
            .enumerate()
            .map(|(idx, ch)| (idx, ch.mixer_track))
            .collect();
        for (channel_idx, track) in routing_info {
            app.state.audio_sync.mark_routing_dirty(channel_idx, track);
        }
        // Destructure to allow simultaneous borrows of different fields
        let AppState {
            audio_sync,
            audio,
            mixer,
            ..
        } = &mut app.state;
        audio_sync.flush(audio, mixer);

        // Sync any effects loaded from the project file to the audio thread
        app.sync_all_effects_to_audio();

        app
    }

    /// Create a default mixer with auto-assigned channel routing
    fn create_default_mixer(channels: &[Channel]) -> Mixer {
        let mut mixer = Mixer::new();
        // Auto-assign each channel to a track
        for (idx, _channel) in channels.iter().enumerate() {
            mixer.auto_assign_generator(idx);
        }
        mixer
    }

    /// Build initial plugin state from channel settings and mixer
    fn build_plugin_init_state(
        &self,
        channel_idx: usize,
        channel: &Channel,
    ) -> crate::audio::PluginInitState {
        use crate::audio::PluginInitState;

        // Get volume from the mixer track this channel routes to
        let track_id = self.mixer.get_generator_track(channel_idx);
        let volume = self.mixer.track(track_id).volume;

        PluginInitState {
            volume,
            params: build_init_params(channel.plugin_params()),
        }
    }

    #[allow(clippy::type_complexity)]
    fn default_state() -> (
        Vec<Channel>,
        Vec<Pattern>,
        f64,
        usize,
        Arrangement,
        Option<DateTime<Utc>>,
        Mixer,
    ) {
        let channels = default_channels();
        let patterns = vec![Pattern::new(0, 16)];
        let mixer = Self::create_default_mixer(&channels);
        (
            channels,
            patterns,
            140.0,
            0,
            Arrangement::new(),
            None,
            mixer,
        )
    }

    // ========================================================================
    // Command Dispatch System
    // ========================================================================

    /// Dispatch a command for execution.
    ///
    /// All state mutations should go through this method. It:
    /// 1. Records undoable commands to history
    /// 2. Executes the command
    /// 3. Marks the project as dirty (for most commands)
    pub fn dispatch(&mut self, cmd: crate::command::AppCommand) {
        // Log command to event log (single write point)
        self.event_log.log(cmd.description(), cmd.is_undoable());

        use crate::command::AppCommand;
        use crate::history::command::{
            AddChannelCmd, AddEffectCmd, AddNoteCmd, AddNotesCmd, DeleteChannelCmd, DeleteNotesCmd,
            DeletePatternCmd, DeleteStepsCmd, RemoveEffectCmd, RemoveNoteCmd, SetStepsCmd,
            TogglePlacementCmd, ToggleStepCmd,
        };

        // For undoable commands with history support, use history.execute()
        // which both executes AND records for undo/redo
        let handled = match &cmd {
            AppCommand::ToggleStep {
                channel,
                pattern,
                step,
            } => {
                let history_cmd =
                    Box::new(ToggleStepCmd::new(*pattern, *channel, *step)) as Box<dyn Command>;
                let mut history = std::mem::take(&mut self.history);
                history.execute(history_cmd, self);
                self.history = history;
                true
            }
            AppCommand::SetSteps {
                channel,
                pattern,
                steps,
            } => {
                // Capture current step states for undo
                let pattern_length = self.patterns.get(*pattern).map(|p| p.length).unwrap_or(16);
                let mut history_cmd = SetStepsCmd::new(*pattern);

                if let Some(ch) = self.channels.get(*channel) {
                    let slice = ch.get_pattern(*pattern);
                    for &(step_idx, new_value) in steps {
                        let old_value = slice.map(|s| s.get_step(step_idx)).unwrap_or(false);
                        if step_idx < pattern_length {
                            history_cmd.add_step(*channel, step_idx, new_value, old_value);
                        }
                    }
                }

                let mut history = std::mem::take(&mut self.history);
                history.execute(Box::new(history_cmd), self);
                self.history = history;
                true
            }
            AppCommand::BatchSetSteps {
                pattern,
                operations,
            } => {
                // Capture current step states for undo
                // Operations use slot-based channel indexing (not Vec indices)
                let pattern_length = self
                    .patterns()
                    .get(*pattern)
                    .map(|p| p.length)
                    .unwrap_or(16);
                let mut history_cmd = SetStepsCmd::new(*pattern);

                for &(slot, step_idx, new_value) in operations {
                    if let Some(ch) = self.get_channel_at_slot(slot) {
                        let slice = ch.get_pattern(*pattern);
                        let old_value = slice.map(|s| s.get_step(step_idx)).unwrap_or(false);
                        if step_idx < pattern_length {
                            history_cmd.add_step(slot, step_idx, new_value, old_value);
                        }
                    }
                }

                let mut history = std::mem::take(&mut self.history);
                history.execute(Box::new(history_cmd), self);
                self.history = history;
                true
            }
            AppCommand::AddNote {
                channel,
                pattern,
                note,
            } => {
                let history_cmd =
                    Box::new(AddNoteCmd::new(*pattern, *channel, note.clone())) as Box<dyn Command>;
                let mut history = std::mem::take(&mut self.history);
                history.execute(history_cmd, self);
                self.history = history;
                true
            }
            AppCommand::BatchAddNotes {
                channel,
                pattern,
                notes,
            } => {
                let history_cmd = AddNotesCmd::new(*pattern, *channel, notes.clone());
                let mut history = std::mem::take(&mut self.history);
                history.execute(Box::new(history_cmd), self);
                self.history = history;
                true
            }
            AppCommand::DeleteNote {
                channel,
                pattern,
                pitch,
                start_step,
            } => {
                // Find the note ID first
                if let Some(ch) = self.channels.get(*channel) {
                    if let Some(slice) = ch.get_pattern(*pattern) {
                        if let Some(note) = slice
                            .notes
                            .iter()
                            .find(|n| n.pitch == *pitch && n.start_step == *start_step)
                        {
                            let history_cmd = Box::new(RemoveNoteCmd::from_note(
                                *pattern,
                                *channel,
                                note.clone(),
                            )) as Box<dyn Command>;
                            let mut history = std::mem::take(&mut self.history);
                            history.execute(history_cmd, self);
                            self.history = history;
                            return; // mark_dirty is handled by the command
                        }
                    }
                }
                false
            }
            AppCommand::BatchClearSteps {
                pattern,
                operations,
            } => {
                // Capture current step states for undo
                let pattern_length = self.patterns.get(*pattern).map(|p| p.length).unwrap_or(16);
                let mut history_cmd = DeleteStepsCmd::new(*pattern);

                for &(channel, start_step, end_step) in operations {
                    if let Some(ch) = self.channels.get(channel) {
                        if let Some(slice) = ch.get_pattern(*pattern) {
                            for step in start_step..=end_step {
                                if step < pattern_length {
                                    let was_active = slice.get_step(step);
                                    history_cmd.add_step(channel, step, was_active);
                                }
                            }
                        }
                    }
                }

                let mut history = std::mem::take(&mut self.history);
                history.execute(Box::new(history_cmd), self);
                self.history = history;
                true
            }
            AppCommand::BatchDeleteNotes {
                channel,
                pattern,
                positions,
            } => {
                // Capture notes for undo
                let notes_to_delete: Vec<_> = if let Some(ch) = self.channels.get(*channel) {
                    if let Some(slice) = ch.get_pattern(*pattern) {
                        positions
                            .iter()
                            .filter_map(|(pitch, start_step)| {
                                slice
                                    .notes
                                    .iter()
                                    .find(|n| n.pitch == *pitch && n.start_step == *start_step)
                                    .cloned()
                            })
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                if !notes_to_delete.is_empty() {
                    let history_cmd = DeleteNotesCmd::new(*pattern, *channel, notes_to_delete);
                    let mut history = std::mem::take(&mut self.history);
                    history.execute(Box::new(history_cmd), self);
                    self.history = history;
                }
                true
            }
            AppCommand::DeleteChannel(slot) => {
                let history_cmd = DeleteChannelCmd::new(*slot);
                let mut history = std::mem::take(&mut self.history);
                history.execute(Box::new(history_cmd), self);
                self.history = history;
                true
            }
            AppCommand::AddChannel { slot, channel } => {
                // Only add if slot is empty
                if self.get_channel_at_slot(*slot).is_none() {
                    let history_cmd = AddChannelCmd::new(*slot, channel.clone());
                    let mut history = std::mem::take(&mut self.history);
                    history.execute(Box::new(history_cmd), self);
                    self.history = history;
                }
                true
            }
            AppCommand::DeletePattern(pattern_id) => {
                let history_cmd = DeletePatternCmd::new(*pattern_id);
                let mut history = std::mem::take(&mut self.history);
                history.execute(Box::new(history_cmd), self);
                self.history = history;
                true
            }
            AppCommand::PlacePattern { pattern_id, bar }
            | AppCommand::RemovePlacement { pattern_id, bar } => {
                let history_cmd =
                    Box::new(TogglePlacementCmd::new(*pattern_id, *bar)) as Box<dyn Command>;
                let mut history = std::mem::take(&mut self.history);
                history.execute(history_cmd, self);
                self.history = history;
                true
            }
            AppCommand::AddEffect {
                track,
                slot,
                effect_type,
            } => {
                let history_cmd = AddEffectCmd::new(*track, *slot, *effect_type);
                let mut history = std::mem::take(&mut self.history);
                history.execute(Box::new(history_cmd), self);
                self.history = history;
                true
            }
            AppCommand::RemoveEffect { track, slot } => {
                let history_cmd = RemoveEffectCmd::new(*track, *slot);
                let mut history = std::mem::take(&mut self.history);
                history.execute(Box::new(history_cmd), self);
                self.history = history;
                true
            }
            _ => false,
        };

        // For commands without history support, execute directly
        if !handled {
            self.execute(cmd.clone());
        }

        // Mark dirty for most commands
        match cmd {
            AppCommand::TogglePlayback | AppCommand::StopPlayback => {
                // Transport commands don't mark dirty
            }
            _ => {
                self.mark_dirty();
            }
        }
    }

    /// Execute a command (internal implementation).
    ///
    /// This method contains the actual mutation logic for each command.
    fn execute(&mut self, cmd: crate::command::AppCommand) {
        use crate::command::AppCommand;
        use crate::mixer::TrackId;

        match cmd {
            // ================================================================
            // Transport
            // ================================================================
            AppCommand::TogglePlayback => {
                self.toggle_play();
            }
            AppCommand::StopPlayback => {
                self.transport.playback.stop();
                self.transport.reset_accumulator();
            }
            AppCommand::SetBpm(bpm) => {
                self.transport.bpm = bpm;
                self.audio.update_tempo(bpm);
            }

            // ================================================================
            // Pattern selection
            // ================================================================
            AppCommand::PreviousPattern => {
                if self.current_pattern > 0 {
                    self.current_pattern -= 1;
                }
            }
            AppCommand::NextPattern => {
                if self.current_pattern + 1 < self.patterns.len() {
                    self.current_pattern += 1;
                } else {
                    // Create a new pattern
                    let new_id = self.patterns.len();
                    self.patterns
                        .push(crate::sequencer::Pattern::new(new_id, 16));
                    self.current_pattern = new_id;
                }
            }
            AppCommand::CreatePattern => {
                let new_id = self.patterns.len();
                self.patterns
                    .push(crate::sequencer::Pattern::new(new_id, 16));
            }
            AppCommand::DeletePattern(id) => {
                if id < self.patterns.len() && self.patterns.len() > 1 {
                    self.patterns.remove(id);
                    if self.current_pattern >= self.patterns.len() {
                        self.current_pattern = self.patterns.len() - 1;
                    }
                }
            }

            // ================================================================
            // Channel operations
            // ================================================================
            AppCommand::CycleChannelMuteState(slot) => {
                if let Some(channel) = self.get_channel_at_slot(slot) {
                    let track_id = TrackId(channel.mixer_track);
                    let track = self.mixer.track_mut(track_id);
                    if track.solo {
                        track.solo = false;
                        track.muted = false;
                    } else if track.muted {
                        track.muted = false;
                        track.solo = true;
                    } else {
                        track.muted = true;
                    }
                    self.audio_sync.mark_mixer_dirty();
                }
            }
            AppCommand::ToggleSolo(slot) => {
                if let Some(channel) = self.get_channel_at_slot(slot) {
                    let track_id = TrackId(channel.mixer_track);
                    self.mixer.toggle_solo(track_id);
                    self.audio_sync.mark_mixer_dirty();
                }
            }
            AppCommand::DeleteChannel(slot) => {
                if let Some(idx) = self.channels.iter().position(|c| c.slot == slot) {
                    self.channels.remove(idx);
                }
            }
            AppCommand::AddChannel { .. } => {
                // Handled through history dispatch, no-op here
            }
            AppCommand::SetChannelSample { slot, path } => {
                self.set_channel_sample(slot, path);
            }
            AppCommand::SetChannelPlugin { slot, path } => {
                self.set_channel_plugin(slot, path);
            }
            AppCommand::SetChannelRouting { slot, track } => {
                if let Some(vec_idx) = self.channels.iter().position(|c| c.slot == slot) {
                    self.channels[vec_idx].mixer_track = track;
                    self.audio.set_generator_track(vec_idx, track);
                }
            }
            AppCommand::IncrementChannelRouting(slot) => {
                if let Some(vec_idx) = self.channels.iter().position(|c| c.slot == slot) {
                    let channel = &mut self.channels[vec_idx];
                    let next = if channel.mixer_track >= 15 {
                        1
                    } else {
                        channel.mixer_track + 1
                    };
                    channel.mixer_track = next;
                    self.audio.set_generator_track(vec_idx, next);
                }
            }
            AppCommand::DecrementChannelRouting(slot) => {
                if let Some(vec_idx) = self.channels.iter().position(|c| c.slot == slot) {
                    let channel = &mut self.channels[vec_idx];
                    let prev = if channel.mixer_track <= 1 {
                        15
                    } else {
                        channel.mixer_track - 1
                    };
                    channel.mixer_track = prev;
                    self.audio.set_generator_track(vec_idx, prev);
                }
            }

            // ================================================================
            // Step grid
            // ================================================================
            AppCommand::ToggleStep {
                channel,
                pattern,
                step,
            } => {
                let pattern_length = self.patterns.get(pattern).map(|p| p.length).unwrap_or(16);
                if let Some(ch) = self.channels.get_mut(channel) {
                    let slice = ch.get_or_create_pattern(pattern, pattern_length);
                    slice.toggle_step(step);
                }
            }
            AppCommand::SetSteps {
                channel,
                pattern,
                steps,
            } => {
                let pattern_length = self.patterns.get(pattern).map(|p| p.length).unwrap_or(16);
                if let Some(ch) = self.channels.get_mut(channel) {
                    let slice = ch.get_or_create_pattern(pattern, pattern_length);
                    for (step_idx, value) in steps {
                        slice.set_step(step_idx, value);
                    }
                }
            }
            AppCommand::ClearSteps {
                channel,
                pattern,
                start_step,
                end_step,
            } => {
                let pattern_length = self.patterns.get(pattern).map(|p| p.length).unwrap_or(16);
                if let Some(ch) = self.channels.get_mut(channel) {
                    let slice = ch.get_or_create_pattern(pattern, pattern_length);
                    for step_idx in start_step..=end_step {
                        slice.set_step(step_idx, false);
                    }
                }
            }
            AppCommand::BatchSetSteps {
                pattern,
                operations,
            } => {
                let pattern_length = self.patterns.get(pattern).map(|p| p.length).unwrap_or(16);
                for (channel, step_idx, value) in operations {
                    if let Some(ch) = self.channels.get_mut(channel) {
                        let slice = ch.get_or_create_pattern(pattern, pattern_length);
                        slice.set_step(step_idx, value);
                    }
                }
            }
            AppCommand::BatchClearSteps {
                pattern,
                operations,
            } => {
                let pattern_length = self.patterns.get(pattern).map(|p| p.length).unwrap_or(16);
                for (channel, start_step, end_step) in operations {
                    if let Some(ch) = self.channels.get_mut(channel) {
                        let slice = ch.get_or_create_pattern(pattern, pattern_length);
                        for step_idx in start_step..=end_step {
                            slice.set_step(step_idx, false);
                        }
                    }
                }
            }

            // ================================================================
            // Piano roll
            // ================================================================
            AppCommand::AddNote {
                channel,
                pattern,
                note,
            } => {
                let pattern_length = self.patterns.get(pattern).map(|p| p.length).unwrap_or(16);
                if let Some(ch) = self.channels.get_mut(channel) {
                    let slice = ch.get_or_create_pattern(pattern, pattern_length);
                    slice.add_note(note);
                }
            }
            AppCommand::DeleteNote {
                channel,
                pattern,
                pitch,
                start_step,
            } => {
                if let Some(ch) = self.channels.get_mut(channel) {
                    if let Some(slice) = ch.pattern_data.get_mut(&pattern) {
                        // Find the note by pitch and start_step, then remove it
                        if let Some(idx) = slice
                            .notes
                            .iter()
                            .position(|n| n.pitch == pitch && n.start_step == start_step)
                        {
                            slice.notes.remove(idx);
                        }
                    }
                }
            }
            AppCommand::BatchAddNotes {
                channel,
                pattern,
                notes,
            } => {
                let pattern_length = self.patterns.get(pattern).map(|p| p.length).unwrap_or(16);
                if let Some(ch) = self.channels.get_mut(channel) {
                    let slice = ch.get_or_create_pattern(pattern, pattern_length);
                    for note in notes {
                        slice.add_note(note);
                    }
                }
            }
            AppCommand::BatchDeleteNotes {
                channel,
                pattern,
                positions,
            } => {
                if let Some(ch) = self.channels.get_mut(channel) {
                    if let Some(slice) = ch.pattern_data.get_mut(&pattern) {
                        for (pitch, start_step) in positions {
                            if let Some(idx) = slice
                                .notes
                                .iter()
                                .position(|n| n.pitch == pitch && n.start_step == start_step)
                            {
                                slice.notes.remove(idx);
                            }
                        }
                    }
                }
            }

            // ================================================================
            // Playlist / Arrangement
            // ================================================================
            AppCommand::PlacePattern { pattern_id, bar } => {
                use crate::arrangement::PatternPlacement;
                self.arrangement
                    .add_placement(PatternPlacement::new(pattern_id, bar));
            }
            AppCommand::RemovePlacement { pattern_id, bar } => {
                self.arrangement
                    .remove_placements_in_range(pattern_id, bar, bar);
            }
            AppCommand::TogglePatternMute(pattern_id) => {
                self.arrangement.toggle_pattern_mute(pattern_id);
            }

            // ================================================================
            // Mixer
            // ================================================================
            AppCommand::SetTrackVolume { track, volume } => {
                self.mixer.set_volume(TrackId(track), volume);
                self.audio_sync.mark_mixer_dirty();
            }
            AppCommand::SetTrackPan { track, pan } => {
                self.mixer.set_pan(TrackId(track), pan);
                self.audio_sync.mark_mixer_dirty();
            }
            AppCommand::ToggleTrackMute(track) => {
                self.mixer.toggle_mute(TrackId(track));
                self.audio_sync.mark_mixer_dirty();
            }
            AppCommand::ToggleTrackSolo(track) => {
                self.mixer.toggle_solo(TrackId(track));
                self.audio_sync.mark_mixer_dirty();
            }
            AppCommand::ResetTrackVolume(track) => {
                self.mixer.set_volume(TrackId(track), 0.8);
                self.audio_sync.mark_mixer_dirty();
            }
            AppCommand::ResetTrackPan(track) => {
                self.mixer.set_pan(TrackId(track), 0.0);
                self.audio_sync.mark_mixer_dirty();
            }

            // ================================================================
            // Effects
            // ================================================================
            AppCommand::AddEffect {
                track,
                slot,
                effect_type,
            } => {
                use crate::effects::EffectSlot;
                let effect_slot = EffectSlot::new(effect_type);
                self.mixer.tracks[track].effects[slot] = Some(effect_slot);
                self.audio.set_effect(track, slot, Some(effect_type));
            }
            AppCommand::RemoveEffect { track, slot } => {
                self.mixer.tracks[track].effects[slot] = None;
                self.audio.set_effect(track, slot, None);
            }
            AppCommand::SetEffectParam {
                track,
                slot,
                param,
                value,
            } => {
                if let Some(ref mut effect_slot) = self.mixer.tracks[track].effects[slot] {
                    effect_slot.params.insert(param, value);
                }
                self.audio.set_effect_param(track, slot, param, value);
            }
            AppCommand::ToggleEffectBypass { track, slot } => {
                if let Some(ref mut effect_slot) = self.mixer.tracks[track].effects[slot] {
                    effect_slot.bypassed = !effect_slot.bypassed;
                    let enabled = !effect_slot.bypassed;
                    self.audio.set_effect_enabled(track, slot, enabled);
                }
            }
        }
    }

    /// Get the current pattern
    pub fn get_current_pattern(&self) -> Option<&Pattern> {
        self.patterns.get(self.current_pattern)
    }

    /// Called when the terminal is resized
    pub fn on_resize(&mut self, width: u16, height: u16) {
        self.ui.terminal_width = width;
        self.ui.terminal_height = height;
    }

    /// Called every frame to update state
    pub fn tick(&mut self, delta: Duration) {
        // Always update peak levels for mixer meters (even when not playing)
        self.update_peak_levels();

        // Flush any pending audio sync changes (batched per frame)
        // Destructure to allow simultaneous borrows of different fields
        let AppState {
            audio_sync,
            audio,
            mixer,
            ..
        } = &mut self.state;
        audio_sync.flush(audio, mixer);

        if !self.transport.playback.is_playing() {
            return;
        }

        // Accumulate time and advance steps as needed
        self.transport.add_time(delta);
        while self.transport.should_advance() {
            self.transport.consume_step();
            self.advance_step();
        }
    }

    /// Advance to the next step and trigger audio
    fn advance_step(&mut self) {
        // Check if we're about to loop (step is 15 and will become 0)
        let will_loop = self
            .transport
            .playback
            .current_step()
            .map(|s| s.0 == 15)
            .unwrap_or(false);

        // Save the current bar BEFORE advancing (for stopping notes from the old bar)
        let old_bar = self.transport.playback.bar_or_zero();

        // Advance the playback state machine
        let events = self.transport.playback.advance();

        // When pattern loops to step 0, stop notes that span the loop boundary
        // Pass the OLD bar so we stop notes from the pattern that was playing
        if will_loop {
            self.stop_spanning_notes(old_bar);
        }

        // Handle playback events
        for event in events {
            self.handle_playback_event(event);
        }
    }

    /// Handle a playback event
    fn handle_playback_event(&mut self, event: PlaybackEvent) {
        match event {
            PlaybackEvent::Step { step: _ } => {
                self.play_current_step();
            }
            PlaybackEvent::PatternLoop => {
                // Pattern looped - could trigger visual feedback
            }
            PlaybackEvent::BarAdvance { bar: _ } => {
                // Bar advanced - could trigger visual feedback
            }
        }
    }

    /// Stop notes that span the loop boundary (started before step 16 but end after)
    /// The `bar` parameter should be the bar that was playing BEFORE the loop occurred.
    fn stop_spanning_notes(&self, bar: usize) {
        // Get patterns to check based on play mode
        let patterns_to_check: Vec<&crate::sequencer::Pattern> =
            if self.transport.playback.is_playing_arrangement() {
                // Get all active patterns at the specified bar (the OLD bar before loop)
                self.arrangement
                    .get_active_placements_at_bar(bar)
                    .iter()
                    .filter_map(|p| self.patterns.get(p.pattern_id))
                    .collect()
            } else {
                self.get_current_pattern().into_iter().collect()
            };

        for pattern in patterns_to_check {
            for (channel_idx, channel) in self.channels.iter().enumerate() {
                if let ChannelSource::Plugin { .. } = &channel.source {
                    // Find notes that span the boundary (start + duration >= 16)
                    // This catches notes that end at or after the loop point
                    if let Some(slice) = channel.get_pattern(pattern.id) {
                        for note in &slice.notes {
                            if note.start_step + note.duration >= 16 {
                                self.audio.plugin_note_off(channel_idx, note.pitch);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Play all active samples at the current step
    fn play_current_step(&self) {
        if self.transport.playback.is_playing_arrangement() {
            self.play_arrangement_step();
        } else {
            self.play_pattern_step();
        }
    }

    /// Play step from current pattern (pattern loop mode)
    fn play_pattern_step(&self) {
        let patterns = self.get_current_pattern().into_iter();
        self.play_step_from_patterns(patterns, self.transport.playback.step_or_zero());
    }

    /// Play step from arrangement (all active patterns at current bar)
    fn play_arrangement_step(&self) {
        let bar = self.transport.playback.bar_or_zero();
        let placements = self.arrangement.get_active_placements_at_bar(bar);
        let patterns = placements
            .iter()
            .filter_map(|p| self.patterns.get(p.pattern_id));
        self.play_step_from_patterns(patterns, self.transport.playback.step_or_zero());
    }

    /// Play a step from the given patterns (unified playback logic)
    fn play_step_from_patterns<'a>(
        &self,
        patterns: impl Iterator<Item = &'a Pattern>,
        step: usize,
    ) {
        for pattern in patterns {
            for (channel_idx, channel) in self.channels.iter().enumerate() {
                // Get the mixer track this channel routes to
                let track_id = TrackId(channel.mixer_track);

                // Skip if track is muted or not audible (solo logic)
                if !self.mixer.is_track_audible(track_id) {
                    continue;
                }

                self.play_channel_step(channel_idx, channel, track_id, pattern, step);
            }
        }
    }

    /// Play a single channel's step from a pattern
    fn play_channel_step(
        &self,
        channel_idx: usize,
        channel: &Channel,
        track_id: TrackId,
        pattern: &Pattern,
        step: usize,
    ) {
        // Get volume from the mixer track
        let volume = self.mixer.track(track_id).volume;

        // Get pattern data for this channel
        let slice = channel.get_pattern(pattern.id);

        match &channel.source {
            ChannelSource::Sampler { path } => {
                // Sampler channels use step sequencer grid
                if slice.map(|s| s.get_step(step)).unwrap_or(false) {
                    if let Some(ref sample_path) = path {
                        let full_path = self.project.samples_path().join(sample_path);
                        self.audio.play_sample(&full_path, volume, channel_idx);
                    }
                }
            }
            ChannelSource::Plugin { .. } => {
                // Plugin channels use piano roll notes
                if let Some(slice) = slice {
                    for note in &slice.notes {
                        if note.start_step == step {
                            self.audio
                                .plugin_note_on(channel_idx, note.pitch, note.velocity);
                        }
                        // Check for note-off events (notes that end at this step)
                        if note.start_step + note.duration == step {
                            self.audio.plugin_note_off(channel_idx, note.pitch);
                        }
                    }
                }
            }
        }
    }

    /// Toggle play/stop
    pub fn toggle_play(&mut self) {
        if self.transport.playback.is_playing() {
            // Stop playback
            self.stop_all_plugin_notes();
            self.transport.playback.stop();
            self.transport.reset_accumulator();
            self.audio.stop_all();
        } else {
            // Start playback based on focused panel
            if self.ui.mode.current_panel() == Panel::Playlist {
                // Start from cursor position in playlist (col 0 is mute, so bar = col - 1)
                let start_bar = self.ui.cursors.playlist.bar.saturating_sub(1);
                self.transport.playback.play_arrangement_from(start_bar);
            } else {
                self.transport.playback.play_pattern();
            }
            // Play the first step immediately
            self.play_current_step();
        }
    }

    /// Check if currently playing (for backward compatibility)
    pub fn is_playing(&self) -> bool {
        self.transport.playback.is_playing()
    }

    /// Get current playhead step (for backward compatibility)
    pub fn playhead_step(&self) -> usize {
        self.transport.playback.step_or_zero()
    }

    /// Get current arrangement bar
    pub fn arrangement_bar(&self) -> usize {
        self.transport.playback.bar_or_zero()
    }

    /// Check if playing arrangement (not pattern)
    pub fn is_playing_arrangement(&self) -> bool {
        self.transport.playback.is_playing_arrangement()
    }

    /// Stop all notes on plugin channels (all notes off)
    fn stop_all_plugin_notes(&self) {
        for (channel_idx, channel) in self.channels.iter().enumerate() {
            if let ChannelSource::Plugin { .. } = &channel.source {
                // Send note_off for all possible MIDI notes (0-127)
                // This is a brute-force approach but ensures all notes stop
                for note in 0..=127u8 {
                    self.audio.plugin_note_off(channel_idx, note);
                }
            }
        }
    }

    /// Cycle to the next panel
    pub fn next_panel(&mut self) {
        self.ui
            .mode
            .next_panel(self.ui.show_browser, self.ui.show_mixer, self.ui.view_mode);
    }

    /// Set the view mode and focus
    ///
    /// Records the current position in the global jumplist before switching,
    /// enabling Ctrl+O/Ctrl+I navigation between views.
    pub fn set_view_mode(&mut self, view_mode: ViewMode) {
        // Record current position before switching (if actually changing views)
        if self.ui.view_mode != view_mode {
            let current = self.current_jump_position();
            self.ui.global_jumplist.push(current);
        }

        self.ui.view_mode = view_mode;
        let panel = match view_mode {
            ViewMode::ChannelRack => Panel::ChannelRack,
            ViewMode::PianoRoll => Panel::PianoRoll,
            ViewMode::Playlist => Panel::Playlist,
        };
        self.ui.mode.switch_panel(panel);
    }

    /// Toggle browser visibility
    ///
    /// Records current position in jumplist when opening browser,
    /// so Ctrl+O can return to it.
    pub fn toggle_browser(&mut self) {
        self.ui.show_browser = !self.ui.show_browser;
        if self.ui.show_browser {
            // Record current position before switching to browser
            let current = self.current_jump_position();
            self.ui.global_jumplist.push(current);
            self.ui.mode.switch_panel(Panel::Browser);
        } else if self.ui.mode.current_panel() == Panel::Browser {
            let panel = match self.ui.view_mode {
                ViewMode::ChannelRack => Panel::ChannelRack,
                ViewMode::PianoRoll => Panel::PianoRoll,
                ViewMode::Playlist => Panel::Playlist,
            };
            self.ui.mode.switch_panel(panel);
        }
    }

    /// Toggle mixer visibility
    ///
    /// Records current position in jumplist when opening mixer,
    /// so Ctrl+O can return to it.
    pub fn toggle_mixer(&mut self) {
        self.ui.show_mixer = !self.ui.show_mixer;
        if self.ui.show_mixer {
            // Record current position before switching to mixer
            let current = self.current_jump_position();
            self.ui.global_jumplist.push(current);
            self.ui.mode.switch_panel(Panel::Mixer);
        } else if self.ui.mode.current_panel() == Panel::Mixer {
            let panel = match self.ui.view_mode {
                ViewMode::ChannelRack => Panel::ChannelRack,
                ViewMode::PianoRoll => Panel::PianoRoll,
                ViewMode::Playlist => Panel::Playlist,
            };
            self.ui.mode.switch_panel(panel);
        }
    }

    /// Toggle the event log panel visibility
    pub fn toggle_event_log(&mut self) {
        self.ui.show_event_log = !self.ui.show_event_log;
        // Event log is a utility panel - no jumplist or panel switching needed
    }

    /// Show the projects modal
    pub fn show_projects_modal(&mut self) {
        let current = self.state.project.name.clone();
        self.ui.projects_modal.show(Some(&current));
    }

    /// Start the export flow (show filename prompt)
    pub fn start_export(&mut self) {
        let default_filename = format!("{}.wav", self.state.project.name);
        self.ui.command_picker.start_export_input(&default_filename);
    }

    /// Perform the actual export to WAV
    pub fn do_export(&mut self, filename: &str) {
        use crate::audio::offline::{render_offline, write_wav, RenderConfig};

        let config = RenderConfig {
            sample_rate: 44100,
            bpm: self.transport.bpm,
            steps_per_bar: 16,
        };

        let samples_path = self.project.samples_path();
        let plugins_path = self.project.plugins_path();

        self.log_event("exporting...", false);

        let samples = render_offline(
            &self.state.channels,
            &self.state.patterns,
            &self.arrangement,
            &self.mixer,
            &samples_path,
            &plugins_path,
            &*self.plugin_loader,
            &config,
        );

        if samples.is_empty() {
            self.log_event("nothing to export (arrangement is empty)", false);
            return;
        }

        let output_path = self.project.path.join(filename);
        match write_wav(&output_path, &samples, config.sample_rate) {
            Ok(()) => {
                self.log_event("export complete", false);
            }
            Err(_e) => {
                self.log_event("export failed", false);
            }
        }
    }

    /// Hide the projects modal
    pub fn hide_projects_modal(&mut self) {
        self.ui.projects_modal.hide();
    }

    /// Load a project file into the app state
    pub fn load_project(&mut self, project_file: ProjectFile, path: std::path::PathBuf) {
        // Stop playback first
        if self.transport.playback.is_playing() {
            self.transport.playback.stop();
        }

        // Update project info - use directory name as canonical project name
        let project_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| project_file.name.clone());
        self.state.project = ProjectState::new(&project_name, path, project_file.created_at);

        // Load channels
        self.state.channels = project_file.channels;

        // Load patterns
        self.state.patterns = project_file.patterns;

        // Load arrangement
        self.state.arrangement = project_file.arrangement;

        // Load mixer or create default
        self.state.mixer = project_file
            .mixer
            .unwrap_or_else(|| Self::create_default_mixer(&self.state.channels));

        // Load transport settings
        self.state.transport = TransportState::new(project_file.bpm);

        // Reset current pattern
        self.state.current_pattern = project_file.current_pattern;

        // Clear history for new project
        self.state.history = History::new();

        // Mark as clean (freshly loaded)
        self.state.dirty = false;

        // Mark all routing as dirty so it gets synced to audio thread
        for (idx, channel) in self.state.channels.iter().enumerate() {
            self.state
                .audio_sync
                .mark_routing_dirty(idx, channel.mixer_track);
        }

        // Sync all effects to audio
        self.sync_all_effects_to_audio();
    }

    /// Get the current step index (0-15) from cursor column
    /// Returns 0 if in sample or mute zone
    pub fn cursor_step(&self) -> usize {
        self.ui.cursors.channel_rack.col.to_step_or_zero()
    }

    /// Get the current zone name
    pub fn cursor_zone(&self) -> &'static str {
        self.ui.cursors.channel_rack.col.zone_name()
    }

    /// Toggle step at cursor in channel rack (only works in steps zone)
    #[allow(dead_code)]
    pub fn toggle_step(&mut self) {
        if !self.ui.cursors.channel_rack.col.is_step_zone() {
            return; // Not in steps zone
        }
        let channel_idx = self.ui.cursors.channel_rack.channel;
        let step = self.ui.cursors.channel_rack.col.to_step_or_zero();
        let pattern_id = self.current_pattern;
        let pattern_length = self
            .patterns
            .get(pattern_id)
            .map(|p| p.length)
            .unwrap_or(16);

        if let Some(channel) = self.channels.get_mut(channel_idx) {
            let slice = channel.get_or_create_pattern(pattern_id, pattern_length);
            slice.toggle_step(step);
            self.mark_dirty();
        }
    }

    /// Channel count
    #[allow(dead_code)]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Adjust mixer track volume
    pub fn adjust_mixer_volume(&mut self, delta: f32) {
        let track_id = TrackId(self.mixer.selected_track);
        let new_volume = (self.mixer.track(track_id).volume + delta).clamp(0.0, 1.0);
        self.mixer.set_volume(track_id, new_volume);
        self.audio_sync.mark_mixer_dirty();
        self.mark_dirty();
    }

    /// Toggle mute on selected mixer track
    pub fn toggle_mute(&mut self) {
        let track_id = TrackId(self.mixer.selected_track);
        self.mixer.toggle_mute(track_id);
        self.audio_sync.mark_mixer_dirty();
        self.mark_dirty();
    }

    /// Toggle solo on selected mixer track
    pub fn toggle_solo(&mut self) {
        let track_id = TrackId(self.mixer.selected_track);
        self.mixer.toggle_solo(track_id);
        self.audio_sync.mark_mixer_dirty();
        self.mark_dirty();
    }

    /// Find an available mixer track (1-15) that's not currently in use by any channel.
    /// If all tracks are in use, returns track 1 (allowing sharing).
    pub fn find_available_mixer_track(&self) -> usize {
        let used_tracks: std::collections::HashSet<usize> =
            self.channels.iter().map(|c| c.mixer_track).collect();

        // Find first available track from 1-15
        for track in 1..=15 {
            if !used_tracks.contains(&track) {
                return track;
            }
        }

        // All tracks in use, return 1 (will share)
        1
    }

    /// Get a channel by its slot number (not Vec index)
    /// Returns None if no channel exists at that slot
    pub fn get_channel_at_slot(&self, slot: usize) -> Option<&Channel> {
        self.channels.iter().find(|c| c.slot == slot)
    }

    /// Get a mutable channel by its slot number (not Vec index)
    /// Returns None if no channel exists at that slot
    pub fn get_channel_at_slot_mut(&mut self, slot: usize) -> Option<&mut Channel> {
        self.channels.iter_mut().find(|c| c.slot == slot)
    }

    /// Set a channel's sample path
    /// Creates the channel at the specified slot if it doesn't exist (sparse)
    pub fn set_channel_sample(&mut self, slot: usize, sample_path: String) {
        // Extract filename without extension for channel name
        let name = std::path::Path::new(&sample_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Sample")
            .to_string();

        // Search for existing channel with this slot
        if let Some(idx) = self.channels.iter().position(|c| c.slot == slot) {
            // Update existing channel
            let channel = &mut self.channels[idx];
            channel.name = name;
            channel.source = ChannelSource::Sampler {
                path: Some(sample_path),
            };
            // Capture mixer_track before dropping the mutable borrow
            let mixer_track = channel.mixer_track;
            // Re-sync routing to audio thread to ensure consistency
            self.audio.set_generator_track(idx, mixer_track);
        } else {
            // Create new channel with unique mixer track
            let mixer_track = self.find_available_mixer_track();
            let channel = Channel::with_sample_at_slot(&name, &sample_path, slot, mixer_track);
            self.channels.push(channel);
            // Update mixer routing
            let channel_idx = self.channels.len() - 1;
            self.mixer.auto_assign_generator(channel_idx);
            // Sync routing to audio thread so audio goes to the correct mixer track
            self.audio.set_generator_track(channel_idx, mixer_track);
        }
        self.mark_dirty();
    }

    /// Set a channel as a plugin channel and load the plugin
    /// Creates the channel at the specified slot if it doesn't exist (sparse)
    pub fn set_channel_plugin(&mut self, slot: usize, plugin_path: String) {
        // Extract plugin name without extension for channel name
        let name = std::path::Path::new(&plugin_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Plugin")
            .to_string();

        // Find or create channel at this slot
        let channel_idx = if let Some(idx) = self.channels.iter().position(|c| c.slot == slot) {
            // Update existing channel
            let channel = &mut self.channels[idx];
            channel.name = name;
            channel.source = ChannelSource::Plugin {
                path: plugin_path.clone(),
                params: std::collections::HashMap::new(),
            };
            // Capture mixer_track before dropping the mutable borrow
            let mixer_track = channel.mixer_track;
            // Re-sync routing to audio thread to ensure consistency
            self.audio.set_generator_track(idx, mixer_track);
            idx
        } else {
            // Create new channel with unique mixer track
            let mixer_track = self.find_available_mixer_track();
            let channel = Channel::with_plugin_at_slot(&name, &plugin_path, slot, mixer_track);
            self.channels.push(channel);
            let idx = self.channels.len() - 1;
            self.mixer.auto_assign_generator(idx);
            // Sync routing to audio thread so audio goes to the correct mixer track
            self.audio.set_generator_track(idx, mixer_track);
            idx
        };

        self.mark_dirty();

        // Load and activate the plugin using the loader trait
        let sample_rate = self.audio.sample_rate() as f64;
        let buffer_size = 512;
        let full_plugin_path = self.project.plugins_path().join(&plugin_path);

        match self
            .plugin_loader
            .load_plugin(&full_plugin_path, sample_rate, buffer_size)
        {
            Ok(loaded) => {
                // For newly assigned plugin, use default params
                let channel = &self.channels[channel_idx];
                let init_state = self.build_plugin_init_state(channel_idx, channel);
                self.audio
                    .send_plugin(channel_idx, loaded.processor, init_state);
            }
            Err(e) => {
                eprintln!("Failed to load plugin for channel {}: {}", channel_idx, e);
            }
        }
    }

    /// Start previewing a channel (called on key press)
    /// Takes slot number, finds Vec index for audio engine
    pub fn start_preview(&mut self, slot: usize) {
        // Find Vec index for audio engine
        if let Some(vec_idx) = self.channels.iter().position(|c| c.slot == slot) {
            let channel = &self.channels[vec_idx];
            match &channel.source {
                ChannelSource::Sampler { path } => {
                    if let Some(ref sample_path) = path {
                        let full_path = self.project.samples_path().join(sample_path);
                        self.audio.preview_sample(&full_path, vec_idx);
                    }
                    // Set previewing to prevent key repeat from re-triggering
                    self.ui.is_previewing = true;
                    self.ui.preview_channel = Some(vec_idx);
                }
                ChannelSource::Plugin { .. } => {
                    // Play a test note (middle C) for plugin preview
                    let note = 60u8;
                    self.audio.plugin_note_on(vec_idx, note, 0.8);
                    self.ui.is_previewing = true;
                    self.ui.preview_channel = Some(vec_idx);
                    self.ui.preview_note = Some(note);
                }
            }
        }
    }

    /// Stop previewing a channel (called on key release)
    pub fn stop_preview(&mut self, channel_idx: usize) {
        if self.ui.is_previewing {
            if let Some(note) = self.ui.preview_note {
                // Send note off to stop the preview
                self.audio.plugin_note_off(channel_idx, note);
            }
            self.ui.is_previewing = false;
            self.ui.preview_channel = None;
            self.ui.preview_note = None;
        }
    }

    /// Preview the current note in piano roll (for plugin channels)
    pub fn preview_piano_note(&mut self) {
        let slot = self.ui.cursors.channel_rack.channel;
        // Find Vec index for audio engine
        if let Some(vec_idx) = self.channels.iter().position(|c| c.slot == slot) {
            if let ChannelSource::Plugin { .. } = &self.channels[vec_idx].source {
                let note = self.ui.cursors.piano_roll.pitch;
                self.audio.plugin_note_on(vec_idx, note, 0.8);
                self.ui.is_previewing = true;
                self.ui.preview_channel = Some(vec_idx);
                self.ui.preview_note = Some(note);
            }
        }
    }

    /// Mark the project as dirty (needs saving)
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.last_change = Instant::now();
    }

    /// Auto-save if needed (debounced)
    pub fn maybe_auto_save(&mut self) {
        if self.dirty && self.last_change.elapsed() > Duration::from_millis(500) {
            self.save_project();
            self.dirty = false;
        }
    }

    /// Save the project to disk
    pub fn save_project(&self) {
        let project_file = ProjectFile::from_state(
            &self.project.name,
            self.transport.bpm,
            self.current_pattern,
            &self.channels,
            &self.patterns,
            &self.arrangement,
            &self.mixer,
            Some(self.project.created_at),
        );

        if let Err(e) = project::save_project(&self.project.path, &project_file) {
            eprintln!("Failed to save project: {}", e);
        }
    }

    /// Update peak levels from audio thread (call every frame)
    pub fn update_peak_levels(&mut self) {
        self.mixer.peak_levels = self.audio.get_peak_levels();
    }

    // ============ Effect Management ============

    /// Open effect editor for the currently selected slot
    /// If slot is empty, opens effect type picker instead
    pub fn open_effect_editor(&mut self) {
        let track_idx = self.mixer.selected_track;
        let slot_idx = self.mixer.selected_effect_slot;

        let has_effect = self.mixer.tracks[track_idx].effects[slot_idx].is_some();

        if has_effect {
            self.ui.mode.open_effect_editor(track_idx, slot_idx);
        } else {
            self.ui.mode.open_effect_picker(track_idx, slot_idx);
        }
    }

    /// Toggle bypass on the currently selected effect
    pub fn toggle_effect_bypass(&mut self) {
        let track_idx = self.mixer.selected_track;
        let slot_idx = self.mixer.selected_effect_slot;

        if let Some(ref mut slot) = self.mixer.tracks[track_idx].effects[slot_idx] {
            slot.bypassed = !slot.bypassed;
            // Capture enabled state before dropping the mutable borrow
            let enabled = !slot.bypassed;
            self.audio.set_effect_enabled(track_idx, slot_idx, enabled);
            self.mark_dirty();
        }
    }

    /// Delete the effect in the currently selected slot
    pub fn delete_effect(&mut self) {
        let track_idx = self.mixer.selected_track;
        let slot_idx = self.mixer.selected_effect_slot;

        if self.mixer.tracks[track_idx].effects[slot_idx].is_some() {
            self.mixer.tracks[track_idx].effects[slot_idx] = None;
            // Tell audio thread to remove the effect
            self.audio.set_effect(track_idx, slot_idx, None);
            self.mark_dirty();
        }
    }

    /// Add an effect to the currently selected slot
    pub fn add_effect(&mut self, effect_type: EffectType) {
        let track_idx = self.mixer.selected_track;
        let slot_idx = self.mixer.selected_effect_slot;

        // Create the effect slot with default parameters
        let slot = EffectSlot::new(effect_type);
        self.mixer.tracks[track_idx].effects[slot_idx] = Some(slot);

        // Tell audio thread to add the effect
        self.audio
            .set_effect(track_idx, slot_idx, Some(effect_type));
        self.mark_dirty();
    }

    /// Set an effect parameter value
    pub fn set_effect_param(&mut self, param_id: crate::effects::EffectParamId, value: f32) {
        if let AppMode::EffectEditor {
            track_idx,
            slot_idx,
            ..
        } = self.ui.mode
        {
            if let Some(ref mut slot) = self.mixer.tracks[track_idx].effects[slot_idx] {
                slot.set_param(param_id, value);
                self.audio
                    .set_effect_param(track_idx, slot_idx, param_id, value);
                self.mark_dirty();
            }
        }
    }

    /// Sync all effects for a track to the audio thread
    #[allow(dead_code)]
    pub fn sync_effects_to_audio(&self, track_idx: usize) {
        for slot_idx in 0..EFFECT_SLOTS {
            if let Some(ref slot) = self.mixer.tracks[track_idx].effects[slot_idx] {
                self.audio
                    .set_effect(track_idx, slot_idx, Some(slot.effect_type));
                for (param_id, value) in &slot.params {
                    self.audio
                        .set_effect_param(track_idx, slot_idx, *param_id, *value);
                }
                self.audio
                    .set_effect_enabled(track_idx, slot_idx, !slot.bypassed);
            }
        }
    }

    /// Sync all effects from all tracks to the audio thread (called on project load)
    pub fn sync_all_effects_to_audio(&self) {
        for track_idx in 0..crate::mixer::NUM_TRACKS {
            self.sync_effects_to_audio(track_idx);
        }
    }

    // ============ Undo/Redo History Helpers ============

    /// Toggle step at cursor with undo/redo support
    pub fn toggle_step_with_history(&mut self) {
        use crate::history::command::ToggleStepCmd;

        if !self.ui.cursors.channel_rack.col.is_step_zone() {
            return;
        }

        let cmd = Box::new(ToggleStepCmd::new(
            self.current_pattern,
            self.ui.cursors.channel_rack.channel,
            self.ui.cursors.channel_rack.col.to_step_or_zero(),
        ));
        // Take history out temporarily to avoid borrow conflict
        let mut history = std::mem::take(&mut self.history);
        history.execute(cmd, self);
        self.history = history;
    }

    /// Add a note to the piano roll with undo/redo support
    pub fn add_note_with_history(&mut self, note: Note) {
        use crate::history::command::AddNoteCmd;

        let cmd = Box::new(AddNoteCmd::new(
            self.current_pattern,
            self.ui.cursors.channel_rack.channel,
            note,
        ));
        // Take history out temporarily to avoid borrow conflict
        let mut history = std::mem::take(&mut self.history);
        history.execute(cmd, self);
        self.history = history;
    }

    /// Remove a note from the piano roll with undo/redo support
    pub fn remove_note_with_history(&mut self, note_id: String) {
        use crate::history::command::RemoveNoteCmd;

        let cmd = Box::new(RemoveNoteCmd::new(
            self.current_pattern,
            self.ui.cursors.channel_rack.channel,
            note_id,
        ));
        // Take history out temporarily to avoid borrow conflict
        let mut history = std::mem::take(&mut self.history);
        history.execute(cmd, self);
        self.history = history;
    }

    /// Toggle placement in playlist with undo/redo support
    pub fn toggle_placement_with_history(&mut self, pattern_id: usize, bar: usize) {
        use crate::history::command::TogglePlacementCmd;

        let cmd = Box::new(TogglePlacementCmd::new(pattern_id, bar));
        // Take history out temporarily to avoid borrow conflict
        let mut history = std::mem::take(&mut self.history);
        history.execute(cmd, self);
        self.history = history;
    }

    // ============ Jump List Helpers ============

    /// Get current position for the global jump list
    pub fn current_jump_position(&self) -> JumpPosition {
        match self.ui.view_mode {
            ViewMode::ChannelRack => JumpPosition::channel_rack(
                self.ui.cursors.channel_rack.channel,
                self.ui.cursors.channel_rack.col.to_step_or_zero(),
            ),
            ViewMode::PianoRoll => {
                // Convert pitch to row (higher pitch = lower row number)
                let pitch_row = (84 - self.ui.cursors.piano_roll.pitch) as usize;
                JumpPosition::piano_roll(pitch_row, self.ui.cursors.piano_roll.step)
            }
            ViewMode::Playlist => {
                JumpPosition::playlist(self.ui.cursors.playlist.row, self.ui.cursors.playlist.bar)
            }
        }
    }

    /// Navigate to a jump position (may switch views)
    ///
    /// NOTE: This directly sets view_mode WITHOUT calling set_view_mode(),
    /// because we don't want to record jumps during Ctrl+O/Ctrl+I navigation.
    pub fn goto_jump_position(&mut self, pos: &JumpPosition) {
        // Switch view directly (don't call set_view_mode to avoid recording jump)
        self.ui.view_mode = pos.view;

        // Switch panel focus to match the view
        let panel = match pos.view {
            ViewMode::ChannelRack => Panel::ChannelRack,
            ViewMode::PianoRoll => Panel::PianoRoll,
            ViewMode::Playlist => Panel::Playlist,
        };
        self.ui.mode.switch_panel(panel);

        // Set cursor position and scroll viewport based on view
        match pos.view {
            ViewMode::ChannelRack => {
                self.ui.cursors.channel_rack.channel =
                    pos.row.min(self.channels.len().saturating_sub(1));
                // Convert step to AppCol (step zone starts at col 3 in vim space)
                self.ui.cursors.channel_rack.col = AppCol::from_step(pos.col);
                // Scroll viewport to keep cursor visible
                let visible_rows = 15;
                if self.ui.cursors.channel_rack.channel
                    >= self.ui.cursors.channel_rack.viewport_top + visible_rows
                {
                    self.ui.cursors.channel_rack.viewport_top =
                        self.ui.cursors.channel_rack.channel - visible_rows + 1;
                }
                if self.ui.cursors.channel_rack.channel < self.ui.cursors.channel_rack.viewport_top
                {
                    self.ui.cursors.channel_rack.viewport_top =
                        self.ui.cursors.channel_rack.channel;
                }
            }
            ViewMode::PianoRoll => {
                // Convert row back to pitch (row 0 = pitch 84, row 48 = pitch 36)
                self.ui.cursors.piano_roll.pitch = (84 - pos.row as i32).clamp(36, 84) as u8;
                self.ui.cursors.piano_roll.step = pos.col.min(15);
                // Scroll viewport to keep cursor visible (viewport_top is highest visible pitch)
                if self.ui.cursors.piano_roll.pitch > self.ui.cursors.piano_roll.viewport_top {
                    self.ui.cursors.piano_roll.viewport_top = self.ui.cursors.piano_roll.pitch;
                }
                let visible_rows = 20u8;
                if self.ui.cursors.piano_roll.pitch
                    < self
                        .ui
                        .cursors
                        .piano_roll
                        .viewport_top
                        .saturating_sub(visible_rows)
                {
                    self.ui.cursors.piano_roll.viewport_top = self.ui.cursors.piano_roll.pitch + 10;
                }
            }
            ViewMode::Playlist => {
                self.ui.cursors.playlist.row = pos.row.min(self.patterns.len().saturating_sub(1));
                self.ui.cursors.playlist.bar = pos.col.min(16);
                // Scroll viewport to keep cursor visible
                let visible_rows = 10;
                if self.ui.cursors.playlist.row
                    >= self.ui.cursors.playlist.viewport_top + visible_rows
                {
                    self.ui.cursors.playlist.viewport_top =
                        self.ui.cursors.playlist.row - visible_rows + 1;
                }
                if self.ui.cursors.playlist.row < self.ui.cursors.playlist.viewport_top {
                    self.ui.cursors.playlist.viewport_top = self.ui.cursors.playlist.row;
                }
            }
        }
    }
}

// ============================================================================
// Context Trait Implementations
// ============================================================================

impl StepGridContext for App {
    fn channel_count(&self) -> usize {
        self.channels.len()
    }

    fn pattern_length(&self) -> usize {
        self.patterns
            .get(self.current_pattern)
            .map(|p| p.length)
            .unwrap_or(16)
    }

    fn get_step(&self, channel: usize, step: usize) -> bool {
        self.channels
            .get(channel)
            .and_then(|c| c.get_pattern(self.current_pattern))
            .map(|s| s.get_step(step))
            .unwrap_or(false)
    }

    fn set_step(&mut self, channel: usize, step: usize, active: bool) {
        let pattern_id = self.current_pattern;
        let pattern_length = self.pattern_length();
        if let Some(ch) = self.channels.get_mut(channel) {
            let slice = ch.get_or_create_pattern(pattern_id, pattern_length);
            slice.set_step(step, active);
            self.mark_dirty();
        }
    }
}

impl PianoRollContext for App {
    fn notes(&self) -> &[Note] {
        let channel = self.ui.cursors.channel_rack.channel;
        let pattern_id = self.current_pattern;
        self.channels
            .get(channel)
            .and_then(|c| c.get_pattern(pattern_id))
            .map(|s| s.notes.as_slice())
            .unwrap_or(&[])
    }

    fn add_note(&mut self, note: Note) {
        let channel = self.ui.cursors.channel_rack.channel;
        let pattern_id = self.current_pattern;
        let pattern_length = self.pattern_length();
        if let Some(ch) = self.channels.get_mut(channel) {
            let slice = ch.get_or_create_pattern(pattern_id, pattern_length);
            slice.notes.push(note);
            self.mark_dirty();
        }
    }

    fn remove_note(&mut self, id: &str) -> Option<Note> {
        let channel = self.ui.cursors.channel_rack.channel;
        let pattern_id = self.current_pattern;
        let pattern_length = self.pattern_length();
        let removed = if let Some(ch) = self.channels.get_mut(channel) {
            let slice = ch.get_or_create_pattern(pattern_id, pattern_length);
            if let Some(idx) = slice.notes.iter().position(|n| n.id == id) {
                Some(slice.notes.remove(idx))
            } else {
                None
            }
        } else {
            None
        };
        if removed.is_some() {
            self.mark_dirty();
        }
        removed
    }
}

impl PlaylistContext for App {
    fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    fn has_placement(&self, pattern_id: usize, bar: usize) -> bool {
        self.arrangement
            .placements
            .iter()
            .any(|p| p.pattern_id == pattern_id && p.start_bar == bar)
    }

    fn add_placement(&mut self, pattern_id: usize, bar: usize) {
        use crate::arrangement::PatternPlacement;
        self.arrangement
            .placements
            .push(PatternPlacement::new(pattern_id, bar));
        self.mark_dirty();
    }

    fn remove_placement(&mut self, pattern_id: usize, bar: usize) {
        self.arrangement
            .placements
            .retain(|p| !(p.pattern_id == pattern_id && p.start_bar == bar));
        self.mark_dirty();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioCommand, AudioHandle};
    use crate::mixer::TrackId;
    use crossbeam_channel::Receiver;
    use tempfile::TempDir;

    /// Create a test App with dummy audio in a temp directory
    fn create_test_app() -> (App, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_path = temp_dir.path().join("test-project");
        std::fs::create_dir_all(&project_path).expect("Failed to create project dir");

        // Create a minimal empty project.json so template is not copied
        let empty_project = crate::project::ProjectFile::new("test-project");
        crate::project::save_project(&project_path, &empty_project)
            .expect("Failed to create test project");

        let audio = AudioHandle::dummy();
        let app = App::new(project_path.to_str().unwrap(), audio);
        (app, temp_dir)
    }

    /// Create a test App with a testable audio handle that captures commands
    fn create_test_app_with_audio_rx() -> (App, TempDir, Receiver<AudioCommand>) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_path = temp_dir.path().join("test-project");
        std::fs::create_dir_all(&project_path).expect("Failed to create project dir");

        // Create a minimal empty project.json so template is not copied
        let empty_project = crate::project::ProjectFile::new("test-project");
        crate::project::save_project(&project_path, &empty_project)
            .expect("Failed to create test project");

        let (audio, rx) = AudioHandle::testable();
        let app = App::new(project_path.to_str().unwrap(), audio);
        (app, temp_dir, rx)
    }

    // ========================================================================
    // Bug 1: New projects should start with no channels
    // ========================================================================

    /// Bug fix: On launch with a new project, there should be no prefilled channels.
    #[test]
    fn test_new_project_has_no_prefilled_channels() {
        let (app, _temp) = create_test_app();
        assert!(
            app.channels.is_empty(),
            "New project should start with 0 channels, got {}",
            app.channels.len()
        );
    }

    // ========================================================================
    // Bug 2: Adding to higher slot shouldn't auto-fill gaps
    // ========================================================================

    /// Bug fix: When adding a sample to channel 20, channels 0-19 should NOT
    /// be auto-created. Only the specific channel should exist.
    #[test]
    fn test_set_channel_sample_sparse() {
        let (mut app, _temp) = create_test_app();

        // Add a sample to channel index 5 (should be the only channel)
        app.set_channel_sample(5, "kick.wav".to_string());

        // Only ONE channel should exist - the one at index 5
        // Channels 0-4 should NOT be auto-filled
        assert_eq!(
            app.channels.len(),
            1,
            "Only the assigned channel should exist, got {} channels",
            app.channels.len()
        );
    }

    /// Verifies that multiple sparse channels can coexist without gap-filling
    #[test]
    fn test_multiple_sparse_channels() {
        let (mut app, _temp) = create_test_app();

        // Add samples at non-contiguous indices
        app.set_channel_sample(2, "kick.wav".to_string());
        app.set_channel_sample(7, "snare.wav".to_string());
        app.set_channel_sample(15, "hihat.wav".to_string());

        // Should have exactly 3 channels
        assert_eq!(
            app.channels.len(),
            3,
            "Should have exactly 3 channels, got {}",
            app.channels.len()
        );
    }

    // ========================================================================
    // Bug 3: Mute should only affect current channel, not all channels
    // ========================================================================

    /// Bug fix: Each channel should route to a unique mixer track by default,
    /// so muting one channel doesn't affect others.
    #[test]
    fn test_channels_have_unique_mixer_tracks() {
        let (mut app, _temp) = create_test_app();

        // Add multiple channels
        app.set_channel_sample(0, "kick.wav".to_string());
        app.set_channel_sample(1, "snare.wav".to_string());
        app.set_channel_sample(2, "hihat.wav".to_string());

        // Each channel should have a unique mixer_track
        let track0 = app.channels[0].mixer_track;
        let track1 = app.channels[1].mixer_track;
        let track2 = app.channels[2].mixer_track;

        assert_ne!(
            track0, track1,
            "Channels 0 and 1 should have different mixer tracks"
        );
        assert_ne!(
            track1, track2,
            "Channels 1 and 2 should have different mixer tracks"
        );
        assert_ne!(
            track0, track2,
            "Channels 0 and 2 should have different mixer tracks"
        );
    }

    /// Verifies that muting one channel's mixer track doesn't affect other channels
    #[test]
    fn test_mute_only_affects_single_channel() {
        let (mut app, _temp) = create_test_app();

        // Add two channels
        app.set_channel_sample(0, "kick.wav".to_string());
        app.set_channel_sample(1, "snare.wav".to_string());

        // Mute channel 0's mixer track
        let track0 = TrackId(app.channels[0].mixer_track);
        let track1 = TrackId(app.channels[1].mixer_track);
        app.mixer.toggle_mute(track0);

        // Channel 0's track should be muted
        assert!(
            app.mixer.track(track0).muted,
            "Channel 0's track should be muted"
        );
        // Channel 1's track should NOT be muted
        assert!(
            !app.mixer.track(track1).muted,
            "Channel 1's track should NOT be muted"
        );
    }

    // ========================================================================
    // Bug 4: Adding channel after delete should still work
    // ========================================================================

    /// Simplest reproduction: add 2 channels, delete first, add new at end
    #[test]
    fn test_add_channel_after_delete_simple() {
        let (mut app, _temp) = create_test_app();

        // Add 2 channels at slots 0 and 1
        app.set_channel_sample(0, "kick.wav".to_string());
        app.set_channel_sample(1, "snare.wav".to_string());
        assert_eq!(app.channels.len(), 2);

        // Delete the first channel (Vec index 0, slot 0)
        app.channels.remove(0);
        assert_eq!(app.channels.len(), 1);

        // Try to add a new channel at slot 2 (the end)
        app.set_channel_sample(2, "hihat.wav".to_string());

        // Should now have 2 channels
        assert_eq!(
            app.channels.len(),
            2,
            "Should have 2 channels after add-delete-add"
        );

        // Both channels should have unique mixer tracks
        let track0 = app.channels[0].mixer_track;
        let track1 = app.channels[1].mixer_track;
        assert_ne!(
            track0, track1,
            "Both channels should have unique mixer tracks, got {} and {}",
            track0, track1
        );
    }

    /// Exact user reproduction: slot 1, slot 2, delete slot 1, add to slot 2
    /// User reports: "nothing is added in slot 2"
    #[test]
    fn test_user_repro_add_delete_add_same_slot() {
        let (mut app, _temp) = create_test_app();

        // Step 1: In slot 1 add a sample
        app.set_channel_sample(1, "sample1.wav".to_string());
        assert_eq!(app.channels.len(), 1);
        assert_eq!(app.channels[0].slot, 1);

        // Step 2: In slot 2 add a sample
        app.set_channel_sample(2, "sample2.wav".to_string());
        assert_eq!(app.channels.len(), 2);

        // Step 3: Delete slot 1
        // Find the channel with slot 1 and remove it
        let idx = app.channels.iter().position(|c| c.slot == 1).unwrap();
        app.channels.remove(idx);
        assert_eq!(app.channels.len(), 1);
        // Only channel with slot 2 remains
        assert_eq!(app.channels[0].slot, 2);

        // Step 4: Attempt to add a sample in slot 2
        // This should UPDATE the existing channel at slot 2, not create a new one
        app.set_channel_sample(2, "new_sample.wav".to_string());

        // The channel at slot 2 should now have the new sample
        let channel = app.channels.iter().find(|c| c.slot == 2);
        assert!(channel.is_some(), "Channel at slot 2 should exist");
        assert_eq!(
            channel.unwrap().sample_path(),
            Some("new_sample.wav"),
            "Channel at slot 2 should have the new sample"
        );
    }

    /// After adding 15 channels, deleting one, and adding another,
    /// the new channel should be created successfully with a unique mixer track.
    #[test]
    fn test_add_channel_after_delete() {
        let (mut app, _temp) = create_test_app();

        // Add 15 channels (slots 0-14)
        for i in 0..15 {
            app.set_channel_sample(i, format!("sample{}.wav", i));
        }
        assert_eq!(app.channels.len(), 15);

        // All channels should have unique mixer tracks
        let mut tracks_before: Vec<usize> = app.channels.iter().map(|c| c.mixer_track).collect();
        tracks_before.sort();
        tracks_before.dedup();
        assert_eq!(
            tracks_before.len(),
            15,
            "All 15 channels should have unique mixer tracks"
        );

        // Delete channel at Vec index 1 (slot 1)
        app.channels.remove(1);
        assert_eq!(app.channels.len(), 14);

        // Add a new channel at slot 15
        app.set_channel_sample(15, "new_sample.wav".to_string());

        // Should now have 15 channels again
        assert_eq!(
            app.channels.len(),
            15,
            "Should have 15 channels after add-delete-add"
        );

        // The new channel should exist and have a unique mixer track
        let new_channel = app.channels.iter().find(|c| c.slot == 15);
        assert!(new_channel.is_some(), "New channel at slot 15 should exist");

        // All channels should still have unique mixer tracks
        let mut tracks_after: Vec<usize> = app.channels.iter().map(|c| c.mixer_track).collect();
        tracks_after.sort();
        tracks_after.dedup();
        assert_eq!(
            tracks_after.len(),
            15,
            "All 15 channels should still have unique mixer tracks after add-delete-add"
        );
    }

    /// Test that get_channel_at_slot works correctly after deletions
    /// This simulates the UI behavior where rendering should find channels by slot
    #[test]
    fn test_get_channel_at_slot_after_deletion() {
        let (mut app, _temp) = create_test_app();

        // Simulate UI: cursor at 0 ("Slot 1"), add sample
        app.set_channel_sample(0, "kick.wav".to_string());
        // Simulate UI: cursor at 1 ("Slot 2"), add sample
        app.set_channel_sample(1, "snare.wav".to_string());

        assert_eq!(app.channels.len(), 2);
        // Verify slots are correct
        assert_eq!(app.channels[0].slot, 0);
        assert_eq!(app.channels[1].slot, 1);

        // Before deletion, get_channel_at_slot should find both
        assert!(
            app.get_channel_at_slot(0).is_some(),
            "Before deletion: slot 0 should exist"
        );
        assert!(
            app.get_channel_at_slot(1).is_some(),
            "Before deletion: slot 1 should exist"
        );

        // Simulate UI: cursor at 0, press 'd' to delete
        // The UI uses Vec index, not slot, for deletion
        app.channels.remove(0);

        assert_eq!(app.channels.len(), 1);
        // After deletion, Vec[0] contains the channel with slot=1
        assert_eq!(
            app.channels[0].slot, 1,
            "After deletion, only channel should have slot 1"
        );

        // CRITICAL TEST: get_channel_at_slot should find by slot, not Vec index
        assert!(
            app.get_channel_at_slot(0).is_none(),
            "After deletion: slot 0 should NOT exist"
        );
        assert!(
            app.get_channel_at_slot(1).is_some(),
            "After deletion: slot 1 SHOULD exist"
        );

        // The bug: if rendering uses channels.get(1), it returns None
        // because Vec only has 1 element. But slot 1 exists at Vec[0].
        assert!(
            app.channels.get(1).is_none(),
            "Vec index 1 is empty (this is the bug - UI looks here)"
        );
        assert!(
            !app.channels.is_empty(),
            "Vec index 0 contains the slot 1 channel"
        );
    }

    // ========================================================================
    // Bug 5: New channels must sync routing to audio thread
    // ========================================================================

    /// When a new channel is created, the audio thread must receive SetGeneratorTrack
    /// to route the channel's audio to the correct mixer track.
    #[test]
    fn test_new_channel_syncs_audio_routing() {
        let (mut app, _temp, rx) = create_test_app_with_audio_rx();

        // Add a sample channel
        app.set_channel_sample(0, "kick.wav".to_string());

        // Get the mixer track assigned to this channel
        let mixer_track = app.channels[0].mixer_track;

        // Drain all commands and check for SetGeneratorTrack
        let commands: Vec<AudioCommand> = rx.try_iter().collect();

        let has_set_generator_track = commands.iter().any(|cmd| {
            matches!(cmd, AudioCommand::SetGeneratorTrack { generator: 0, track } if *track == mixer_track)
        });

        assert!(
            has_set_generator_track,
            "set_channel_sample must send SetGeneratorTrack(0, {}) to audio thread. Commands sent: {:?}",
            mixer_track, commands
        );
    }

    /// When multiple channels are created, each should sync their routing
    #[test]
    fn test_multiple_channels_sync_audio_routing() {
        let (mut app, _temp, rx) = create_test_app_with_audio_rx();

        // Add two channels
        app.set_channel_sample(0, "kick.wav".to_string());
        app.set_channel_sample(1, "snare.wav".to_string());

        // Get the mixer tracks assigned
        let track0 = app.channels[0].mixer_track;
        let track1 = app.channels[1].mixer_track;

        // Drain all commands
        let commands: Vec<AudioCommand> = rx.try_iter().collect();

        // Both channels should have sent their routing
        let has_ch0_routing = commands.iter().any(|cmd| {
            matches!(cmd, AudioCommand::SetGeneratorTrack { generator: 0, track } if *track == track0)
        });
        let has_ch1_routing = commands.iter().any(|cmd| {
            matches!(cmd, AudioCommand::SetGeneratorTrack { generator: 1, track } if *track == track1)
        });

        assert!(
            has_ch0_routing,
            "Channel 0 must sync routing to track {}. Commands: {:?}",
            track0, commands
        );
        assert!(
            has_ch1_routing,
            "Channel 1 must sync routing to track {}. Commands: {:?}",
            track1, commands
        );
    }

    // ========================================================================
    // Command dispatch tests
    // ========================================================================

    #[test]
    fn test_dispatch_toggle_step() {
        let (mut app, _temp) = create_test_app();

        // Create a channel and pattern
        app.set_channel_sample(0, "kick.wav".to_string());
        app.patterns.push(Pattern::new(0, 16));

        // Step should be off initially
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert!(!slice.get_step(0), "Step should be off initially");

        // Dispatch toggle command
        use crate::command::AppCommand;
        app.dispatch(AppCommand::ToggleStep {
            channel: 0,
            pattern: 0,
            step: 0,
        });

        // Step should now be on
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert!(slice.get_step(0), "Step should be on after dispatch");
    }

    #[test]
    fn test_dispatch_add_note() {
        let (mut app, _temp) = create_test_app();

        // Create a channel and pattern
        app.set_channel_sample(0, "kick.wav".to_string());
        app.patterns.push(Pattern::new(0, 16));

        // Dispatch add note command
        use crate::command::AppCommand;
        use crate::sequencer::Note;
        app.dispatch(AppCommand::AddNote {
            channel: 0,
            pattern: 0,
            note: Note::new(60, 0, 4), // C4 at step 0, 4 steps long
        });

        // Note should exist
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert_eq!(slice.notes.len(), 1, "Should have 1 note");
        assert_eq!(slice.notes[0].pitch, 60, "Note pitch should be 60");
        assert_eq!(slice.notes[0].start_step, 0, "Note start should be 0");
    }

    #[test]
    fn test_dispatch_delete_note() {
        let (mut app, _temp) = create_test_app();

        // Create a channel and pattern with a note
        app.set_channel_sample(0, "kick.wav".to_string());
        app.patterns.push(Pattern::new(0, 16));

        use crate::command::AppCommand;
        use crate::sequencer::Note;

        // Add a note first
        app.dispatch(AppCommand::AddNote {
            channel: 0,
            pattern: 0,
            note: Note::new(60, 0, 4),
        });
        assert_eq!(app.channels[0].get_or_create_pattern(0, 16).notes.len(), 1);

        // Delete the note
        app.dispatch(AppCommand::DeleteNote {
            channel: 0,
            pattern: 0,
            pitch: 60,
            start_step: 0,
        });

        // Note should be gone
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert_eq!(slice.notes.len(), 0, "Note should be deleted");
    }

    #[test]
    fn test_dispatch_set_track_volume() {
        let (mut app, _temp) = create_test_app();

        use crate::command::AppCommand;
        use crate::mixer::TrackId;

        // Set volume on track 1
        app.dispatch(AppCommand::SetTrackVolume {
            track: 1,
            volume: 0.5,
        });

        // Verify volume was set
        assert!(
            (app.mixer.track(TrackId(1)).volume - 0.5).abs() < 0.001,
            "Track 1 volume should be 0.5"
        );
    }

    #[test]
    fn test_dispatch_marks_dirty_for_undoable_commands() {
        let (mut app, _temp) = create_test_app();
        app.set_channel_sample(0, "kick.wav".to_string());
        app.patterns.push(Pattern::new(0, 16));

        // Clear dirty flag
        app.dirty = false;

        // Dispatch an undoable command
        use crate::command::AppCommand;
        app.dispatch(AppCommand::ToggleStep {
            channel: 0,
            pattern: 0,
            step: 0,
        });

        assert!(
            app.dirty,
            "Dispatch should mark dirty for undoable commands"
        );
    }

    #[test]
    fn test_dispatch_does_not_mark_dirty_for_transport() {
        let (mut app, _temp) = create_test_app();

        // Clear dirty flag
        app.dirty = false;

        // Dispatch transport command (not undoable)
        use crate::command::AppCommand;
        app.dispatch(AppCommand::TogglePlayback);

        assert!(
            !app.dirty,
            "Dispatch should NOT mark dirty for transport commands"
        );
    }

    #[test]
    fn test_dispatch_records_to_history_for_undo() {
        let (mut app, _temp) = create_test_app();
        app.set_channel_sample(0, "kick.wav".to_string());
        app.patterns.push(Pattern::new(0, 16));

        // Verify no undo available initially
        assert!(
            !app.history.can_undo(),
            "Should have no undo history initially"
        );

        // Verify step is off
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert!(!slice.get_step(0), "Step should be off initially");

        // Dispatch toggle step command
        use crate::command::AppCommand;
        app.dispatch(AppCommand::ToggleStep {
            channel: 0,
            pattern: 0,
            step: 0,
        });

        // Verify step is now on
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert!(slice.get_step(0), "Step should be on after dispatch");

        // Verify undo is available
        assert!(
            app.history.can_undo(),
            "Should have undo history after dispatch"
        );

        // Perform undo - take history temporarily to avoid borrow conflict
        let mut history = std::mem::take(&mut app.history);
        history.undo(&mut app);
        app.history = history;

        // Verify step is back to off
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert!(!slice.get_step(0), "Step should be off after undo");
    }

    #[test]
    fn test_dispatch_batch_clear_steps_records_to_history() {
        let (mut app, _temp) = create_test_app();
        app.set_channel_sample(0, "kick.wav".to_string());
        app.patterns.push(Pattern::new(0, 16));

        // Set some steps first
        use crate::command::AppCommand;
        app.dispatch(AppCommand::ToggleStep {
            channel: 0,
            pattern: 0,
            step: 0,
        });
        app.dispatch(AppCommand::ToggleStep {
            channel: 0,
            pattern: 0,
            step: 1,
        });
        app.dispatch(AppCommand::ToggleStep {
            channel: 0,
            pattern: 0,
            step: 2,
        });

        // Verify steps are on
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert!(slice.get_step(0) && slice.get_step(1) && slice.get_step(2));

        // Clear history so we can test BatchClearSteps specifically
        app.history = History::new();
        assert!(!app.history.can_undo());

        // Use BatchClearSteps to delete steps 0-2
        app.dispatch(AppCommand::BatchClearSteps {
            pattern: 0,
            operations: vec![(0, 0, 2)],
        });

        // Verify steps are now off
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert!(
            !slice.get_step(0) && !slice.get_step(1) && !slice.get_step(2),
            "Steps should be off after BatchClearSteps"
        );

        // Verify undo is available
        assert!(
            app.history.can_undo(),
            "BatchClearSteps should record to history"
        );

        // Perform undo
        let mut history = std::mem::take(&mut app.history);
        history.undo(&mut app);
        app.history = history;

        // Verify steps are restored
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert!(
            slice.get_step(0) && slice.get_step(1) && slice.get_step(2),
            "Steps should be restored after undo"
        );
    }

    #[test]
    fn test_dispatch_batch_delete_notes_records_to_history() {
        let (mut app, _temp) = create_test_app();
        app.set_channel_sample(0, "kick.wav".to_string());
        app.patterns.push(Pattern::new(0, 16));

        // Add some notes first
        use crate::command::AppCommand;
        use crate::sequencer::Note;
        app.dispatch(AppCommand::AddNote {
            channel: 0,
            pattern: 0,
            note: Note::new(60, 0, 4),
        });
        app.dispatch(AppCommand::AddNote {
            channel: 0,
            pattern: 0,
            note: Note::new(62, 4, 4),
        });

        // Verify notes exist
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert_eq!(slice.notes.len(), 2, "Should have 2 notes");

        // Clear history so we can test BatchDeleteNotes specifically
        app.history = History::new();
        assert!(!app.history.can_undo());

        // Use BatchDeleteNotes to delete both notes
        app.dispatch(AppCommand::BatchDeleteNotes {
            channel: 0,
            pattern: 0,
            positions: vec![(60, 0), (62, 4)],
        });

        // Verify notes are now gone
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert_eq!(
            slice.notes.len(),
            0,
            "Notes should be deleted after BatchDeleteNotes"
        );

        // Verify undo is available
        assert!(
            app.history.can_undo(),
            "BatchDeleteNotes should record to history"
        );

        // Perform undo
        let mut history = std::mem::take(&mut app.history);
        history.undo(&mut app);
        app.history = history;

        // Verify notes are restored
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert_eq!(slice.notes.len(), 2, "Notes should be restored after undo");
    }

    // ========================================================================
    // Paste operations undo tests
    // ========================================================================

    #[test]
    fn test_dispatch_set_steps_records_to_history() {
        let (mut app, _temp) = create_test_app();
        app.set_channel_sample(0, "kick.wav".to_string());
        app.patterns.push(Pattern::new(0, 16));

        // Verify steps are off initially
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert!(!slice.get_step(0) && !slice.get_step(1));

        // Clear history
        app.history = History::new();

        // Use SetSteps to set steps (simulates paste)
        use crate::command::AppCommand;
        app.dispatch(AppCommand::SetSteps {
            channel: 0,
            pattern: 0,
            steps: vec![(0, true), (1, true), (2, true)],
        });

        // Verify steps are now on
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert!(
            slice.get_step(0) && slice.get_step(1) && slice.get_step(2),
            "Steps should be on after SetSteps"
        );

        // Verify undo is available
        assert!(app.history.can_undo(), "SetSteps should record to history");

        // Perform undo
        let mut history = std::mem::take(&mut app.history);
        history.undo(&mut app);
        app.history = history;

        // Verify steps are back to off
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert!(
            !slice.get_step(0) && !slice.get_step(1) && !slice.get_step(2),
            "Steps should be off after undo"
        );
    }

    #[test]
    fn test_dispatch_batch_set_steps_records_to_history() {
        let (mut app, _temp) = create_test_app();
        app.set_channel_sample(0, "kick.wav".to_string());
        app.set_channel_sample(1, "snare.wav".to_string());
        app.patterns.push(Pattern::new(0, 16));

        // Clear history
        app.history = History::new();

        // Use BatchSetSteps to set steps across multiple channels (simulates multi-row paste)
        use crate::command::AppCommand;
        app.dispatch(AppCommand::BatchSetSteps {
            pattern: 0,
            operations: vec![(0, 0, true), (0, 1, true), (1, 0, true), (1, 1, true)],
        });

        // Verify steps are now on
        assert!(app.channels[0].get_or_create_pattern(0, 16).get_step(0));
        assert!(app.channels[0].get_or_create_pattern(0, 16).get_step(1));
        assert!(app.channels[1].get_or_create_pattern(0, 16).get_step(0));
        assert!(app.channels[1].get_or_create_pattern(0, 16).get_step(1));

        // Verify undo is available
        assert!(
            app.history.can_undo(),
            "BatchSetSteps should record to history"
        );

        // Perform undo
        let mut history = std::mem::take(&mut app.history);
        history.undo(&mut app);
        app.history = history;

        // Verify steps are back to off
        assert!(!app.channels[0].get_or_create_pattern(0, 16).get_step(0));
        assert!(!app.channels[0].get_or_create_pattern(0, 16).get_step(1));
        assert!(!app.channels[1].get_or_create_pattern(0, 16).get_step(0));
        assert!(!app.channels[1].get_or_create_pattern(0, 16).get_step(1));
    }

    #[test]
    fn test_dispatch_batch_add_notes_records_to_history() {
        let (mut app, _temp) = create_test_app();
        app.set_channel_sample(0, "kick.wav".to_string());
        app.patterns.push(Pattern::new(0, 16));

        // Clear history
        app.history = History::new();

        // Use BatchAddNotes to add notes (simulates paste in piano roll)
        use crate::command::AppCommand;
        use crate::sequencer::Note;
        app.dispatch(AppCommand::BatchAddNotes {
            channel: 0,
            pattern: 0,
            notes: vec![Note::new(60, 0, 4), Note::new(62, 4, 4)],
        });

        // Verify notes exist
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert_eq!(
            slice.notes.len(),
            2,
            "Should have 2 notes after BatchAddNotes"
        );

        // Verify undo is available
        assert!(
            app.history.can_undo(),
            "BatchAddNotes should record to history"
        );

        // Perform undo
        let mut history = std::mem::take(&mut app.history);
        history.undo(&mut app);
        app.history = history;

        // Verify notes are removed
        let slice = app.channels[0].get_or_create_pattern(0, 16);
        assert_eq!(slice.notes.len(), 0, "Notes should be removed after undo");
    }

    // ========================================================================
    // Destructive operations undo tests
    // ========================================================================

    #[test]
    fn test_dispatch_delete_channel_records_to_history() {
        let (mut app, _temp) = create_test_app();
        app.set_channel_sample(0, "kick.wav".to_string());
        app.set_channel_sample(1, "snare.wav".to_string());

        // Verify we have 2 channels
        assert_eq!(app.channels.len(), 2);

        // Clear history
        app.history = History::new();

        // Delete channel 0
        use crate::command::AppCommand;
        app.dispatch(AppCommand::DeleteChannel(0));

        // Verify channel was deleted
        assert_eq!(app.channels.len(), 1, "Should have 1 channel after delete");

        // Verify undo is available
        assert!(
            app.history.can_undo(),
            "DeleteChannel should record to history"
        );

        // Perform undo
        let mut history = std::mem::take(&mut app.history);
        history.undo(&mut app);
        app.history = history;

        // Verify channel is restored
        assert_eq!(app.channels.len(), 2, "Should have 2 channels after undo");
    }

    #[test]
    fn test_dispatch_delete_pattern_records_to_history() {
        let (mut app, _temp) = create_test_app();

        // App starts with 1 default pattern, add one more
        let initial_count = app.patterns.len();
        app.patterns.push(Pattern::new(initial_count, 16));
        let pattern_count = app.patterns.len();

        // Clear history
        app.history = History::new();

        // Delete the last pattern
        use crate::command::AppCommand;
        app.dispatch(AppCommand::DeletePattern(pattern_count - 1));

        // Verify pattern was deleted
        assert_eq!(
            app.patterns.len(),
            pattern_count - 1,
            "Should have one less pattern after delete"
        );

        // Verify undo is available
        assert!(
            app.history.can_undo(),
            "DeletePattern should record to history"
        );

        // Perform undo
        let mut history = std::mem::take(&mut app.history);
        history.undo(&mut app);
        app.history = history;

        // Verify pattern is restored
        assert_eq!(
            app.patterns.len(),
            pattern_count,
            "Pattern count should be restored after undo"
        );
    }

    #[test]
    fn test_dispatch_add_channel_records_to_history() {
        let (mut app, _temp) = create_test_app();

        // Create a channel to "paste"
        let channel = Channel::with_sample("Test", "test.wav");

        // Clear history
        app.history = History::new();

        // Verify we start with 0 channels
        assert_eq!(app.channels().len(), 0);

        // Add channel via dispatch
        use crate::command::AppCommand;
        app.dispatch(AppCommand::AddChannel { slot: 5, channel });

        // Verify channel was added at slot 5
        assert_eq!(app.channels().len(), 1, "Should have 1 channel after add");
        assert!(
            app.get_channel_at_slot(5).is_some(),
            "Channel should be at slot 5"
        );

        // Verify undo is available
        assert!(
            app.history.can_undo(),
            "AddChannel should record to history"
        );

        // Perform undo
        let mut history = std::mem::take(&mut app.history);
        history.undo(&mut app);
        app.history = history;

        // Verify channel is removed
        assert_eq!(app.channels().len(), 0, "Should have 0 channels after undo");
        assert!(
            app.get_channel_at_slot(5).is_none(),
            "Channel should be gone from slot 5"
        );
    }

    #[test]
    fn test_add_channel_does_not_overwrite_existing() {
        let (mut app, _temp) = create_test_app();

        // Add a channel at slot 5
        app.set_channel_sample(5, "kick.wav".to_string());
        assert_eq!(app.channels().len(), 1);

        // Try to add another channel at slot 5
        let channel = Channel::with_sample("Test", "test.wav");
        use crate::command::AppCommand;
        app.dispatch(AppCommand::AddChannel { slot: 5, channel });

        // Should still have only 1 channel (add was blocked)
        assert_eq!(
            app.channels().len(),
            1,
            "AddChannel should not overwrite existing channel"
        );
    }

    // ========================================================================
    // Arrangement playback note-off bug test
    // ========================================================================

    #[test]
    fn test_spanning_notes_stopped_at_bar_boundary_in_arrangement() {
        use crate::arrangement::PatternPlacement;
        use crate::sequencer::{ChannelSource, Note, Pattern, PatternSlice};
        use std::collections::HashMap;
        use std::time::Duration;

        let (mut app, _temp, rx) = create_test_app_with_audio_rx();

        // Create two patterns
        app.patterns = vec![Pattern::new(0, 16), Pattern::new(1, 16)];

        // Create a plugin channel with a note that spans the full pattern (duration 16)
        let mut pattern_data = HashMap::new();
        pattern_data.insert(
            0,
            PatternSlice {
                steps: vec![false; 16],
                notes: vec![Note::with_velocity(60, 0, 16, 0.8)], // Note spans entire pattern
            },
        );
        pattern_data.insert(
            1,
            PatternSlice {
                steps: vec![false; 16],
                notes: vec![], // Pattern 1 has no notes
            },
        );

        let channel = crate::sequencer::Channel {
            name: "synth".to_string(),
            slot: 0,
            source: ChannelSource::Plugin {
                path: "test.clap".to_string(),
                params: HashMap::new(),
            },
            mixer_track: 1,
            pattern_data,
        };
        app.state.channels = vec![channel];

        // Set up arrangement: pattern 0 at bar 0, pattern 1 at bar 1
        app.arrangement.placements = vec![
            PatternPlacement::new(0, 0), // Pattern 0 at bar 0
            PatternPlacement::new(1, 1), // Pattern 1 at bar 1
        ];

        // Start arrangement playback
        app.transport.playback.play_arrangement();

        // Clear any commands from setup
        let _: Vec<_> = rx.try_iter().collect();

        // Advance through all 16 steps to complete bar 0 and enter bar 1
        for _ in 0..16 {
            app.tick(Duration::from_millis(125)); // At 120 BPM, each step is ~125ms
        }

        // Collect all audio commands
        let commands: Vec<AudioCommand> = rx.try_iter().collect();

        // The note from pattern 0 should have received note_off when bar boundary was crossed
        let has_note_off = commands.iter().any(|cmd| {
            matches!(
                cmd,
                AudioCommand::PluginNoteOff {
                    channel: 0,
                    note: 60
                }
            )
        });

        assert!(
            has_note_off,
            "Spanning note should receive note_off at bar boundary. Commands: {:?}",
            commands
        );
    }
}
