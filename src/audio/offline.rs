//! Offline audio rendering for WAV export
//!
//! Uses the same MixingEngine as real-time playback to ensure
//! exported audio is identical to what is heard during playback.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hound::{SampleFormat, WavSpec, WavWriter};
use rodio::{Decoder, Source};

use super::{setup_engine, MixingEngine, SampleData};
use crate::arrangement::Arrangement;
use crate::mixer::{Mixer, TrackId};
use crate::plugin_host::PluginLoader;
use crate::sequencer::{Channel, ChannelSource, Pattern};

/// Configuration for offline rendering
pub struct RenderConfig {
    pub sample_rate: u32,
    pub bpm: f64,
    pub steps_per_bar: usize,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            bpm: 140.0,
            steps_per_bar: 16,
        }
    }
}

/// Render arrangement to stereo audio samples
///
/// Returns interleaved stereo f32 samples (L, R, L, R, ...)
#[allow(clippy::too_many_arguments)]
pub fn render_offline(
    channels: &[Channel],
    patterns: &[Pattern],
    arrangement: &Arrangement,
    mixer: &Mixer,
    samples_path: &Path,
    plugins_path: &Path,
    plugin_loader: &dyn PluginLoader,
    config: &RenderConfig,
) -> Vec<f32> {
    let mut engine = MixingEngine::new(config.sample_rate);

    // Use shared setup function (loads plugins, effects, sets mixer state)
    setup_engine(
        &mut engine,
        channels,
        mixer,
        plugins_path,
        plugin_loader,
        config.sample_rate,
        config.bpm,
    );

    // Load samples into cache (for sampler channels)
    let sample_cache = load_samples(channels, samples_path);

    // Calculate total length from arrangement
    if arrangement.placements.is_empty() {
        return Vec::new();
    }
    let total_bars = get_last_bar(arrangement);
    let total_steps = total_bars * config.steps_per_bar;

    // Calculate samples per step
    let samples_per_beat = (60.0 / config.bpm * config.sample_rate as f64) as usize;
    let beats_per_step = 4.0 / config.steps_per_bar as f64; // Assuming 4/4 time
    let samples_per_step = (samples_per_beat as f64 * beats_per_step) as usize;

    let mut output = Vec::new();
    let block_size = 512;

    for step in 0..total_steps {
        let bar = step / config.steps_per_bar;
        let step_in_bar = step % config.steps_per_bar;

        // Trigger notes for this step
        trigger_step(
            &mut engine,
            &sample_cache,
            channels,
            patterns,
            arrangement,
            mixer,
            samples_path,
            bar,
            step_in_bar,
        );

        // Process audio for this step
        let mut samples_remaining = samples_per_step;
        while samples_remaining > 0 {
            let frames = samples_remaining.min(block_size);
            engine.process_block(frames);
            let master = engine.master_buffer();

            // Append interleaved stereo to output
            for i in 0..frames {
                output.push(master.left[i].clamp(-1.0, 1.0));
                output.push(master.right[i].clamp(-1.0, 1.0));
            }
            samples_remaining -= frames;
        }
    }

    output
}

/// Write rendered audio to WAV file
pub fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<(), hound::Error> {
    let spec = WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut wav_writer = WavWriter::new(writer, spec)?;

    for &sample in samples {
        // Convert f32 (-1.0 to 1.0) to i16
        let sample_i16 = (sample * i16::MAX as f32) as i16;
        wav_writer.write_sample(sample_i16)?;
    }

    wav_writer.finalize()?;
    Ok(())
}

fn get_last_bar(arrangement: &Arrangement) -> usize {
    arrangement
        .placements
        .iter()
        .map(|p| p.start_bar + p.length)
        .max()
        .unwrap_or(0)
}

fn load_samples(channels: &[Channel], samples_path: &Path) -> HashMap<PathBuf, SampleData> {
    use std::collections::hash_map::Entry;

    let mut cache = HashMap::new();

    for channel in channels {
        if let ChannelSource::Sampler {
            path: Some(ref rel_path),
        } = channel.source
        {
            let full_path = samples_path.join(rel_path);
            if let Entry::Vacant(e) = cache.entry(full_path.clone()) {
                if let Some(sample) = load_sample(&full_path) {
                    e.insert(sample);
                }
            }
        }
    }

    cache
}

fn load_sample(path: &Path) -> Option<SampleData> {
    let file = File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let decoder = Decoder::new(reader).ok()?;

    let sample_rate = decoder.sample_rate();
    let channels = decoder.channels();
    let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();

    if samples.is_empty() {
        return None;
    }

    Some(SampleData {
        data: Arc::new(samples),
        sample_rate,
        channels,
    })
}

#[allow(clippy::too_many_arguments)]
fn trigger_step(
    engine: &mut MixingEngine,
    sample_cache: &HashMap<PathBuf, SampleData>,
    channels: &[Channel],
    patterns: &[Pattern],
    arrangement: &Arrangement,
    mixer: &Mixer,
    samples_path: &Path,
    bar: usize,
    step: usize,
) {
    // Get active patterns at this bar
    let placements = arrangement.get_active_placements_at_bar(bar);

    for placement in placements {
        if let Some(pattern) = patterns.get(placement.pattern_id) {
            trigger_pattern_step(
                engine,
                sample_cache,
                channels,
                pattern,
                mixer,
                samples_path,
                step,
            );
        }
    }
}

