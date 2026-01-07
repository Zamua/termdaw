//! State Variable Filter (SVF) effect
//!
//! A resonant filter with low-pass, high-pass, and band-pass modes.
//! Uses the SVF topology for stability at high resonance.

use crate::effects::{Effect, EffectParamId, EffectType};

/// Filter mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)] // LowPass/HighPass/BandPass are standard filter terminology
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
        // Clamp cutoff to valid range (0.4 * Nyquist for stability margin)
        let cutoff = self.cutoff.clamp(20.0, self.sample_rate * 0.4);

        // g = tan(pi * fc / fs)
        let omega = std::f32::consts::PI * cutoff / self.sample_rate;
        self.g = omega.tan();

        // Clamp g to prevent instability at high cutoff
        if !self.g.is_finite() || self.g > 10.0 {
            self.g = 10.0;
        }

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
        // Check for corrupted state and reset if needed
        if !state.band.is_finite() {
            state.band = 0.0;
        }
        if !state.low.is_finite() {
            state.low = 0.0;
        }

        // SVF equations (Chamberlin topology, optimized)
        let denom = 1.0 + k * g + g * g;
        if denom < 0.001 {
            return input; // Bypass filter if unstable
        }

        let high = (input - k * state.band - state.low) / denom;
        let band = g * high + state.band;
        let low = g * band + state.low;

        // Update state
        state.band = band + g * high;
        state.low = low + g * band;

        // Select output based on mode
        let output = match mode {
            FilterMode::LowPass => low,
            FilterMode::HighPass => high,
            FilterMode::BandPass => band,
        };

        // Soft limit at high threshold to catch infinity without audible clipping
        if output.is_finite() {
            output.clamp(-100.0, 100.0)
        } else {
            0.0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_produces_finite_output_at_max_cutoff() {
        let mut filter = FilterEffect::new(44100.0);
        filter.set_param(EffectParamId::FilterCutoff, 20000.0);
        filter.set_param(EffectParamId::FilterResonance, 1.0);

        let mut left = vec![0.5; 1024];
        let mut right = vec![0.5; 1024];
        filter.process(&mut left, &mut right);

        assert!(
            left.iter().all(|&x| x.is_finite()),
            "Left channel has NaN/infinity"
        );
        assert!(
            right.iter().all(|&x| x.is_finite()),
            "Right channel has NaN/infinity"
        );
    }

    #[test]
    fn filter_output_stays_bounded_at_extreme_settings() {
        let mut filter = FilterEffect::new(44100.0);
        filter.set_param(EffectParamId::FilterCutoff, 20000.0);
        filter.set_param(EffectParamId::FilterResonance, 1.0);

        let mut left = vec![1.0; 1024];
        let mut right = vec![1.0; 1024];
        filter.process(&mut left, &mut right);

        let max_val = left
            .iter()
            .chain(right.iter())
            .map(|x| x.abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_val <= 100.0,
            "Output exceeds safety bounds: {}",
            max_val
        );
    }

    #[test]
    fn filter_recovers_from_near_nyquist_cutoff() {
        let mut filter = FilterEffect::new(44100.0);

        // Push to extreme
        filter.set_param(EffectParamId::FilterCutoff, 22000.0);
        filter.set_param(EffectParamId::FilterResonance, 1.0);
        let mut left = vec![0.5; 256];
        let mut right = vec![0.5; 256];
        filter.process(&mut left, &mut right);

        // Return to normal
        filter.set_param(EffectParamId::FilterCutoff, 1000.0);
        let mut left = vec![0.5; 256];
        let mut right = vec![0.5; 256];
        filter.process(&mut left, &mut right);

        assert!(
            left.iter().all(|&x| x.is_finite()),
            "Filter state corrupted after extreme cutoff"
        );
    }

    #[test]
    fn filter_handles_rapid_cutoff_changes() {
        let mut filter = FilterEffect::new(44100.0);
        let mut left = vec![0.5; 64];
        let mut right = vec![0.5; 64];

        for cutoff in (20..20000).step_by(999) {
            filter.set_param(EffectParamId::FilterCutoff, cutoff as f32);
            filter.process(&mut left, &mut right);
        }

        assert!(
            left.iter().all(|&x| x.is_finite()),
            "Rapid changes caused NaN"
        );
    }

    #[test]
    fn filter_does_not_hard_clip_resonant_output() {
        // High resonance filter excited at cutoff frequency should produce
        // output that exceeds ±10.0 naturally - verify no hard clipping
        let mut filter = FilterEffect::new(44100.0);
        filter.set_param(EffectParamId::FilterCutoff, 1000.0);
        filter.set_param(EffectParamId::FilterResonance, 0.95);

        // Generate sine wave at cutoff frequency to excite resonance
        let num_samples = 4410; // 100ms at 44.1kHz
        let mut left: Vec<f32> = (0..num_samples)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 44100.0).sin())
            .collect();
        let mut right = left.clone();

        filter.process(&mut left, &mut right);

        let max_val = left
            .iter()
            .chain(right.iter())
            .map(|x| x.abs())
            .fold(0.0f32, f32::max);

        // With high resonance, output should exceed 10.0 naturally
        // If it's exactly 10.0, that indicates hard clipping
        assert!(
            max_val > 10.0,
            "Filter output appears hard-clipped at 10.0 (max was {})",
            max_val
        );
    }

    #[test]
    fn filter_output_is_smooth_during_cutoff_sweep() {
        // Sweeping cutoff should produce smooth output without sudden jumps
        let mut filter = FilterEffect::new(44100.0);
        filter.set_param(EffectParamId::FilterResonance, 0.5);

        // Generate a steady tone
        let mut left: Vec<f32> = (0..441)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        let mut right = left.clone();

        // Sweep cutoff from 200 to 2000 Hz while processing
        let mut max_delta = 0.0f32;
        for cutoff in (200..2000).step_by(100) {
            filter.set_param(EffectParamId::FilterCutoff, cutoff as f32);
            filter.process(&mut left, &mut right);

            // Check for discontinuities (sudden jumps between consecutive samples)
            for i in 1..left.len() {
                let delta = (left[i] - left[i - 1]).abs();
                max_delta = max_delta.max(delta);
            }
        }

        // Max sample-to-sample change should be reasonable (< 0.5 for smooth audio)
        // Large jumps indicate distortion or instability
        assert!(
            max_delta < 0.5,
            "Filter output has discontinuities during cutoff sweep (max delta: {})",
            max_delta
        );
    }
}
