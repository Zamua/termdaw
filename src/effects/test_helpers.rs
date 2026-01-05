//! Test helpers for audio effects
//!
//! Provides utilities for testing Effect implementations without requiring
//! the full audio engine infrastructure.

use super::{create_effect, get_param_defs, Effect, EffectSlot, EffectType};

/// Default sample rate for tests (44.1kHz)
pub const TEST_SAMPLE_RATE: f32 = 44100.0;

/// Default BPM for tests
pub const TEST_BPM: f64 = 120.0;

/// Generate a test signal (sine wave at given frequency)
pub fn generate_sine(samples: usize, frequency: f32, sample_rate: f32) -> Vec<f32> {
    (0..samples)
        .map(|i| {
            let t = i as f32 / sample_rate;
            (2.0 * std::f32::consts::PI * frequency * t).sin()
        })
        .collect()
}

/// Generate a DC signal (constant value)
pub fn generate_dc(samples: usize, value: f32) -> Vec<f32> {
    vec![value; samples]
}

/// Generate an impulse signal (1.0 at sample 0, 0.0 elsewhere)
pub fn generate_impulse(samples: usize) -> Vec<f32> {
    let mut signal = vec![0.0; samples];
    if !signal.is_empty() {
        signal[0] = 1.0;
    }
    signal
}

/// Generate white noise (random samples between -1.0 and 1.0)
pub fn generate_noise(samples: usize, seed: u64) -> Vec<f32> {
    // Simple PRNG for reproducible tests (xorshift)
    let mut state = seed;
    (0..samples)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state as f32 / u64::MAX as f32) * 2.0 - 1.0
        })
        .collect()
}

/// Calculate RMS (root mean square) of a signal
pub fn calculate_rms(signal: &[f32]) -> f32 {
    if signal.is_empty() {
        return 0.0;
    }
    let sum_of_squares: f32 = signal.iter().map(|&x| x * x).sum();
    (sum_of_squares / signal.len() as f32).sqrt()
}

/// Calculate peak amplitude of a signal
pub fn calculate_peak(signal: &[f32]) -> f32 {
    signal.iter().map(|&x| x.abs()).fold(0.0f32, f32::max)
}

/// Test that an effect passes through silence unchanged
pub fn assert_effect_passes_silence<E: Effect + ?Sized>(effect: &mut E, samples: usize) {
    let mut left = vec![0.0; samples];
    let mut right = vec![0.0; samples];
    effect.process(&mut left, &mut right);

    let peak = calculate_peak(&left).max(calculate_peak(&right));
    assert!(
        peak < 1e-6,
        "Effect should pass silence unchanged, but peak is {}",
        peak
    );
}

/// Test that effect parameters are within valid ranges
pub fn assert_params_in_range(effect: &dyn Effect) {
    let effect_type = effect.effect_type();
    for def in get_param_defs(effect_type) {
        let value = effect.get_param(def.id);
        assert!(
            value >= def.min && value <= def.max,
            "Param {:?} value {} out of range [{}, {}]",
            def.id,
            value,
            def.min,
            def.max
        );
    }
}

/// Test that reset clears effect state
pub fn assert_reset_clears_state<E: Effect + ?Sized>(effect: &mut E) {
    // Process some audio to build up state
    let mut left = generate_sine(1024, 440.0, TEST_SAMPLE_RATE);
    let mut right = generate_sine(1024, 440.0, TEST_SAMPLE_RATE);
    effect.process(&mut left, &mut right);

    // Reset
    effect.reset();

    // Process silence - should be silent if state is cleared
    let mut left = vec![0.0; 256];
    let mut right = vec![0.0; 256];
    effect.process(&mut left, &mut right);

    // Allow small epsilon for numerical precision
    let peak = calculate_peak(&left).max(calculate_peak(&right));
    assert!(
        peak < 0.001,
        "Effect should produce silence after reset, but peak is {}",
        peak
    );
}

/// Create an effect from type with default params
pub fn create_test_effect(effect_type: EffectType) -> Box<dyn Effect> {
    let slot = EffectSlot::new(effect_type);
    create_effect(&slot, TEST_SAMPLE_RATE, TEST_BPM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_sine() {
        let signal = generate_sine(44100, 1.0, 44100.0);
        assert_eq!(signal.len(), 44100);
        // After exactly 1 second at 1Hz, should be back to ~0
        assert!((signal[0]).abs() < 0.001);
        // Quarter way through should be at peak
        assert!((signal[11025] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_generate_impulse() {
        let signal = generate_impulse(10);
        assert_eq!(signal[0], 1.0);
        assert!(signal[1..].iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_calculate_rms() {
        // DC signal should have RMS equal to its value
        let dc = generate_dc(100, 0.5);
        assert!((calculate_rms(&dc) - 0.5).abs() < 0.001);

        // Sine wave RMS should be amplitude / sqrt(2)
        let sine = generate_sine(44100, 100.0, 44100.0);
        let expected_rms = 1.0 / std::f32::consts::SQRT_2;
        assert!((calculate_rms(&sine) - expected_rms).abs() < 0.01);
    }

    #[test]
    fn test_filter_default_params() {
        let effect = create_test_effect(EffectType::Filter);
        assert_params_in_range(effect.as_ref());
    }

    #[test]
    fn test_delay_default_params() {
        let effect = create_test_effect(EffectType::Delay);
        assert_params_in_range(effect.as_ref());
    }

    #[test]
    fn test_filter_passes_silence() {
        let mut effect = create_test_effect(EffectType::Filter);
        assert_effect_passes_silence(effect.as_mut(), 256);
    }

    #[test]
    fn test_filter_reset() {
        let mut effect = create_test_effect(EffectType::Filter);
        assert_reset_clears_state(effect.as_mut());
    }
}
