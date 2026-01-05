//! Built-in audio effects for mixer insert slots
//!
//! Each mixer track has 8 effect slots. Effects process audio in-place
//! after generators write to track buffers, before volume/pan is applied.

// Allow dead code during initial implementation - will be used by audio thread
#![allow(dead_code)]

pub mod delay;
pub mod filter;
pub mod test_helpers;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Number of effect slots per mixer track
pub const EFFECT_SLOTS: usize = 8;

/// Effect types available in the DAW
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectType {
    /// Resonant state-variable filter (LP/HP/BP)
    Filter,
    /// Tempo-synced delay with feedback
    Delay,
}

impl EffectType {
    /// Get display name for the effect type
    pub fn name(&self) -> &'static str {
        match self {
            EffectType::Filter => "Filter",
            EffectType::Delay => "Delay",
        }
    }

    /// Get all available effect types
    pub fn all() -> &'static [EffectType] {
        &[EffectType::Filter, EffectType::Delay]
    }
}

/// Effect parameter identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectParamId {
    // Filter parameters
    FilterCutoff,
    FilterResonance,
    FilterMode,

    // Delay parameters
    DelayTime,
    DelayFeedback,
    DelayMix,
    DelaySync,
    DelayFreeMs,
}

impl EffectParamId {
    /// Get display name for the parameter
    pub fn name(&self) -> &'static str {
        match self {
            EffectParamId::FilterCutoff => "Cutoff",
            EffectParamId::FilterResonance => "Resonance",
            EffectParamId::FilterMode => "Mode",
            EffectParamId::DelayTime => "Time",
            EffectParamId::DelayFeedback => "Feedback",
            EffectParamId::DelayMix => "Mix",
            EffectParamId::DelaySync => "Sync",
            EffectParamId::DelayFreeMs => "Free Time",
        }
    }
}

/// Display type for effect parameters
#[derive(Debug, Clone)]
pub enum ParamDisplay {
    /// Continuous value with unit (e.g., "Hz", "%", "ms")
    Continuous { unit: &'static str, decimals: u8 },
    /// Discrete choices (e.g., "LP", "HP", "BP")
    Discrete { choices: &'static [&'static str] },
}

/// Parameter definition for UI and normalization
#[derive(Debug, Clone)]
pub struct EffectParamDef {
    pub id: EffectParamId,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub display: ParamDisplay,
}

impl EffectParamDef {
    /// Normalize a value to 0.0-1.0 range
    pub fn normalize(&self, value: f32) -> f32 {
        (value - self.min) / (self.max - self.min)
    }

    /// Denormalize from 0.0-1.0 to actual range
    pub fn denormalize(&self, normalized: f32) -> f32 {
        self.min + normalized * (self.max - self.min)
    }

    /// Format value for display
    pub fn format_value(&self, value: f32) -> String {
        match &self.display {
            ParamDisplay::Continuous { unit, decimals } => {
                if *decimals == 0 {
                    format!("{:.0}{}", value, unit)
                } else {
                    format!("{:.1$}{2}", value, *decimals as usize, unit)
                }
            }
            ParamDisplay::Discrete { choices } => {
                let idx = (value as usize).min(choices.len() - 1);
                choices[idx].to_string()
            }
        }
    }
}

/// An effect slot in a mixer track (serializable state)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectSlot {
    /// Type of effect
    pub effect_type: EffectType,
    /// Whether the effect is bypassed
    pub bypassed: bool,
    /// Parameter values (param_id -> value)
    pub params: HashMap<EffectParamId, f32>,
}

impl EffectSlot {
    /// Create a new effect slot with default parameters
    pub fn new(effect_type: EffectType) -> Self {
        let params = get_default_params(effect_type);
        Self {
            effect_type,
            bypassed: false,
            params,
        }
    }

    /// Get a parameter value
    pub fn get_param(&self, id: EffectParamId) -> f32 {
        self.params.get(&id).copied().unwrap_or(0.0)
    }

    /// Set a parameter value
    pub fn set_param(&mut self, id: EffectParamId, value: f32) {
        self.params.insert(id, value);
    }
}

