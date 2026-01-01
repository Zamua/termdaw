import { AudioContext, OscillatorNode, GainNode, BiquadFilterNode } from 'node-web-audio-api';

// Waveform types
export type WaveformType = 'sine' | 'square' | 'sawtooth' | 'triangle';

// Oscillator settings
export interface OscillatorSettings {
  enabled: boolean;
  waveform: WaveformType;
  coarse: number;      // Semitones offset (-24 to +24)
  fine: number;        // Cents offset (-100 to +100)
  volume: number;      // 0 to 1
}

// ADSR envelope settings
export interface EnvelopeSettings {
  attack: number;      // seconds (0.001 to 2)
  decay: number;       // seconds (0.001 to 2)
  sustain: number;     // level 0 to 1
  release: number;     // seconds (0.001 to 3)
}

// Filter settings
export interface FilterSettings {
  enabled: boolean;
  type: 'lowpass' | 'highpass' | 'bandpass';
  cutoff: number;      // Hz (20 to 20000)
  resonance: number;   // Q factor (0.1 to 30)
}

// Complete synth patch
export interface SynthPatch {
  name: string;
  oscillators: [OscillatorSettings, OscillatorSettings, OscillatorSettings];
  envelope: EnvelopeSettings;
  filter: FilterSettings;
  masterVolume: number;
}

// Default patch (basic saw lead)
export const defaultPatch: SynthPatch = {
  name: 'Init',
  oscillators: [
    { enabled: true, waveform: 'sawtooth', coarse: 0, fine: 0, volume: 0.5 },
    { enabled: false, waveform: 'square', coarse: 0, fine: 0, volume: 0.3 },
    { enabled: false, waveform: 'sine', coarse: -12, fine: 0, volume: 0.4 },
  ],
  envelope: {
    attack: 0.01,
    decay: 0.1,
    sustain: 0.7,
    release: 0.3,
  },
  filter: {
    enabled: true,
    type: 'lowpass',
    cutoff: 2000,
    resonance: 1,
  },
  masterVolume: 0.5,
};

// Some preset patches
export const presets: SynthPatch[] = [
  defaultPatch,
  {
    name: 'Soft Pad',
    oscillators: [
      { enabled: true, waveform: 'sine', coarse: 0, fine: 0, volume: 0.6 },
      { enabled: true, waveform: 'triangle', coarse: 12, fine: 7, volume: 0.3 },
      { enabled: true, waveform: 'sine', coarse: -12, fine: 0, volume: 0.4 },
    ],
    envelope: { attack: 0.3, decay: 0.2, sustain: 0.8, release: 0.8 },
    filter: { enabled: true, type: 'lowpass', cutoff: 1500, resonance: 0.5 },
    masterVolume: 0.4,
  },
  {
    name: 'Bass',
    oscillators: [
      { enabled: true, waveform: 'sawtooth', coarse: -12, fine: 0, volume: 0.6 },
      { enabled: true, waveform: 'square', coarse: -12, fine: -10, volume: 0.4 },
      { enabled: false, waveform: 'sine', coarse: 0, fine: 0, volume: 0 },
    ],
    envelope: { attack: 0.01, decay: 0.2, sustain: 0.4, release: 0.1 },
    filter: { enabled: true, type: 'lowpass', cutoff: 800, resonance: 2 },
    masterVolume: 0.6,
  },
  {
    name: 'Pluck',
    oscillators: [
      { enabled: true, waveform: 'sawtooth', coarse: 0, fine: 0, volume: 0.5 },
      { enabled: true, waveform: 'square', coarse: 0, fine: 5, volume: 0.3 },
      { enabled: false, waveform: 'sine', coarse: 0, fine: 0, volume: 0 },
    ],
    envelope: { attack: 0.001, decay: 0.15, sustain: 0.1, release: 0.2 },
    filter: { enabled: true, type: 'lowpass', cutoff: 3000, resonance: 1.5 },
    masterVolume: 0.5,
  },
  {
    name: 'Lead',
    oscillators: [
      { enabled: true, waveform: 'square', coarse: 0, fine: 0, volume: 0.4 },
      { enabled: true, waveform: 'sawtooth', coarse: 0, fine: 7, volume: 0.4 },
      { enabled: true, waveform: 'square', coarse: 12, fine: 0, volume: 0.2 },
    ],
    envelope: { attack: 0.01, decay: 0.1, sustain: 0.8, release: 0.2 },
    filter: { enabled: true, type: 'lowpass', cutoff: 4000, resonance: 2 },
    masterVolume: 0.4,
  },
];

