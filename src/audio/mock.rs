//! Mock audio handle for testing
//!
//! Provides a MockAudioHandle that captures commands instead of sending them
//! to a real audio engine. This enables testing App behavior without audio hardware.

use std::path::Path;
use std::sync::{Arc, Mutex};

use super::{AudioCommand, AudioMixerState, PeakLevelsBuffer, PluginInitState, WaveformBuffer};
use crate::effects::{EffectParamId, EffectType};
use crate::mixer::{StereoLevels, NUM_TRACKS};
use crate::plugin_host::ActivePluginProcessor;

/// A mock audio handle that captures commands for testing
///
/// Use this in tests instead of a real AudioHandle to:
/// - Verify the correct audio commands are being sent
/// - Test App behavior without requiring audio hardware
/// - Inspect command history
#[derive(Clone)]
pub struct MockAudioHandle {
    /// All captured commands (newest last)
    commands: Arc<Mutex<Vec<AudioCommand>>>,
    /// Sample rate to report
    sample_rate: u32,
    /// Shared waveform buffer (returns zeros)
    waveform_buffer: WaveformBuffer,
    /// Shared peak levels buffer (returns zeros)
    peak_levels: PeakLevelsBuffer,
}

impl Default for MockAudioHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAudioHandle {
    /// Create a new mock audio handle
    pub fn new() -> Self {
        Self {
            commands: Arc::new(Mutex::new(Vec::new())),
            sample_rate: 44100,
            waveform_buffer: Arc::new(Mutex::new(vec![0.0; 512])),
            peak_levels: Arc::new(Mutex::new([StereoLevels::default(); NUM_TRACKS])),
        }
    }

