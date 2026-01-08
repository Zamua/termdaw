//! Simple State Variable Filter (SVF) effect
//!
//! A clean filter with low-pass, high-pass, and band-pass modes.
//! No resonance - just smooth frequency cutoff.

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

/// Simple State Variable Filter effect (no resonance)
pub struct FilterEffect {
    /// Sample rate in Hz
    sample_rate: f32,
    /// Cutoff frequency in Hz
    cutoff: f32,
    /// Filter mode
    mode: FilterMode,
    /// Left channel state
    state_l: FilterState,
    /// Right channel state
    state_r: FilterState,
    /// Precomputed coefficient: 2 * sin(pi * cutoff / sample_rate)
    g: f32,
}

impl FilterEffect {
    /// Create a new filter effect
    pub fn new(sample_rate: f32) -> Self {
        let mut effect = Self {
            sample_rate,
            cutoff: 1000.0,
            mode: FilterMode::LowPass,
            state_l: FilterState::default(),
            state_r: FilterState::default(),
            g: 0.0,
        };
        effect.update_coefficients();
        effect
    }

    /// Update filter coefficients after parameter change
    fn update_coefficients(&mut self) {
        // Clamp cutoff to valid range (up to ~20kHz at 44.1kHz)
        let cutoff = self.cutoff.clamp(20.0, self.sample_rate * 0.45);

        // Simple SVF coefficient: g = 2 * sin(pi * fc / fs)
        // With no resonance (k=sqrt(2) for Butterworth response), filter is always stable
        let omega = std::f32::consts::PI * cutoff / self.sample_rate;
        self.g = (2.0 * omega.sin()).min(1.0);
    }

    /// Process a single sample through the SVF
    #[inline]
    fn process_sample(input: f32, state: &mut FilterState, g: f32, mode: FilterMode) -> f32 {
        // Check for corrupted state and reset if needed
        if !state.band.is_finite() {
            state.band = 0.0;
        }
        if !state.low.is_finite() {
            state.low = 0.0;
        }

        // Butterworth SVF (k = sqrt(2) ≈ 1.414 for flat passband)
        const K: f32 = 1.414;

        // Standard Chamberlin SVF
        let high = input - K * state.band - state.low;
        let band = state.band + g * high;
        let low = state.low + g * band;

        // Update state
        state.band = band;
        state.low = low;

        // Select output based on mode
        let output = match mode {
            FilterMode::LowPass => low,
            FilterMode::HighPass => high,
            FilterMode::BandPass => band,
        };

        if output.is_finite() {
            output
        } else {
            0.0
        }
    }
}

