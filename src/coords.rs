//! Type-safe coordinate spaces for the DAW.
//!
//! Each coordinate space is a newtype wrapper that prevents accidental mixing.
//! Conversions between spaces are explicit and implemented via From/Into traits.

// Allow dead code - these types define a complete API for future use
#![allow(dead_code)]

/// Channel rack column in app space: -3 (mute), -2 (track), -1 (sample), 0-15 (steps)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct AppCol(pub i32);

impl AppCol {
    pub const MUTE_ZONE: Self = Self(-3);
    pub const TRACK_ZONE: Self = Self(-2);
    pub const SAMPLE_ZONE: Self = Self(-1);
    pub const FIRST_STEP: Self = Self(0);
    pub const LAST_STEP: Self = Self(15);

    pub fn is_mute_zone(self) -> bool {
        self.0 == -3
    }

    pub fn is_track_zone(self) -> bool {
        self.0 == -2
    }

    pub fn is_sample_zone(self) -> bool {
        self.0 == -1
    }

    pub fn is_step_zone(self) -> bool {
        self.0 >= 0 && self.0 <= 15
    }

    /// Get the zone name for this column
    pub fn zone_name(self) -> &'static str {
        match self.0 {
            -3 => "mute",
            -2 => "track",
            -1 => "sample",
            _ => "steps",
        }
    }

    pub fn to_step(self) -> Option<StepIdx> {
        if self.is_step_zone() {
            Some(StepIdx(self.0 as usize))
        } else {
            None
        }
    }

    /// Get step index, defaulting to 0 if not in step zone
    pub fn to_step_or_zero(self) -> usize {
        if self.0 >= 0 {
            self.0 as usize
        } else {
            0
        }
    }

    pub fn clamp(self) -> Self {
        Self(self.0.clamp(-3, 15))
    }

    /// Create an AppCol from a step index (0-15)
    pub fn from_step(step: usize) -> Self {
        Self(step.min(15) as i32)
    }
}

/// Channel rack column in vim space: 0-18 (0=mute, 1=track, 2=sample, 3-18=steps)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VimCol(pub usize);

impl VimCol {
    pub const MUTE_ZONE: Self = Self(0);
    pub const TRACK_ZONE: Self = Self(1);
    pub const SAMPLE_ZONE: Self = Self(2);
    pub const FIRST_STEP: Self = Self(3);
    pub const LAST_STEP: Self = Self(18);

    /// Convert to step index if in step zone
    pub fn to_step(self) -> Option<usize> {
        if self.0 >= 3 {
            Some(self.0 - 3)
        } else {
            None
        }
    }
}

/// Conversion: VimCol -> AppCol
impl From<VimCol> for AppCol {
    fn from(col: VimCol) -> Self {
        AppCol(col.0 as i32 - 3)
    }
}

/// Conversion: AppCol -> VimCol
impl From<AppCol> for VimCol {
    fn from(col: AppCol) -> Self {
        VimCol((col.0 + 3).max(0) as usize)
    }
}

/// Pattern step index: 0-15
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StepIdx(pub usize);

impl StepIdx {
    pub const FIRST: Self = Self(0);
    pub const LAST: Self = Self(15);
    pub const COUNT: usize = 16;

    pub fn next(self) -> Self {
        Self((self.0 + 1) % Self::COUNT)
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

/// MIDI pitch: 0-127
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MidiPitch(pub u8);

impl MidiPitch {
    pub const MIN: Self = Self(0);
    pub const MAX: Self = Self(127);
    pub const MIDDLE_C: Self = Self(60);

    // Piano roll display range
    pub const PIANO_MIN: Self = Self(36); // C2
    pub const PIANO_MAX: Self = Self(84); // C6

    /// Convert to piano roll row (inverted: higher pitch = lower row)
    pub fn to_piano_row(self) -> usize {
        (Self::PIANO_MAX.0 - self.0) as usize
    }

    /// Convert from piano roll row
    pub fn from_piano_row(row: usize) -> Self {
        Self(Self::PIANO_MAX.0.saturating_sub(row as u8))
    }

    /// Clamp to piano roll range
    pub fn clamp_piano(self) -> Self {
        Self(self.0.clamp(Self::PIANO_MIN.0, Self::PIANO_MAX.0))
    }
}

impl Default for MidiPitch {
    fn default() -> Self {
        Self::MIDDLE_C
    }
}

/// Bar index in arrangement: 0-15
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BarIdx(pub usize);

impl BarIdx {
    pub const FIRST: Self = Self(0);
    pub const COUNT: usize = 16;

    pub fn next(self) -> Self {
        Self((self.0 + 1) % Self::COUNT)
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

/// Channel index: 0-98
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ChannelIdx(pub usize);

impl ChannelIdx {
    pub const MAX: usize = 99;

    pub fn as_usize(self) -> usize {
        self.0
    }

    pub fn clamp(self) -> Self {
        Self(self.0.min(Self::MAX - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_vim_col_roundtrip() {
        for i in -3..=15 {
            let app = AppCol(i);
            let vim: VimCol = app.into();
            let back: AppCol = vim.into();
            assert_eq!(app, back);
        }
    }

    #[test]
    fn test_app_col_zones() {
        assert!(AppCol(-3).is_mute_zone());
        assert!(AppCol(-2).is_track_zone());
        assert!(AppCol(-1).is_sample_zone());
        assert!(AppCol(0).is_step_zone());
        assert!(AppCol(15).is_step_zone());
        assert!(!AppCol(16).is_step_zone());
    }

    #[test]
    fn test_pitch_row_roundtrip() {
        for p in MidiPitch::PIANO_MIN.0..=MidiPitch::PIANO_MAX.0 {
            let pitch = MidiPitch(p);
            let row = pitch.to_piano_row();
            let back = MidiPitch::from_piano_row(row);
            assert_eq!(pitch, back);
        }
    }

    #[test]
    fn test_vim_col_to_step() {
        assert_eq!(VimCol(0).to_step(), None); // mute zone
        assert_eq!(VimCol(1).to_step(), None); // track zone
        assert_eq!(VimCol(2).to_step(), None); // sample zone
        assert_eq!(VimCol(3).to_step(), Some(0)); // first step
        assert_eq!(VimCol(18).to_step(), Some(15)); // last step
    }
}
