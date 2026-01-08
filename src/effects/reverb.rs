//! Simple reverb effect
//!
//! A Freeverb-style reverb with room size, damping, and wet/dry mix controls.

use crate::effects::{Effect, EffectParamId, EffectType};

/// Comb filter delay times in samples (at 44100 Hz)
/// These are carefully chosen prime-ish numbers to avoid resonance
const COMB_DELAYS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];

/// Allpass filter delay times in samples (at 44100 Hz)
const ALLPASS_DELAYS: [usize; 4] = [556, 441, 341, 225];

/// Comb filter with lowpass damping
struct CombFilter {
    buffer: Vec<f32>,
    write_pos: usize,
    filterstore: f32,
    feedback: f32,
    damp1: f32,
    damp2: f32,
}

impl CombFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            write_pos: 0,
            filterstore: 0.0,
            feedback: 0.5,
            damp1: 0.5,
            damp2: 0.5,
        }
    }

    fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback;
    }

    fn set_damp(&mut self, damp: f32) {
        self.damp1 = damp;
        self.damp2 = 1.0 - damp;
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.write_pos];

        // Lowpass filter in feedback loop (damping)
        self.filterstore = output * self.damp2 + self.filterstore * self.damp1;

        // Write input + filtered feedback
        self.buffer[self.write_pos] = input + self.filterstore * self.feedback;

        self.write_pos = (self.write_pos + 1) % self.buffer.len();

        output
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.filterstore = 0.0;
    }
}

/// Allpass filter for diffusion
struct AllpassFilter {
    buffer: Vec<f32>,
    write_pos: usize,
}

impl AllpassFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            write_pos: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let bufout = self.buffer[self.write_pos];
        let output = -input + bufout;

        self.buffer[self.write_pos] = input + bufout * 0.5;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();

        output
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
    }
}

/// Freeverb-style reverb effect
pub struct ReverbEffect {
    /// Sample rate in Hz
    sample_rate: f32,
    /// Room size (affects decay time)
    room_size: f32,
    /// Damping (high frequency absorption)
    damping: f32,
    /// Wet/dry mix (0.0 = dry, 1.0 = wet)
    mix: f32,
    /// Left channel comb filters
    combs_l: Vec<CombFilter>,
    /// Right channel comb filters
    combs_r: Vec<CombFilter>,
    /// Left channel allpass filters
    allpasses_l: Vec<AllpassFilter>,
    /// Right channel allpass filters
    allpasses_r: Vec<AllpassFilter>,
}

impl ReverbEffect {
    /// Create a new reverb effect
    pub fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / 44100.0;

        // Create comb filters with scaled delays
        let combs_l: Vec<CombFilter> = COMB_DELAYS
            .iter()
            .map(|&d| CombFilter::new(((d as f32) * scale) as usize))
            .collect();

        // Right channel has slightly different delays for stereo spread
        let combs_r: Vec<CombFilter> = COMB_DELAYS
            .iter()
            .map(|&d| CombFilter::new(((d as f32 + 23.0) * scale) as usize))
            .collect();

        // Create allpass filters
        let allpasses_l: Vec<AllpassFilter> = ALLPASS_DELAYS
            .iter()
            .map(|&d| AllpassFilter::new(((d as f32) * scale) as usize))
            .collect();

        let allpasses_r: Vec<AllpassFilter> = ALLPASS_DELAYS
            .iter()
            .map(|&d| AllpassFilter::new(((d as f32 + 23.0) * scale) as usize))
            .collect();

        let mut effect = Self {
            sample_rate,
            room_size: 0.8,
            damping: 0.1,
            mix: 0.05,
            combs_l,
            combs_r,
            allpasses_l,
            allpasses_r,
        };

        effect.update_coefficients();
        effect
    }

    /// Update comb filter coefficients based on room size and damping
    fn update_coefficients(&mut self) {
        // Room size affects feedback (decay time)
        let feedback = 0.28 + self.room_size * 0.7;

        for comb in &mut self.combs_l {
            comb.set_feedback(feedback);
            comb.set_damp(self.damping);
        }
        for comb in &mut self.combs_r {
            comb.set_feedback(feedback);
            comb.set_damp(self.damping);
        }
    }
}