fn trigger_pattern_step(
    engine: &mut MixingEngine,
    sample_cache: &HashMap<PathBuf, SampleData>,
    channels: &[Channel],
    pattern: &Pattern,
    mixer: &Mixer,
    samples_path: &Path,
    step: usize,
) {
    for (channel_idx, channel) in channels.iter().enumerate() {
        let track_id = TrackId(channel.mixer_track);

        // Skip if track is muted or not audible
        if !mixer.is_track_audible(track_id) {
            continue;
        }

        let volume = mixer.track(track_id).volume;
        let slice = channel.get_pattern(pattern.id);

        match &channel.source {
            ChannelSource::Sampler { path } => {
                // Sampler channels use step sequencer grid
                if slice.map(|s| s.get_step(step)).unwrap_or(false) {
                    if let Some(ref sample_path) = path {
                        let full_path = samples_path.join(sample_path);
                        if let Some(sample) = sample_cache.get(&full_path) {
                            engine.add_voice(sample.clone(), volume, channel_idx, false);
                        }
                    }
                }
            }
            ChannelSource::Plugin { .. } => {
                // Plugin channels use piano roll notes
                if let Some(slice) = slice {
                    for note in &slice.notes {
                        if note.start_step == step {
                            engine.send_plugin_note(channel_idx, note.pitch, note.velocity, true);
                        }
                        // Check for note-off events
                        if note.start_step + note.duration == step {
                            engine.send_plugin_note(channel_idx, note.pitch, 0.0, false);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::PatternPlacement;
    use crate::plugin_host::MockPluginLoader;

    #[test]
    fn test_render_empty_arrangement() {
        let channels: Vec<Channel> = vec![];
        let patterns: Vec<Pattern> = vec![];
        let arrangement = Arrangement::new();
        let mixer = Mixer::new();
        let samples_path = Path::new("/tmp");
        let plugins_path = Path::new("/tmp");
        let plugin_loader = MockPluginLoader::new();
        let config = RenderConfig::default();

        let samples = render_offline(
            &channels,
            &patterns,
            &arrangement,
            &mixer,
            samples_path,
            plugins_path,
            &plugin_loader,
            &config,
        );

        // Empty arrangement should produce no audio
        assert!(samples.is_empty());
    }

    #[test]
    fn test_render_produces_correct_length() {
        let channels: Vec<Channel> = vec![];
        let pattern = Pattern {
            id: 0,
            name: "Test".to_string(),
            length: 16,
        };
        let patterns = vec![pattern];

        let mut arrangement = Arrangement::new();
        // Add one bar placement
        arrangement.placements.push(PatternPlacement {
            id: "p1".to_string(),
            pattern_id: 0,
            start_bar: 0,
            length: 1,
        });

        let mixer = Mixer::new();
        let samples_path = Path::new("/tmp");
        let plugins_path = Path::new("/tmp");
        let plugin_loader = MockPluginLoader::new();
        let config = RenderConfig {
            sample_rate: 44100,
            bpm: 120.0, // 0.5 sec per beat, 2 sec per bar (4 beats)
            steps_per_bar: 16,
        };

        let samples = render_offline(
            &channels,
            &patterns,
            &arrangement,
            &mixer,
            samples_path,
            plugins_path,
            &plugin_loader,
            &config,
        );

        // At 120 BPM: 1 beat = 0.5 sec, 1 bar = 4 beats = 2 sec
        // 2 sec * 44100 samples/sec * 2 channels = 176400 samples
        // Allow some tolerance for rounding
        let expected_samples = 44100 * 2 * 2; // 2 sec * sample_rate * stereo
        assert!(
            samples.len() >= expected_samples - 1000 && samples.len() <= expected_samples + 1000,
            "Expected ~{} samples, got {}",
            expected_samples,
            samples.len()
        );

        // Output should be stereo interleaved (even number of samples)
        assert_eq!(samples.len() % 2, 0);
    }

    #[test]
    fn test_render_multiple_bars() {
        let channels: Vec<Channel> = vec![];
        let pattern = Pattern {
            id: 0,
            name: "Test".to_string(),
            length: 16,
        };
        let patterns = vec![pattern];

        let mut arrangement = Arrangement::new();
        // Add 4 bars
        for i in 0..4 {
            arrangement.placements.push(PatternPlacement {
                id: format!("p{}", i),
                pattern_id: 0,
                start_bar: i,
                length: 1,
            });
        }

        let mixer = Mixer::new();
        let samples_path = Path::new("/tmp");
        let plugins_path = Path::new("/tmp");
        let plugin_loader = MockPluginLoader::new();
        let config = RenderConfig {
            sample_rate: 44100,
            bpm: 120.0,
            steps_per_bar: 16,
        };

        let samples = render_offline(
            &channels,
            &patterns,
            &arrangement,
            &mixer,
            samples_path,
            plugins_path,
            &plugin_loader,
            &config,
        );

        // 4 bars at 120 BPM = 8 sec
        let expected_samples = 44100 * 8 * 2; // 8 sec * sample_rate * stereo
        assert!(
            samples.len() >= expected_samples - 2000 && samples.len() <= expected_samples + 2000,
            "Expected ~{} samples, got {}",
            expected_samples,
            samples.len()
        );
    }
}
