//! Enhancer effect
//!
//! Adds warmth, presence, and punch through soft saturation and high-frequency excitation.

use crate::effects::{Effect, EffectParamId, EffectType};

/// One-pole lowpass filter for smoothing
struct OnePole {
    y1: f32,
    coef: f32,
}

impl OnePole {
    fn new(cutoff: f32, sample_rate: f32) -> Self {
        let coef = (-2.0 * std::f32::consts::PI * cutoff / sample_rate).exp();
        Self { y1: 0.0, coef }
    }

    fn process(&mut self, input: f32) -> f32 {
        self.y1 = input * (1.0 - self.coef) + self.y1 * self.coef;
        self.y1
    }

    fn set_cutoff(&mut self, cutoff: f32, sample_rate: f32) {
        self.coef = (-2.0 * std::f32::consts::PI * cutoff / sample_rate).exp();
    }
}

/// Enhancer effect
pub struct EnhancerEffect {
    /// Sample rate in Hz
    sample_rate: f32,
    /// Amount of effect (0.0 = clean, 1.0 = full effect)
    amount: f32,
    /// Effect mode (different character presets)
    mode: u32,
    /// Highpass filter for extracting highs (left)
    hp_l: OnePole,
    /// Highpass filter for extracting highs (right)
    hp_r: OnePole,
    /// Envelope follower for compression (left)
    env_l: OnePole,
    /// Envelope follower for compression (right)
    env_r: OnePole,
}

impl EnhancerEffect {
    /// Create a new enhancer effect
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            amount: 0.5,
            mode: 0,
            hp_l: OnePole::new(3000.0, sample_rate),
            hp_r: OnePole::new(3000.0, sample_rate),
            env_l: OnePole::new(50.0, sample_rate),
            env_r: OnePole::new(50.0, sample_rate),
        }
    }

    /// Soft saturation curve
    fn saturate(x: f32, drive: f32) -> f32 {
        let driven = x * (1.0 + drive * 3.0);
        // Soft clipping using tanh-like curve
        let abs_x = driven.abs();
        if abs_x < 1.0 {
            driven - driven.powi(3) / 3.0
        } else {
            driven.signum() * 2.0 / 3.0
        }
    }

    /// Get mode-specific parameters
    fn get_mode_params(&self) -> (f32, f32, f32, f32) {
        // Returns: (saturation_drive, exciter_freq, exciter_amount, compression)
        match self.mode {
            0 => (0.3, 3000.0, 0.15, 0.2), // Warm: subtle saturation
            1 => (0.5, 4000.0, 0.25, 0.3), // Bright: more presence
            2 => (0.7, 2500.0, 0.2, 0.5),  // Punch: more compression
            _ => (0.9, 5000.0, 0.35, 0.4), // Loud: aggressive
        }
    }
}

impl Effect for EnhancerEffect {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let (sat_drive, exciter_freq, exciter_amt, compression) = self.get_mode_params();

        // Update exciter frequency based on mode
        self.hp_l.set_cutoff(exciter_freq, self.sample_rate);
        self.hp_r.set_cutoff(exciter_freq, self.sample_rate);

        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let dry_l = *l;
            let dry_r = *r;

            // 1. Soft saturation for warmth
            let sat_l = Self::saturate(dry_l, sat_drive * self.amount);
            let sat_r = Self::saturate(dry_r, sat_drive * self.amount);

            // 2. High-frequency excitation
            // Extract highs, saturate them, add back
            let lp_l = self.hp_l.process(dry_l);
            let lp_r = self.hp_r.process(dry_r);
            let highs_l = dry_l - lp_l;
            let highs_r = dry_r - lp_r;

            // Saturate the highs for harmonic generation
            let excited_l = Self::saturate(highs_l * 2.0, 0.5) * exciter_amt * self.amount;
            let excited_r = Self::saturate(highs_r * 2.0, 0.5) * exciter_amt * self.amount;

            // 3. Subtle compression via envelope following
            let env_l = self.env_l.process(sat_l.abs());
            let env_r = self.env_r.process(sat_r.abs());

            // Soft knee compression
            let gain_l = 1.0 / (1.0 + env_l * compression * self.amount * 2.0);
            let gain_r = 1.0 / (1.0 + env_r * compression * self.amount * 2.0);

            // Combine: saturated signal + excitation, with compression
            let wet_l = (sat_l + excited_l) * gain_l;
            let wet_r = (sat_r + excited_r) * gain_r;

            // Mix dry and wet
            *l = dry_l * (1.0 - self.amount) + wet_l * self.amount;
            *r = dry_r * (1.0 - self.amount) + wet_r * self.amount;

            // Soft limit output
            *l = l.clamp(-1.0, 1.0);
            *r = r.clamp(-1.0, 1.0);
        }
    }

    fn set_param(&mut self, id: EffectParamId, value: f32) {
        match id {
            EffectParamId::EnhancerAmount => {
                self.amount = value.clamp(0.0, 1.0);
            }
            EffectParamId::EnhancerMode => {
                self.mode = (value as u32).min(3);
            }
            _ => {}
        }
    }

    fn get_param(&self, id: EffectParamId) -> f32 {
        match id {
            EffectParamId::EnhancerAmount => self.amount,
            EffectParamId::EnhancerMode => self.mode as f32,
            _ => 0.0,
        }
    }

    fn reset(&mut self) {
        self.hp_l.y1 = 0.0;
        self.hp_r.y1 = 0.0;
        self.env_l.y1 = 0.0;
        self.env_r.y1 = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.hp_l = OnePole::new(3000.0, sample_rate);
        self.hp_r = OnePole::new(3000.0, sample_rate);
        self.env_l = OnePole::new(50.0, sample_rate);
        self.env_r = OnePole::new(50.0, sample_rate);
    }

    fn set_tempo(&mut self, _bpm: f64) {
        // Enhancer doesn't use tempo
    }

    fn effect_type(&self) -> EffectType {
        EffectType::Enhancer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enhancer_produces_finite_output() {
        let mut fx = EnhancerEffect::new(44100.0);
        let mut left = vec![0.5; 1024];
        let mut right = vec![0.5; 1024];

        fx.process(&mut left, &mut right);

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
    fn enhancer_output_bounded() {
        let mut fx = EnhancerEffect::new(44100.0);
        fx.set_param(EffectParamId::EnhancerAmount, 1.0);
        fx.set_param(EffectParamId::EnhancerMode, 3.0);

        let mut left = vec![1.0; 2048];
        let mut right = vec![1.0; 2048];
        fx.process(&mut left, &mut right);

        let max_val = left
            .iter()
            .chain(right.iter())
            .map(|x| x.abs())
            .fold(0.0f32, f32::max);
        assert!(max_val <= 1.0, "Output exceeds bounds: {}", max_val);
    }

    #[test]
    fn enhancer_all_modes_work() {
        let mut fx = EnhancerEffect::new(44100.0);
        fx.set_param(EffectParamId::EnhancerAmount, 0.5);

        for mode in 0..4 {
            fx.set_param(EffectParamId::EnhancerMode, mode as f32);
            let mut left = vec![0.5; 512];
            let mut right = vec![0.5; 512];
            fx.process(&mut left, &mut right);

            assert!(
                left.iter().all(|&x| x.is_finite()),
                "Mode {} produced NaN",
                mode
            );
        }
    }
}
