//! Simple polyphonic synthesizer plugin
//!
//! Features:
//! - 4 waveforms: Sine, Square, Saw, Triangle
//! - ADSR envelope
//! - 8-voice polyphony

use nih_plug::prelude::*;
use std::f32::consts::PI;
use std::sync::Arc;

/// Maximum number of simultaneous voices
const MAX_VOICES: usize = 8;

/// Waveform types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum Waveform {
    #[name = "Sine"]
    Sine,
    #[name = "Square"]
    Square,
    #[name = "Saw"]
    Saw,
    #[name = "Triangle"]
    Triangle,
}

/// Envelope stage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvelopeStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// A single voice in the synth
#[derive(Debug, Clone)]
struct Voice {
    /// MIDI note number (0-127)
    note: u8,
    /// Note velocity (0.0-1.0)
    velocity: f32,
    /// Oscillator phase (0.0-1.0)
    phase: f32,
    /// Current envelope stage
    envelope_stage: EnvelopeStage,
    /// Current envelope level (0.0-1.0)
    envelope_level: f32,
    /// Whether this voice is active
    active: bool,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            note: 0,
            velocity: 0.0,
            phase: 0.0,
            envelope_stage: EnvelopeStage::Idle,
            envelope_level: 0.0,
            active: false,
        }
    }
}

impl Voice {
    /// Get the frequency for this voice's note
    fn frequency(&self) -> f32 {
        // A4 = 440 Hz = MIDI note 69
        440.0 * 2.0_f32.powf((self.note as f32 - 69.0) / 12.0)
    }

    /// Generate a sample for the given waveform
    fn oscillator(&self, waveform: Waveform) -> f32 {
        match waveform {
            Waveform::Sine => (self.phase * 2.0 * PI).sin(),
            Waveform::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Saw => 2.0 * self.phase - 1.0,
            Waveform::Triangle => {
                if self.phase < 0.5 {
                    4.0 * self.phase - 1.0
                } else {
                    3.0 - 4.0 * self.phase
                }
            }
        }
    }

    /// Advance the oscillator phase
    fn advance_phase(&mut self, frequency: f32, sample_rate: f32) {
        self.phase += frequency / sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
    }

    /// Process the envelope and return the current level
    fn process_envelope(
        &mut self,
        attack_samples: f32,
        decay_samples: f32,
        sustain_level: f32,
        release_samples: f32,
    ) -> f32 {
        match self.envelope_stage {
            EnvelopeStage::Idle => 0.0,
            EnvelopeStage::Attack => {
                self.envelope_level += 1.0 / attack_samples;
                if self.envelope_level >= 1.0 {
                    self.envelope_level = 1.0;
                    self.envelope_stage = EnvelopeStage::Decay;
                }
                self.envelope_level
            }
            EnvelopeStage::Decay => {
                self.envelope_level -= (1.0 - sustain_level) / decay_samples;
                if self.envelope_level <= sustain_level {
                    self.envelope_level = sustain_level;
                    self.envelope_stage = EnvelopeStage::Sustain;
                }
                self.envelope_level
            }
            EnvelopeStage::Sustain => sustain_level,
            EnvelopeStage::Release => {
                self.envelope_level -= self.envelope_level / release_samples;
                if self.envelope_level <= 0.001 {
                    self.envelope_level = 0.0;
                    self.envelope_stage = EnvelopeStage::Idle;
                    self.active = false;
                }
                self.envelope_level
            }
        }
    }

    /// Trigger note on
    fn note_on(&mut self, note: u8, velocity: f32) {
        self.note = note;
        self.velocity = velocity;
        self.phase = 0.0;
        self.envelope_stage = EnvelopeStage::Attack;
        self.envelope_level = 0.0;
        self.active = true;
    }

    /// Trigger note off (start release)
    fn note_off(&mut self) {
        if self.active && self.envelope_stage != EnvelopeStage::Release {
            self.envelope_stage = EnvelopeStage::Release;
        }
    }
}

/// Plugin parameters
#[derive(Params)]
struct SynthParams {
    /// Waveform selection
    #[id = "wave"]
    waveform: EnumParam<Waveform>,

    /// Attack time in milliseconds
    #[id = "atk"]
    attack_ms: FloatParam,

    /// Decay time in milliseconds
    #[id = "dec"]
    decay_ms: FloatParam,

    /// Sustain level (0.0-1.0)
    #[id = "sus"]
    sustain: FloatParam,

    /// Release time in milliseconds
    #[id = "rel"]
    release_ms: FloatParam,

