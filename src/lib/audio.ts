import {
  AudioContext,
  AudioBuffer,
  AudioBufferSourceNode,
  GainNode,
  AnalyserNode,
} from 'node-web-audio-api';
import path from 'path';
import fs from 'fs';

// Debug logging to file
const LOG_FILE = '/tmp/daw-audio.log';
function log(msg: string) {
  fs.appendFileSync(LOG_FILE, `${new Date().toISOString()} ${msg}\n`);
}

// Global audio context and nodes
let audioContext: AudioContext | null = null;
let masterGain: GainNode | null = null;
let analyser: AnalyserNode | null = null;

// Cache for decoded audio buffers
const bufferCache = new Map<string, AudioBuffer>();

// Track active sources for stopping
const activeSources = new Set<AudioBufferSourceNode>();
let previewSource: AudioBufferSourceNode | null = null;

// Manual waveform tracking (AnalyserNode doesn't work in node-web-audio-api)
interface ActivePlayback {
  buffer: AudioBuffer;
  startTime: number;
  playbackRate: number;
}
const activePlaybacks: ActivePlayback[] = [];


// Initialize audio context lazily
function getAudioContext(): AudioContext {
  if (!audioContext) {
    audioContext = new AudioContext();

    // Create master gain -> analyser -> destination chain
    masterGain = new GainNode(audioContext, { gain: 1.0 });
    analyser = new AnalyserNode(audioContext, {
      fftSize: 256,  // Smaller = faster response
      smoothingTimeConstant: 0.3,  // Less smoothing = more responsive
    });

    masterGain.connect(analyser);
    analyser.connect(audioContext.destination);
  }

  // Ensure context is running (may be suspended)
  if (audioContext.state === 'suspended') {
    audioContext.resume();
  }

  return audioContext;
}

// Get the analyser node for visualization
export function getAnalyser(): AnalyserNode | null {
  getAudioContext(); // Ensure initialized
  return analyser;
}

// Get waveform data - manually sample from active playbacks
const WAVEFORM_SIZE = 256;
let debugCounter = 0;
export function getWaveformData(): Uint8Array {
  const ctx = getAudioContext();
  const currentTime = ctx.currentTime;
  const dataArray = new Uint8Array(WAVEFORM_SIZE);

  // Clean up finished playbacks
  for (let i = activePlaybacks.length - 1; i >= 0; i--) {
    const pb = activePlaybacks[i]!;
    const elapsed = (currentTime - pb.startTime) * pb.playbackRate;
    if (elapsed >= pb.buffer.duration) {
      activePlaybacks.splice(i, 1);
    }
  }

  // If no active playbacks, return silence (128)
  if (activePlaybacks.length === 0) {
    dataArray.fill(128);
    return dataArray;
  }

  // Sample from active playbacks - take a ~5ms window of audio
  const sampleWindow = 0.005;

  for (let i = 0; i < WAVEFORM_SIZE; i++) {
    let sum = 0;

    for (const pb of activePlaybacks) {
      const elapsed = (currentTime - pb.startTime) * pb.playbackRate;
      const windowOffset = (i / WAVEFORM_SIZE - 0.5) * sampleWindow;
      const sampleTime = elapsed + windowOffset;

      if (sampleTime >= 0 && sampleTime < pb.buffer.duration) {
        const channelData = pb.buffer.getChannelData(0);
        const sampleIndex = Math.floor(sampleTime * pb.buffer.sampleRate);
        if (sampleIndex >= 0 && sampleIndex < channelData.length) {
          sum += channelData[sampleIndex]!;
        }
      }
    }

    // Convert summed samples to 0..255 (128 = center) with gain boost
    const gain = 5.0; // amplify for better visibility
    const amplified = Math.max(-1, Math.min(1, sum * gain));
    dataArray[i] = Math.max(0, Math.min(255, Math.floor((amplified + 1) * 127.5)));
  }

  // Debug log
  debugCounter++;
  if (debugCounter % 60 === 0) {
    const min = Math.min(...dataArray);
    const max = Math.max(...dataArray);
    const avg = dataArray.reduce((a, b) => a + b, 0) / dataArray.length;
    log(`playbacks=${activePlaybacks.length} min=${min} max=${max} avg=${avg.toFixed(1)}`);
  }

  return dataArray;
}

// Get frequency data
export function getFrequencyData(): Uint8Array {
  getAudioContext(); // Ensure initialized
  if (!analyser) return new Uint8Array(0);

  const dataArray = new Uint8Array(analyser.frequencyBinCount);
  analyser.getByteFrequencyData(dataArray);
  return dataArray;
}

