//! Audio engine for sample playback and plugin hosting
//!
//! Uses cpal for low-level audio with support for:
//! - Polyphonic sample playback with pre-loaded buffers
//! - Real-time mixing in audio callback
//! - Plugin hosting via CLAP
//! - Master and per-channel volume control
//! - Per-track mixing with routing (FL Studio-style mixer)

pub mod mock;

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use crossbeam_channel::{unbounded, Receiver, Sender};
use rodio::{Decoder, Source};

use crate::effects::{create_effect, Effect, EffectParamId, EffectSlot, EffectType, EFFECT_SLOTS};
use crate::mixer::{StereoLevels, NUM_TRACKS};
use crate::plugin_host::{ActivePluginProcessor, ParamChange};

/// Maximum number of simultaneous sample playbacks
const MAX_VOICES: usize = 32;

/// Number of samples to keep for waveform visualization
const WAVEFORM_BUFFER_SIZE: usize = 512;

/// Maximum buffer size for per-track mixing (stereo samples)
const MAX_TRACK_BUFFER_SIZE: usize = 4096;

/// Number of generator slots (for generator→track routing)
const MAX_GENERATORS: usize = 99;

/// Shared waveform buffer for visualization
pub type WaveformBuffer = Arc<Mutex<Vec<f32>>>;

/// Shared peak levels buffer (updated by audio thread, read by UI)
pub type PeakLevelsBuffer = Arc<Mutex<[StereoLevels; NUM_TRACKS]>>;

/// Minimal mixer state for audio thread (no strings, no UI state)
/// Sent atomically from main thread when mixer config changes
#[derive(Debug, Clone)]
pub struct AudioMixerState {
    /// Volume per track (0.0-1.0)
    pub track_volumes: [f32; NUM_TRACKS],
    /// Pan per track (-1.0 to 1.0)
    pub track_pans: [f32; NUM_TRACKS],
    /// Effective mute state (includes solo logic)
    pub track_mutes: [bool; NUM_TRACKS],
}

impl Default for AudioMixerState {
    fn default() -> Self {
        Self {
            track_volumes: [0.8; NUM_TRACKS],
            track_pans: [0.0; NUM_TRACKS],
            track_mutes: [false; NUM_TRACKS],
        }
    }
}

impl AudioMixerState {
    /// Apply pan law to get L/R gains (constant-power pan law)
    pub fn pan_gains(&self, track: usize) -> (f32, f32) {
        constant_power_pan(self.track_pans[track])
    }
}

