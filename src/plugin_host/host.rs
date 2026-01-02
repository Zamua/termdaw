//! CLAP plugin host implementation using clack-host
//!
//! This module provides a CLAP plugin host that can load and process audio
//! through CLAP plugins. It wraps clack-host to provide a simpler API.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use clack_host::events::event_types::{NoteOffEvent, NoteOnEvent, ParamValueEvent};
use clack_host::factory::plugin::PluginFactory;
use clack_host::prelude::*;
use clack_host::process::{StartedPluginAudioProcessor, StoppedPluginAudioProcessor};
use clack_host::utils::Cookie;

use super::PluginInfo;

/// Shared host state (thread-safe)
#[derive(Default)]
struct DawHostShared {
    #[allow(dead_code)]
    restart_requested: AtomicBool,
    #[allow(dead_code)]
    process_requested: AtomicBool,
    #[allow(dead_code)]
    callback_requested: AtomicBool,
}

impl<'a> SharedHandler<'a> for DawHostShared {
    fn initializing(&self, _instance: InitializingPluginHandle<'a>) {
        // Query extensions here if needed
    }

    fn request_restart(&self) {
        self.restart_requested.store(true, Ordering::SeqCst);
    }

    fn request_process(&self) {
        self.process_requested.store(true, Ordering::SeqCst);
    }

    fn request_callback(&self) {
        self.callback_requested.store(true, Ordering::SeqCst);
    }
}

/// Main thread host handler
struct DawHostMainThread<'a> {
    #[allow(dead_code)]
    shared: &'a DawHostShared,
}

impl<'a> MainThreadHandler<'a> for DawHostMainThread<'a> {
    fn initialized(&mut self, _instance: InitializedPluginHandle<'a>) {
        // Plugin is initialized
    }
}

/// Audio processor handler
struct DawAudioProcessor;

impl<'a> AudioProcessorHandler<'a> for DawAudioProcessor {}

/// Combined host handlers
struct DawHost;

impl HostHandlers for DawHost {
    type Shared<'a> = DawHostShared;
    type MainThread<'a> = DawHostMainThread<'a>;
    type AudioProcessor<'a> = DawAudioProcessor;
}

/// MIDI note event to send to a plugin
#[derive(Debug, Clone, Copy)]
pub struct MidiNote {
    pub note: u8,
    pub velocity: f32,
    pub is_note_on: bool,
}

/// Parameter change event to send to a plugin
#[derive(Debug, Clone, Copy)]
pub struct ParamChange {
    pub param_id: u32,
    pub value: f64,
}

/// A loaded and activated plugin that can process audio.
/// This struct owns everything needed to process audio through a CLAP plugin.
pub struct PluginHost {
    /// The plugin bundle (must stay alive while plugin is active)
    bundle: PluginBundle,
    /// Plugin instance
    instance: PluginInstance<DawHost>,
    /// Plugin info
    info: PluginInfo,
    /// Sample rate
    sample_rate: f64,
    /// Buffer size
    buffer_size: u32,
    /// Whether the plugin is activated
    activated: bool,
}

