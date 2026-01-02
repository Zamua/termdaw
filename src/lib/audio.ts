import {
  AudioContext,
  AudioBuffer,
  AudioBufferSourceNode,
  GainNode,
  AnalyserNode,
} from "node-web-audio-api";
import path from "path";
import fs from "fs";

// Debug logging to file
const LOG_FILE = "/tmp/daw-audio.log";
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

// Initialize audio context lazily
export function getAudioContext(): AudioContext {
  if (!audioContext) {
    audioContext = new AudioContext();

    // Create master gain -> analyser -> destination chain
    masterGain = new GainNode(audioContext, { gain: 1.0 });
    analyser = new AnalyserNode(audioContext, {
      fftSize: 2048, // Larger buffer captures more audio data
      smoothingTimeConstant: 0.8, // Higher smoothing holds peaks longer for visualization
    });

    masterGain.connect(analyser);
    analyser.connect(audioContext.destination);
  }

  // Ensure context is running (may be suspended)
  if (audioContext.state === "suspended") {
    audioContext.resume();
  }

  return audioContext;
}

// Get the analyser node for visualization
export function getAnalyser(): AnalyserNode | null {
  getAudioContext(); // Ensure initialized
  return analyser;
}

// Get the master gain node (for routing external audio sources through the main bus)
export function getMasterGain(): GainNode {
  getAudioContext(); // Ensure initialized
  return masterGain!;
}

// Get waveform data from the analyser node
export function getWaveformData(): Uint8Array {
  getAudioContext(); // Ensure initialized
  if (!analyser) return new Uint8Array(256).fill(128);

  const dataArray = new Uint8Array(analyser.fftSize);
  analyser.getByteTimeDomainData(dataArray);
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
      fileBuffer.byteOffset + fileBuffer.byteLength,
    );

    const audioBuffer = await ctx.decodeAudioData(arrayBuffer);
    bufferCache.set(filePath, audioBuffer);
    return audioBuffer;
  } catch (err) {
    console.error("Error loading audio:", filePath, err);
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

  source.onended = () => {
    activeSources.delete(source);
  };

  source.start();
}

// Base pitch for samples (C4 = middle C = MIDI note 60)
const BASE_PITCH = 60;

// Play sample with pitch shift (polyphonic, for sequencer)
export async function playSamplePitched(
  samplePath: string,
  pitch: number,
): Promise<void> {
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
    } catch {
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

  previewSource = source;
  source.onended = () => {
    if (previewSource === source) {
      previewSource = null;
    }
  };

  source.start();
}

// Preview sample with pitch shift (exclusive, stops previous preview)
export async function previewSamplePitched(
  samplePath: string,
  pitch: number,
): Promise<void> {
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
    } catch {
      // Already stopped
    }
  }
  activeSources.clear();
}

// Get the samples directory path
export function getSamplesDir(): string {
  return path.join(process.cwd(), "samples");
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