// Load and decode audio file
async function loadAudioBuffer(filePath: string): Promise<AudioBuffer | null> {
  // Check cache first
  if (bufferCache.has(filePath)) {
    return bufferCache.get(filePath)!;
  }

  try {
    if (!fs.existsSync(filePath)) {
      return null;
    }

    const ctx = getAudioContext();
    const fileBuffer = fs.readFileSync(filePath);
    const arrayBuffer = fileBuffer.buffer.slice(
      fileBuffer.byteOffset,
      fileBuffer.byteOffset + fileBuffer.byteLength
    );

    const audioBuffer = await ctx.decodeAudioData(arrayBuffer);
    bufferCache.set(filePath, audioBuffer);
    return audioBuffer;
  } catch (err) {
    console.error('Error loading audio:', filePath, err);
    return null;
  }
}

// Play a sample (polyphonic, for sequencer)
export async function playSample(samplePath: string): Promise<void> {
  const buffer = await loadAudioBuffer(samplePath);
  if (!buffer || !masterGain) {
    log(`playSample: no buffer or masterGain for ${samplePath}`);
    return;
  }

  const ctx = getAudioContext();
  const source = new AudioBufferSourceNode(ctx, { buffer });
  source.connect(masterGain);

  activeSources.add(source);

  // Track for waveform visualization
  const playback: ActivePlayback = { buffer, startTime: ctx.currentTime, playbackRate: 1 };
  activePlaybacks.push(playback);

  source.onended = () => {
    activeSources.delete(source);
  };

  source.start();
}

// Base pitch for samples (C4 = middle C = MIDI note 60)
const BASE_PITCH = 60;

// Play sample with pitch shift (polyphonic, for sequencer)
export async function playSamplePitched(samplePath: string, pitch: number): Promise<void> {
  const buffer = await loadAudioBuffer(samplePath);
  if (!buffer || !masterGain) return;

  const ctx = getAudioContext();
  const semitoneOffset = pitch - BASE_PITCH;
  const playbackRate = Math.pow(2, semitoneOffset / 12);

  const source = new AudioBufferSourceNode(ctx, {
    buffer,
    playbackRate,
  });
  source.connect(masterGain);

  activeSources.add(source);

  // Track for waveform visualization
  activePlaybacks.push({ buffer, startTime: ctx.currentTime, playbackRate });

  source.onended = () => {
    activeSources.delete(source);
  };

  source.start();
}

// Stop any currently playing preview
export function stopPreview(): void {
  if (previewSource) {
    try {
      previewSource.stop();
    } catch (e) {
      // Already stopped
    }
    previewSource = null;
  }
}

// Play sample exclusively (stops previous preview) - for browser
export async function previewSample(samplePath: string): Promise<void> {
  stopPreview();

  const buffer = await loadAudioBuffer(samplePath);
  if (!buffer || !masterGain) {
    log(`previewSample: no buffer or masterGain for ${samplePath}`);
    return;
  }

  const ctx = getAudioContext();
  const source = new AudioBufferSourceNode(ctx, { buffer });
  source.connect(masterGain);

  // Track for waveform visualization
  activePlaybacks.push({ buffer, startTime: ctx.currentTime, playbackRate: 1 });

  previewSource = source;
  source.onended = () => {
    if (previewSource === source) {
      previewSource = null;
    }
  };

  source.start();
}

// Preview sample with pitch shift (exclusive, stops previous preview)
export async function previewSamplePitched(samplePath: string, pitch: number): Promise<void> {
  stopPreview();

  const buffer = await loadAudioBuffer(samplePath);
  if (!buffer || !masterGain) return;

  const ctx = getAudioContext();
  const semitoneOffset = pitch - BASE_PITCH;
  const playbackRate = Math.pow(2, semitoneOffset / 12);

  const source = new AudioBufferSourceNode(ctx, {
    buffer,
    playbackRate,
  });
  source.connect(masterGain);

  // Track for waveform visualization
  activePlaybacks.push({ buffer, startTime: ctx.currentTime, playbackRate });

  previewSource = source;
  source.onended = () => {
    if (previewSource === source) {
      previewSource = null;
    }
  };

  source.start();
}

// Stop all playback
export function stopPlayback(): void {
  stopPreview();

  for (const source of activeSources) {
    try {
      source.stop();
    } catch (e) {
      // Already stopped
    }
  }
  activeSources.clear();
}

// Get the samples directory path
export function getSamplesDir(): string {
  return path.join(process.cwd(), 'samples');
}

// Get full path to a sample
export function getSamplePath(sampleName: string): string {
  return path.join(getSamplesDir(), sampleName);
}

// Close audio context (cleanup)
export function closeAudio(): void {
  stopPlayback();
  if (audioContext) {
    audioContext.close();
    audioContext = null;
    masterGain = null;
    analyser = null;
  }
  bufferCache.clear();
}
