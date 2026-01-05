//! Application state and core logic

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::arrangement::Arrangement;
use crate::audio::AudioHandle;
use crate::browser::BrowserState;
use crate::command_picker::CommandPicker;
use crate::coords::AppCol;
use crate::cursor::{ChannelRackCursor, PianoRollCursor, PlaylistCursor};
use crate::effects::{EffectSlot, EffectType, EFFECT_SLOTS};
use crate::history::{GlobalJumplist, History, JumpPosition};
use crate::input::context::{PianoRollContext, PlaylistContext, StepGridContext};
use crate::input::mouse::MouseState;
use crate::input::vim::{GridSemantics, VimState, Zone};
use crate::mixer::{Mixer, TrackId};
use crate::playback::{PlaybackEvent, PlaybackState};
use crate::plugin_host::params::build_init_params;
use crate::plugin_host::PluginHost;
use crate::project::{self, ProjectFile};
use crate::sequencer::{
    default_channels, Channel, ChannelSource, Note, Pattern, YankedNote, YankedPlacement,
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

    /// Mixer (FL Studio-style with routing)
    pub mixer: Mixer,

    /// Channels (sound sources - samplers, plugins)
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

    /// Effect picker selection index
    pub effect_picker_selection: usize,

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

    /// Undo/redo history
    pub history: History,

    /// Global cross-view jump list for Ctrl+O/Ctrl+I
    pub global_jumplist: GlobalJumplist,
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
            mixer,
            channels,
            patterns,
            current_pattern,
            // Separate vim instances per panel type
            // 99 channel slots, 19 columns (3 metadata + 16 steps)
            vim_channel_rack: VimState::with_grid_semantics(99, 19, channel_rack_zones),
            vim_piano_roll: VimState::new(49, 16), // 49 pitches (C2-C6), 16 steps
            vim_playlist: VimState::new(num_channels, 17), // rows = patterns, 16 bars + mute col
            audio,
            browser: BrowserState::new(samples_path),
            command_picker: CommandPicker::new(),
            plugin_editor: PluginEditorState::new(),
            screen_areas: ScreenAreas::new(),
            mouse: MouseState::new(),
            context_menu: ContextMenu::new(),
            effect_picker_selection: 0,
            dirty: false,
            last_change: Instant::now(),
            is_previewing: false,
            preview_channel: None,
            preview_note: None,
            history: History::new(),
            global_jumplist: GlobalJumplist::new(),
        };

        // Load plugins for plugin channels
        app.load_plugins();

        // Sync initial mixer and channel routing state to audio thread
        app.sync_mixer_to_audio();
        app.sync_channel_routing();

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

    /// Load all plugins for plugin channels
    fn load_plugins(&self) {
        // Get the actual sample rate from the audio system
        let sample_rate = self.audio.sample_rate() as f64;
        let buffer_size = 512;

        // Get the plugins directory path
        let plugins_path = self.project_path.join("plugins");

        for (channel_idx, channel) in self.channels.iter().enumerate() {
            if let ChannelSource::Plugin { path, .. } = &channel.source {
                let plugin_path = plugins_path.join(path);

                // Try to load and activate the plugin
                match PluginHost::load(&plugin_path, sample_rate, buffer_size) {
                    Ok(mut host) => match host.activate() {
                        Ok(processor) => {
                            // Build initial state with volume from mixer and params from channel
                            let init_state = self.build_plugin_init_state(channel_idx, channel);
                            // Send the activated processor with initial state
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
        }
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

    /// Get the current pattern
    pub fn get_current_pattern(&self) -> Option<&Pattern> {
        self.patterns.get(self.current_pattern)
    }

    /// Called when the terminal is resized
    pub fn on_resize(&mut self, width: u16, height: u16) {
        self.terminal_width = width;
        self.terminal_height = height;
    }

    /// Called every frame to update state
    pub fn tick(&mut self, delta: Duration) {
        // Always update peak levels for mixer meters (even when not playing)
        self.update_peak_levels();

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
                        let full_path = self.project_path.join("samples").join(sample_path);
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
        self.mode
            .next_panel(self.show_browser, self.show_mixer, self.view_mode);
    }

    /// Set the view mode and focus
    ///
    /// Records the current position in the global jumplist before switching,
    /// enabling Ctrl+O/Ctrl+I navigation between views.
    pub fn set_view_mode(&mut self, view_mode: ViewMode) {
        // Record current position before switching (if actually changing views)
        if self.view_mode != view_mode {
            let current = self.current_jump_position();
            self.global_jumplist.push(current);
        }

        self.view_mode = view_mode;
        let panel = match view_mode {
            ViewMode::ChannelRack => Panel::ChannelRack,
            ViewMode::PianoRoll => Panel::PianoRoll,
            ViewMode::Playlist => Panel::Playlist,
        };
        self.mode.switch_panel(panel);
    }

    /// Toggle browser visibility
    ///
    /// Records current position in jumplist when opening browser,
    /// so Ctrl+O can return to it.
    pub fn toggle_browser(&mut self) {
        self.show_browser = !self.show_browser;
        if self.show_browser {
            // Record current position before switching to browser
            let current = self.current_jump_position();
            self.global_jumplist.push(current);
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
    ///
    /// Records current position in jumplist when opening mixer,
    /// so Ctrl+O can return to it.
    pub fn toggle_mixer(&mut self) {
        self.show_mixer = !self.show_mixer;
        if self.show_mixer {
            // Record current position before switching to mixer
            let current = self.current_jump_position();
            self.global_jumplist.push(current);
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
    #[allow(dead_code)]
    pub fn toggle_step(&mut self) {
        if !self.channel_rack.col.is_step_zone() {
            return; // Not in steps zone
        }
        let channel_idx = self.channel_rack.channel;
        let step = self.channel_rack.col.to_step_or_zero();
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
        self.sync_mixer_to_audio();
        self.mark_dirty();
    }

    /// Toggle mute on selected mixer track
    pub fn toggle_mute(&mut self) {
        let track_id = TrackId(self.mixer.selected_track);
        self.mixer.toggle_mute(track_id);
        self.sync_mixer_to_audio();
        self.mark_dirty();
    }

    /// Toggle solo on selected mixer track
    pub fn toggle_solo(&mut self) {
        let track_id = TrackId(self.mixer.selected_track);
        self.mixer.toggle_solo(track_id);
        self.sync_mixer_to_audio();
        self.mark_dirty();
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
            channel.source = ChannelSource::Sampler {
                path: Some(sample_path),
            };
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
            channel.source = ChannelSource::Plugin {
                path: plugin_path.clone(),
                params: std::collections::HashMap::new(),
            };
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
                    let init_state = self.build_plugin_init_state(channel_idx, channel);
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
            let new_idx = self.channels.len();
            self.channels
                .push(Channel::new(&format!("Channel {}", new_idx + 1)));
            // Auto-assign the new channel to a mixer track
            self.mixer.auto_assign_generator(new_idx);
        }
        // Pattern data is now stored in Channel, so no need to expand patterns
    }

    /// Start previewing a channel (called on key press)
    pub fn start_preview(&mut self, channel_idx: usize) {
        if let Some(channel) = self.channels.get(channel_idx) {
            match &channel.source {
                ChannelSource::Sampler { path } => {
                    if let Some(ref sample_path) = path {
                        let full_path = self.project_path.join("samples").join(sample_path);
                        self.audio.preview_sample(&full_path, channel_idx);
                    }
                    // Set previewing to prevent key repeat from re-triggering
                    self.is_previewing = true;
                    self.preview_channel = Some(channel_idx);
                }
                ChannelSource::Plugin { .. } => {
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
            if let ChannelSource::Plugin { .. } = &channel.source {
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
            &self.mixer,
            Some(self.created_at),
        );

        if let Err(e) = project::save_project(&self.project_path, &project) {
            eprintln!("Failed to save project: {}", e);
        }
    }

    /// Sync mixer state to audio thread
    /// Called when volume, pan, mute, or solo changes
    pub fn sync_mixer_to_audio(&self) {
        use crate::audio::AudioMixerState;

        let has_solo = self.mixer.has_solo();

        let mut state = AudioMixerState {
            track_volumes: [0.0; crate::mixer::NUM_TRACKS],
            track_pans: [0.0; crate::mixer::NUM_TRACKS],
            track_mutes: [false; crate::mixer::NUM_TRACKS],
        };

        for (i, track) in self.mixer.tracks.iter().enumerate() {
            state.track_volumes[i] = track.volume;
            state.track_pans[i] = track.pan;
            // Effective mute considers solo state
            state.track_mutes[i] = track.muted || (has_solo && !track.solo && i != 0);
        }

        self.audio.update_mixer_state(state);
    }

    /// Sync channel→track routing to audio thread
    /// Called when channel routing changes
    pub fn sync_channel_routing(&self) {
        for (channel_idx, channel) in self.channels.iter().enumerate() {
            self.audio
                .set_generator_track(channel_idx, channel.mixer_track);
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
            self.mode.open_effect_editor(track_idx, slot_idx);
        } else {
            self.mode.open_effect_picker(track_idx, slot_idx);
        }
    }

    /// Toggle bypass on the currently selected effect
    pub fn toggle_effect_bypass(&mut self) {
        let track_idx = self.mixer.selected_track;
        let slot_idx = self.mixer.selected_effect_slot;

        if let Some(ref mut slot) = self.mixer.tracks[track_idx].effects[slot_idx] {
            slot.bypassed = !slot.bypassed;
            self.audio
                .set_effect_enabled(track_idx, slot_idx, !slot.bypassed);
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
        } = self.mode
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
    #[allow(dead_code)]
    pub fn sync_all_effects_to_audio(&self) {
        for track_idx in 0..crate::mixer::NUM_TRACKS {
            self.sync_effects_to_audio(track_idx);
        }
    }

    // ============ Undo/Redo History Helpers ============

    /// Toggle step at cursor with undo/redo support
    pub fn toggle_step_with_history(&mut self) {
        use crate::history::command::ToggleStepCmd;

        if !self.channel_rack.col.is_step_zone() {
            return;
        }

        let cmd = Box::new(ToggleStepCmd::new(
            self.current_pattern,
            self.channel_rack.channel,
            self.channel_rack.col.to_step_or_zero(),
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
            self.channel_rack.channel,
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
            self.channel_rack.channel,
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
        match self.view_mode {
            ViewMode::ChannelRack => JumpPosition::channel_rack(
                self.channel_rack.channel,
                self.channel_rack.col.to_step_or_zero(),
            ),
            ViewMode::PianoRoll => {
                // Convert pitch to row (higher pitch = lower row number)
                let pitch_row = (84 - self.piano_roll.pitch) as usize;
                JumpPosition::piano_roll(pitch_row, self.piano_roll.step)
            }
            ViewMode::Playlist => {
                JumpPosition::playlist(self.playlist.row, self.playlist.bar)
            }
        }
    }

    /// Navigate to a jump position (may switch views)
    ///
    /// NOTE: This directly sets view_mode WITHOUT calling set_view_mode(),
    /// because we don't want to record jumps during Ctrl+O/Ctrl+I navigation.
    pub fn goto_jump_position(&mut self, pos: &JumpPosition) {
        // Switch view directly (don't call set_view_mode to avoid recording jump)
        self.view_mode = pos.view;

        // Switch panel focus to match the view
        let panel = match pos.view {
            ViewMode::ChannelRack => Panel::ChannelRack,
            ViewMode::PianoRoll => Panel::PianoRoll,
            ViewMode::Playlist => Panel::Playlist,
        };
        self.mode.switch_panel(panel);

        // Set cursor position and scroll viewport based on view
        match pos.view {
            ViewMode::ChannelRack => {
                self.channel_rack.channel = pos.row.min(self.channels.len().saturating_sub(1));
                // Convert step to AppCol (step zone starts at col 3 in vim space)
                self.channel_rack.col = AppCol::from_step(pos.col);
                // Scroll viewport to keep cursor visible
                let visible_rows = 15;
                if self.channel_rack.channel >= self.channel_rack.viewport_top + visible_rows {
                    self.channel_rack.viewport_top =
                        self.channel_rack.channel - visible_rows + 1;
                }
                if self.channel_rack.channel < self.channel_rack.viewport_top {
                    self.channel_rack.viewport_top = self.channel_rack.channel;
                }
            }
            ViewMode::PianoRoll => {
                // Convert row back to pitch (row 0 = pitch 84, row 48 = pitch 36)
                self.piano_roll.pitch = (84 - pos.row as i32).clamp(36, 84) as u8;
                self.piano_roll.step = pos.col.min(15);
                // Scroll viewport to keep cursor visible (viewport_top is highest visible pitch)
                if self.piano_roll.pitch > self.piano_roll.viewport_top {
                    self.piano_roll.viewport_top = self.piano_roll.pitch;
                }
                let visible_rows = 20u8;
                if self.piano_roll.pitch < self.piano_roll.viewport_top.saturating_sub(visible_rows)
                {
                    self.piano_roll.viewport_top = self.piano_roll.pitch + 10;
                }
            }
            ViewMode::Playlist => {
                self.playlist.row = pos.row.min(self.patterns.len().saturating_sub(1));
                self.playlist.bar = pos.col.min(16);
                // Scroll viewport to keep cursor visible
                let visible_rows = 10;
                if self.playlist.row >= self.playlist.viewport_top + visible_rows {
                    self.playlist.viewport_top = self.playlist.row - visible_rows + 1;
                }
                if self.playlist.row < self.playlist.viewport_top {
                    self.playlist.viewport_top = self.playlist.row;
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
        let channel = self.channel_rack.channel;
        let pattern_id = self.current_pattern;
        self.channels
            .get(channel)
            .and_then(|c| c.get_pattern(pattern_id))
            .map(|s| s.notes.as_slice())
            .unwrap_or(&[])
    }

    fn add_note(&mut self, note: Note) {
        let channel = self.channel_rack.channel;
        let pattern_id = self.current_pattern;
        let pattern_length = self.pattern_length();
        if let Some(ch) = self.channels.get_mut(channel) {
            let slice = ch.get_or_create_pattern(pattern_id, pattern_length);
            slice.notes.push(note);
            self.mark_dirty();
        }
    }

    fn remove_note(&mut self, id: &str) -> Option<Note> {
        let channel = self.channel_rack.channel;
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
