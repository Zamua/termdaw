//! Application state and core logic

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::arrangement::Arrangement;
use crate::audio::AudioHandle;
use crate::browser::BrowserState;
use crate::command_picker::CommandPicker;
use crate::cursor::{ChannelRackCursor, MixerCursor, PianoRollCursor, PlaylistCursor};
use crate::input::mouse::MouseState;
use crate::input::vim::{GridSemantics, VimState, Zone};
use crate::playback::{PlaybackEvent, PlaybackState};
use crate::plugin_host::params::build_init_params;
use crate::plugin_host::PluginHost;
use crate::project::{self, ProjectFile};
use crate::sequencer::{
    default_channels, Channel, ChannelType, Pattern, YankedNote, YankedPlacement,
};
use crate::ui::areas::ScreenAreas;
use crate::ui::context_menu::ContextMenu;
use crate::ui::plugin_editor::PluginEditorState;

// Re-export types from mode module for external use
pub use crate::mode::{AppMode, Panel, ViewMode};

/// Main application state
#[allow(dead_code)]
pub struct App {
    /// Project name
    pub project_name: String,

    /// Project path
    pub project_path: PathBuf,

    /// When the project was created
    pub created_at: DateTime<Utc>,

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

    /// Playback state machine
    pub playback: PlaybackState,

    /// BPM
    pub bpm: f64,

    /// Time accumulator for step timing
    step_accumulator: Duration,

    /// Terminal dimensions
    pub terminal_width: u16,
    pub terminal_height: u16,

    /// Channel rack cursor state
    pub channel_rack: ChannelRackCursor,

    /// Piano roll cursor state
    pub piano_roll: PianoRollCursor,

    /// Playlist cursor state
    pub playlist: PlaylistCursor,

    /// Arrangement data
    pub arrangement: Arrangement,

    /// Mixer cursor state
    pub mixer: MixerCursor,

    /// Channels (instruments)
    pub channels: Vec<Channel>,

    /// Patterns
    pub patterns: Vec<Pattern>,

    /// Currently selected pattern
    pub current_pattern: usize,

    /// Vim state machines - one per panel type
    pub vim_channel_rack: VimState<Vec<Vec<bool>>>,
    pub vim_piano_roll: VimState<Vec<YankedNote>>,
    pub vim_playlist: VimState<Vec<YankedPlacement>>,

    /// Audio handle for playback
    pub audio: AudioHandle,

    /// File browser state
    pub browser: BrowserState,

    /// Command picker (which-key style)
    pub command_picker: CommandPicker,

    /// Plugin editor modal
    pub plugin_editor: PluginEditorState,

    /// Screen areas for mouse hit testing (populated during render)
    pub screen_areas: ScreenAreas,

    /// Mouse state machine (gesture tracking)
    pub mouse: MouseState,

    /// Context menu state
    pub context_menu: ContextMenu,

    /// Dirty flag for auto-save
    dirty: bool,

    /// Last change time for debounced auto-save
    last_change: Instant,

    /// Whether we're currently previewing a channel (for hold-to-preview)
    pub is_previewing: bool,

    /// Which channel is being previewed (for sending note_off)
    pub preview_channel: Option<usize>,

    /// Which note is being previewed (for plugins)
    preview_note: Option<u8>,
}

