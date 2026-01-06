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
        if let Some(channel) = self.channels.iter_mut().find(|c| c.slot == slot) {
            // Update existing channel
            channel.name = name;
            channel.source = ChannelSource::Sampler {
                path: Some(sample_path),
            };
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

    /// Start previewing a channel (called on key press)
    /// Takes slot number, finds Vec index for audio engine
    pub fn start_preview(&mut self, slot: usize) {
        // Find Vec index for audio engine
        if let Some(vec_idx) = self.channels.iter().position(|c| c.slot == slot) {
            let channel = &self.channels[vec_idx];
            match &channel.source {
                ChannelSource::Sampler { path } => {
                    if let Some(ref sample_path) = path {
                        let full_path = self.project_path.join("samples").join(sample_path);
                        self.audio.preview_sample(&full_path, vec_idx);
                    }
                    // Set previewing to prevent key repeat from re-triggering
                    self.is_previewing = true;
                    self.preview_channel = Some(vec_idx);
                }
                ChannelSource::Plugin { .. } => {
                    // Play a test note (middle C) for plugin preview
                    let note = 60u8;
                    self.audio.plugin_note_on(vec_idx, note, 0.8);
                    self.is_previewing = true;
                    self.preview_channel = Some(vec_idx);
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
        let slot = self.channel_rack.channel;
        // Find Vec index for audio engine
        if let Some(vec_idx) = self.channels.iter().position(|c| c.slot == slot) {
            if let ChannelSource::Plugin { .. } = &self.channels[vec_idx].source {
                let note = self.piano_roll.pitch;
                self.audio.plugin_note_on(vec_idx, note, 0.8);
                self.is_previewing = true;
                self.preview_channel = Some(vec_idx);
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
            ViewMode::Playlist => JumpPosition::playlist(self.playlist.row, self.playlist.bar),
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
                    self.channel_rack.viewport_top = self.channel_rack.channel - visible_rows + 1;
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

        let audio = AudioHandle::dummy();
        let app = App::new(project_path.to_str().unwrap(), audio);
        (app, temp_dir)
    }

    /// Create a test App with a testable audio handle that captures commands
    fn create_test_app_with_audio_rx() -> (App, TempDir, Receiver<AudioCommand>) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_path = temp_dir.path().join("test-project");
        std::fs::create_dir_all(&project_path).expect("Failed to create project dir");

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
            app.channels.get(0).is_some(),
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
}