    /// Master gain
    #[id = "gain"]
    gain: FloatParam,
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            waveform: EnumParam::new("Waveform", Waveform::Saw),

            attack_ms: FloatParam::new(
                "Attack",
                10.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 5000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            decay_ms: FloatParam::new(
                "Decay",
                100.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 5000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            sustain: FloatParam::new("Sustain", 0.7, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            release_ms: FloatParam::new(
                "Release",
                200.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 5000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(-6.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-36.0),
                    max: util::db_to_gain(6.0),
                    factor: FloatRange::gain_skew_factor(-36.0, 6.0),
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

/// The main synthesizer plugin
pub struct SimpleSynth {
    params: Arc<SynthParams>,
    sample_rate: f32,
    voices: [Voice; MAX_VOICES],
}

impl Default for SimpleSynth {
    fn default() -> Self {
        Self {
            params: Arc::new(SynthParams::default()),
            sample_rate: 44100.0,
            voices: std::array::from_fn(|_| Voice::default()),
        }
    }
}

impl SimpleSynth {
    /// Find a free voice or steal the oldest one
    fn allocate_voice(&mut self) -> &mut Voice {
        // First, try to find an inactive voice
        let mut best_idx = None;
        let mut best_level = f32::MAX;

        for (idx, voice) in self.voices.iter().enumerate() {
            if !voice.active {
                // Found an inactive voice, use it immediately
                best_idx = Some(idx);
                break;
            }
            // Track the best voice to steal (lowest level in release stage)
            if voice.envelope_stage == EnvelopeStage::Release && voice.envelope_level < best_level {
                best_level = voice.envelope_level;
                best_idx = Some(idx);
            }
        }

        // Use the best found voice, or steal voice 0 as fallback
        let idx = best_idx.unwrap_or(0);
        &mut self.voices[idx]
    }

    /// Find a voice playing a specific note
    fn find_voice_for_note(&mut self, note: u8) -> Option<&mut Voice> {
        self.voices
            .iter_mut()
            .find(|v| v.active && v.note == note && v.envelope_stage != EnvelopeStage::Release)
    }
}

impl Plugin for SimpleSynth {
    const NAME: &'static str = "Simple Synth";
    const VENDOR: &'static str = "TermDAW";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        true
    }

    fn reset(&mut self) {
        // Reset all voices
        for voice in &mut self.voices {
            *voice = Voice::default();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let waveform = self.params.waveform.value();
        let attack_samples = self.params.attack_ms.value() * self.sample_rate / 1000.0;
        let decay_samples = self.params.decay_ms.value() * self.sample_rate / 1000.0;
        let sustain_level = self.params.sustain.value();
        let release_samples = self.params.release_ms.value() * self.sample_rate / 1000.0;
        let gain = self.params.gain.value();

        for (sample_idx, channel_samples) in buffer.iter_samples().enumerate() {
            // Process MIDI events for this sample
            while let Some(event) = context.next_event() {
                // Only process events at or before this sample
                if event.timing() > sample_idx as u32 {
                    break;
                }

                match event {
                    NoteEvent::NoteOn { note, velocity, .. } => {
                        let voice = self.allocate_voice();
                        voice.note_on(note, velocity);
                    }
                    NoteEvent::NoteOff { note, .. } => {
                        if let Some(voice) = self.find_voice_for_note(note) {
                            voice.note_off();
                        }
                    }
                    _ => {}
                }
            }

            // Generate audio from all active voices
            let mut output = 0.0;
            for voice in &mut self.voices {
                if voice.active {
                    let freq = voice.frequency();
                    let osc = voice.oscillator(waveform);
                    let env = voice.process_envelope(
                        attack_samples,
                        decay_samples,
                        sustain_level,
                        release_samples,
                    );

                    output += osc * env * voice.velocity;
                    voice.advance_phase(freq, self.sample_rate);
                }
            }

            // Apply gain and write to all channels (stereo)
            let sample = output * gain;
            for channel_sample in channel_samples {
                *channel_sample = sample;
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for SimpleSynth {
    const CLAP_ID: &'static str = "com.termdaw.simple-synth";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A simple polyphonic synthesizer with ADSR envelope");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for SimpleSynth {
    const VST3_CLASS_ID: [u8; 16] = *b"TDawSimpleSynth!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
        Vst3SubCategory::Stereo,
    ];
}

nih_export_clap!(SimpleSynth);
nih_export_vst3!(SimpleSynth);
