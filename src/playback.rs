//! Playback state machine.
//!
//! Encapsulates all playback-related state in a single enum, making it
//! impossible to have inconsistent state (e.g., playing with wrong step).

// Allow dead code - some methods are defined for API completeness
#![allow(dead_code)]

use crate::coords::{BarIdx, StepIdx};

/// Events emitted during playback
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackEvent {
    /// A new step should be triggered
    Step { step: StepIdx },
    /// Pattern looped back to beginning
    PatternLoop,
    /// Arrangement bar advanced
    BarAdvance { bar: BarIdx },
}

/// Playback state - exactly one variant is active
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackState {
    #[default]
    Stopped,

    PlayingPattern {
        step: StepIdx,
    },

    PlayingArrangement {
        bar: BarIdx,
        step: StepIdx,
    },
}

impl PlaybackState {
    /// Check if currently playing
    pub fn is_playing(&self) -> bool {
        !matches!(self, Self::Stopped)
    }

    /// Check if playing a pattern
    pub fn is_playing_pattern(&self) -> bool {
        matches!(self, Self::PlayingPattern { .. })
    }

    /// Check if playing arrangement
    pub fn is_playing_arrangement(&self) -> bool {
        matches!(self, Self::PlayingArrangement { .. })
    }

    /// Get current step (if playing)
    pub fn current_step(&self) -> Option<StepIdx> {
        match self {
            Self::Stopped => None,
            Self::PlayingPattern { step } => Some(*step),
            Self::PlayingArrangement { step, .. } => Some(*step),
        }
    }

    /// Get current step as usize, or 0 if stopped
    pub fn step_or_zero(&self) -> usize {
        self.current_step().map(|s| s.0).unwrap_or(0)
    }

    /// Get current bar (if playing arrangement)
    pub fn current_bar(&self) -> Option<BarIdx> {
        match self {
            Self::PlayingArrangement { bar, .. } => Some(*bar),
            _ => None,
        }
    }

    /// Get current bar as usize, or 0 if not in arrangement mode
    pub fn bar_or_zero(&self) -> usize {
        self.current_bar().map(|b| b.0).unwrap_or(0)
    }

    /// Start playing a pattern from the beginning
    pub fn play_pattern(&mut self) {
        *self = Self::PlayingPattern {
            step: StepIdx::FIRST,
        };
    }

    /// Start playing a pattern from a specific step
    pub fn play_pattern_from(&mut self, step: usize) {
        *self = Self::PlayingPattern {
            step: StepIdx(step % StepIdx::COUNT),
        };
    }

    /// Start playing arrangement from the beginning
    pub fn play_arrangement(&mut self) {
        *self = Self::PlayingArrangement {
            bar: BarIdx::FIRST,
            step: StepIdx::FIRST,
        };
    }

    /// Start playing arrangement from a specific bar
    pub fn play_arrangement_from(&mut self, bar: usize) {
        *self = Self::PlayingArrangement {
            bar: BarIdx(bar % BarIdx::COUNT),
            step: StepIdx::FIRST,
        };
    }

    /// Stop playback
    pub fn stop(&mut self) {
        *self = Self::Stopped;
    }

    /// Toggle between playing pattern and stopped
    pub fn toggle_pattern(&mut self) {
        if self.is_playing() {
            self.stop();
        } else {
            self.play_pattern();
        }
    }

    /// Toggle arrangement playback
    pub fn toggle_arrangement(&mut self) {
        if self.is_playing() {
            self.stop();
        } else {
            self.play_arrangement();
        }
    }

    /// Toggle arrangement playback from a specific bar
    pub fn toggle_arrangement_from(&mut self, bar: usize) {
        if self.is_playing() {
            self.stop();
        } else {
            self.play_arrangement_from(bar);
        }
    }

    /// Advance to next step, returning any events that occurred
    pub fn advance(&mut self) -> Vec<PlaybackEvent> {
        let mut events = Vec::new();

        match self {
            Self::Stopped => {}

            Self::PlayingPattern { step } => {
                *step = step.next();
                events.push(PlaybackEvent::Step { step: *step });

                if step.0 == 0 {
                    events.push(PlaybackEvent::PatternLoop);
                }
            }

            Self::PlayingArrangement { bar, step } => {
                *step = step.next();
                events.push(PlaybackEvent::Step { step: *step });

                if step.0 == 0 {
                    *bar = bar.next();
                    events.push(PlaybackEvent::BarAdvance { bar: *bar });
                }
            }
        }

        events
    }

    /// Set playhead to specific step (for scrubbing/seeking)
    pub fn seek_step(&mut self, new_step: usize) {
        match self {
            Self::PlayingPattern { step } => *step = StepIdx(new_step % StepIdx::COUNT),
            Self::PlayingArrangement { step, .. } => *step = StepIdx(new_step % StepIdx::COUNT),
            Self::Stopped => {}
        }
    }

    /// Set playhead to specific bar (arrangement only)
    pub fn seek_bar(&mut self, new_bar: usize) {
        if let Self::PlayingArrangement { bar, step } = self {
            *bar = BarIdx(new_bar % BarIdx::COUNT);
            *step = StepIdx::FIRST;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_playback() {
        let mut state = PlaybackState::Stopped;
        assert!(!state.is_playing());

        state.play_pattern();
        assert!(state.is_playing());
        assert_eq!(state.current_step(), Some(StepIdx::FIRST));

        // Advance through all 16 steps
        for i in 1..16 {
            let events = state.advance();
            assert_eq!(state.current_step(), Some(StepIdx(i)));
            assert!(!events.contains(&PlaybackEvent::PatternLoop));
        }

        // Step 16 wraps to 0 and emits loop event
        let events = state.advance();
        assert_eq!(state.current_step(), Some(StepIdx::FIRST));
        assert!(events.contains(&PlaybackEvent::PatternLoop));
    }

    #[test]
    fn test_arrangement_playback() {
        let mut state = PlaybackState::Stopped;
        state.play_arrangement();

        assert_eq!(state.current_bar(), Some(BarIdx::FIRST));
        assert_eq!(state.current_step(), Some(StepIdx::FIRST));

        // Advance through 16 steps to reach next bar
        for _ in 0..16 {
            state.advance();
        }

        assert_eq!(state.current_bar(), Some(BarIdx(1)));
        assert_eq!(state.current_step(), Some(StepIdx::FIRST));
    }

    #[test]
    fn test_stop_resets_state() {
        let mut state = PlaybackState::PlayingPattern { step: StepIdx(5) };
        state.stop();
        assert!(!state.is_playing());
        assert_eq!(state.current_step(), None);
    }

    #[test]
    fn test_toggle() {
        let mut state = PlaybackState::Stopped;

        state.toggle_pattern();
        assert!(state.is_playing_pattern());

        state.toggle_pattern();
        assert!(!state.is_playing());
    }

    #[test]
    fn test_play_from_position() {
        let mut state = PlaybackState::Stopped;

        state.play_arrangement_from(5);
        assert_eq!(state.current_bar(), Some(BarIdx(5)));
        assert_eq!(state.current_step(), Some(StepIdx::FIRST));
    }
}