impl App {
    /// Create a new App instance
    pub fn new(project_name: &str, audio: AudioHandle) -> Self {
        let project_path = PathBuf::from(project_name);
        let samples_path = project_path.join("samples");

        // If project doesn't exist, create from template
        if !project::is_valid_project(&project_path) {
            if let Err(e) = project::copy_template(&project_path) {
                eprintln!("Warning: Failed to copy template: {}", e);
            }
        }

        // Load project (either existing or newly created from template)
        let (channels, patterns, bpm, current_pattern, arrangement, created_at) =
            if project::is_valid_project(&project_path) {
                match project::load_project(&project_path) {
                    Ok(project) => {
                        let channels: Vec<Channel> =
                            project.channels.iter().map(Channel::from).collect();
                        let patterns: Vec<Pattern> =
                            project.patterns.iter().map(Pattern::from).collect();
                        (
                            channels,
                            patterns,
                            project.bpm,
                            project.current_pattern,
                            project.arrangement,
                            Some(project.created_at),
                        )
                    }
                    Err(_) => Self::default_state(),
                }
            } else {
                Self::default_state()
            };

        let num_channels = channels.len();

        // Channel rack zones in vim coordinate space:
        // - Metadata zone (cols 0-1): sample name + mute/solo indicator
        // - Steps zone (cols 2-17): the 16-step sequencer grid (main zone)
        let channel_rack_zones = GridSemantics::with_zones(vec![
            Zone::new(0, 1),
            Zone::new(2, 17).main().with_word_interval(4),
        ]);

        let app = Self {
            project_name: project_name.to_string(),
            project_path,
            created_at: created_at.unwrap_or_else(Utc::now),
            should_quit: false,
            mode: AppMode::default(),
            view_mode: ViewMode::default(),
            show_browser: true,
            show_mixer: false,
            playback: PlaybackState::default(),
            bpm,
            step_accumulator: Duration::ZERO,
            terminal_width: 80,
            terminal_height: 24,
            channel_rack: ChannelRackCursor::default(),
            piano_roll: PianoRollCursor::default(),
            playlist: PlaylistCursor::default(),
            arrangement,
            mixer: MixerCursor::default(),
            channels,
            patterns,
            current_pattern,
            // Separate vim instances per panel type
            // 99 channel slots, 18 columns (2 metadata + 16 steps)
            vim_channel_rack: VimState::with_grid_semantics(99, 18, channel_rack_zones),
            vim_piano_roll: VimState::new(49, 16), // 49 pitches (C2-C6), 16 steps
            vim_playlist: VimState::new(num_channels, 17), // rows = patterns, 16 bars + mute col
            audio,
            browser: BrowserState::new(samples_path),
            command_picker: CommandPicker::new(),
            plugin_editor: PluginEditorState::new(),
            screen_areas: ScreenAreas::new(),
            mouse: MouseState::new(),
            context_menu: ContextMenu::new(),
            dirty: false,
            last_change: Instant::now(),
            is_previewing: false,
            preview_channel: None,
            preview_note: None,
        };

        // Load plugins for plugin channels
        app.load_plugins();

        app
    }

    /// Load all plugins for plugin channels
    fn load_plugins(&self) {
        // Get the actual sample rate from the audio system
        let sample_rate = self.audio.sample_rate() as f64;
        let buffer_size = 512;

        // Get the plugins directory path
        let plugins_path = self.project_path.join("plugins");

        for (ch_idx, channel) in self.channels.iter().enumerate() {
            if let ChannelType::Plugin { path } = &channel.channel_type {
                let plugin_path = plugins_path.join(path);

                // Try to load and activate the plugin
                match PluginHost::load(&plugin_path, sample_rate, buffer_size) {
                    Ok(mut host) => match host.activate() {
                        Ok(processor) => {
                            // Build initial state with volume and params
                            let init_state = Self::build_plugin_init_state(channel);
                            // Send the activated processor with initial state
                            self.audio.send_plugin(ch_idx, processor, init_state);
                        }
                        Err(e) => {
                            eprintln!("Failed to activate plugin for channel {}: {}", ch_idx, e);
                        }
                    },
                    Err(e) => {
                        eprintln!("Failed to load plugin for channel {}: {}", ch_idx, e);
                    }
                }
            }
        }
    }

