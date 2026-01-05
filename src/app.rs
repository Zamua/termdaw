//! Application state and core logic

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::arrangement::Arrangement;
use crate::audio::AudioHandle;
use crate::browser::BrowserState;
use crate::command_picker::CommandPicker;
use crate::cursor::{ChannelRackCursor, PianoRollCursor, PlaylistCursor};
use crate::effects::{EffectSlot, EffectType, EFFECT_SLOTS};
use crate::input::mouse::MouseState;
use crate::input::vim::{GridSemantics, VimState, Zone};
use crate::mixer::{Mixer, TrackId};
use crate::playback::{PlaybackEvent, PlaybackState};
use crate::plugin_host::params::build_init_params;
use crate::plugin_host::PluginHost;
use crate::project::{self, ProjectFile};
use crate::sequencer::{
    default_generators, Generator, GeneratorType, Pattern, YankedNote, YankedPlacement,
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

    /// Generators (sound sources - samplers, plugins)
    pub generators: Vec<Generator>,

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
        let (generators, patterns, bpm, current_pattern, arrangement, created_at, mixer) =
            if project::is_valid_project(&project_path) {
                match project::load_project(&project_path) {
                    Ok(project) => {
                        let generators: Vec<Generator> =
                            project.channels.iter().map(Generator::from).collect();
                        let patterns: Vec<Pattern> =
                            project.patterns.iter().map(Pattern::from).collect();
                        // Load mixer from project file, or create default for old projects
                        let mixer = project
                            .mixer
                            .unwrap_or_else(|| Self::create_default_mixer(&generators));
                        (
                            generators,
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

        let num_generators = generators.len();

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
            generators,
            patterns,
            current_pattern,
            // Separate vim instances per panel type
            // 99 generator slots, 19 columns (3 metadata + 16 steps)
            vim_channel_rack: VimState::with_grid_semantics(99, 19, channel_rack_zones),
            vim_piano_roll: VimState::new(49, 16), // 49 pitches (C2-C6), 16 steps
            vim_playlist: VimState::new(num_generators, 17), // rows = patterns, 16 bars + mute col
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
        };

        // Load plugins for plugin generators
        app.load_plugins();

        // Sync initial mixer and generator routing state to audio thread
        app.sync_mixer_to_audio();
        app.sync_generator_routing();

        app
    }

    /// Create a default mixer with auto-assigned generator routing
    fn create_default_mixer(generators: &[Generator]) -> Mixer {
        let mut mixer = Mixer::new();
        // Auto-assign each generator to a track
        for (idx, _gen) in generators.iter().enumerate() {
            mixer.auto_assign_generator(idx);
        }
        mixer
    }

    /// Load all plugins for plugin generators
    fn load_plugins(&self) {
        // Get the actual sample rate from the audio system
        let sample_rate = self.audio.sample_rate() as f64;
        let buffer_size = 512;

        // Get the plugins directory path
        let plugins_path = self.project_path.join("plugins");

        for (gen_idx, generator) in self.generators.iter().enumerate() {
            if let GeneratorType::Plugin { path } = &generator.generator_type {
                let plugin_path = plugins_path.join(path);

                // Try to load and activate the plugin
                match PluginHost::load(&plugin_path, sample_rate, buffer_size) {
                    Ok(mut host) => match host.activate() {
                        Ok(processor) => {
                            // Build initial state with volume from mixer and params from generator
                            let init_state = self.build_plugin_init_state(gen_idx, generator);
                            // Send the activated processor with initial state
                            self.audio.send_plugin(gen_idx, processor, init_state);
                        }
                        Err(e) => {
                            eprintln!("Failed to activate plugin for generator {}: {}", gen_idx, e);
                        }
                    },
                    Err(e) => {
                        eprintln!("Failed to load plugin for generator {}: {}", gen_idx, e);
                    }
                }
            }
        }
    }

    /// Build initial plugin state from generator settings and mixer
    fn build_plugin_init_state(
        &self,
        gen_idx: usize,
        generator: &Generator,
    ) -> crate::audio::PluginInitState {
        use crate::audio::PluginInitState;

        // Get volume from the mixer track this generator routes to
        let track_id = self.mixer.get_generator_track(gen_idx);
        let volume = self.mixer.track(track_id).volume;

        PluginInitState {
            volume,
            params: build_init_params(&generator.plugin_params),
        }
    }

    #[allow(clippy::type_complexity)]
    fn default_state() -> (
        Vec<Generator>,
        Vec<Pattern>,
        f64,
        usize,
        Arrangement,
        Option<DateTime<Utc>>,
        Mixer,
    ) {
        let generators = default_generators();
        let num_generators = generators.len();
        let patterns = vec![Pattern::new(0, num_generators, 16)];
        let mixer = Self::create_default_mixer(&generators);
        (
            generators,
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
            for (gen_idx, generator) in self.generators.iter().enumerate() {
                if let GeneratorType::Plugin { .. } = &generator.generator_type {
                    // Find notes that span the boundary (start + duration >= 16)
                    // This catches notes that end at or after the loop point
                    for note in pattern.get_notes(gen_idx) {
                        if note.start_step + note.duration >= 16 {
                            self.audio.plugin_note_off(gen_idx, note.pitch);
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
            for (gen_idx, generator) in self.generators.iter().enumerate() {
                // Get the mixer track this generator routes to
                let track_id = self.mixer.get_generator_track(gen_idx);

                // Skip if track is muted or not audible (solo logic)
                if !self.mixer.is_track_audible(track_id) {
                    continue;
                }

                self.play_generator_step(gen_idx, generator, track_id, pattern, step);
            }
        }
    }

    /// Play a single generator's step from a pattern
    fn play_generator_step(
        &self,
        gen_idx: usize,
        generator: &Generator,
        track_id: TrackId,
        pattern: &Pattern,
        step: usize,
    ) {
        // Get volume from the mixer track
        let volume = self.mixer.track(track_id).volume;

        match &generator.generator_type {
            GeneratorType::Sampler => {
                // Sampler generators use step sequencer grid
                if pattern.get_step(gen_idx, step) {
                    if let Some(ref sample_path) = generator.sample_path {
                        let full_path = self.project_path.join("samples").join(sample_path);
                        self.audio.play_sample(&full_path, volume, gen_idx);
                    }
                }
            }
            GeneratorType::Plugin { path: _ } => {
                // Plugin generators use piano roll notes
                for note in pattern.get_notes(gen_idx) {
                    if note.start_step == step {
                        self.audio
                            .plugin_note_on(gen_idx, note.pitch, note.velocity);
                    }
                    // Check for note-off events (notes that end at this step)
                    if note.start_step + note.duration == step {
                        self.audio.plugin_note_off(gen_idx, note.pitch);
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

    /// Stop all notes on plugin generators (all notes off)
    fn stop_all_plugin_notes(&self) {
        for (gen_idx, generator) in self.generators.iter().enumerate() {
            if let GeneratorType::Plugin { .. } = &generator.generator_type {
                // Send note_off for all possible MIDI notes (0-127)
                // This is a brute-force approach but ensures all notes stop
                for note in 0..=127u8 {
                    self.audio.plugin_note_off(gen_idx, note);
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

    /// Generator count
    #[allow(dead_code)]
    pub fn generator_count(&self) -> usize {
        self.generators.len()
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

    /// Set a generator's sample path
    /// If the generator doesn't exist, creates new generators up to and including gen_idx
    pub fn set_channel_sample(&mut self, gen_idx: usize, sample_path: String) {
        self.ensure_generator_exists(gen_idx);

        if let Some(generator) = self.generators.get_mut(gen_idx) {
            // Extract filename without extension for generator name
            let name = std::path::Path::new(&sample_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Sample")
                .to_string();

            generator.name = name;
            generator.generator_type = GeneratorType::Sampler;
            generator.sample_path = Some(sample_path);
            self.mark_dirty();
        }
    }

    /// Set a generator as a plugin generator and load the plugin
    /// If the generator doesn't exist, creates new generators up to and including gen_idx
    pub fn set_channel_plugin(&mut self, gen_idx: usize, plugin_path: String) {
        self.ensure_generator_exists(gen_idx);

        if let Some(generator) = self.generators.get_mut(gen_idx) {
            // Extract plugin name without extension for generator name
            let name = std::path::Path::new(&plugin_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Plugin")
                .to_string();

            generator.name = name;
            generator.generator_type = GeneratorType::Plugin {
                path: plugin_path.clone(),
            };
            generator.sample_path = None;
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
                    let generator = &self.generators[gen_idx];
                    let init_state = self.build_plugin_init_state(gen_idx, generator);
                    self.audio.send_plugin(gen_idx, processor, init_state);
                }
                Err(e) => {
                    eprintln!("Failed to activate plugin for generator {}: {}", gen_idx, e);
                }
            },
            Err(e) => {
                eprintln!("Failed to load plugin for generator {}: {}", gen_idx, e);
            }
        }
    }

    /// Ensure a generator exists at the given index, creating empty generators if needed
    fn ensure_generator_exists(&mut self, gen_idx: usize) {
        // Create generators up to the requested index if they don't exist
        while self.generators.len() <= gen_idx {
            let new_idx = self.generators.len();
            self.generators
                .push(Generator::new(&format!("Generator {}", new_idx + 1)));
            // Auto-assign the new generator to a mixer track
            self.mixer.auto_assign_generator(new_idx);
        }

        // Also expand pattern steps if needed
        for pattern in &mut self.patterns {
            while pattern.steps.len() <= gen_idx {
                pattern.steps.push(vec![false; pattern.length]);
            }
            while pattern.notes.len() <= gen_idx {
                pattern.notes.push(Vec::new());
            }
        }
    }

    /// Start previewing a generator (called on key press)
    pub fn start_preview(&mut self, gen_idx: usize) {
        if let Some(generator) = self.generators.get(gen_idx) {
            match &generator.generator_type {
                GeneratorType::Sampler => {
                    if let Some(ref sample_path) = generator.sample_path {
                        let full_path = self.project_path.join("samples").join(sample_path);
                        self.audio.preview_sample(&full_path, gen_idx);
                    }
                    // Set previewing to prevent key repeat from re-triggering
                    self.is_previewing = true;
                    self.preview_channel = Some(gen_idx);
                }
                GeneratorType::Plugin { .. } => {
                    // Play a test note (middle C) for plugin preview
                    let note = 60u8;
                    self.audio.plugin_note_on(gen_idx, note, 0.8);
                    self.is_previewing = true;
                    self.preview_channel = Some(gen_idx);
                    self.preview_note = Some(note);
                }
            }
        }
    }

    /// Stop previewing a generator (called on key release)
    pub fn stop_preview(&mut self, gen_idx: usize) {
        if self.is_previewing {
            if let Some(note) = self.preview_note {
                // Send note off to stop the preview
                self.audio.plugin_note_off(gen_idx, note);
            }
            self.is_previewing = false;
            self.preview_channel = None;
            self.preview_note = None;
        }
    }

    /// Preview the current note in piano roll (for plugin generators)
    pub fn preview_piano_note(&mut self) {
        let gen_idx = self.channel_rack.channel;
        if let Some(generator) = self.generators.get(gen_idx) {
            if let GeneratorType::Plugin { .. } = &generator.generator_type {
                let note = self.piano_roll.pitch;
                self.audio.plugin_note_on(gen_idx, note, 0.8);
                self.is_previewing = true;
                self.preview_channel = Some(gen_idx);
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
            &self.generators,
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

    /// Sync generator→track routing to audio thread
    /// Called when generator routing changes
    pub fn sync_generator_routing(&self) {
        for gen_idx in 0..self.generators.len() {
            let track = self.mixer.get_generator_track(gen_idx);
            self.audio.set_generator_track(gen_idx, track.index());
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
}