    /// Create with a specific sample rate
    pub fn with_sample_rate(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            ..Self::new()
        }
    }

    /// Get all captured commands
    pub fn commands(&self) -> Vec<AudioCommand> {
        self.commands.lock().unwrap().clone()
    }

    /// Get the last command sent (if any)
    pub fn last_command(&self) -> Option<AudioCommand> {
        self.commands.lock().unwrap().last().cloned()
    }

    /// Clear all captured commands
    pub fn clear_commands(&self) {
        self.commands.lock().unwrap().clear();
    }

    /// Get number of commands captured
    pub fn command_count(&self) -> usize {
        self.commands.lock().unwrap().len()
    }

    /// Check if a specific command type was sent
    pub fn has_command<F>(&self, predicate: F) -> bool
    where
        F: Fn(&AudioCommand) -> bool,
    {
        self.commands.lock().unwrap().iter().any(predicate)
    }

    // ========================================================================
    // AudioHandle-compatible methods
    // ========================================================================

    fn push_command(&self, cmd: AudioCommand) {
        self.commands.lock().unwrap().push(cmd);
    }

    pub fn play_sample(&self, path: &Path, volume: f32, generator_idx: usize) {
        self.push_command(AudioCommand::PlaySample {
            path: path.to_path_buf(),
            volume,
            generator_idx,
        });
    }

    pub fn preview_sample(&self, path: &Path, generator_idx: usize) {
        self.push_command(AudioCommand::PreviewSample {
            path: path.to_path_buf(),
            generator_idx,
            route_to_master: false,
        });
    }

    pub fn preview_sample_to_master(&self, path: &Path) {
        self.push_command(AudioCommand::PreviewSample {
            path: path.to_path_buf(),
            generator_idx: 0,
            route_to_master: true,
        });
    }

    pub fn stop_preview(&self) {
        self.push_command(AudioCommand::StopPreview);
    }

    pub fn stop_all(&self) {
        self.push_command(AudioCommand::StopAll);
    }

    pub fn set_master_volume(&self, volume: f32) {
        self.push_command(AudioCommand::SetMasterVolume(volume));
    }

    pub fn preload_sample(&self, path: &Path) {
        self.push_command(AudioCommand::PreloadSample {
            path: path.to_path_buf(),
        });
    }

    pub fn plugin_note_on(&self, channel: usize, note: u8, velocity: f32) {
        self.push_command(AudioCommand::PluginNoteOn {
            channel,
            note,
            velocity,
        });
    }

    pub fn plugin_note_off(&self, channel: usize, note: u8) {
        self.push_command(AudioCommand::PluginNoteOff { channel, note });
    }

    pub fn plugin_set_param(&self, channel: usize, param_id: u32, value: f64) {
        self.push_command(AudioCommand::PluginSetParam {
            channel,
            param_id,
            value,
        });
    }

    pub fn plugin_set_volume(&self, channel: usize, volume: f32) {
        self.push_command(AudioCommand::PluginSetVolume { channel, volume });
    }

    pub fn send_plugin(
        &self,
        _channel: usize,
        _processor: ActivePluginProcessor,
        _init_state: PluginInitState,
    ) {
        // Note: We don't capture plugin processors as they're not Clone
        // Tests that need to verify plugin sending should use a different approach
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn waveform_buffer(&self) -> &WaveformBuffer {
        &self.waveform_buffer
    }

    pub fn update_mixer_state(&self, state: AudioMixerState) {
        self.push_command(AudioCommand::UpdateMixerState(state));
    }

    pub fn set_generator_track(&self, generator: usize, track: usize) {
        self.push_command(AudioCommand::SetGeneratorTrack { generator, track });
    }

    pub fn get_peak_levels(&self) -> [StereoLevels; NUM_TRACKS] {
        *self.peak_levels.lock().unwrap()
    }

    pub fn peak_levels_buffer(&self) -> &PeakLevelsBuffer {
        &self.peak_levels
    }

    pub fn set_effect(&self, track: usize, slot: usize, effect_type: Option<EffectType>) {
        self.push_command(AudioCommand::SetEffect {
            track,
            slot,
            effect_type,
        });
    }

    pub fn set_effect_param(&self, track: usize, slot: usize, param_id: EffectParamId, value: f32) {
        self.push_command(AudioCommand::SetEffectParam {
            track,
            slot,
            param_id,
            value,
        });
    }

    pub fn set_effect_enabled(&self, track: usize, slot: usize, enabled: bool) {
        self.push_command(AudioCommand::SetEffectEnabled {
            track,
            slot,
            enabled,
        });
    }

    pub fn update_tempo(&self, bpm: f64) {
        self.push_command(AudioCommand::UpdateTempo(bpm));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_mock_captures_commands() {
        let mock = MockAudioHandle::new();
        mock.set_master_volume(0.5);
        mock.stop_all();

        assert_eq!(mock.command_count(), 2);
        assert!(matches!(
            mock.commands()[0],
            AudioCommand::SetMasterVolume(v) if (v - 0.5).abs() < 0.001
        ));
        assert!(matches!(mock.commands()[1], AudioCommand::StopAll));
    }

    #[test]
    fn test_mock_last_command() {
        let mock = MockAudioHandle::new();
        assert!(mock.last_command().is_none());

        mock.stop_preview();
        assert!(matches!(
            mock.last_command(),
            Some(AudioCommand::StopPreview)
        ));
    }

    #[test]
    fn test_mock_clear_commands() {
        let mock = MockAudioHandle::new();
        mock.stop_all();
        mock.stop_preview();
        assert_eq!(mock.command_count(), 2);

        mock.clear_commands();
        assert_eq!(mock.command_count(), 0);
    }

    #[test]
    fn test_mock_has_command() {
        let mock = MockAudioHandle::new();
        mock.play_sample(&PathBuf::from("/test.wav"), 0.8, 0);

        assert!(mock.has_command(|cmd| matches!(cmd, AudioCommand::PlaySample { .. })));
        assert!(!mock.has_command(|cmd| matches!(cmd, AudioCommand::StopAll)));
    }

    #[test]
    fn test_mock_sample_rate() {
        let mock = MockAudioHandle::with_sample_rate(48000);
        assert_eq!(mock.sample_rate(), 48000);
    }

    #[test]
    fn test_mock_mixer_state() {
        let mock = MockAudioHandle::new();
        let state = AudioMixerState::default();
        mock.update_mixer_state(state);

        assert!(mock.has_command(|cmd| matches!(cmd, AudioCommand::UpdateMixerState(_))));
    }

    #[test]
    fn test_mock_effect_commands() {
        let mock = MockAudioHandle::new();
        mock.set_effect(1, 0, Some(EffectType::Filter));
        mock.set_effect_param(1, 0, EffectParamId::FilterCutoff, 0.5);
        mock.set_effect_enabled(1, 0, false);

        assert_eq!(mock.command_count(), 3);
    }
}
