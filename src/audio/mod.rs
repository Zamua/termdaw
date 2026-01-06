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
struct TrackBuffer {
    left: Vec<f32>,
    right: Vec<f32>,
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
struct SampleData {
    /// Interleaved stereo samples (f32)
    data: Arc<Vec<f32>>,
    /// Sample rate of the original file
    sample_rate: u32,
    /// Number of channels (1 or 2)
    channels: u16,
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

/// Shared state between audio thread and main thread
struct AudioState {
    /// Active voices
    voices: Vec<Voice>,
    /// Master volume
    master_volume: f32,
    /// Sample cache
    sample_cache: HashMap<PathBuf, SampleData>,
    /// Command receiver
    rx: Receiver<AudioCommand>,
    /// Output sample rate
    output_sample_rate: u32,
    /// Plugin channels (sparse - only Some if plugin is loaded)
    #[allow(dead_code)]
    plugin_channels: Vec<Option<PluginChannel>>,
    /// Receiver for plugin processors from main thread (channel, processor, initial state)
    #[allow(dead_code)]
    plugin_rx: Receiver<(usize, ActivePluginProcessor, PluginInitState)>,
    /// Waveform buffer for visualization (shared with UI)
    waveform_buffer: WaveformBuffer,
    /// Write position in waveform buffer
    waveform_write_pos: usize,
    /// Per-track stereo buffers for mixing
    track_buffers: Vec<TrackBuffer>,
    /// Mixer state (volumes, pans, mutes)
    mixer_state: AudioMixerState,
    /// Generator-to-track routing (generator_idx -> track_idx)
    generator_tracks: [usize; MAX_GENERATORS],
    /// Peak levels buffer (shared with UI for meter visualization)
    peak_levels: PeakLevelsBuffer,
    /// Effect processors per track (16 tracks x 8 slots)
    /// None = empty slot, Some = active effect processor
    track_effects: Vec<[Option<Box<dyn Effect>>; EFFECT_SLOTS]>,
    /// Effect bypass state per track/slot (true = bypassed)
    effect_bypassed: [[bool; EFFECT_SLOTS]; NUM_TRACKS],
    /// Current tempo in BPM (for tempo-synced effects)
    tempo_bpm: f64,
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

        // Create per-track stereo buffers
        let track_buffers: Vec<TrackBuffer> = (0..NUM_TRACKS)
            .map(|_| TrackBuffer {
                left: vec![0.0; MAX_TRACK_BUFFER_SIZE],
                right: vec![0.0; MAX_TRACK_BUFFER_SIZE],
            })
            .collect();

        // Default generator routing: all to track 1 (first non-master)
        let generator_tracks = [1usize; MAX_GENERATORS];

        // Initialize empty effect slots for all tracks
        let track_effects: Vec<[Option<Box<dyn Effect>>; EFFECT_SLOTS]> = (0..NUM_TRACKS)
            .map(|_| std::array::from_fn(|_| None))
            .collect();