/// Get default parameter values for an effect type
pub fn get_default_params(effect_type: EffectType) -> HashMap<EffectParamId, f32> {
    let mut params = HashMap::new();
    for def in get_param_defs(effect_type) {
        params.insert(def.id, def.default);
    }
    params
}

/// Get parameter definitions for an effect type
pub fn get_param_defs(effect_type: EffectType) -> Vec<EffectParamDef> {
    match effect_type {
        EffectType::Filter => vec![
            EffectParamDef {
                id: EffectParamId::FilterCutoff,
                min: 20.0,
                max: 20000.0,
                default: 1000.0,
                display: ParamDisplay::Continuous {
                    unit: "Hz",
                    decimals: 0,
                },
            },
            EffectParamDef {
                id: EffectParamId::FilterResonance,
                min: 0.0,
                max: 1.0,
                default: 0.5,
                display: ParamDisplay::Continuous {
                    unit: "%",
                    decimals: 0,
                },
            },
            EffectParamDef {
                id: EffectParamId::FilterMode,
                min: 0.0,
                max: 2.0,
                default: 0.0,
                display: ParamDisplay::Discrete {
                    choices: &["LP", "HP", "BP"],
                },
            },
        ],
        EffectType::Delay => vec![
            EffectParamDef {
                id: EffectParamId::DelayTime,
                min: 0.0,
                max: 7.0,
                default: 3.0,
                display: ParamDisplay::Discrete {
                    choices: &["1/32", "1/16", "1/8", "1/4", "1/2", "1", "2", "4"],
                },
            },
            EffectParamDef {
                id: EffectParamId::DelayFeedback,
                min: 0.0,
                max: 0.95,
                default: 0.5,
                display: ParamDisplay::Continuous {
                    unit: "%",
                    decimals: 0,
                },
            },
            EffectParamDef {
                id: EffectParamId::DelayMix,
                min: 0.0,
                max: 1.0,
                default: 0.5,
                display: ParamDisplay::Continuous {
                    unit: "%",
                    decimals: 0,
                },
            },
            EffectParamDef {
                id: EffectParamId::DelaySync,
                min: 0.0,
                max: 1.0,
                default: 1.0,
                display: ParamDisplay::Discrete {
                    choices: &["Off", "On"],
                },
            },
            EffectParamDef {
                id: EffectParamId::DelayFreeMs,
                min: 10.0,
                max: 2000.0,
                default: 250.0,
                display: ParamDisplay::Continuous {
                    unit: "ms",
                    decimals: 0,
                },
            },
        ],
    }
}

/// Trait for audio effect processors (implemented by filter, delay, etc.)
///
/// Effects must be Send to work in the audio thread.
pub trait Effect: Send {
    /// Process stereo audio in-place
    fn process(&mut self, left: &mut [f32], right: &mut [f32]);

    /// Set a parameter value
    fn set_param(&mut self, id: EffectParamId, value: f32);

    /// Get current parameter value
    fn get_param(&self, id: EffectParamId) -> f32;

    /// Reset internal state (delay lines, filter coefficients)
    fn reset(&mut self);

    /// Set sample rate (called when audio engine initializes)
    fn set_sample_rate(&mut self, sample_rate: f32);

    /// Set tempo in BPM (for tempo-synced effects)
    fn set_tempo(&mut self, bpm: f64);

    /// Get the effect type
    fn effect_type(&self) -> EffectType;
}

/// Create a new effect processor from an EffectSlot
pub fn create_effect(slot: &EffectSlot, sample_rate: f32, bpm: f64) -> Box<dyn Effect> {
    match slot.effect_type {
        EffectType::Filter => {
            let mut effect = filter::FilterEffect::new(sample_rate);
            for (id, value) in &slot.params {
                effect.set_param(*id, *value);
            }
            Box::new(effect)
        }
        EffectType::Delay => {
            let mut effect = delay::DelayEffect::new(sample_rate, bpm);
            for (id, value) in &slot.params {
                effect.set_param(*id, *value);
            }
            Box::new(effect)
        }
    }
}