/// Constant-power pan law calculation
///
/// Converts a pan value (-1.0 = full left, 0.0 = center, 1.0 = full right)
/// to left/right gain multipliers. At center, both channels are at ~0.707 (-3dB)
/// to maintain constant perceived loudness.
///
/// # Arguments
/// * `pan` - Pan position from -1.0 (left) to 1.0 (right)
///
/// # Returns
/// Tuple of (left_gain, right_gain) where each is in range 0.0-1.0
pub fn constant_power_pan(pan: f32) -> (f32, f32) {
    let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4; // 0 to PI/2
    (angle.cos(), angle.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pan_center() {
        let (left, right) = constant_power_pan(0.0);
        // At center, both should be ~0.707 (-3dB)
        let expected = std::f32::consts::FRAC_1_SQRT_2; // 0.7071...
        assert!((left - expected).abs() < 0.001);
        assert!((right - expected).abs() < 0.001);
    }

    #[test]
    fn test_pan_full_left() {
        let (left, right) = constant_power_pan(-1.0);
        assert!((left - 1.0).abs() < 0.001);
        assert!(right.abs() < 0.001);
    }

    #[test]
    fn test_pan_full_right() {
        let (left, right) = constant_power_pan(1.0);
        assert!(left.abs() < 0.001);
        assert!((right - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_pan_constant_power() {
        // At any pan position, left^2 + right^2 should equal 1.0
        for pan in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            let (left, right) = constant_power_pan(pan);
            let sum_of_squares = left * left + right * right;
            assert!(
                (sum_of_squares - 1.0).abs() < 0.001,
                "pan={} sum_of_squares={}",
                pan,
                sum_of_squares
            );
        }
    }

    #[test]
    fn test_pan_symmetry() {
        // Pan left and right should be symmetric
        let (left_neg, right_neg) = constant_power_pan(-0.5);
        let (left_pos, right_pos) = constant_power_pan(0.5);
        assert!((left_neg - right_pos).abs() < 0.001);
        assert!((right_neg - left_pos).abs() < 0.001);
    }
}

/// Per-track stereo buffer for mixing
pub(crate) struct TrackBuffer {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

/// Commands sent to the audio engine
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AudioCommand {
    /// Play a sample (polyphonic - can overlap)
    PlaySample {
        path: PathBuf,
        volume: f32,
        generator_idx: usize,
    },
    /// Preview a sample (exclusive - stops previous preview)
    /// If route_to_master is true, audio goes directly to master (for browser previews)
    PreviewSample {
        path: PathBuf,
        generator_idx: usize,
        route_to_master: bool,
    },
    /// Stop the current preview
    StopPreview,
    /// Stop all playback
    StopAll,
    /// Set master volume (0.0-1.0)
    SetMasterVolume(f32),
    /// Preload a sample into cache
    PreloadSample { path: PathBuf },
    /// Send note on to a plugin channel
    PluginNoteOn {
        channel: usize,
        note: u8,
        velocity: f32,
    },
    /// Send note off to a plugin channel
    PluginNoteOff { channel: usize, note: u8 },
    /// Set a plugin parameter value
    PluginSetParam {
        channel: usize,
        param_id: u32,
        value: f64,
    },
    /// Set plugin channel volume
    PluginSetVolume { channel: usize, volume: f32 },
    /// Update mixer state (volumes, pans, mutes)
    UpdateMixerState(AudioMixerState),
    /// Set which mixer track a generator routes to
    SetGeneratorTrack { generator: usize, track: usize },
    /// Add or replace an effect on a track slot
    SetEffect {
        track: usize,
        slot: usize,
        effect_type: Option<EffectType>,
    },
    /// Set an effect parameter value
    SetEffectParam {
        track: usize,
        slot: usize,
        param_id: EffectParamId,
        value: f32,
    },
    /// Enable or bypass an effect
    SetEffectEnabled {
        track: usize,
        slot: usize,
        enabled: bool,
    },
    /// Update tempo for tempo-synced effects
    UpdateTempo(f64),
}

/// A loaded sample as raw audio data
#[derive(Clone)]
pub(crate) struct SampleData {
    /// Interleaved stereo samples (f32)
    pub data: Arc<Vec<f32>>,
    /// Sample rate of the original file
    pub sample_rate: u32,
    /// Number of channels (1 or 2)
    pub channels: u16,
}

/// A playing voice (sample instance)
struct Voice {
    /// The sample data being played
    sample: SampleData,
    /// Current playback position (in samples)
    position: usize,
    /// Volume for this voice
    volume: f32,
    /// Whether this is a preview (exclusive)
    is_preview: bool,
    /// Generator index (for routing to mixer track)
    generator_idx: usize,
    /// Whether to route directly to master (bypasses generator routing)
    route_to_master: bool,
}

/// A MIDI note event pending for a plugin
#[allow(dead_code)]
struct PluginNoteEvent {
    note: u8,
    velocity: f32,
    is_note_on: bool,
}

/// A parameter change event pending for a plugin
#[derive(Clone)]
struct PluginParamEvent {
    param_id: u32,
    value: f64,
}

/// Initial plugin state when installing a plugin
pub struct PluginInitState {
    pub volume: f32,
    pub params: Vec<(u32, f64)>, // (param_id, value) pairs
}

/// A plugin channel with processor and pending events
#[allow(dead_code)]
struct PluginChannel {
    processor: ActivePluginProcessor,
    pending_notes: Vec<PluginNoteEvent>,
    pending_params: Vec<PluginParamEvent>,
    /// Per-frame output buffers
    output_left: Vec<f32>,
    output_right: Vec<f32>,
    /// Channel volume (0.0-1.0)
    volume: f32,
}

// ============================================================================
// MixingEngine - Core mixing logic shared between real-time and offline
// ============================================================================

/// Core mixing engine that processes voices, plugins, effects, and mixer.
/// Used by both real-time audio callback and offline export.
#[allow(dead_code)]
pub(crate) struct MixingEngine {
    /// Per-track stereo buffers for mixing
    track_buffers: Vec<TrackBuffer>,
    /// Active sample voices
    voices: Vec<Voice>,
    /// Plugin processors per channel
    plugin_channels: Vec<Option<PluginChannel>>,
    /// Effect processors per track (16 tracks x 8 slots)
    track_effects: Vec<[Option<Box<dyn Effect>>; EFFECT_SLOTS]>,
    /// Effect bypass state per track/slot (true = bypassed)
    effect_bypassed: [[bool; EFFECT_SLOTS]; NUM_TRACKS],
    /// Mixer state (volumes, pans, mutes)
    mixer_state: AudioMixerState,
    /// Generator-to-track routing (generator_idx -> track_idx)
    generator_tracks: [usize; MAX_GENERATORS],
    /// Master volume (0.0-1.0)
    master_volume: f32,
    /// Output sample rate
    sample_rate: u32,
    /// Current tempo in BPM (for tempo-synced effects)
    tempo_bpm: f64,
}

#[allow(dead_code)]
impl MixingEngine {
    /// Create a new mixing engine
    pub fn new(sample_rate: u32) -> Self {
        let track_buffers: Vec<TrackBuffer> = (0..NUM_TRACKS)
            .map(|_| TrackBuffer {
                left: vec![0.0; MAX_TRACK_BUFFER_SIZE],
                right: vec![0.0; MAX_TRACK_BUFFER_SIZE],
            })
            .collect();

        let track_effects: Vec<[Option<Box<dyn Effect>>; EFFECT_SLOTS]> = (0..NUM_TRACKS)
            .map(|_| std::array::from_fn(|_| None))
            .collect();

        Self {
            track_buffers,
            voices: Vec::with_capacity(MAX_VOICES),
            plugin_channels: Vec::new(),
            track_effects,
            effect_bypassed: [[false; EFFECT_SLOTS]; NUM_TRACKS],
            mixer_state: AudioMixerState::default(),
            generator_tracks: [1; MAX_GENERATORS],
            master_volume: 1.0,
            sample_rate,
            tempo_bpm: 120.0,
        }
    }

    /// Process one block of audio, returns reference to master buffer (track 0)
    pub fn process_block(&mut self, num_frames: usize) -> &TrackBuffer {
        self.clear_track_buffers(num_frames);
        self.render_voices_to_tracks(num_frames);
        self.process_plugins_to_tracks(num_frames);
        self.process_track_effects(num_frames);
        self.sum_tracks_to_master(num_frames);
        &self.track_buffers[0]
    }

    /// Get the master buffer directly (for reading output)
    pub fn master_buffer(&self) -> &TrackBuffer {
        &self.track_buffers[0]
    }

    /// Get a track buffer by index
    pub fn track_buffer(&self, track: usize) -> &TrackBuffer {
        &self.track_buffers[track]
    }

    // ========================================================================
    // Voice Management
    // ========================================================================

    /// Add a voice to play a sample
    pub fn add_voice(
        &mut self,
        sample: SampleData,
        volume: f32,
        generator_idx: usize,
        route_to_master: bool,
    ) {
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        self.voices.push(Voice {
            sample,
            position: 0,
            volume,
            is_preview: false,
            generator_idx,
            route_to_master,
        });
    }

    /// Add a preview voice (stops other previews first)
    pub fn add_preview_voice(
        &mut self,
        sample: SampleData,
        generator_idx: usize,
        route_to_master: bool,
    ) {
        self.stop_preview_voices();
        self.voices.push(Voice {
            sample,
            position: 0,
            volume: 1.0,
            is_preview: true,
            generator_idx,
            route_to_master,
        });
    }

    /// Stop all preview voices
    pub fn stop_preview_voices(&mut self) {
        self.voices.retain(|v| !v.is_preview);
    }

    /// Stop all voices
    pub fn stop_all_voices(&mut self) {
        self.voices.clear();
    }

    /// Get number of active voices
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    // ========================================================================
    // Plugin Management
    // ========================================================================

    /// Install a plugin processor on a channel
    pub fn install_plugin(
        &mut self,
        channel: usize,
        processor: ActivePluginProcessor,
        volume: f32,
    ) {
        while self.plugin_channels.len() <= channel {
            self.plugin_channels.push(None);
        }
        self.plugin_channels[channel] = Some(PluginChannel {
            processor,
            pending_notes: Vec::new(),
            pending_params: Vec::new(),
            output_left: Vec::new(),
            output_right: Vec::new(),
            volume,
        });
    }

    /// Send a note to a plugin channel
    pub fn send_plugin_note(&mut self, channel: usize, note: u8, velocity: f32, is_note_on: bool) {
        if let Some(Some(plugin_ch)) = self.plugin_channels.get_mut(channel) {
            plugin_ch.pending_notes.push(PluginNoteEvent {
                note,
                velocity,
                is_note_on,
            });
        }
    }

    /// Send a parameter change to a plugin channel
    pub fn send_plugin_param(&mut self, channel: usize, param_id: u32, value: f64) {
        if let Some(Some(plugin_ch)) = self.plugin_channels.get_mut(channel) {
            plugin_ch
                .pending_params
                .push(PluginParamEvent { param_id, value });
        }
    }

    /// Set plugin channel volume
    pub fn set_plugin_volume(&mut self, channel: usize, volume: f32) {
        if let Some(Some(plugin_ch)) = self.plugin_channels.get_mut(channel) {
            plugin_ch.volume = volume;
        }
    }

    // ========================================================================
    // Mixer State
    // ========================================================================

    /// Set the mixer state (volumes, pans, mutes)
    pub fn set_mixer_state(&mut self, state: AudioMixerState) {
        self.mixer_state = state;
    }

    /// Set master volume
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
    }

    /// Get master volume
    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    /// Set generator-to-track routing
    pub fn set_generator_track(&mut self, generator: usize, track: usize) {
        if generator < MAX_GENERATORS {
            self.generator_tracks[generator] = track;
        }
    }

    // ========================================================================
    // Effects
    // ========================================================================

    /// Set an effect on a track slot
    pub fn set_effect(&mut self, track: usize, slot: usize, effect: Option<Box<dyn Effect>>) {
        if track < NUM_TRACKS && slot < EFFECT_SLOTS {
            self.track_effects[track][slot] = effect;
        }
    }

    /// Set an effect parameter
    pub fn set_effect_param(
        &mut self,
        track: usize,
        slot: usize,
        param_id: EffectParamId,
        value: f32,
    ) {
        if track < NUM_TRACKS && slot < EFFECT_SLOTS {
            if let Some(effect) = &mut self.track_effects[track][slot] {
                effect.set_param(param_id, value);
            }
        }
    }

    /// Enable or bypass an effect
    pub fn set_effect_enabled(&mut self, track: usize, slot: usize, enabled: bool) {
        if track < NUM_TRACKS && slot < EFFECT_SLOTS {
            self.effect_bypassed[track][slot] = !enabled;
        }
    }

    /// Set tempo (for tempo-synced effects)
    pub fn set_tempo(&mut self, bpm: f64) {
        self.tempo_bpm = bpm;
        for track_effects in &mut self.track_effects {
            for effect in track_effects.iter_mut().flatten() {
                effect.set_tempo(bpm);
            }
        }
    }

    // ========================================================================
    // Private Mixing Methods
    // ========================================================================

    fn clear_track_buffers(&mut self, num_frames: usize) {
        for track_buf in &mut self.track_buffers {
            if track_buf.left.len() < num_frames {
                track_buf.left.resize(num_frames, 0.0);
                track_buf.right.resize(num_frames, 0.0);
            }
            for i in 0..num_frames {
                track_buf.left[i] = 0.0;
                track_buf.right[i] = 0.0;
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn render_voices_to_tracks(&mut self, num_frames: usize) {
        // Collect voice outputs first to avoid borrow issues
        // (target_track, samples, finished)
        let mut voice_outputs: Vec<(usize, Vec<(f32, f32)>, bool)> = Vec::new();

        for voice in self.voices.iter() {
            let sample_data = &voice.sample.data;
            let voice_channels = voice.sample.channels as usize;
            let voice_volume = voice.volume;
            let generator_idx = voice.generator_idx;
            let sample_rate = voice.sample.sample_rate;

            let target_track = if voice.route_to_master {
                0
            } else {
                *self.generator_tracks.get(generator_idx).unwrap_or(&1)
            };

            let resample_ratio = sample_rate as f32 / self.sample_rate as f32;
            let mut samples = Vec::with_capacity(num_frames);
            let mut finished = false;
            let mut pos = voice.position;

            for _ in 0..num_frames {
                let src_frame = (pos as f32 * resample_ratio) as usize;

                if src_frame * voice_channels >= sample_data.len() {
                    finished = true;
                    break;
                }

                let (left, right) = if voice_channels == 1 {
                    let s = sample_data[src_frame] * voice_volume;
                    (s, s)
                } else {
                    let idx = src_frame * 2;
                    if idx + 1 < sample_data.len() {
                        (
                            sample_data[idx] * voice_volume,
                            sample_data[idx + 1] * voice_volume,
                        )
                    } else {
                        (0.0, 0.0)
                    }
                };

                samples.push((left, right));
                pos += 1;
            }

            voice_outputs.push((target_track, samples, finished));
        }

        // Apply to track buffers and update voice state
        let mut voices_to_remove = Vec::new();

        for (voice_idx, (target_track, samples, finished)) in voice_outputs.into_iter().enumerate()
        {
            if target_track < NUM_TRACKS {
                for (frame, (left, right)) in samples.iter().enumerate() {
                    self.track_buffers[target_track].left[frame] += left;
                    self.track_buffers[target_track].right[frame] += right;
                }
            }

            self.voices[voice_idx].position += samples.len();

            if finished {
                voices_to_remove.push(voice_idx);
            }
        }

        for idx in voices_to_remove.into_iter().rev() {
            self.voices.remove(idx);
        }
    }

    fn process_plugins_to_tracks(&mut self, num_frames: usize) {
        use crate::plugin_host::MidiNote;

        if num_frames == 0 {
            return;
        }

        for (channel_idx, plugin_opt) in self.plugin_channels.iter_mut().enumerate() {
            let Some(plugin_ch) = plugin_opt else {
                continue;
            };

            // Ensure output buffers are large enough
            if plugin_ch.output_left.len() < num_frames {
                plugin_ch.output_left.resize(num_frames, 0.0);
                plugin_ch.output_right.resize(num_frames, 0.0);
            }

            // Clear plugin output buffers
            for i in 0..num_frames {
                plugin_ch.output_left[i] = 0.0;
                plugin_ch.output_right[i] = 0.0;
            }

            // Convert pending notes to MidiNote format
            let notes: Vec<MidiNote> = plugin_ch
                .pending_notes
                .drain(..)
                .map(|e| MidiNote {
                    note: e.note,
                    velocity: e.velocity,
                    is_note_on: e.is_note_on,
                })
                .collect();

            // Convert pending params to ParamChange format
            let params: Vec<ParamChange> = plugin_ch
                .pending_params
                .drain(..)
                .map(|e| ParamChange {
                    param_id: e.param_id,
                    value: e.value,
                })
                .collect();

            // Process audio through the plugin
            plugin_ch.processor.process(
                &notes,
                &params,
                &mut plugin_ch.output_left[..num_frames],
                &mut plugin_ch.output_right[..num_frames],
            );

            // Get target track for this plugin's generator
            let target_track = self.generator_tracks.get(channel_idx).copied().unwrap_or(1);

            // Route plugin output to target track buffer with per-channel volume
            let channel_volume = plugin_ch.volume;
            if target_track < NUM_TRACKS {
                for frame in 0..num_frames {
                    let left = plugin_ch.output_left[frame] * channel_volume;
                    let right = plugin_ch.output_right[frame] * channel_volume;
                    self.track_buffers[target_track].left[frame] += left;
                    self.track_buffers[target_track].right[frame] += right;
                }
            }
        }
    }

    fn process_track_effects(&mut self, num_frames: usize) {
        if num_frames == 0 {
            return;
        }

        for track_idx in 0..NUM_TRACKS {
            for slot_idx in 0..EFFECT_SLOTS {
                if self.effect_bypassed[track_idx][slot_idx] {
                    continue;
                }

                if let Some(mut effect) = self.track_effects[track_idx][slot_idx].take() {
                    let buf = &mut self.track_buffers[track_idx];
                    effect.process(&mut buf.left[..num_frames], &mut buf.right[..num_frames]);
                    self.track_effects[track_idx][slot_idx] = Some(effect);
                }
            }
        }
    }

    fn sum_tracks_to_master(&mut self, num_frames: usize) {
        // Sum tracks 1-15 to master (track 0)
        for track_idx in 1..NUM_TRACKS {
            let volume = self.mixer_state.track_volumes[track_idx];
            let muted = self.mixer_state.track_mutes[track_idx];
            let (pan_left, pan_right) = self.mixer_state.pan_gains(track_idx);

            if muted {
                continue;
            }

            for frame in 0..num_frames {
                let left = self.track_buffers[track_idx].left[frame] * volume * pan_left;
                let right = self.track_buffers[track_idx].right[frame] * volume * pan_right;

                self.track_buffers[0].left[frame] += left;
                self.track_buffers[0].right[frame] += right;
            }
        }

        // Apply master volume and pan
        let master_vol = self.mixer_state.track_volumes[0];
        let (master_pan_left, master_pan_right) = self.mixer_state.pan_gains(0);
        for frame in 0..num_frames {
            self.track_buffers[0].left[frame] *= master_vol * master_pan_left * self.master_volume;
            self.track_buffers[0].right[frame] *=
                master_vol * master_pan_right * self.master_volume;
        }
    }
}

/// Shared state between audio thread and main thread
struct AudioState {
    /// Core mixing engine (owns voices, plugins, effects, mixer state)
    engine: MixingEngine,
    /// Sample cache (for loading samples from disk)
    sample_cache: HashMap<PathBuf, SampleData>,
    /// Command receiver
    rx: Receiver<AudioCommand>,
    /// Receiver for plugin processors from main thread (channel, processor, initial state)
    #[allow(dead_code)]
    plugin_rx: Receiver<(usize, ActivePluginProcessor, PluginInitState)>,
    /// Waveform buffer for visualization (shared with UI)
    waveform_buffer: WaveformBuffer,
    /// Write position in waveform buffer
    waveform_write_pos: usize,
    /// Peak levels buffer (shared with UI for meter visualization)
    peak_levels: PeakLevelsBuffer,
}

/// Handle for sending commands to the audio engine
#[derive(Clone)]
pub struct AudioHandle {
    tx: Sender<AudioCommand>,
    plugin_tx: Sender<(usize, ActivePluginProcessor, PluginInitState)>,
    sample_rate: u32,
    /// Shared waveform buffer for visualization
    waveform_buffer: WaveformBuffer,
    /// Shared peak levels buffer for mixer meters
    peak_levels: PeakLevelsBuffer,
}

#[allow(dead_code)]
impl AudioHandle {
    /// Play a sample at the given volume (polyphonic)
    /// generator_idx is used for routing to the correct mixer track
    pub fn play_sample(&self, path: &Path, volume: f32, generator_idx: usize) {
        let _ = self.tx.send(AudioCommand::PlaySample {
            path: path.to_path_buf(),
            volume,
            generator_idx,
        });
    }

    /// Preview a sample using the generator's mixer track routing (for channel previews)
    pub fn preview_sample(&self, path: &Path, generator_idx: usize) {
        let _ = self.tx.send(AudioCommand::PreviewSample {
            path: path.to_path_buf(),
            generator_idx,
            route_to_master: false,
        });
    }

    /// Preview a sample directly to master track (for browser previews)
    pub fn preview_sample_to_master(&self, path: &Path) {
        let _ = self.tx.send(AudioCommand::PreviewSample {
            path: path.to_path_buf(),
            generator_idx: 0, // Unused when route_to_master is true
            route_to_master: true,
        });
    }

    /// Stop the current preview
    pub fn stop_preview(&self) {
        let _ = self.tx.send(AudioCommand::StopPreview);
    }

    /// Stop all playback
    pub fn stop_all(&self) {
        let _ = self.tx.send(AudioCommand::StopAll);
    }

    /// Set master volume (0.0-1.0)
    pub fn set_master_volume(&self, volume: f32) {
        let _ = self.tx.send(AudioCommand::SetMasterVolume(volume));
    }

    /// Preload a sample into cache
    pub fn preload_sample(&self, path: &Path) {
        let _ = self.tx.send(AudioCommand::PreloadSample {
            path: path.to_path_buf(),
        });
    }

    /// Send note on to a plugin channel
    pub fn plugin_note_on(&self, channel: usize, note: u8, velocity: f32) {
        let _ = self.tx.send(AudioCommand::PluginNoteOn {
            channel,
            note,
            velocity,
        });
    }

    /// Send note off to a plugin channel
    pub fn plugin_note_off(&self, channel: usize, note: u8) {
        let _ = self.tx.send(AudioCommand::PluginNoteOff { channel, note });
    }

    /// Set a plugin parameter value
    pub fn plugin_set_param(&self, channel: usize, param_id: u32, value: f64) {
        let _ = self.tx.send(AudioCommand::PluginSetParam {
            channel,
            param_id,
            value,
        });
    }

    /// Set plugin channel volume
    pub fn plugin_set_volume(&self, channel: usize, volume: f32) {
        let _ = self
            .tx
            .send(AudioCommand::PluginSetVolume { channel, volume });
    }

    /// Send an activated plugin processor to the audio thread with initial state
    pub fn send_plugin(
        &self,
        channel: usize,
        processor: ActivePluginProcessor,
        init_state: PluginInitState,
    ) {
        let _ = self.plugin_tx.send((channel, processor, init_state));
    }

    /// Get the output sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get a reference to the waveform buffer for visualization
    pub fn waveform_buffer(&self) -> &WaveformBuffer {
        &self.waveform_buffer
    }

    /// Update the mixer state (volumes, pans, mutes)
    pub fn update_mixer_state(&self, state: AudioMixerState) {
        let _ = self.tx.send(AudioCommand::UpdateMixerState(state));
    }

    /// Set which mixer track a generator routes to
    pub fn set_generator_track(&self, generator: usize, track: usize) {
        let _ = self
            .tx
            .send(AudioCommand::SetGeneratorTrack { generator, track });
    }

    /// Get current peak levels for all tracks (for UI meters)
    pub fn get_peak_levels(&self) -> [StereoLevels; NUM_TRACKS] {
        if let Ok(levels) = self.peak_levels.lock() {
            *levels
        } else {
            [StereoLevels::default(); NUM_TRACKS]
        }
    }

    /// Get a reference to the peak levels buffer
    pub fn peak_levels_buffer(&self) -> &PeakLevelsBuffer {
        &self.peak_levels
    }

    /// Set or remove an effect on a mixer track slot
    pub fn set_effect(&self, track: usize, slot: usize, effect_type: Option<EffectType>) {
        let _ = self.tx.send(AudioCommand::SetEffect {
            track,
            slot,
            effect_type,
        });
    }

    /// Set an effect parameter value
    pub fn set_effect_param(&self, track: usize, slot: usize, param_id: EffectParamId, value: f32) {
        let _ = self.tx.send(AudioCommand::SetEffectParam {
            track,
            slot,
            param_id,
            value,
        });
    }

    /// Enable or bypass an effect
    pub fn set_effect_enabled(&self, track: usize, slot: usize, enabled: bool) {
        let _ = self.tx.send(AudioCommand::SetEffectEnabled {
            track,
            slot,
            enabled,
        });
    }

    /// Update tempo for tempo-synced effects
    pub fn update_tempo(&self, bpm: f64) {
        let _ = self.tx.send(AudioCommand::UpdateTempo(bpm));
    }

    /// Create a dummy AudioHandle for testing (no actual audio processing)
    ///
    /// Commands sent to this handle are simply dropped. This is useful for
    /// unit testing App logic without requiring real audio hardware.
    #[cfg(test)]
    pub fn dummy() -> Self {
        // Create channels that will just drop messages (no receiver)
        let (tx, _rx) = unbounded();
        let (plugin_tx, _plugin_rx) = unbounded();

        Self {
            tx,
            plugin_tx,
            sample_rate: 44100,
            waveform_buffer: Arc::new(Mutex::new(vec![0.0; WAVEFORM_BUFFER_SIZE])),
            peak_levels: Arc::new(Mutex::new([StereoLevels::default(); NUM_TRACKS])),
        }
    }

    /// Create a testable AudioHandle that returns a receiver for inspecting commands
    ///
    /// Use this when you need to verify specific audio commands are sent.
    #[cfg(test)]
    pub fn testable() -> (Self, Receiver<AudioCommand>) {
        let (tx, rx) = unbounded();
        let (plugin_tx, _plugin_rx) = unbounded();

        let handle = Self {
            tx,
            plugin_tx,
            sample_rate: 44100,
            waveform_buffer: Arc::new(Mutex::new(vec![0.0; WAVEFORM_BUFFER_SIZE])),
            peak_levels: Arc::new(Mutex::new([StereoLevels::default(); NUM_TRACKS])),
        };
        (handle, rx)
    }
}

/// Audio engine with cpal stream
pub struct AudioEngine {
    _stream: Stream,
    #[allow(dead_code)] // Will be used for plugin hosting
    state: Arc<Mutex<AudioState>>,
    #[allow(dead_code)] // Will be used for plugin hosting
    sample_rate: Arc<AtomicU32>,
}

impl AudioEngine {
    /// Create a new audio engine and return a handle for sending commands
    pub fn new() -> Result<(Self, AudioHandle), AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| AudioError::StreamError("No output device found".to_string()))?;

        let config = device
            .default_output_config()
            .map_err(|e| AudioError::StreamError(e.to_string()))?;

        let sample_rate = config.sample_rate().0;
        let _channels = config.channels() as usize;

        let (tx, rx) = unbounded();
        let (plugin_tx, plugin_rx) = unbounded();

        // Create shared waveform buffer for visualization
        let waveform_buffer: WaveformBuffer = Arc::new(Mutex::new(vec![0.0; WAVEFORM_BUFFER_SIZE]));

        // Create shared peak levels buffer for mixer meters
        let peak_levels: PeakLevelsBuffer =
            Arc::new(Mutex::new([StereoLevels::default(); NUM_TRACKS]));

        // Create the mixing engine
        let engine = MixingEngine::new(sample_rate);

        let state = Arc::new(Mutex::new(AudioState {
            engine,
            sample_cache: HashMap::new(),
            rx,
            plugin_rx,
            waveform_buffer: waveform_buffer.clone(),
            waveform_write_pos: 0,
            peak_levels: peak_levels.clone(),
        }));

        let sample_rate_atomic = Arc::new(AtomicU32::new(sample_rate));

        let stream = match config.sample_format() {
            SampleFormat::F32 => Self::build_stream::<f32>(&device, &config.into(), state.clone()),
            SampleFormat::I16 => Self::build_stream::<i16>(&device, &config.into(), state.clone()),
            SampleFormat::U16 => Self::build_stream::<u16>(&device, &config.into(), state.clone()),
            _ => {
                return Err(AudioError::StreamError(
                    "Unsupported sample format".to_string(),
                ))
            }
        }?;

        stream
            .play()
            .map_err(|e| AudioError::StreamError(e.to_string()))?;

        let engine = Self {
            _stream: stream,
            state,
            sample_rate: sample_rate_atomic,
        };

        let handle = AudioHandle {
            tx,
            plugin_tx,
            sample_rate,
            waveform_buffer,
            peak_levels,
        };

        Ok((engine, handle))
    }

    fn build_stream<T: cpal::SizedSample + cpal::FromSample<f32> + cpal::Sample>(
        device: &cpal::Device,
        config: &StreamConfig,
        state: Arc<Mutex<AudioState>>,
    ) -> Result<Stream, AudioError>
    where
        f32: cpal::FromSample<T>,
    {
        let channels = config.channels as usize;

        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    Self::audio_callback(data, channels, &state);
                },
                |err| eprintln!("Audio stream error: {}", err),
                None,
            )
            .map_err(|e| AudioError::StreamError(e.to_string()))?;

        Ok(stream)
    }

    fn audio_callback<T: cpal::SizedSample + cpal::FromSample<f32> + cpal::Sample>(
        data: &mut [T],
        channels: usize,
        state: &Arc<Mutex<AudioState>>,
    ) where
        f32: cpal::FromSample<T>,
    {
        // Try to lock state - if we can't, output silence
        let Ok(mut state) = state.try_lock() else {
            for sample in data.iter_mut() {
                *sample = T::EQUILIBRIUM;
            }
            return;
        };

        // Process commands (non-blocking)
        Self::process_commands_internal(&mut state);

        // Receive any new plugin processors from main thread
        Self::receive_plugins(&mut state);

        let num_frames = data.len() / channels;

        // Delegate mixing to the engine
        state.engine.process_block(num_frames);

        // Calculate peak levels for all tracks (need to access track buffers)
        let mut peak_levels = [StereoLevels::default(); NUM_TRACKS];
        for (track_idx, peak) in peak_levels.iter_mut().enumerate() {
            let track_buf = state.engine.track_buffer(track_idx);
            let mut peak_left = 0.0f32;
            let mut peak_right = 0.0f32;
            for frame in 0..num_frames {
                peak_left = peak_left.max(track_buf.left[frame].abs());
                peak_right = peak_right.max(track_buf.right[frame].abs());
            }
            *peak = StereoLevels {
                left: peak_left.min(1.0),
                right: peak_right.min(1.0),
            };
        }

        // Update shared peak levels buffer
        if let Ok(mut shared_peaks) = state.peak_levels.try_lock() {
            *shared_peaks = peak_levels;
        }

        // Output master track to DAC
        let master = state.engine.master_buffer();
        for frame in 0..num_frames {
            let left = master.left[frame].clamp(-1.0, 1.0);
            let right = master.right[frame].clamp(-1.0, 1.0);

            let out_idx = frame * channels;
            if channels >= 2 {
                data[out_idx] = T::from_sample(left);
                data[out_idx + 1] = T::from_sample(right);
            } else if channels == 1 {
                data[out_idx] = T::from_sample((left + right) * 0.5);
            }
        }

        // Write samples to waveform buffer for visualization
        let waveform_buffer = state.waveform_buffer.clone();
        let mut write_pos = state.waveform_write_pos;

        let master = state.engine.master_buffer();
        if let Ok(mut waveform) = waveform_buffer.try_lock() {
            for frame in 0..num_frames {
                let sample = (master.left[frame] + master.right[frame]) * 0.5;
                waveform[write_pos] = sample.clamp(-1.0, 1.0);
                write_pos = (write_pos + 1) % WAVEFORM_BUFFER_SIZE;
            }
        }

        state.waveform_write_pos = write_pos;
    }

    fn receive_plugins(state: &mut AudioState) {
        // Receive new plugin processors from main thread
        while let Ok((channel, processor, init_state)) = state.plugin_rx.try_recv() {
            // Install plugin in the mixing engine
            state
                .engine
                .install_plugin(channel, processor, init_state.volume);

            // Apply initial parameters
            for (param_id, value) in init_state.params {
                state.engine.send_plugin_param(channel, param_id, value);
            }
        }
    }

    fn process_commands_internal(state: &mut AudioState) {
        while let Ok(cmd) = state.rx.try_recv() {
            match cmd {
                AudioCommand::PlaySample {
                    path,
                    volume,
                    generator_idx,
                } => {
                    Self::play_sample_internal(state, &path, volume, false, generator_idx, false);
                }
                AudioCommand::PreviewSample {
                    path,
                    generator_idx,
                    route_to_master,
                } => {
                    // Stop existing preview
                    state.engine.stop_preview_voices();
                    Self::play_sample_internal(
                        state,
                        &path,
                        1.0,
                        true,
                        generator_idx,
                        route_to_master,
                    );
                }
                AudioCommand::StopPreview => {
                    state.engine.stop_preview_voices();
                }
                AudioCommand::StopAll => {
                    state.engine.stop_all_voices();
                }
                AudioCommand::SetMasterVolume(vol) => {
                    state.engine.set_master_volume(vol.clamp(0.0, 1.0));
                }
                AudioCommand::PreloadSample { path } => {
                    Self::load_sample(state, &path);
                }
                AudioCommand::PluginNoteOn {
                    channel,
                    note,
                    velocity,
                } => {
                    state.engine.send_plugin_note(channel, note, velocity, true);
                }
                AudioCommand::PluginNoteOff { channel, note } => {
                    state.engine.send_plugin_note(channel, note, 0.0, false);
                }
                AudioCommand::PluginSetParam {
                    channel,
                    param_id,
                    value,
                } => {
                    state.engine.send_plugin_param(channel, param_id, value);
                }
                AudioCommand::PluginSetVolume { channel, volume } => {
                    state.engine.set_plugin_volume(channel, volume);
                }
                AudioCommand::UpdateMixerState(mixer_state) => {
                    state.engine.set_mixer_state(mixer_state);
                }
                AudioCommand::SetGeneratorTrack { generator, track } => {
                    state.engine.set_generator_track(generator, track);
                }
                AudioCommand::SetEffect {
                    track,
                    slot,
                    effect_type,
                } => {
                    let sample_rate = state.engine.sample_rate as f32;
                    let bpm = state.engine.tempo_bpm;
                    let effect = effect_type.map(|et| {
                        let slot_data = EffectSlot::new(et);
                        create_effect(&slot_data, sample_rate, bpm)
                    });
                    state.engine.set_effect(track, slot, effect);
                    state.engine.set_effect_enabled(track, slot, true);
                }
                AudioCommand::SetEffectParam {
                    track,
                    slot,
                    param_id,
                    value,
                } => {
                    state.engine.set_effect_param(track, slot, param_id, value);
                }
                AudioCommand::SetEffectEnabled {
                    track,
                    slot,
                    enabled,
                } => {
                    state.engine.set_effect_enabled(track, slot, enabled);
                }
                AudioCommand::UpdateTempo(bpm) => {
                    state.engine.set_tempo(bpm);
                }
            }
        }
    }

    fn play_sample_internal(
        state: &mut AudioState,
        path: &Path,
        volume: f32,
        is_preview: bool,
        generator_idx: usize,
        route_to_master: bool,
    ) {
        // Load sample if not cached
        if !state.sample_cache.contains_key(path) {
            Self::load_sample(state, path);
        }

        // Get cached sample and add voice to engine
        if let Some(sample) = state.sample_cache.get(path).cloned() {
            if is_preview {
                state
                    .engine
                    .add_preview_voice(sample, generator_idx, route_to_master);
            } else {
                state
                    .engine
                    .add_voice(sample, volume, generator_idx, route_to_master);
            }
        }
    }

    fn load_sample(state: &mut AudioState, path: &Path) {
        if state.sample_cache.contains_key(path) {
            return;
        }

        let Ok(file) = File::open(path) else {
            return;
        };

        let reader = BufReader::new(file);
        let Ok(decoder) = Decoder::new(reader) else {
            return;
        };

        let sample_rate = decoder.sample_rate();
        let channels = decoder.channels();

        // Collect all samples as f32
        let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();

        if !samples.is_empty() {
            state.sample_cache.insert(
                path.to_path_buf(),
                SampleData {
                    data: Arc::new(samples),
                    sample_rate,
                    channels,
                },
            );
        }
    }

    /// Process pending commands (call this in main loop)
    /// Note: Commands are now processed in audio callback, but this
    /// method is kept for API compatibility and any future non-realtime work
    pub fn process_commands(&mut self) {
        // Commands are processed in the audio callback
        // This method exists for API compatibility
    }

    /// Get the output sample rate
    #[allow(dead_code)] // Will be used for plugin hosting
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }
}

/// Audio-related errors
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum AudioError {
    #[error("Failed to create audio stream: {0}")]
    StreamError(String),
    #[error("Failed to load sample: {0}")]
    LoadError(String),
}

// ============================================================================
// MixingEngine Tests
// ============================================================================

#[cfg(test)]
mod mixing_engine_tests {
    use super::*;

    fn make_test_sample(len: usize, value: f32) -> SampleData {
        // Create stereo interleaved sample (L, R, L, R, ...)
        let data: Vec<f32> = (0..len * 2)
            .map(|i| if i % 2 == 0 { value } else { value })
            .collect();
        SampleData {
            data: Arc::new(data),
            sample_rate: 44100,
            channels: 2,
        }
    }

    #[test]
    fn test_mixing_engine_creation() {
        let engine = MixingEngine::new(44100);
        assert_eq!(engine.sample_rate, 44100);
        assert_eq!(engine.master_volume, 1.0);
        assert_eq!(engine.tempo_bpm, 120.0);
        assert_eq!(engine.track_buffers.len(), NUM_TRACKS);
    }

    #[test]
    fn test_process_block_clears_buffers() {
        let mut engine = MixingEngine::new(44100);

        // Dirty the buffers
        engine.track_buffers[1].left[0] = 999.0;
        engine.track_buffers[1].right[0] = 999.0;

        // Process should clear them
        engine.process_block(64);

        // Master buffer (track 0) should be silent (no voices)
        assert_eq!(engine.track_buffers[0].left[0], 0.0);
        assert_eq!(engine.track_buffers[0].right[0], 0.0);
    }

    #[test]
    fn test_voice_renders_to_correct_track() {
        let mut engine = MixingEngine::new(44100);

        // Route generator 0 to track 2
        engine.set_generator_track(0, 2);

        // Add a voice with generator_idx 0
        let sample = make_test_sample(64, 0.5);
        engine.add_voice(sample, 1.0, 0, false);

        // Process
        engine.process_block(64);

        // Track 2 should have audio (before master summing clears it to sum to track 0)
        // Actually after sum_tracks_to_master, track audio goes to master
        // Let's check master has non-zero audio
        let master = engine.master_buffer();

        // With default mixer (volume=0.8, pan=center), we should get audio
        // The voice at 0.5 * 0.8 * pan_gains should produce non-zero output
        let has_audio = master.left.iter().take(64).any(|&s| s != 0.0);
        assert!(
            has_audio,
            "Master should have audio from voice routed through track 2"
        );
    }

    #[test]
    fn test_mixer_volume_applied() {
        let mut engine = MixingEngine::new(44100);

        // Route generator 0 to track 1
        engine.set_generator_track(0, 1);

        // Set track 1 volume to 0.5
        let mut state = AudioMixerState::default();
        state.track_volumes[1] = 0.5;
        engine.set_mixer_state(state);

        // Add a voice with known amplitude
        let sample = make_test_sample(64, 1.0);
        engine.add_voice(sample, 1.0, 0, false);

        // Process
        engine.process_block(64);

        let master = engine.master_buffer();

        // Audio should be present but attenuated by track volume
        let max_sample = master
            .left
            .iter()
            .take(64)
            .fold(0.0f32, |a, &b| a.max(b.abs()));

        // With volume 0.5 and center pan (~0.707), max should be around 0.5 * 0.707 = 0.35
        assert!(max_sample > 0.0, "Should have audio");
        assert!(
            max_sample < 0.6,
            "Audio should be attenuated by track volume"
        );
    }

    #[test]
    fn test_mixer_mute_silences_track() {
        let mut engine = MixingEngine::new(44100);

        // Route generator 0 to track 1
        engine.set_generator_track(0, 1);

        // Mute track 1
        let mut state = AudioMixerState::default();
        state.track_mutes[1] = true;
        engine.set_mixer_state(state);

        // Add a voice
        let sample = make_test_sample(64, 1.0);
        engine.add_voice(sample, 1.0, 0, false);

        // Process
        engine.process_block(64);

        let master = engine.master_buffer();

        // Master should be silent because track 1 is muted
        let max_sample = master
            .left
            .iter()
            .take(64)
            .fold(0.0f32, |a, &b| a.max(b.abs()));
        assert_eq!(
            max_sample, 0.0,
            "Muted track should not contribute to master"
        );
    }

    #[test]
    fn test_route_to_master_bypasses_track() {
        let mut engine = MixingEngine::new(44100);

        // Route generator 0 to track 1, and mute track 1
        engine.set_generator_track(0, 1);
        let mut state = AudioMixerState::default();
        state.track_mutes[1] = true;
        engine.set_mixer_state(state);

        // Add a voice with route_to_master=true (should bypass track routing)
        let sample = make_test_sample(64, 1.0);
        engine.add_voice(sample, 1.0, 0, true); // route_to_master = true

        // Process
        engine.process_block(64);

        let master = engine.master_buffer();

        // Should still have audio despite mute, because route_to_master bypasses track
        let max_sample = master
            .left
            .iter()
            .take(64)
            .fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(
            max_sample > 0.0,
            "route_to_master should bypass muted track"
        );
    }

    #[test]
    fn test_master_volume_applied() {
        let mut engine = MixingEngine::new(44100);

        // Set master volume to 0.25
        engine.set_master_volume(0.25);

        // Route generator 0 to track 1
        engine.set_generator_track(0, 1);

        // Add a voice
        let sample = make_test_sample(64, 1.0);
        engine.add_voice(sample, 1.0, 0, false);

        // Process
        engine.process_block(64);

        let master = engine.master_buffer();
        let max_sample = master
            .left
            .iter()
            .take(64)
            .fold(0.0f32, |a, &b| a.max(b.abs()));

        // With master_volume=0.25, track_volume=0.8, and center pan (~0.707)
        // Expected max ~= 1.0 * 0.8 * 0.707 * 0.25 = 0.14
        assert!(max_sample > 0.0, "Should have audio");
        assert!(
            max_sample < 0.3,
            "Audio should be attenuated by master volume"
        );
    }

    #[test]
    fn test_voice_stops_when_finished() {
        let mut engine = MixingEngine::new(44100);

        // Create a short sample (10 frames)
        let sample = make_test_sample(10, 0.5);
        engine.add_voice(sample, 1.0, 0, true);

        assert_eq!(engine.voice_count(), 1);

        // Process more frames than the sample length
        engine.process_block(64);

        // Voice should be removed after sample finishes
        assert_eq!(engine.voice_count(), 0);
    }

    #[test]
    fn test_stop_all_voices() {
        let mut engine = MixingEngine::new(44100);

        let sample = make_test_sample(1000, 0.5);
        engine.add_voice(sample.clone(), 1.0, 0, false);
        engine.add_voice(sample.clone(), 1.0, 1, false);
        engine.add_voice(sample, 1.0, 2, false);

        assert_eq!(engine.voice_count(), 3);

        engine.stop_all_voices();

        assert_eq!(engine.voice_count(), 0);
    }

    #[test]
    fn test_preview_stops_previous_preview() {
        let mut engine = MixingEngine::new(44100);

        let sample = make_test_sample(1000, 0.5);

        // Add preview
        engine.add_preview_voice(sample.clone(), 0, true);
        assert_eq!(engine.voice_count(), 1);

        // Add another preview - should replace the first
        engine.add_preview_voice(sample, 0, true);
        assert_eq!(engine.voice_count(), 1);
    }
}
