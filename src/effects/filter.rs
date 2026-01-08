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
        // Clamp cutoff to valid range
        let cutoff = self.cutoff.clamp(20.0, self.sample_rate * 0.45);

        // Chamberlin SVF: g = 2 * sin(pi * fc / fs)
        // CRITICAL: Chamberlin SVF is only stable when g < ~0.8 for all k values
        // At g >= 0.85 with high k (low resonance), eigenvalues exceed 1.0
        let omega = std::f32::consts::PI * cutoff / self.sample_rate;
        self.g = (2.0 * omega.sin()).min(0.75);

        // k = damping factor = 1/Q
        // k=2.0 is critically damped (no resonance), k→0 is high resonance
        // Range: 2.0 (0% resonance) to 0.1 (100% resonance)
        self.k = 2.0 * (1.0 - self.resonance * 0.95).max(0.05);
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

        // Standard Chamberlin SVF
        let high = input - k * state.band - state.low;
        let band = state.band + g * high;
        let low = state.low + g * band;

        // Update state (no clamping - filter is stable with g ≤ 1.5)
        state.band = band;
        state.low = low;

        // Select output based on mode
        let output = match mode {
            FilterMode::LowPass => low,
            FilterMode::HighPass => high,
            FilterMode::BandPass => band,
        };

        // Return 0.0 if output is NaN/infinity (shouldn't happen with stable filter)
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
                self.cutoff = value.clamp(20.0, 16000.0);
                self.update_coefficients();
            }
            EffectParamId::FilterResonance => {
                // UI sends 0-100%, convert to 0.0-1.0 internal
                self.resonance = (value / 100.0).clamp(0.0, 1.0);
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
            EffectParamId::FilterResonance => self.resonance * 100.0, // Return 0-100%
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
        filter.set_param(EffectParamId::FilterCutoff, 16000.0);
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
        filter.set_param(EffectParamId::FilterCutoff, 16000.0);
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
        filter.set_param(EffectParamId::FilterResonance, 95.0);

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

        // With high resonance, output should show significant gain (> 5.0)
        // Very low values indicate filter is not resonating properly
        assert!(
            max_val > 5.0,
            "Filter not resonating properly (max was {})",
            max_val
        );
    }

    #[test]
    fn filter_works_at_high_cutoff() {
        // Filter should work properly at high cutoff frequencies without going silent
        let mut filter = FilterEffect::new(44100.0);
        filter.set_param(EffectParamId::FilterCutoff, 16000.0);
        filter.set_param(EffectParamId::FilterResonance, 50.0);

        // Generate a high frequency tone that should pass through
        let mut left: Vec<f32> = (0..4410)
            .map(|i| (2.0 * std::f32::consts::PI * 8000.0 * i as f32 / 44100.0).sin())
            .collect();
        let mut right = left.clone();

        filter.process(&mut left, &mut right);

        // Output should have significant energy (not silent)
        let rms: f32 = (left.iter().map(|x| x * x).sum::<f32>() / left.len() as f32).sqrt();
        assert!(
            rms > 0.1,
            "Filter output is too quiet at 16kHz cutoff (RMS: {})",
            rms
        );

        // Output should be finite
        assert!(
            left.iter().all(|x| x.is_finite()),
            "Filter produced NaN/infinity at high cutoff"
        );
    }

    #[test]
    fn filter_works_at_all_parameter_combinations() {
        // Exhaustively test all combinations of cutoff and resonance
        // to ensure no combination causes NaN or glitches
        let cutoffs = [
            100.0, 500.0, 1000.0, 2000.0, 4000.0, 6000.0, 8000.0, 10000.0, 12000.0, 14000.0,
            16000.0,
        ];
        let resonances = [
            0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0,
        ];

        for &cutoff in &cutoffs {
            for &resonance in &resonances {
                let mut filter = FilterEffect::new(44100.0);
                filter.set_param(EffectParamId::FilterCutoff, cutoff);
                filter.set_param(EffectParamId::FilterResonance, resonance);

                // Test with signal at half the cutoff frequency (should pass through)
                let test_freq = (cutoff / 2.0).max(20.0).min(1000.0);
                let mut left: Vec<f32> = (0..4410)
                    .map(|i| (2.0 * std::f32::consts::PI * test_freq * i as f32 / 44100.0).sin())
                    .collect();
                let mut right = left.clone();

                filter.process(&mut left, &mut right);

                // Check for NaN/infinity
                assert!(
                    left.iter().all(|x| x.is_finite()),
                    "NaN/infinity at cutoff={}, resonance={}",
                    cutoff,
                    resonance
                );

                // Check output is not completely silent
                let rms: f32 = (left.iter().map(|x| x * x).sum::<f32>() / left.len() as f32).sqrt();
                assert!(
                    rms > 0.001,
                    "Silent output at cutoff={}, resonance={} (RMS: {})",
                    cutoff,
                    resonance,
                    rms
                );

                // Check for extreme discontinuities (max sample-to-sample delta)
                let max_delta: f32 = left
                    .windows(2)
                    .map(|w| (w[1] - w[0]).abs())
                    .fold(0.0, f32::max);
                assert!(
                    max_delta < 10.0,
                    "Large discontinuity at cutoff={}, resonance={} (max_delta: {})",
                    cutoff,
                    resonance,
                    max_delta
                );
            }
        }
    }

    #[test]
    fn filter_stable_during_parameter_sweeps() {
        // Test that sweeping parameters during playback doesn't cause glitches
        let mut filter = FilterEffect::new(44100.0);

        // Start in middle of range
        filter.set_param(EffectParamId::FilterCutoff, 5000.0);
        filter.set_param(EffectParamId::FilterResonance, 50.0);

        // Sweep cutoff while keeping resonance constant
        for cutoff in (100..16000).step_by(500) {
            filter.set_param(EffectParamId::FilterCutoff, cutoff as f32);

            let mut left: Vec<f32> = (0..441)
                .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
                .collect();
            let mut right = left.clone();

            filter.process(&mut left, &mut right);

            assert!(
                left.iter().all(|x| x.is_finite()),
                "NaN during cutoff sweep at {}",
                cutoff
            );
        }

        // Sweep resonance while keeping cutoff constant
        filter.set_param(EffectParamId::FilterCutoff, 12000.0);
        for resonance in (0..=100).step_by(5) {
            filter.set_param(EffectParamId::FilterResonance, resonance as f32);

            let mut left: Vec<f32> = (0..441)
                .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
                .collect();
            let mut right = left.clone();

            filter.process(&mut left, &mut right);

            assert!(
                left.iter().all(|x| x.is_finite()),
                "NaN during resonance sweep at {}",
                resonance
            );

            let rms: f32 = (left.iter().map(|x| x * x).sum::<f32>() / left.len() as f32).sqrt();
            assert!(
                rms > 0.001,
                "Silent during resonance sweep at {} (RMS: {})",
                resonance,
                rms
            );
        }
    }

    #[test]
    fn filter_works_at_zero_resonance() {
        // Filter should work at minimum resonance (0%)
        let mut filter = FilterEffect::new(44100.0);
        filter.set_param(EffectParamId::FilterCutoff, 1000.0);
        filter.set_param(EffectParamId::FilterResonance, 0.0);

        let mut left: Vec<f32> = (0..4410)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        let mut right = left.clone();

        filter.process(&mut left, &mut right);

        // Output should have significant energy (not silent)
        let rms: f32 = (left.iter().map(|x| x * x).sum::<f32>() / left.len() as f32).sqrt();
        assert!(rms > 0.1, "Filter is silent at 0% resonance (RMS: {})", rms);

        // Output should be finite
        assert!(
            left.iter().all(|x| x.is_finite()),
            "Filter produced NaN/infinity at 0% resonance"
        );
    }

    #[test]
    fn filter_stable_at_max_cutoff_with_resonance() {
        // Filter must remain stable at maximum cutoff with high resonance
        let mut filter = FilterEffect::new(44100.0);
        filter.set_param(EffectParamId::FilterCutoff, 16000.0);
        filter.set_param(EffectParamId::FilterResonance, 90.0);

        // Process multiple buffers to check for instability over time
        for _ in 0..10 {
            let mut left: Vec<f32> = (0..4410)
                .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 44100.0).sin())
                .collect();
            let mut right = left.clone();

            filter.process(&mut left, &mut right);

            // Check that output doesn't explode or go silent
            let rms: f32 = (left.iter().map(|x| x * x).sum::<f32>() / left.len() as f32).sqrt();
            assert!(
                rms > 0.001 && rms < 1000.0,
                "Filter became unstable at max cutoff (RMS: {})",
                rms
            );
            assert!(
                left.iter().all(|x| x.is_finite()),
                "Filter produced NaN/infinity"
            );
        }
    }

    #[test]
    fn filter_output_is_smooth_during_cutoff_sweep() {
        // Sweeping cutoff should produce smooth output without sudden jumps
        let mut filter = FilterEffect::new(44100.0);
        filter.set_param(EffectParamId::FilterResonance, 50.0);

        let mut max_delta = 0.0f32;

        // Process fresh input at each cutoff step (simulates real usage)
        for (step, cutoff) in (500..5000).step_by(250).enumerate() {
            filter.set_param(EffectParamId::FilterCutoff, cutoff as f32);

            // Generate fresh sine wave for each iteration
            let mut left: Vec<f32> = (0..441)
                .map(|i| {
                    let sample_idx = step * 441 + i;
                    (2.0 * std::f32::consts::PI * 440.0 * sample_idx as f32 / 44100.0).sin()
                })
                .collect();
            let mut right = left.clone();

            filter.process(&mut left, &mut right);

            // Check for discontinuities (sudden jumps between consecutive samples)
            for i in 1..left.len() {
                let delta = (left[i] - left[i - 1]).abs();
                max_delta = max_delta.max(delta);
            }
        }

        // Max sample-to-sample change should be reasonable (< 1.5 for smooth audio)
        // A 440 Hz sine at 44.1kHz has natural max delta ~0.06, filter resonance
        // and cutoff changes can increase this, but should stay bounded
        assert!(
            max_delta < 1.5,
            "Filter output has discontinuities during cutoff sweep (max delta: {})",
            max_delta
        );
    }
}