        let state = Arc::new(Mutex::new(AudioState {
            voices: Vec::with_capacity(MAX_VOICES),
            master_volume: 1.0,
            sample_cache: HashMap::new(),
            rx,
            output_sample_rate: sample_rate,
            plugin_channels: Vec::new(),
            plugin_rx,
            waveform_buffer: waveform_buffer.clone(),
            waveform_write_pos: 0,
            track_buffers,
            mixer_state: AudioMixerState::default(),
            generator_tracks,
            peak_levels: peak_levels.clone(),
            track_effects,
            effect_bypassed: [[false; EFFECT_SLOTS]; NUM_TRACKS],
            tempo_bpm: 140.0,
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

    #[allow(clippy::type_complexity)]
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

        let master_volume = state.master_volume;
        let output_sample_rate = state.output_sample_rate;
        let num_frames = data.len() / channels;

        // Ensure track buffers are large enough
        for track_buf in &mut state.track_buffers {
            if track_buf.left.len() < num_frames {
                track_buf.left.resize(num_frames, 0.0);
                track_buf.right.resize(num_frames, 0.0);
            }
        }

        // Step 1: Clear all track buffers
        for track_buf in &mut state.track_buffers {
            for i in 0..num_frames {
                track_buf.left[i] = 0.0;
                track_buf.right[i] = 0.0;
            }
        }

        // Step 2: Route voices to their assigned track buffers
        // We need to split borrows to satisfy the borrow checker
        // First, collect what we need to write, then write it
        let mut voice_outputs: Vec<(usize, Vec<(f32, f32)>, bool)> = Vec::new(); // (target_track, samples, finished)

        for voice in state.voices.iter() {
            let sample_data = &voice.sample.data;
            let voice_channels = voice.sample.channels as usize;
            let voice_volume = voice.volume;
            let generator_idx = voice.generator_idx;
            let sample_rate = voice.sample.sample_rate;

            // Browser previews go to master, channel sounds use their assigned routing
            let target_track = if voice.route_to_master {
                0 // Master track (for browser previews)
            } else {
                *state.generator_tracks.get(generator_idx).unwrap_or(&1)
            };

            let resample_ratio = sample_rate as f32 / output_sample_rate as f32;
            let mut samples = Vec::with_capacity(num_frames);
            let mut finished = false;
            let mut pos = voice.position;

            for _ in 0..num_frames {
                let src_frame = (pos as f32 * resample_ratio) as usize;

                if src_frame * voice_channels >= sample_data.len() {
                    finished = true;
                    break;
                }

                // Get sample (mono or stereo)
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

        // Now apply the samples to track buffers and update voice positions
        let mut voices_to_remove = Vec::new();

        for (voice_idx, (target_track, samples, finished)) in voice_outputs.into_iter().enumerate()
        {
            // Write samples to track buffer
            if target_track < NUM_TRACKS {
                for (frame, (left, right)) in samples.iter().enumerate() {
                    state.track_buffers[target_track].left[frame] += left;
                    state.track_buffers[target_track].right[frame] += right;
                }
            }

            // Update voice position
            state.voices[voice_idx].position += samples.len();

            if finished {
                voices_to_remove.push(voice_idx);
            }
        }

        // Remove finished voices (in reverse order to maintain indices)
        for idx in voices_to_remove.into_iter().rev() {
            state.voices.remove(idx);
        }

        // Step 3: Process plugins and route to their assigned track buffers
        Self::process_plugins_to_tracks(&mut state, num_frames);

        // Step 3.5: Process insert effects on each track buffer
        Self::process_track_effects(&mut state, num_frames);

        // Step 4: Apply mixer state and sum all tracks to master
        // Process tracks 1-15 first, then apply their output to master (track 0)
        for track_idx in 1..NUM_TRACKS {
            let volume = state.mixer_state.track_volumes[track_idx];
            let muted = state.mixer_state.track_mutes[track_idx];
            let (pan_left, pan_right) = state.mixer_state.pan_gains(track_idx);

            if muted {
                // If muted, don't route to master
                continue;
            }

            for frame in 0..num_frames {
                let left = state.track_buffers[track_idx].left[frame] * volume * pan_left;
                let right = state.track_buffers[track_idx].right[frame] * volume * pan_right;

                // Route to master (track 0)
                state.track_buffers[0].left[frame] += left;
                state.track_buffers[0].right[frame] += right;
            }
        }

        // Apply master volume and pan
        let master_pan_left;
        let master_pan_right;
        {
            let master_vol = state.mixer_state.track_volumes[0];
            (master_pan_left, master_pan_right) = state.mixer_state.pan_gains(0);
            for frame in 0..num_frames {
                state.track_buffers[0].left[frame] *= master_vol * master_pan_left * master_volume;
                state.track_buffers[0].right[frame] *=
                    master_vol * master_pan_right * master_volume;
            }
        }

        // Step 5: Calculate peak levels for all tracks (before soft clip)
        let mut peak_levels = [StereoLevels::default(); NUM_TRACKS];
        for (track_idx, track_buf) in state.track_buffers.iter().enumerate().take(NUM_TRACKS) {
            let mut peak_left = 0.0f32;
            let mut peak_right = 0.0f32;
            for frame in 0..num_frames {
                peak_left = peak_left.max(track_buf.left[frame].abs());
                peak_right = peak_right.max(track_buf.right[frame].abs());
            }
            peak_levels[track_idx] = StereoLevels {
                left: peak_left.min(1.0),
                right: peak_right.min(1.0),
            };
        }

        // Update shared peak levels buffer
        if let Ok(mut shared_peaks) = state.peak_levels.try_lock() {
            *shared_peaks = peak_levels;
        }

        // Step 6: Output master track to DAC
        for frame in 0..num_frames {
            let left = state.track_buffers[0].left[frame].clamp(-1.0, 1.0);
            let right = state.track_buffers[0].right[frame].clamp(-1.0, 1.0);

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

        if let Ok(mut waveform) = waveform_buffer.try_lock() {
            for frame in 0..num_frames {
                // Use master track output for waveform
                let sample = (state.track_buffers[0].left[frame]
                    + state.track_buffers[0].right[frame])
                    * 0.5;
                waveform[write_pos] = sample.clamp(-1.0, 1.0);
                write_pos = (write_pos + 1) % WAVEFORM_BUFFER_SIZE;
            }
        }

        state.waveform_write_pos = write_pos;
    }

    fn receive_plugins(state: &mut AudioState) {
        // Receive new plugin processors from main thread
        while let Ok((channel, processor, init_state)) = state.plugin_rx.try_recv() {
            // Ensure plugin_channels is large enough
            while state.plugin_channels.len() <= channel {
                state.plugin_channels.push(None);
            }

            // Convert initial params to pending params
            let pending_params: Vec<PluginParamEvent> = init_state
                .params
                .into_iter()
                .map(|(param_id, value)| PluginParamEvent { param_id, value })
                .collect();

            // Store the plugin processor with initial state
            state.plugin_channels[channel] = Some(PluginChannel {
                processor,
                pending_notes: Vec::new(),
                pending_params,
                output_left: Vec::new(),
                output_right: Vec::new(),
                volume: init_state.volume,
            });
        }
    }

    /// Process plugins and route their output to assigned mixer track buffers
    fn process_plugins_to_tracks(state: &mut AudioState, num_frames: usize) {
        use crate::plugin_host::MidiNote;

        if num_frames == 0 {
            return;
        }

        // Process each active plugin (channel index matches generator index)
        for (channel_idx, plugin_opt) in state.plugin_channels.iter_mut().enumerate() {
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
            let target_track = state
                .generator_tracks
                .get(channel_idx)
                .copied()
                .unwrap_or(1);

            // Route plugin output to target track buffer with per-channel volume
            let channel_volume = plugin_ch.volume;
            if target_track < NUM_TRACKS {
                for frame in 0..num_frames {
                    let left = plugin_ch.output_left[frame] * channel_volume;
                    let right = plugin_ch.output_right[frame] * channel_volume;
                    state.track_buffers[target_track].left[frame] += left;
                    state.track_buffers[target_track].right[frame] += right;
                }
            }
        }
    }

    /// Process insert effects on each track buffer
    fn process_track_effects(state: &mut AudioState, num_frames: usize) {
        if num_frames == 0 {
            return;
        }

        // We need to process effects for each track, but we can't borrow
        // track_effects and track_buffers at the same time through state.
        // Solution: process one track at a time using indices.
        for track_idx in 0..NUM_TRACKS {
            // Process each effect slot in order (serial chain)
            for slot_idx in 0..EFFECT_SLOTS {
                // Skip if bypassed
                if state.effect_bypassed[track_idx][slot_idx] {
                    continue;
                }

                // Take the effect temporarily to avoid borrow issues
                if let Some(mut effect) = state.track_effects[track_idx][slot_idx].take() {
                    // Process the track buffer in-place
                    let buf = &mut state.track_buffers[track_idx];
                    effect.process(&mut buf.left[..num_frames], &mut buf.right[..num_frames]);
                    // Put the effect back
                    state.track_effects[track_idx][slot_idx] = Some(effect);
                }
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
                    state.voices.retain(|v| !v.is_preview);
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
                    state.voices.retain(|v| !v.is_preview);
                }
                AudioCommand::StopAll => {
                    state.voices.clear();
                }
                AudioCommand::SetMasterVolume(vol) => {
                    state.master_volume = vol.clamp(0.0, 1.0);
                }
                AudioCommand::PreloadSample { path } => {
                    Self::load_sample(state, &path);
                }
                AudioCommand::PluginNoteOn {
                    channel,
                    note,
                    velocity,
                } => {
                    // Add note on event to plugin channel's pending notes
                    if let Some(Some(plugin_ch)) = state.plugin_channels.get_mut(channel) {
                        plugin_ch.pending_notes.push(PluginNoteEvent {
                            note,
                            velocity,
                            is_note_on: true,
                        });
                    }
                }
                AudioCommand::PluginNoteOff { channel, note } => {
                    // Add note off event to plugin channel's pending notes
                    if let Some(Some(plugin_ch)) = state.plugin_channels.get_mut(channel) {
                        plugin_ch.pending_notes.push(PluginNoteEvent {
                            note,
                            velocity: 0.0,
                            is_note_on: false,
                        });
                    }
                }
                AudioCommand::PluginSetParam {
                    channel,
                    param_id,
                    value,
                } => {
                    // Add param event to plugin channel's pending params
                    if let Some(Some(plugin_ch)) = state.plugin_channels.get_mut(channel) {
                        plugin_ch
                            .pending_params
                            .push(PluginParamEvent { param_id, value });
                    }
                }
                AudioCommand::PluginSetVolume { channel, volume } => {
                    // Set plugin channel volume
                    if let Some(Some(plugin_ch)) = state.plugin_channels.get_mut(channel) {
                        plugin_ch.volume = volume.clamp(0.0, 1.0);
                    }
                }
                AudioCommand::UpdateMixerState(mixer_state) => {
                    state.mixer_state = mixer_state;
                }
                AudioCommand::SetGeneratorTrack { generator, track } => {
                    if generator < MAX_GENERATORS && track < NUM_TRACKS {
                        state.generator_tracks[generator] = track;
                    }
                }
                AudioCommand::SetEffect {
                    track,
                    slot,
                    effect_type,
                } => {
                    if track < NUM_TRACKS && slot < EFFECT_SLOTS {
                        let sample_rate = state.output_sample_rate as f32;
                        let bpm = state.tempo_bpm;
                        state.track_effects[track][slot] = effect_type.map(|et| {
                            // Create a temporary slot with default params
                            let slot_data = EffectSlot::new(et);
                            create_effect(&slot_data, sample_rate, bpm)
                        });
                        state.effect_bypassed[track][slot] = false;
                    }
                }
                AudioCommand::SetEffectParam {
                    track,
                    slot,
                    param_id,
                    value,
                } => {
                    if track < NUM_TRACKS && slot < EFFECT_SLOTS {
                        if let Some(effect) = &mut state.track_effects[track][slot] {
                            effect.set_param(param_id, value);
                        }
                    }
                }
                AudioCommand::SetEffectEnabled {
                    track,
                    slot,
                    enabled,
                } => {
                    if track < NUM_TRACKS && slot < EFFECT_SLOTS {
                        state.effect_bypassed[track][slot] = !enabled;
                    }
                }
                AudioCommand::UpdateTempo(bpm) => {
                    state.tempo_bpm = bpm;
                    // Update all tempo-synced effects
                    for track in &mut state.track_effects {
                        for effect in track.iter_mut().flatten() {
                            effect.set_tempo(bpm);
                        }
                    }
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

        // Get cached sample and create voice
        if let Some(sample) = state.sample_cache.get(path).cloned() {
            // Limit voice count
            if state.voices.len() >= MAX_VOICES {
                // Remove oldest non-preview voice
                if let Some(idx) = state.voices.iter().position(|v| !v.is_preview) {
                    state.voices.remove(idx);
                }
            }

            state.voices.push(Voice {
                sample,
                position: 0,
                volume,
                is_preview,
                generator_idx,
                route_to_master,
            });
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