impl Effect for ReverbEffect {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let input = (*l + *r) * 0.5; // Mix to mono for reverb input

            // Sum parallel comb filters
            let mut out_l = 0.0f32;
            let mut out_r = 0.0f32;

            for comb in &mut self.combs_l {
                out_l += comb.process(input);
            }
            for comb in &mut self.combs_r {
                out_r += comb.process(input);
            }

            // Series allpass filters for diffusion
            for allpass in &mut self.allpasses_l {
                out_l = allpass.process(out_l);
            }
            for allpass in &mut self.allpasses_r {
                out_r = allpass.process(out_r);
            }

            // Scale output (8 comb filters summed)
            out_l *= 0.125;
            out_r *= 0.125;

            // Mix dry and wet
            *l = *l * (1.0 - self.mix) + out_l * self.mix;
            *r = *r * (1.0 - self.mix) + out_r * self.mix;
        }
    }

    fn set_param(&mut self, id: EffectParamId, value: f32) {
        match id {
            EffectParamId::ReverbRoomSize => {
                self.room_size = value.clamp(0.0, 1.0);
                self.update_coefficients();
            }
            EffectParamId::ReverbDamping => {
                self.damping = value.clamp(0.0, 1.0);
                self.update_coefficients();
            }
            EffectParamId::ReverbMix => {
                self.mix = value.clamp(0.0, 1.0);
            }
            _ => {} // Ignore non-reverb parameters
        }
    }

    fn get_param(&self, id: EffectParamId) -> f32 {
        match id {
            EffectParamId::ReverbRoomSize => self.room_size,
            EffectParamId::ReverbDamping => self.damping,
            EffectParamId::ReverbMix => self.mix,
            _ => 0.0,
        }
    }

    fn reset(&mut self) {
        for comb in &mut self.combs_l {
            comb.clear();
        }
        for comb in &mut self.combs_r {
            comb.clear();
        }
        for allpass in &mut self.allpasses_l {
            allpass.clear();
        }
        for allpass in &mut self.allpasses_r {
            allpass.clear();
        }
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        // Recreate all filters with new sample rate
        *self = Self::new(sample_rate);
    }

    fn set_tempo(&mut self, _bpm: f64) {
        // Reverb doesn't use tempo
    }

    fn effect_type(&self) -> EffectType {
        EffectType::Reverb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverb_produces_finite_output() {
        let mut reverb = ReverbEffect::new(44100.0);
        let mut left = vec![0.5; 1024];
        let mut right = vec![0.5; 1024];

        reverb.process(&mut left, &mut right);

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
    fn reverb_output_bounded_at_extreme_settings() {
        let mut reverb = ReverbEffect::new(44100.0);
        reverb.set_param(EffectParamId::ReverbRoomSize, 1.0);
        reverb.set_param(EffectParamId::ReverbDamping, 0.0);
        reverb.set_param(EffectParamId::ReverbMix, 1.0);

        let mut left = vec![1.0; 2048];
        let mut right = vec![1.0; 2048];
        reverb.process(&mut left, &mut right);

        let max_val = left
            .iter()
            .chain(right.iter())
            .map(|x| x.abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_val < 10.0,
            "Output exceeds reasonable bounds: {}",
            max_val
        );
    }

    #[test]
    fn reverb_params_clamp_correctly() {
        let mut reverb = ReverbEffect::new(44100.0);

        reverb.set_param(EffectParamId::ReverbRoomSize, 2.0);
        assert_eq!(reverb.get_param(EffectParamId::ReverbRoomSize), 1.0);

        reverb.set_param(EffectParamId::ReverbDamping, -1.0);
        assert_eq!(reverb.get_param(EffectParamId::ReverbDamping), 0.0);

        reverb.set_param(EffectParamId::ReverbMix, 1.5);
        assert_eq!(reverb.get_param(EffectParamId::ReverbMix), 1.0);
    }
}