// Global audio context (singleton)
let audioContext: AudioContext | null = null;

function getAudioContext(): AudioContext {
  if (!audioContext) {
    // Use playback latency hint for better stability
    audioContext = new AudioContext({ latencyHint: 'playback' });
  }
  return audioContext;
}

// Convert MIDI note to frequency
function midiToFrequency(midi: number): number {
  return 440 * Math.pow(2, (midi - 69) / 12);
}

// Active voices for note management
interface Voice {
  oscillators: OscillatorNode[];
  gainNodes: GainNode[];
  envelope: GainNode;
  filter: BiquadFilterNode | null;
  noteId: string;
}

const activeVoices: Map<string, Voice> = new Map();

// Play a synth note
export function playSynthNote(
  patch: SynthPatch,
  pitch: number,
  duration: number,
  noteId?: string
): void {
  const ctx = getAudioContext();
  const now = ctx.currentTime;
  const id = noteId || `${Date.now()}-${Math.random()}`;

  // Create master gain for this voice
  const masterGain = new GainNode(ctx, { gain: patch.masterVolume });

  // Create filter if enabled
  let filterNode: BiquadFilterNode | null = null;
  let lastNode: AudioNode = masterGain;

  if (patch.filter.enabled) {
    filterNode = new BiquadFilterNode(ctx, {
      type: patch.filter.type,
      frequency: patch.filter.cutoff,
      Q: patch.filter.resonance,
    });
    filterNode.connect(masterGain);
    lastNode = filterNode;
  }

  // Create envelope gain
  const envelopeGain = new GainNode(ctx, { gain: 0 });
  envelopeGain.connect(lastNode);

  // Create oscillators
  const oscillators: OscillatorNode[] = [];
  const gainNodes: GainNode[] = [];

  for (const oscSettings of patch.oscillators) {
    if (!oscSettings.enabled) continue;

    // Calculate frequency with coarse and fine tuning
    const semitoneOffset = oscSettings.coarse + oscSettings.fine / 100;
    const frequency = midiToFrequency(pitch + semitoneOffset);

    // Create oscillator
    const osc = new OscillatorNode(ctx, {
      type: oscSettings.waveform,
      frequency,
    });

    // Create gain for this oscillator
    const oscGain = new GainNode(ctx, { gain: oscSettings.volume });
    osc.connect(oscGain);
    oscGain.connect(envelopeGain);

    oscillators.push(osc);
    gainNodes.push(oscGain);
  }

  // Connect master to destination
  masterGain.connect(ctx.destination);

  // Apply ADSR envelope
  const { attack, decay, sustain, release } = patch.envelope;
  const noteEndTime = now + duration;
  const releaseStartTime = Math.max(now + attack + decay, noteEndTime);

  envelopeGain.gain.setValueAtTime(0, now);
  envelopeGain.gain.linearRampToValueAtTime(1, now + attack);
  envelopeGain.gain.linearRampToValueAtTime(sustain, now + attack + decay);
  envelopeGain.gain.setValueAtTime(sustain, releaseStartTime);
  envelopeGain.gain.linearRampToValueAtTime(0.0001, releaseStartTime + release);

  // Start oscillators
  for (const osc of oscillators) {
    osc.start(now);
    osc.stop(releaseStartTime + release + 0.1);
  }

  // Store voice for potential early release
  const voice: Voice = {
    oscillators,
    gainNodes,
    envelope: envelopeGain,
    filter: filterNode,
    noteId: id,
  };
  activeVoices.set(id, voice);

  // Cleanup after note ends
  setTimeout(() => {
    activeVoices.delete(id);
  }, (releaseStartTime + release + 0.2 - now) * 1000);
}

