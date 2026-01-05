//! State Variable Filter (SVF) effect
//!
//! A resonant filter with low-pass, high-pass, and band-pass modes.
//! Uses the SVF topology for stability at high resonance.

use crate::effects::{Effect, EffectParamId, EffectType};

/// Filter mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    LowPass = 0,
    HighPass = 1,
    BandPass = 2,
}

impl From<f32> for FilterMode {
    fn from(value: f32) -> Self {
        match value as u32 {
            0 => FilterMode::LowPass,
            1 => FilterMode::HighPass,
            _ => FilterMode::BandPass,
        }
    }
}

/// Per-channel filter state
#[derive(Debug, Clone, Default)]
struct FilterState {
    /// Low-pass output
    low: f32,
    /// Band-pass output
    band: f32,
}

/// State Variable Filter effect
pub struct FilterEffect {
    /// Sample rate in Hz
    sample_rate: f32,
    /// Cutoff frequency in Hz
    cutoff: f32,
    /// Resonance (0.0 - 1.0)
    resonance: f32,
    /// Filter mode
    mode: FilterMode,
    /// Left channel state
    state_l: FilterState,
    /// Right channel state
    state_r: FilterState,
    /// Precomputed coefficient: 2 * sin(pi * cutoff / sample_rate)
    g: f32,
    /// Precomputed coefficient: 1 / Q
    k: f32,
}

impl FilterEffect {
    /// Create a new filter effect
    pub fn new(sample_rate: f32) -> Self {
        let mut effect = Self {
            sample_rate,
            cutoff: 1000.0,
            resonance: 0.5,
            mode: FilterMode::LowPass,
            state_l: FilterState::default(),
            state_r: FilterState::default(),
            g: 0.0,
            k: 0.0,
        };
        effect.update_coefficients();
        effect
    }

    /// Update filter coefficients after parameter change
    fn update_coefficients(&mut self) {
        // Clamp cutoff to valid range
        let cutoff = self.cutoff.clamp(20.0, self.sample_rate * 0.49);

        // g = tan(pi * fc / fs) approximated as 2 * sin(pi * fc / fs) for stability
        // Using the approximation for the digital SVF
        let omega = std::f32::consts::PI * cutoff / self.sample_rate;
        self.g = omega.tan();

        // k = 1/Q, where Q ranges from 0.5 (no resonance) to ~20 (high resonance)
        // Map resonance 0-1 to Q 0.5-20
        let q = 0.5 + self.resonance * 19.5;
        self.k = 1.0 / q;
    }

    /// Process a single sample through the SVF (standalone function to avoid borrow issues)
    #[inline]
    fn process_sample(
        input: f32,
        state: &mut FilterState,
        g: f32,
        k: f32,
        mode: FilterMode,
    ) -> f32 {
        // SVF equations (Chamberlin topology, optimized)
        let high = (input - k * state.band - state.low) / (1.0 + k * g + g * g);
        let band = g * high + state.band;
        let low = g * band + state.low;

        // Update state
        state.band = band + g * high;
        state.low = low + g * band;

        // Select output based on mode
        match mode {
            FilterMode::LowPass => low,
            FilterMode::HighPass => high,
            FilterMode::BandPass => band,
        }
    }
}

impl Effect for FilterEffect {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let g = self.g;
        let k = self.k;
        let mode = self.mode;

        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            *l = Self::process_sample(*l, &mut self.state_l, g, k, mode);
            *r = Self::process_sample(*r, &mut self.state_r, g, k, mode);
        }
    }

    fn set_param(&mut self, id: EffectParamId, value: f32) {
        match id {
            EffectParamId::FilterCutoff => {
                self.cutoff = value.clamp(20.0, 20000.0);
                self.update_coefficients();
            }
            EffectParamId::FilterResonance => {
                self.resonance = value.clamp(0.0, 1.0);
                self.update_coefficients();
            }
            EffectParamId::FilterMode => {
                self.mode = FilterMode::from(value);
            }
            _ => {} // Ignore non-filter parameters
        }
    }

    fn get_param(&self, id: EffectParamId) -> f32 {
        match id {
            EffectParamId::FilterCutoff => self.cutoff,
            EffectParamId::FilterResonance => self.resonance,
            EffectParamId::FilterMode => self.mode as u32 as f32,
            _ => 0.0,
        }
    }

    fn reset(&mut self) {
        self.state_l = FilterState::default();
        self.state_r = FilterState::default();
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.update_coefficients();
    }

    fn set_tempo(&mut self, _bpm: f64) {
        // Filter doesn't use tempo
    }

    fn effect_type(&self) -> EffectType {
        EffectType::Filter
    }
}