impl PluginHost {
    /// Load and create a plugin from a path
    pub fn load(path: &Path, sample_rate: f64, buffer_size: u32) -> Result<Self, String> {
        // Load the plugin bundle
        let bundle = unsafe { PluginBundle::load(path) }
            .map_err(|e| format!("Failed to load plugin bundle: {:?}", e))?;

        // Get the plugin factory
        let factory: PluginFactory = bundle
            .get_factory()
            .ok_or_else(|| "No plugin factory found".to_string())?;

        // Get the first plugin descriptor
        let descriptor = factory
            .plugin_descriptors()
            .next()
            .ok_or_else(|| "No plugins in bundle".to_string())?;

        let plugin_id = descriptor
            .id()
            .map(|s: &std::ffi::CStr| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let plugin_name = descriptor
            .name()
            .map(|s: &std::ffi::CStr| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let plugin_vendor = descriptor
            .vendor()
            .map(|s: &std::ffi::CStr| s.to_string_lossy().to_string())
            .unwrap_or_default();

        // Create host info
        let host_info = HostInfo::new(
            "TermDAW",
            "TermDAW Project",
            "https://github.com/termdaw",
            "0.1.0",
        )
        .map_err(|e| format!("Failed to create host info: {:?}", e))?;

        // Create the plugin instance
        let instance = PluginInstance::<DawHost>::new(
            |_| DawHostShared::default(),
            |shared| DawHostMainThread { shared },
            &bundle,
            descriptor.id().ok_or("No plugin ID")?,
            &host_info,
        )
        .map_err(|e| format!("Failed to create plugin instance: {:?}", e))?;

        let info = PluginInfo {
            id: plugin_id,
            name: plugin_name,
            vendor: plugin_vendor,
            params: Vec::new(),
        };

        Ok(Self {
            bundle,
            instance,
            info,
            sample_rate,
            buffer_size,
            activated: false,
        })
    }

    /// Get the plugin info
    pub fn info(&self) -> &PluginInfo {
        &self.info
    }

    /// Activate the plugin and return a processor for the audio thread.
    /// The processor must be used on the audio thread and returned via deactivate().
    pub fn activate(&mut self) -> Result<ActivePluginProcessor, String> {
        if self.activated {
            return Err("Plugin already activated".to_string());
        }

        let audio_config = PluginAudioConfiguration {
            sample_rate: self.sample_rate,
            min_frames_count: 1,
            max_frames_count: 8192, // Allow any reasonable buffer size
        };

        let activated = self
            .instance
            .activate(|_, _| DawAudioProcessor, audio_config)
            .map_err(|e| format!("Failed to activate plugin: {:?}", e))?;

        let started = activated
            .start_processing()
            .map_err(|e| format!("Failed to start processing: {:?}", e))?;

        self.activated = true;

        Ok(ActivePluginProcessor::new(
            started,
            self.buffer_size as usize,
        ))
    }

    /// Deactivate the plugin after audio processing is done.
    pub fn deactivate(&mut self, processor: ActivePluginProcessor) {
        if self.activated {
            let stopped = processor.stop();
            self.instance.deactivate(stopped);
            self.activated = false;
        }
    }
}

/// An active plugin processor that can process audio.
/// Must be used on the audio thread.
pub struct ActivePluginProcessor {
    processor: StartedPluginAudioProcessor<DawHost>,
    /// Pre-allocated input audio buffers (stereo)
    input_buffers: [Vec<f32>; 2],
    /// Pre-allocated output audio buffers (stereo)
    output_buffers: [Vec<f32>; 2],
    /// Audio ports for input
    input_ports: AudioPorts,
    /// Audio ports for output
    output_ports: AudioPorts,
    /// Note on events buffer
    note_on_events: Vec<NoteOnEvent>,
    /// Note off events buffer
    note_off_events: Vec<NoteOffEvent>,
    /// Parameter change events buffer
    param_events: Vec<ParamValueEvent>,
    /// Steady time counter (in frames)
    steady_time: u64,
}

impl ActivePluginProcessor {
    fn new(processor: StartedPluginAudioProcessor<DawHost>, buffer_size: usize) -> Self {
        Self {
            processor,
            input_buffers: [vec![0.0; buffer_size], vec![0.0; buffer_size]],
            output_buffers: [vec![0.0; buffer_size], vec![0.0; buffer_size]],
            input_ports: AudioPorts::with_capacity(2, 1),
            output_ports: AudioPorts::with_capacity(2, 1),
            note_on_events: Vec::new(),
            note_off_events: Vec::new(),
            param_events: Vec::new(),
            steady_time: 0,
        }
    }

    /// Process audio through the plugin.
    /// Takes MIDI notes, parameter changes, and returns stereo audio output.
    pub fn process(
        &mut self,
        notes: &[MidiNote],
        params: &[ParamChange],
        output_left: &mut [f32],
        output_right: &mut [f32],
    ) {
        let frame_count = output_left.len().min(output_right.len());

        // Resize buffers if needed
        if self.output_buffers[0].len() < frame_count {
            self.output_buffers[0].resize(frame_count, 0.0);
            self.output_buffers[1].resize(frame_count, 0.0);
            self.input_buffers[0].resize(frame_count, 0.0);
            self.input_buffers[1].resize(frame_count, 0.0);
        }

        // Clear output buffers
        self.output_buffers[0][..frame_count].fill(0.0);
        self.output_buffers[1][..frame_count].fill(0.0);

        // Build input events from MIDI notes
        self.note_on_events.clear();
        self.note_off_events.clear();
        self.param_events.clear();

        for note in notes {
            if note.is_note_on {
                // Pckn: Port, Channel, Key (MIDI note), NoteID
                let event = NoteOnEvent::new(
                    0, // time offset 0
                    Pckn::new(0u16, 0u16, note.note as u16, note.note as u32),
                    note.velocity as f64,
                );
                self.note_on_events.push(event);
            } else {
                let event = NoteOffEvent::new(
                    0, // time offset 0
                    Pckn::new(0u16, 0u16, note.note as u16, note.note as u32),
                    0.0,
                );
                self.note_off_events.push(event);
            }
        }

        // Build parameter change events
        for param in params {
            let event = ParamValueEvent::new(
                0, // time offset 0
                ClapId::new(param.param_id),
                Pckn::match_all(),
                param.value,
                Cookie::empty(),
            );
            self.param_events.push(event);
        }

        // Set up audio buffers
        let input_audio = self.input_ports.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only(
                self.input_buffers
                    .iter_mut()
                    .map(|b| InputChannel::constant(&mut b[..frame_count])),
            ),
        }]);

        let mut output_audio = self.output_ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(
                self.output_buffers
                    .iter_mut()
                    .map(|b| &mut b[..frame_count]),
            ),
        }]);

        // Set up events - combine all events into a single buffer
        let mut input_event_buffer = EventBuffer::new();
        for event in &self.note_on_events {
            input_event_buffer.push(event);
        }
        for event in &self.note_off_events {
            input_event_buffer.push(event);
        }
        for event in &self.param_events {
            input_event_buffer.push(event);
        }
        let input_events = InputEvents::from_buffer(&input_event_buffer);
        let mut output_event_buffer = EventBuffer::new();
        let mut output_events = OutputEvents::from_buffer(&mut output_event_buffer);

        // Process audio
        let _status = self.processor.process(
            &input_audio,
            &mut output_audio,
            &input_events,
            &mut output_events,
            Some(self.steady_time),
            None,
        );

        // Copy output to provided buffers
        output_left[..frame_count].copy_from_slice(&self.output_buffers[0][..frame_count]);
        output_right[..frame_count].copy_from_slice(&self.output_buffers[1][..frame_count]);

        self.steady_time += frame_count as u64;
    }

    /// Stop processing and return the processor for deactivation
    fn stop(self) -> StoppedPluginAudioProcessor<DawHost> {
        self.processor.stop_processing()
    }
}