// Preview a synth note (for piano roll preview)
let previewVoice: Voice | null = null;

export function previewSynthNote(patch: SynthPatch, pitch: number): void {
  // Stop previous preview
  stopSynthPreview();

  const ctx = getAudioContext();
  const now = ctx.currentTime;

  // Create master gain
  const masterGain = new GainNode(ctx, { gain: patch.masterVolume });

  // Create filter if enabled
  let filterNode: BiquadFilterNode | null = null;
  let lastNode: AudioNode = masterGain;

  if (patch.filter.enabled) {
    filterNode = new BiquadFilterNode(ctx, {
      type: patch.filter.type,
      frequency: patch.filter.cutoff,
      Q: patch.filter.resonance,
    });
    filterNode.connect(masterGain);
    lastNode = filterNode;
  }

  // Create envelope
  const envelopeGain = new GainNode(ctx, { gain: 0 });
  envelopeGain.connect(lastNode);

  const oscillators: OscillatorNode[] = [];
  const gainNodes: GainNode[] = [];

  for (const oscSettings of patch.oscillators) {
    if (!oscSettings.enabled) continue;

    const semitoneOffset = oscSettings.coarse + oscSettings.fine / 100;
    const frequency = midiToFrequency(pitch + semitoneOffset);

    const osc = new OscillatorNode(ctx, {
      type: oscSettings.waveform,
      frequency,
    });

    const oscGain = new GainNode(ctx, { gain: oscSettings.volume });
    osc.connect(oscGain);
    oscGain.connect(envelopeGain);

    oscillators.push(osc);
    gainNodes.push(oscGain);
  }

  masterGain.connect(ctx.destination);

  // Quick attack for preview
  const { attack, decay, sustain } = patch.envelope;
  envelopeGain.gain.setValueAtTime(0, now);
  envelopeGain.gain.linearRampToValueAtTime(1, now + Math.min(attack, 0.05));
  envelopeGain.gain.linearRampToValueAtTime(sustain, now + Math.min(attack, 0.05) + decay);

  for (const osc of oscillators) {
    osc.start(now);
  }

  previewVoice = {
    oscillators,
    gainNodes,
    envelope: envelopeGain,
    filter: filterNode,
    noteId: 'preview',
  };

  // Auto-stop after 1 second
  setTimeout(() => {
    if (previewVoice?.noteId === 'preview') {
      stopSynthPreview();
    }
  }, 1000);
}

export function stopSynthPreview(): void {
  if (!previewVoice) return;

  const ctx = getAudioContext();
  const now = ctx.currentTime;

  // Quick release
  previewVoice.envelope.gain.cancelScheduledValues(now);
  previewVoice.envelope.gain.setValueAtTime(previewVoice.envelope.gain.value, now);
  previewVoice.envelope.gain.linearRampToValueAtTime(0.0001, now + 0.05);

  for (const osc of previewVoice.oscillators) {
    osc.stop(now + 0.1);
  }

  previewVoice = null;
}

// Stop all synth playback
export function stopAllSynths(): void {
  stopSynthPreview();

  const ctx = getAudioContext();
  const now = ctx.currentTime;

  for (const voice of activeVoices.values()) {
    voice.envelope.gain.cancelScheduledValues(now);
    voice.envelope.gain.setValueAtTime(voice.envelope.gain.value, now);
    voice.envelope.gain.linearRampToValueAtTime(0.0001, now + 0.02);

    for (const osc of voice.oscillators) {
      osc.stop(now + 0.05);
    }
  }
  activeVoices.clear();
}

// Cleanup on exit
export function closeSynth(): void {
  stopAllSynths();
  if (audioContext) {
    audioContext.close();
    audioContext = null;
  }
}