impl Effect for FilterEffect {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let g = self.g;
        let mode = self.mode;

        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            *l = Self::process_sample(*l, &mut self.state_l, g, mode);
            *r = Self::process_sample(*r, &mut self.state_r, g, mode);
        }
    }

    fn set_param(&mut self, id: EffectParamId, value: f32) {
        match id {
            EffectParamId::FilterCutoff => {
                self.cutoff = value.clamp(20.0, 20000.0);
                self.update_coefficients();
            }
            EffectParamId::FilterMode => {
                self.mode = FilterMode::from(value);
            }
            _ => {} // Ignore non-filter parameters (resonance removed)
        }
    }

    fn get_param(&self, id: EffectParamId) -> f32 {
        match id {
            EffectParamId::FilterCutoff => self.cutoff,
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
    fn filter_output_stays_bounded() {
        let mut filter = FilterEffect::new(44100.0);
        filter.set_param(EffectParamId::FilterCutoff, 16000.0);

        let mut left = vec![1.0; 1024];
        let mut right = vec![1.0; 1024];
        filter.process(&mut left, &mut right);

        let max_val = left
            .iter()
            .chain(right.iter())
            .map(|x| x.abs())
            .fold(0.0f32, f32::max);
        assert!(max_val <= 10.0, "Output exceeds bounds: {}", max_val);
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
    fn filter_works_at_all_cutoff_frequencies() {
        let cutoffs = [
            20.0, 100.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 15000.0, 20000.0,
        ];

        for &cutoff in &cutoffs {
            let mut filter = FilterEffect::new(44100.0);
            filter.set_param(EffectParamId::FilterCutoff, cutoff);

            // Test with 440Hz tone
            let mut left: Vec<f32> = (0..4410)
                .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
                .collect();
            let mut right = left.clone();

            filter.process(&mut left, &mut right);

            // Check for NaN/infinity
            assert!(
                left.iter().all(|x| x.is_finite()),
                "NaN/infinity at cutoff={}",
                cutoff
            );

            // Check output is not completely silent (except at very low cutoff)
            let rms: f32 = (left.iter().map(|x| x * x).sum::<f32>() / left.len() as f32).sqrt();
            if cutoff > 440.0 {
                assert!(
                    rms > 0.01,
                    "Silent output at cutoff={} (RMS: {})",
                    cutoff,
                    rms
                );
            }
        }
    }

    #[test]
    fn filter_lowpass_attenuates_high_frequencies() {
        let mut filter = FilterEffect::new(44100.0);
        filter.set_param(EffectParamId::FilterCutoff, 500.0);
        filter.set_param(EffectParamId::FilterMode, 0.0); // LowPass

        // Generate 5kHz tone (well above cutoff)
        let mut left: Vec<f32> = (0..4410)
            .map(|i| (2.0 * std::f32::consts::PI * 5000.0 * i as f32 / 44100.0).sin())
            .collect();
        let mut right = left.clone();

        let input_rms: f32 = (left.iter().map(|x| x * x).sum::<f32>() / left.len() as f32).sqrt();
        filter.process(&mut left, &mut right);
        let output_rms: f32 = (left.iter().map(|x| x * x).sum::<f32>() / left.len() as f32).sqrt();

        // Output should be significantly attenuated
        assert!(
            output_rms < input_rms * 0.2,
            "Lowpass didn't attenuate high frequency (in: {}, out: {})",
            input_rms,
            output_rms
        );
    }

    #[test]
    fn filter_highpass_attenuates_low_frequencies() {
        let mut filter = FilterEffect::new(44100.0);
        filter.set_param(EffectParamId::FilterCutoff, 5000.0);
        filter.set_param(EffectParamId::FilterMode, 1.0); // HighPass

        // Generate 200Hz tone (well below cutoff)
        let mut left: Vec<f32> = (0..4410)
            .map(|i| (2.0 * std::f32::consts::PI * 200.0 * i as f32 / 44100.0).sin())
            .collect();
        let mut right = left.clone();

        let input_rms: f32 = (left.iter().map(|x| x * x).sum::<f32>() / left.len() as f32).sqrt();
        filter.process(&mut left, &mut right);
        let output_rms: f32 = (left.iter().map(|x| x * x).sum::<f32>() / left.len() as f32).sqrt();

        // Output should be significantly attenuated
        assert!(
            output_rms < input_rms * 0.2,
            "Highpass didn't attenuate low frequency (in: {}, out: {})",
            input_rms,
            output_rms
        );
    }

    #[test]
    fn filter_output_is_smooth_during_cutoff_sweep() {
        let mut filter = FilterEffect::new(44100.0);
        let mut max_delta = 0.0f32;

        for (step, cutoff) in (500..5000).step_by(250).enumerate() {
            filter.set_param(EffectParamId::FilterCutoff, cutoff as f32);

            let mut left: Vec<f32> = (0..441)
                .map(|i| {
                    let sample_idx = step * 441 + i;
                    (2.0 * std::f32::consts::PI * 440.0 * sample_idx as f32 / 44100.0).sin()
                })
                .collect();
            let mut right = left.clone();

            filter.process(&mut left, &mut right);

            for i in 1..left.len() {
                let delta = (left[i] - left[i - 1]).abs();
                max_delta = max_delta.max(delta);
            }
        }

        assert!(
            max_delta < 1.0,
            "Filter output has discontinuities during cutoff sweep (max delta: {})",
            max_delta
        );
    }
}
