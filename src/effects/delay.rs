//! Tempo-synced stereo delay effect
//!
//! A delay with tempo sync, feedback, and dry/wet mix control.

use crate::effects::{Effect, EffectParamId, EffectType};

/// Maximum delay time in seconds (4 bars at 60 BPM = 16 seconds)
const MAX_DELAY_SECONDS: f32 = 16.0;

/// Delay time divisions (in beats)
const DELAY_DIVISIONS: [f32; 8] = [
    0.125, // 1/32
    0.25,  // 1/16
    0.5,   // 1/8
    1.0,   // 1/4
    2.0,   // 1/2
    4.0,   // 1 bar
    8.0,   // 2 bars
    16.0,  // 4 bars
];

/// Circular buffer for delay line
struct DelayLine {
    buffer: Vec<f32>,
    write_pos: usize,
    size: usize,
}

impl DelayLine {
    fn new(max_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; max_samples],
            write_pos: 0,
            size: max_samples,
        }
    }

    fn write(&mut self, sample: f32) {
        self.buffer[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % self.size;
    }

    fn read(&self, delay_samples: usize) -> f32 {
        let delay = delay_samples.min(self.size - 1);
        let read_pos = (self.write_pos + self.size - delay) % self.size;
        self.buffer[read_pos]
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

/// Tempo-synced delay effect
pub struct DelayEffect {
    /// Sample rate in Hz
    sample_rate: f32,
    /// Tempo in BPM
    bpm: f64,
    /// Delay time index (0-7, maps to DELAY_DIVISIONS)
    delay_time_idx: usize,
    /// Feedback amount (0.0 - 0.95)
    feedback: f32,
    /// Dry/wet mix (0.0 = dry, 1.0 = wet)
    mix: f32,
    /// Tempo sync enabled
    sync: bool,
    /// Free delay time in ms (when sync is off)
    free_ms: f32,
    /// Left channel delay line
    delay_l: DelayLine,
    /// Right channel delay line
    delay_r: DelayLine,
    /// Current delay in samples
    delay_samples: usize,
}

impl DelayEffect {
    /// Create a new delay effect
    pub fn new(sample_rate: f32, bpm: f64) -> Self {
        let max_samples = (MAX_DELAY_SECONDS * sample_rate) as usize;
        let mut effect = Self {
            sample_rate,
            bpm,
            delay_time_idx: 3, // 1/4 note default
            feedback: 0.5,
            mix: 0.5,
            sync: true,
            free_ms: 250.0,
            delay_l: DelayLine::new(max_samples),
            delay_r: DelayLine::new(max_samples),
            delay_samples: 0,
        };
        effect.update_delay_samples();
        effect
    }

    /// Update delay time in samples based on current settings
    fn update_delay_samples(&mut self) {
        if self.sync {
            // Tempo-synced: delay_beats * samples_per_beat
            let beats = DELAY_DIVISIONS[self.delay_time_idx];
            let samples_per_beat = (60.0 / self.bpm) * self.sample_rate as f64;
            self.delay_samples = (beats as f64 * samples_per_beat) as usize;
        } else {
            // Free time in ms
            self.delay_samples = ((self.free_ms / 1000.0) * self.sample_rate) as usize;
        }
        // Clamp to buffer size
        let max_samples = (MAX_DELAY_SECONDS * self.sample_rate) as usize;
        self.delay_samples = self.delay_samples.min(max_samples - 1);
    }
}

impl Effect for DelayEffect {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            // Read delayed samples
            let delayed_l = self.delay_l.read(self.delay_samples);
            let delayed_r = self.delay_r.read(self.delay_samples);

            // Write input + feedback to delay line
            self.delay_l.write(*l + delayed_l * self.feedback);
            self.delay_r.write(*r + delayed_r * self.feedback);

            // Mix dry and wet signals
            *l = *l * (1.0 - self.mix) + delayed_l * self.mix;
            *r = *r * (1.0 - self.mix) + delayed_r * self.mix;
        }
    }

    fn set_param(&mut self, id: EffectParamId, value: f32) {
        match id {
            EffectParamId::DelayTime => {
                self.delay_time_idx = (value as usize).min(DELAY_DIVISIONS.len() - 1);
                self.update_delay_samples();
            }
            EffectParamId::DelayFeedback => {
                self.feedback = value.clamp(0.0, 0.95);
            }
            EffectParamId::DelayMix => {
                self.mix = value.clamp(0.0, 1.0);
            }
            EffectParamId::DelaySync => {
                self.sync = value >= 0.5;
                self.update_delay_samples();
            }
            EffectParamId::DelayFreeMs => {
                self.free_ms = value.clamp(10.0, 2000.0);
                self.update_delay_samples();
            }
            _ => {} // Ignore non-delay parameters
        }
    }

    fn get_param(&self, id: EffectParamId) -> f32 {
        match id {
            EffectParamId::DelayTime => self.delay_time_idx as f32,
            EffectParamId::DelayFeedback => self.feedback,
            EffectParamId::DelayMix => self.mix,
            EffectParamId::DelaySync => {
                if self.sync {
                    1.0
                } else {
                    0.0
                }
            }
            EffectParamId::DelayFreeMs => self.free_ms,
            _ => 0.0,
        }
    }

    fn reset(&mut self) {
        self.delay_l.clear();
        self.delay_r.clear();
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        // Recreate delay lines with new sample rate
        let max_samples = (MAX_DELAY_SECONDS * sample_rate) as usize;
        self.delay_l = DelayLine::new(max_samples);
        self.delay_r = DelayLine::new(max_samples);
        self.sample_rate = sample_rate;
        self.update_delay_samples();
    }

    fn set_tempo(&mut self, bpm: f64) {
        self.bpm = bpm;
        self.update_delay_samples();
    }

    fn effect_type(&self) -> EffectType {
        EffectType::Delay
    }
}
