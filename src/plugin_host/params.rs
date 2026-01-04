//! Plugin parameter definitions - single source of truth.
//!
//! All plugin parameter metadata lives here. Other modules should
//! reference these definitions rather than hardcoding values.

// Allow dead code - many items are defined for API completeness
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Typed plugin parameter identifiers for the SimpleSynth plugin.
/// Using an enum prevents typos and allows compile-time checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginParamId {
    Attack,
    Decay,
    Sustain,
    Release,
    Gain,
    Waveform,
}

impl PluginParamId {
    pub const ALL: &'static [Self] = &[
        Self::Attack,
        Self::Decay,
        Self::Sustain,
        Self::Release,
        Self::Gain,
        Self::Waveform,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Attack => "Attack",
            Self::Decay => "Decay",
            Self::Sustain => "Sustain",
            Self::Release => "Release",
            Self::Gain => "Gain",
            Self::Waveform => "Waveform",
        }
    }

    /// Get the parameter definition for this ID
    pub fn def(&self) -> &'static ParamDef {
        ParamDef::get(*self).expect("all PluginParamIds have definitions")
    }
}

/// How to display the parameter value
#[derive(Debug, Clone, Copy)]
pub enum ParamDisplay {
    /// Continuous value with unit suffix (e.g., "100 ms")
    Continuous { unit: &'static str, decimals: u8 },
    /// Discrete choices (e.g., waveform selector)
    Discrete { choices: &'static [&'static str] },
}

/// Complete parameter definition
#[derive(Debug, Clone)]
pub struct ParamDef {
    pub id: PluginParamId,
    pub clap_id: u32,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub display: ParamDisplay,
}

/// Parameter definitions for the built-in SimpleSynth plugin.
/// These CLAP IDs are nih-plug Rabin fingerprint hashes.
pub static SIMPLE_SYNTH_PARAMS: &[ParamDef] = &[
    ParamDef {
        id: PluginParamId::Attack,
        clap_id: 96920,
        min: 1.0,
        max: 5000.0,
        default: 10.0,
        display: ParamDisplay::Continuous {
            unit: "ms",
            decimals: 0,
        },
    },
    ParamDef {
        id: PluginParamId::Decay,
        clap_id: 99330,
        min: 1.0,
        max: 5000.0,
        default: 100.0,
        display: ParamDisplay::Continuous {
            unit: "ms",
            decimals: 0,
        },
    },
    ParamDef {
        id: PluginParamId::Sustain,
        clap_id: 114257,
        min: 0.0,
        max: 1.0,
        default: 0.7,
        display: ParamDisplay::Continuous {
            unit: "",
            decimals: 2,
        },
    },
    ParamDef {
        id: PluginParamId::Release,
        clap_id: 112793,
        min: 1.0,
        max: 5000.0,
        default: 200.0,
        display: ParamDisplay::Continuous {
            unit: "ms",
            decimals: 0,
        },
    },
    ParamDef {
        id: PluginParamId::Gain,
        clap_id: 3165055,
        min: 0.0,
        max: 1.0,
        default: 0.5,
        display: ParamDisplay::Continuous {
            unit: "",
            decimals: 2,
        },
    },
    ParamDef {
        id: PluginParamId::Waveform,
        clap_id: 3642105,
        min: 0.0,
        max: 3.0,
        default: 2.0,
        display: ParamDisplay::Discrete {
            choices: &["Sine", "Triangle", "Saw", "Square"],
        },
    },
];

impl ParamDef {
    /// Get parameter definition by ID
    pub fn get(id: PluginParamId) -> Option<&'static ParamDef> {
        SIMPLE_SYNTH_PARAMS.iter().find(|p| p.id == id)
    }

    /// Get parameter definition by CLAP ID
    pub fn get_by_clap_id(clap_id: u32) -> Option<&'static ParamDef> {
        SIMPLE_SYNTH_PARAMS.iter().find(|p| p.clap_id == clap_id)
    }

    /// Normalize value to 0.0-1.0 range for CLAP
    pub fn normalize(&self, value: f32) -> f64 {
        if matches!(self.id, PluginParamId::Waveform) {
            // Discrete/enum param - send raw value
            value.round() as f64
        } else {
            // Continuous param - normalize to 0-1
            ((value - self.min) / (self.max - self.min)) as f64
        }
    }

    /// Denormalize from 0.0-1.0 range
    pub fn denormalize(&self, normalized: f64) -> f32 {
        self.min + (normalized as f32) * (self.max - self.min)
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
                let idx = (value as usize).min(choices.len().saturating_sub(1));
                choices[idx].to_string()
            }
        }
    }

    /// Check if this is a discrete (enum-like) parameter
    pub fn is_discrete(&self) -> bool {
        matches!(self.display, ParamDisplay::Discrete { .. })
    }
}

/// Build default parameter values for a new plugin channel
pub fn default_plugin_params() -> HashMap<PluginParamId, f32> {
    SIMPLE_SYNTH_PARAMS
        .iter()
        .map(|p| (p.id, p.default))
        .collect()
}

/// Build PluginParam structs for the plugin editor UI
pub fn build_editor_params(stored: &HashMap<PluginParamId, f32>) -> Vec<super::PluginParam> {
    SIMPLE_SYNTH_PARAMS
        .iter()
        .enumerate()
        .map(|(idx, def)| super::PluginParam {
            id: idx as u32,
            name: def.id.as_str().to_string(),
            value: stored.get(&def.id).copied().unwrap_or(def.default),
            min: def.min,
            max: def.max,
            default: def.default,
        })
        .collect()
}

/// Build initial plugin state for audio thread
pub fn build_init_params(stored: &HashMap<PluginParamId, f32>) -> Vec<(u32, f64)> {
    SIMPLE_SYNTH_PARAMS
        .iter()
        .map(|def| {
            let value = stored.get(&def.id).copied().unwrap_or(def.default);
            let normalized = def.normalize(value);
            (def.clap_id, normalized)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_params_have_definitions() {
        for id in PluginParamId::ALL {
            assert!(ParamDef::get(*id).is_some());
        }
    }

    #[test]
    fn test_normalize_continuous() {
        let attack = ParamDef::get(PluginParamId::Attack).unwrap();
        // min should normalize to 0
        assert!((attack.normalize(1.0) - 0.0).abs() < 0.001);
        // max should normalize to 1
        assert!((attack.normalize(5000.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_discrete_passthrough() {
        let waveform = ParamDef::get(PluginParamId::Waveform).unwrap();
        // Discrete values should pass through as-is
        assert_eq!(waveform.normalize(0.0), 0.0);
        assert_eq!(waveform.normalize(2.0), 2.0);
        assert_eq!(waveform.normalize(3.0), 3.0);
    }
}