    /// Build initial plugin state from channel settings
    fn build_plugin_init_state(channel: &Channel) -> crate::audio::PluginInitState {
        use crate::audio::PluginInitState;

        PluginInitState {
            volume: channel.volume,
            params: build_init_params(&channel.plugin_params),
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
    ) {
        let channels = default_channels();
        let num_channels = channels.len();
        let patterns = vec![Pattern::new(0, num_channels, 16)];
        (channels, patterns, 140.0, 0, Arrangement::new(), None)
    }

    /// Get the current pattern
    pub fn get_current_pattern(&self) -> Option<&Pattern> {
        self.patterns.get(self.current_pattern)
    }

    /// Get the current pattern mutably
    pub fn get_current_pattern_mut(&mut self) -> Option<&mut Pattern> {
        self.patterns.get_mut(self.current_pattern)
    }

    /// Called when the terminal is resized
    pub fn on_resize(&mut self, width: u16, height: u16) {
        self.terminal_width = width;
        self.terminal_height = height;
    }

    /// Called every frame to update state
    pub fn tick(&mut self, delta: Duration) {
        if !self.playback.is_playing() {
            return;
        }

        self.step_accumulator += delta;

        // Calculate step duration: 60 / bpm / 4 (4 steps per beat)
        let step_duration = Duration::from_secs_f64(60.0 / self.bpm / 4.0);

        while self.step_accumulator >= step_duration {
            self.step_accumulator -= step_duration;
            self.advance_step();
        }
    }

    /// Advance to the next step and trigger audio
    fn advance_step(&mut self) {
        // Check if we're about to loop (step is 15 and will become 0)
        let will_loop = self
            .playback
            .current_step()
            .map(|s| s.0 == 15)
            .unwrap_or(false);

        // Advance the playback state machine
        let events = self.playback.advance();

        // When pattern loops to step 0, stop notes that span the loop boundary
        if will_loop {
            self.stop_spanning_notes();
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
    fn stop_spanning_notes(&self) {
        // Get patterns to check based on play mode
        let patterns_to_check: Vec<&crate::sequencer::Pattern> =
            if self.playback.is_playing_arrangement() {
                // Get all active patterns at current bar
                self.arrangement
                    .get_active_placements_at_bar(self.playback.bar_or_zero())
                    .iter()
                    .filter_map(|p| self.patterns.get(p.pattern_id))
                    .collect()
            } else {
                self.get_current_pattern().into_iter().collect()
            };

        for pattern in patterns_to_check {
            for (ch_idx, channel) in self.channels.iter().enumerate() {
                if let ChannelType::Plugin { .. } = &channel.channel_type {
                    // Find notes that span the boundary (start + duration >= 16)
                    // This catches notes that end at or after the loop point
                    for note in pattern.get_notes(ch_idx) {
                        if note.start_step + note.duration >= 16 {
                            self.audio.plugin_note_off(ch_idx, note.pitch);
                        }
                    }
                }
            }
        }
    }

    /// Play all active samples at the current step
    fn play_current_step(&self) {
        if self.playback.is_playing_arrangement() {
            self.play_arrangement_step();
        } else {
            self.play_pattern_step();
        }
    }

    /// Play step from current pattern (pattern loop mode)
    fn play_pattern_step(&self) {
        let patterns = self.get_current_pattern().into_iter();
        self.play_step_from_patterns(patterns, self.playback.step_or_zero());
    }

    /// Play step from arrangement (all active patterns at current bar)
    fn play_arrangement_step(&self) {
        let bar = self.playback.bar_or_zero();
        let placements = self.arrangement.get_active_placements_at_bar(bar);
        let patterns = placements
            .iter()
            .filter_map(|p| self.patterns.get(p.pattern_id));
        self.play_step_from_patterns(patterns, self.playback.step_or_zero());
    }

    /// Play a step from the given patterns (unified playback logic)
    fn play_step_from_patterns<'a>(
        &self,
        patterns: impl Iterator<Item = &'a Pattern>,
        step: usize,
    ) {
        // Check if any channel has solo enabled
        let has_solo = self.channels.iter().any(|c| c.solo);

        for pattern in patterns {
            for (ch_idx, channel) in self.channels.iter().enumerate() {
                // Skip muted channels, or non-solo channels when solo is active
                if channel.muted || (has_solo && !channel.solo) {
                    continue;
                }
                self.play_channel_step(ch_idx, channel, pattern, step);
            }
        }
    }

    /// Play a single channel's step from a pattern
    fn play_channel_step(&self, ch_idx: usize, channel: &Channel, pattern: &Pattern, step: usize) {
        match &channel.channel_type {
            ChannelType::Sampler => {
                // Sampler channels use step sequencer grid
                if pattern.get_step(ch_idx, step) {
                    if let Some(ref sample_path) = channel.sample_path {
                        let full_path = self.project_path.join("samples").join(sample_path);
                        self.audio.play_sample(&full_path, channel.volume);
                    }
                }
            }
            ChannelType::Plugin { path: _ } => {
                // Plugin channels use piano roll notes
                for note in pattern.get_notes(ch_idx) {
                    if note.start_step == step {
                        self.audio.plugin_note_on(ch_idx, note.pitch, note.velocity);
                    }
                    // Check for note-off events (notes that end at this step)
                    if note.start_step + note.duration == step {
                        self.audio.plugin_note_off(ch_idx, note.pitch);
                    }
                }
            }
        }
    }

    /// Toggle play/stop
    pub fn toggle_play(&mut self) {
        if self.playback.is_playing() {
            // Stop playback
            self.stop_all_plugin_notes();
            self.playback.stop();
            self.step_accumulator = Duration::ZERO;
            self.audio.stop_all();
        } else {
            // Start playback based on focused panel
            if self.mode.current_panel() == Panel::Playlist {
                // Start from cursor position in playlist (col 0 is mute, so bar = col - 1)
                let start_bar = self.playlist.bar.saturating_sub(1);
                self.playback.play_arrangement_from(start_bar);
            } else {
                self.playback.play_pattern();
            }
            // Play the first step immediately
            self.play_current_step();
        }
    }

    /// Check if currently playing (for backward compatibility)
    pub fn is_playing(&self) -> bool {
        self.playback.is_playing()
    }

    /// Get current playhead step (for backward compatibility)
    pub fn playhead_step(&self) -> usize {
        self.playback.step_or_zero()
    }

    /// Get current arrangement bar
    pub fn arrangement_bar(&self) -> usize {
        self.playback.bar_or_zero()
    }

    /// Check if playing arrangement (not pattern)
    pub fn is_playing_arrangement(&self) -> bool {
        self.playback.is_playing_arrangement()
    }

    /// Stop all notes on plugin channels (all notes off)
    fn stop_all_plugin_notes(&self) {
        for (ch_idx, channel) in self.channels.iter().enumerate() {
            if let ChannelType::Plugin { .. } = &channel.channel_type {
                // Send note_off for all possible MIDI notes (0-127)
                // This is a brute-force approach but ensures all notes stop
                for note in 0..=127u8 {
                    self.audio.plugin_note_off(ch_idx, note);
                }
            }
        }
    }

    /// Cycle to the next panel
    pub fn next_panel(&mut self) {
        self.mode
            .next_panel(self.show_browser, self.show_mixer, self.view_mode);
    }

    /// Set the view mode and focus
    pub fn set_view_mode(&mut self, view_mode: ViewMode) {
        self.view_mode = view_mode;
        let panel = match view_mode {
            ViewMode::ChannelRack => Panel::ChannelRack,
            ViewMode::PianoRoll => Panel::PianoRoll,
            ViewMode::Playlist => Panel::Playlist,
        };
        self.mode.switch_panel(panel);
    }

    /// Toggle browser visibility
    pub fn toggle_browser(&mut self) {
        self.show_browser = !self.show_browser;
        if self.show_browser {
            self.mode.switch_panel(Panel::Browser);
        } else if self.mode.current_panel() == Panel::Browser {
            let panel = match self.view_mode {
                ViewMode::ChannelRack => Panel::ChannelRack,
                ViewMode::PianoRoll => Panel::PianoRoll,
                ViewMode::Playlist => Panel::Playlist,
            };
            self.mode.switch_panel(panel);
        }
    }

    /// Toggle mixer visibility
    pub fn toggle_mixer(&mut self) {
        self.show_mixer = !self.show_mixer;
        if self.show_mixer {
            self.mode.switch_panel(Panel::Mixer);
        } else if self.mode.current_panel() == Panel::Mixer {
            let panel = match self.view_mode {
                ViewMode::ChannelRack => Panel::ChannelRack,
                ViewMode::PianoRoll => Panel::PianoRoll,
                ViewMode::Playlist => Panel::Playlist,
            };
            self.mode.switch_panel(panel);
        }
    }

    /// Get the current step index (0-15) from cursor column
    /// Returns 0 if in sample or mute zone
    pub fn cursor_step(&self) -> usize {
        self.channel_rack.col.to_step_or_zero()
    }

    /// Get the current zone name
    pub fn cursor_zone(&self) -> &'static str {
        self.channel_rack.col.zone_name()
    }

    /// Toggle step at cursor in channel rack (only works in steps zone)
    pub fn toggle_step(&mut self) {
        if !self.channel_rack.col.is_step_zone() {
            return; // Not in steps zone
        }
        let channel = self.channel_rack.channel;
        let step = self.channel_rack.col.to_step_or_zero();
        if let Some(pattern) = self.get_current_pattern_mut() {
            pattern.toggle_step(channel, step);
            self.mark_dirty();
        }
    }

    /// Channel count
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Adjust mixer channel volume
    pub fn adjust_mixer_volume(&mut self, delta: f32) {
        if let Some(channel) = self.channels.get_mut(self.mixer.selected_channel) {
            channel.volume = (channel.volume + delta).clamp(0.0, 1.0);
            // Sync volume to audio thread for plugin channels
            if channel.is_plugin() {
                self.audio
                    .plugin_set_volume(self.mixer.selected_channel, channel.volume);
            }
            self.mark_dirty();
        }
    }

    /// Toggle mute on selected mixer channel
    pub fn toggle_mute(&mut self) {
        if let Some(channel) = self.channels.get_mut(self.mixer.selected_channel) {
            channel.muted = !channel.muted;
            self.mark_dirty();
        }
    }

    /// Toggle solo on selected mixer channel
    pub fn toggle_solo(&mut self) {
        if let Some(channel) = self.channels.get_mut(self.mixer.selected_channel) {
            channel.solo = !channel.solo;
            self.mark_dirty();
        }
    }

    /// Set a channel's sample path
    /// If the channel doesn't exist, creates new channels up to and including channel_idx
    pub fn set_channel_sample(&mut self, channel_idx: usize, sample_path: String) {
        self.ensure_channel_exists(channel_idx);

        if let Some(channel) = self.channels.get_mut(channel_idx) {
            // Extract filename without extension for channel name
            let name = std::path::Path::new(&sample_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Sample")
                .to_string();

            channel.name = name;
            channel.channel_type = ChannelType::Sampler;
            channel.sample_path = Some(sample_path);
            self.mark_dirty();
        }
    }

    /// Set a channel as a plugin channel and load the plugin
    /// If the channel doesn't exist, creates new channels up to and including channel_idx
    pub fn set_channel_plugin(&mut self, channel_idx: usize, plugin_path: String) {
        self.ensure_channel_exists(channel_idx);

        if let Some(channel) = self.channels.get_mut(channel_idx) {
            // Extract plugin name without extension for channel name
            let name = std::path::Path::new(&plugin_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Plugin")
                .to_string();

            channel.name = name;
            channel.channel_type = ChannelType::Plugin {
                path: plugin_path.clone(),
            };
            channel.sample_path = None;
        }

        self.mark_dirty();

        // Load and activate the plugin
        let sample_rate = self.audio.sample_rate() as f64;
        let buffer_size = 512;
        let full_plugin_path = self.project_path.join("plugins").join(&plugin_path);

        match PluginHost::load(&full_plugin_path, sample_rate, buffer_size) {
            Ok(mut host) => match host.activate() {
                Ok(processor) => {
                    // For newly assigned plugin, use default params
                    let channel = &self.channels[channel_idx];
                    let init_state = Self::build_plugin_init_state(channel);
                    self.audio.send_plugin(channel_idx, processor, init_state);
                }
                Err(e) => {
                    eprintln!(
                        "Failed to activate plugin for channel {}: {}",
                        channel_idx, e
                    );
                }
            },
            Err(e) => {
                eprintln!("Failed to load plugin for channel {}: {}", channel_idx, e);
            }
        }
    }

    /// Ensure a channel exists at the given index, creating empty channels if needed
    fn ensure_channel_exists(&mut self, channel_idx: usize) {
        // Create channels up to the requested index if they don't exist
        while self.channels.len() <= channel_idx {
            self.channels.push(Channel::new(&format!(
                "Channel {}",
                self.channels.len() + 1
            )));
        }

        // Also expand pattern steps if needed
        for pattern in &mut self.patterns {
            while pattern.steps.len() <= channel_idx {
                pattern.steps.push(vec![false; pattern.length]);
            }
            while pattern.notes.len() <= channel_idx {
                pattern.notes.push(Vec::new());
            }
        }
    }

    /// Start previewing a channel (called on key press)
    pub fn start_preview(&mut self, channel_idx: usize) {
        if let Some(channel) = self.channels.get(channel_idx) {
            match &channel.channel_type {
                ChannelType::Sampler => {
                    if let Some(ref sample_path) = channel.sample_path {
                        let full_path = self.project_path.join("samples").join(sample_path);
                        self.audio.preview_sample(&full_path);
                    }
                    // Set previewing to prevent key repeat from re-triggering
                    self.is_previewing = true;
                    self.preview_channel = Some(channel_idx);
                }
                ChannelType::Plugin { .. } => {
                    // Play a test note (middle C) for plugin preview
                    let note = 60u8;
                    self.audio.plugin_note_on(channel_idx, note, 0.8);
                    self.is_previewing = true;
                    self.preview_channel = Some(channel_idx);
                    self.preview_note = Some(note);
                }
            }
        }
    }

    /// Stop previewing a channel (called on key release)
    pub fn stop_preview(&mut self, channel_idx: usize) {
        if self.is_previewing {
            if let Some(note) = self.preview_note {
                // Send note off to stop the preview
                self.audio.plugin_note_off(channel_idx, note);
            }
            self.is_previewing = false;
            self.preview_channel = None;
            self.preview_note = None;
        }
    }

    /// Preview the current note in piano roll (for plugin channels)
    pub fn preview_piano_note(&mut self) {
        let channel_idx = self.channel_rack.channel;
        if let Some(channel) = self.channels.get(channel_idx) {
            if let ChannelType::Plugin { .. } = &channel.channel_type {
                let note = self.piano_roll.pitch;
                self.audio.plugin_note_on(channel_idx, note, 0.8);
                self.is_previewing = true;
                self.preview_channel = Some(channel_idx);
                self.preview_note = Some(note);
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
        let project = ProjectFile::from_state(
            &self.project_name,
            self.bpm,
            self.current_pattern,
            &self.channels,
            &self.patterns,
            &self.arrangement,
            Some(self.created_at),
        );

        if let Err(e) = project::save_project(&self.project_path, &project) {
            eprintln!("Failed to save project: {}", e);
        }
    }
}
