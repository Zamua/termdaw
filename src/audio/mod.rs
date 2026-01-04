//! Audio engine for sample playback and plugin hosting
//!
//! Uses cpal for low-level audio with support for:
//! - Polyphonic sample playback with pre-loaded buffers
//! - Real-time mixing in audio callback
//! - Plugin hosting via CLAP
//! - Master and per-channel volume control

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

use crate::plugin_host::{ActivePluginProcessor, ParamChange};

/// Maximum number of simultaneous sample playbacks
const MAX_VOICES: usize = 32;

/// Number of samples to keep for waveform visualization
const WAVEFORM_BUFFER_SIZE: usize = 512;

/// Shared waveform buffer for visualization
pub type WaveformBuffer = Arc<Mutex<Vec<f32>>>;

/// Commands sent to the audio engine
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AudioCommand {
    /// Play a sample (polyphonic - can overlap)
    PlaySample { path: PathBuf, volume: f32 },
    /// Preview a sample (exclusive - stops previous preview)
    PreviewSample { path: PathBuf },
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
}

/// Handle for sending commands to the audio engine
#[derive(Clone)]
pub struct AudioHandle {
    tx: Sender<AudioCommand>,
    plugin_tx: Sender<(usize, ActivePluginProcessor, PluginInitState)>,
    sample_rate: u32,
    /// Shared waveform buffer for visualization
    waveform_buffer: WaveformBuffer,
}

#[allow(dead_code)]
impl AudioHandle {
    /// Play a sample at the given volume (polyphonic)
    pub fn play_sample(&self, path: &Path, volume: f32) {
        let _ = self.tx.send(AudioCommand::PlaySample {
            path: path.to_path_buf(),
            volume,
        });
    }

    /// Preview a sample (exclusive - stops previous preview)
    pub fn preview_sample(&self, path: &Path) {
        let _ = self.tx.send(AudioCommand::PreviewSample {
            path: path.to_path_buf(),
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

        let master_volume = state.master_volume;
        let output_sample_rate = state.output_sample_rate;

        // Clear output buffer
        for sample in data.iter_mut() {
            *sample = T::EQUILIBRIUM;
        }

        // Mix all active voices
        let num_frames = data.len() / channels;

        // We need to iterate in a way that allows removal
        let mut i = 0;
        while i < state.voices.len() {
            let voice = &mut state.voices[i];
            let sample_data = &voice.sample.data;
            let voice_channels = voice.sample.channels as usize;
            let voice_volume = voice.volume * master_volume;

            // Simple resampling ratio (crude but works for now)
            let resample_ratio = voice.sample.sample_rate as f32 / output_sample_rate as f32;

            let mut finished = false;

            for frame in 0..num_frames {
                // Calculate source frame position with resampling
                // voice.position counts output frames, multiply by ratio to get source frame
                let src_frame = (voice.position as f32 * resample_ratio) as usize;

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

                // Mix into output (assuming stereo output)
                let out_idx = frame * channels;
                if channels >= 2 {
                    let current_left: f32 = data[out_idx].to_sample();
                    let current_right: f32 = data[out_idx + 1].to_sample();
                    data[out_idx] = T::from_sample(current_left + left);
                    data[out_idx + 1] = T::from_sample(current_right + right);
                } else if channels == 1 {
                    let current: f32 = data[out_idx].to_sample();
                    data[out_idx] = T::from_sample(current + (left + right) * 0.5);
                }

                voice.position += 1;
            }

            if finished {
                state.voices.remove(i);
            } else {
                i += 1;
            }
        }

        // Receive any new plugin processors from main thread
        Self::receive_plugins(&mut state);

        // Process plugins and mix their output
        Self::process_plugins(&mut state, data, channels, master_volume);

        // Soft clip to prevent harsh distortion
        for sample in data.iter_mut() {
            let s: f32 = sample.to_sample();
            *sample = T::from_sample(s.clamp(-1.0, 1.0));
        }

        // Write samples to waveform buffer for visualization
        // We only need mono for the waveform, so take left channel (or mix to mono)
        let waveform_buffer = state.waveform_buffer.clone();
        let mut write_pos = state.waveform_write_pos;

        if let Ok(mut waveform) = waveform_buffer.try_lock() {
            for frame in 0..num_frames {
                let out_idx = frame * channels;
                let sample: f32 = if channels >= 2 {
                    // Mix stereo to mono
                    let left: f32 = data[out_idx].to_sample();
                    let right: f32 = data[out_idx + 1].to_sample();
                    (left + right) * 0.5
                } else {
                    data[out_idx].to_sample()
                };

                waveform[write_pos] = sample;
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

    fn process_plugins<T: cpal::SizedSample + cpal::FromSample<f32> + cpal::Sample>(
        state: &mut AudioState,
        data: &mut [T],
        channels: usize,
        master_volume: f32,
    ) where
        f32: cpal::FromSample<T>,
    {
        use crate::plugin_host::MidiNote;

        let num_frames = data.len() / channels;
        if num_frames == 0 {
            return;
        }

        // Process each active plugin
        for plugin_ch in state.plugin_channels.iter_mut().flatten() {
            // Ensure output buffers are large enough
            if plugin_ch.output_left.len() < num_frames {
                plugin_ch.output_left.resize(num_frames, 0.0);
                plugin_ch.output_right.resize(num_frames, 0.0);
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

            // Mix plugin output into the main output with per-channel volume
            let channel_volume = plugin_ch.volume;
            for frame in 0..num_frames {
                let out_idx = frame * channels;
                let left = plugin_ch.output_left[frame] * channel_volume * master_volume;
                let right = plugin_ch.output_right[frame] * channel_volume * master_volume;

                if channels >= 2 {
                    let current_left: f32 = data[out_idx].to_sample();
                    let current_right: f32 = data[out_idx + 1].to_sample();
                    data[out_idx] = T::from_sample(current_left + left);
                    data[out_idx + 1] = T::from_sample(current_right + right);
                } else if channels == 1 {
                    let current: f32 = data[out_idx].to_sample();
                    data[out_idx] = T::from_sample(current + (left + right) * 0.5);
                }
            }
        }
    }

    fn process_commands_internal(state: &mut AudioState) {
        while let Ok(cmd) = state.rx.try_recv() {
            match cmd {
                AudioCommand::PlaySample { path, volume } => {
                    Self::play_sample_internal(state, &path, volume, false);
                }
                AudioCommand::PreviewSample { path } => {
                    // Stop existing preview
                    state.voices.retain(|v| !v.is_preview);
                    Self::play_sample_internal(state, &path, 1.0, true);
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
            }
        }
    }

    fn play_sample_internal(state: &mut AudioState, path: &Path, volume: f32, is_preview: bool) {
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
