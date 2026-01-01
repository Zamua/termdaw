import { createContext, useContext, useState, useRef, useCallback, useEffect, type ReactNode } from 'react';
import { playSample, playSamplePitched, getSamplePath } from '../lib/audio.js';
import { playSynthNote, defaultPatch, presets as synthPresets, type SynthPatch } from '../lib/synth.js';

export type { SynthPatch };
export { synthPresets };
export type ChannelType = 'sample' | 'synth';

interface Channel {
  name: string;
  type: ChannelType;
  sample: string;           // Used when type === 'sample'
  synthPatch: SynthPatch;   // Used when type === 'synth'
  muted: boolean;
  solo: boolean;
  volume: number;
}

export interface Note {
  id: string;
  pitch: number;        // 0-127 (MIDI standard, C0=0, C4=60)
  startStep: number;    // 0-15 for 16-step pattern
  duration: number;     // 1-16 steps
}

interface Pattern {
  id: number;
  name: string;
  steps: boolean[][];   // [channelIndex][stepIndex] - drum steps
  notes: Note[][];      // [channelIndex][noteIndex] - piano roll notes
}

const NUM_STEPS = 16;
const NUM_CHANNELS = 99;

const createEmptyChannel = (index: number): Channel => ({
  name: `Ch ${index + 1}`,
  type: 'sample',
  sample: '',  // Empty sample = empty channel
  synthPatch: defaultPatch,
  muted: false,
  solo: false,
  volume: 100,
});

const createDefaultChannels = (): Channel[] => {
  const defaults: Channel[] = [
    { name: 'Kick', type: 'sample', sample: 'Kits/Acoustic/kick.wav', synthPatch: defaultPatch, muted: false, solo: false, volume: 100 },
    { name: 'Snare', type: 'sample', sample: 'Kits/Acoustic/snare.wav', synthPatch: defaultPatch, muted: false, solo: false, volume: 100 },
    { name: 'HiHat', type: 'sample', sample: 'Kits/Acoustic/hihat-closed.wav', synthPatch: defaultPatch, muted: false, solo: false, volume: 100 },
    { name: 'OpenHat', type: 'sample', sample: 'Kits/Acoustic/hihat-open.wav', synthPatch: defaultPatch, muted: false, solo: false, volume: 100 },
    { name: 'Crash', type: 'sample', sample: 'Kits/Acoustic/crash.wav', synthPatch: defaultPatch, muted: false, solo: false, volume: 100 },
    { name: 'Tom Hi', type: 'sample', sample: 'Kits/Acoustic/tom-high.wav', synthPatch: defaultPatch, muted: false, solo: false, volume: 100 },
    { name: 'Synth 1', type: 'synth', sample: '', synthPatch: defaultPatch, muted: false, solo: false, volume: 100 },
    { name: 'Synth 2', type: 'synth', sample: '', synthPatch: { ...defaultPatch, name: 'Bass', oscillators: [{ enabled: true, waveform: 'sawtooth', coarse: -12, fine: 0, volume: 0.6 }, { enabled: true, waveform: 'square', coarse: -12, fine: -10, volume: 0.4 }, { enabled: false, waveform: 'sine', coarse: 0, fine: 0, volume: 0 }] }, muted: false, solo: false, volume: 100 },
  ];
  // Fill remaining slots with empty channels
  for (let i = defaults.length; i < NUM_CHANNELS; i++) {
    defaults.push(createEmptyChannel(i));
  }
  return defaults;
};

const createEmptyPattern = (id: number): Pattern => ({
  id,
  name: `Pattern ${id}`,
  steps: Array.from({ length: NUM_CHANNELS }, () => Array(NUM_STEPS).fill(false)),
  notes: Array.from({ length: NUM_CHANNELS }, () => []),
});

// Channel with steps and notes for the current pattern (used by UI)
interface ChannelWithSteps extends Channel {
  steps: boolean[];
  notes: Note[];
}

interface SequencerContextType {
  channels: ChannelWithSteps[];
  setChannels: React.Dispatch<React.SetStateAction<Channel[]>>;
  isPlaying: boolean;
  setIsPlaying: (playing: boolean) => void;
  playheadStep: number;
  bpm: number;
  setBpm: (bpm: number) => void;
  toggleStep: (channelIndex: number, stepIndex: number) => void;
  toggleMute: (channelIndex: number) => void;
  cycleMuteState: (channelIndex: number) => void;
  setChannelSample: (channelIndex: number, samplePath: string) => void;
  clearChannel: (channelIndex: number) => void;
  clearStepRange: (channelIndex: number, startStep: number, endStep: number) => void;
  setStepsAt: (channelIndex: number, startStep: number, steps: boolean[]) => void;
  // Pattern management
  patterns: Pattern[];
  currentPatternId: number;
  switchPattern: (patternId: number) => void;
  createPattern: () => number;
  // Piano roll note management
  selectedChannel: number;
  setSelectedChannel: (channelIndex: number) => void;
  addNote: (channelIndex: number, pitch: number, startStep: number, duration: number) => void;
  removeNote: (channelIndex: number, noteId: string) => void;
  updateNote: (channelIndex: number, noteId: string, updates: Partial<Pick<Note, 'startStep' | 'duration' | 'pitch'>>) => void;
  toggleNote: (channelIndex: number, pitch: number, startStep: number, duration: number) => void;
  // Synth management
  setChannelType: (channelIndex: number, type: ChannelType) => void;
  setChannelSynthPatch: (channelIndex: number, patch: SynthPatch) => void;
}

const SequencerContext = createContext<SequencerContextType | null>(null);

export function SequencerProvider({ children }: { children: ReactNode }) {
  const [channelMeta, setChannelMeta] = useState<Channel[]>(createDefaultChannels);
  const [patterns, setPatterns] = useState<Pattern[]>([createEmptyPattern(1)]);
  const [currentPatternId, setCurrentPatternId] = useState(1);
  const [isPlaying, setIsPlayingState] = useState(false);
  const [playheadStep, setPlayheadStep] = useState(0);
  const [bpm, setBpm] = useState(140);
  const [selectedChannel, setSelectedChannel] = useState(0);

  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const bpmRef = useRef(bpm);

  // Keep bpmRef in sync
  useEffect(() => {
    bpmRef.current = bpm;
  }, [bpm]);

  // Get current pattern
  const currentPattern = patterns.find(p => p.id === currentPatternId) || patterns[0];

  // Combine channel metadata with current pattern's steps and notes
  const channels: ChannelWithSteps[] = channelMeta.map((channel, idx) => ({
    ...channel,
    steps: currentPattern?.steps?.[idx] || Array(NUM_STEPS).fill(false),
    notes: currentPattern?.notes?.[idx] || [],
  }));

  // Keep ref in sync for playback
  const channelsRef = useRef(channels);
  useEffect(() => {
    channelsRef.current = channels;
  }, [channels]);

  const playStep = useCallback((step: number, stepsPerBeat: number = 4) => {
    const currentChannels = channelsRef.current;
    const hasSolo = currentChannels.some(ch => ch.solo);
    // Calculate step duration in seconds for synth notes
    const stepDuration = 60 / bpmRef.current / stepsPerBeat;

    for (const channel of currentChannels) {
      const shouldPlay = hasSolo ? channel.solo : !channel.muted;
      if (!shouldPlay) continue;

      if (channel.type === 'sample') {
        // Play drum step if active (sample channels)
        if (channel.steps[step]) {
          playSample(getSamplePath(channel.sample));
        }

        // Play piano roll notes with pitch-shifted samples
        const notes = channel.notes || [];
        for (const note of notes) {
          if (note.startStep === step) {
            playSamplePitched(getSamplePath(channel.sample), note.pitch);
          }
        }
      } else if (channel.type === 'synth') {
        // Play piano roll notes with synth
        const notes = channel.notes || [];
        for (const note of notes) {
          if (note.startStep === step) {
            const noteDuration = note.duration * stepDuration;
            playSynthNote(channel.synthPatch, note.pitch, noteDuration);
          }
        }
      }
    }
  }, []);

  const setIsPlaying = useCallback((playing: boolean) => {
    setIsPlayingState(playing);

    if (playing) {
      const intervalMs = (60 / bpm / 4) * 1000;
      playStep(playheadStep);

      intervalRef.current = setInterval(() => {
        setPlayheadStep(prev => {
          const nextStep = (prev + 1) % NUM_STEPS;
          playStep(nextStep);
          return nextStep;
        });
      }, intervalMs);
    } else {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    }
  }, [bpm, playheadStep, playStep]);

  // Update interval when BPM changes during playback
  useEffect(() => {
    if (isPlaying && intervalRef.current) {
      clearInterval(intervalRef.current);
      const intervalMs = (60 / bpm / 4) * 1000;
      intervalRef.current = setInterval(() => {
        setPlayheadStep(prev => {
          const nextStep = (prev + 1) % NUM_STEPS;
          playStep(nextStep);
          return nextStep;
        });
      }, intervalMs);
    }
  }, [bpm, isPlaying, playStep]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
    };
  }, []);

  // Toggle step in current pattern
  const toggleStep = useCallback((channelIndex: number, stepIndex: number) => {
    setPatterns(prev => prev.map(pattern => {
      if (pattern.id !== currentPatternId) return pattern;
      const newSteps = pattern.steps.map((channelSteps, idx) => {
        if (idx !== channelIndex) return channelSteps;
        const updated = [...channelSteps];
        updated[stepIndex] = !updated[stepIndex];
        return updated;
      });
      return { ...pattern, steps: newSteps };
    }));
  }, [currentPatternId]);

  const toggleMute = useCallback((channelIndex: number) => {
    setChannelMeta(prev => prev.map((channel, idx) => {
      if (idx !== channelIndex) return channel;
      return { ...channel, muted: !channel.muted };
    }));
  }, []);

  const cycleMuteState = useCallback((channelIndex: number) => {
    setChannelMeta(prev => {
      const channel = prev[channelIndex];
      if (!channel) return prev;

      let nextMuted = false;
      let nextSolo = false;

      if (!channel.muted && !channel.solo) {
        nextMuted = true;
      } else if (channel.muted && !channel.solo) {
        nextSolo = true;
      }

      return prev.map((ch, idx) => {
        if (idx === channelIndex) {
          return { ...ch, muted: nextMuted, solo: nextSolo };
        }
        if (nextSolo && ch.solo) {
          return { ...ch, solo: false };
        }
        return ch;
      });
    });
  }, []);

  const setChannelSample = useCallback((channelIndex: number, samplePath: string) => {
    const name = samplePath.split('/').pop()?.replace(/\.[^/.]+$/, '') || 'Sample';
    setChannelMeta(prev => prev.map((channel, idx) => {
      if (idx !== channelIndex) return channel;
      return { ...channel, sample: samplePath, name };
    }));
  }, []);

  // Clear channel in current pattern
  const clearChannel = useCallback((channelIndex: number) => {
    setPatterns(prev => prev.map(pattern => {
      if (pattern.id !== currentPatternId) return pattern;
      const newSteps = pattern.steps.map((channelSteps, idx) => {
        if (idx !== channelIndex) return channelSteps;
        return Array(NUM_STEPS).fill(false);
      });
      return { ...pattern, steps: newSteps };
    }));
  }, [currentPatternId]);

  // Clear a range of steps in current pattern
  const clearStepRange = useCallback((channelIndex: number, startStep: number, endStep: number) => {
    const minStep = Math.min(startStep, endStep);
    const maxStep = Math.max(startStep, endStep);
    setPatterns(prev => prev.map(pattern => {
      if (pattern.id !== currentPatternId) return pattern;
      const newSteps = pattern.steps.map((channelSteps, idx) => {
        if (idx !== channelIndex) return channelSteps;
        const updated = [...channelSteps];
        for (let i = minStep; i <= maxStep; i++) {
          updated[i] = false;
        }
        return updated;
      });
      return { ...pattern, steps: newSteps };
    }));
  }, [currentPatternId]);

  // Set steps at a position in current pattern (for paste)
  const setStepsAt = useCallback((channelIndex: number, startStep: number, steps: boolean[]) => {
    setPatterns(prev => prev.map(pattern => {
      if (pattern.id !== currentPatternId) return pattern;
      const newSteps = pattern.steps.map((channelSteps, idx) => {
        if (idx !== channelIndex) return channelSteps;
        const updated = [...channelSteps];
        for (let i = 0; i < steps.length && startStep + i < NUM_STEPS; i++) {
          updated[startStep + i] = steps[i] as boolean;
        }
        return updated;
      });
      return { ...pattern, steps: newSteps };
    }));
  }, [currentPatternId]);

  // Switch to a pattern (creates it if it doesn't exist)
  const switchPattern = useCallback((patternId: number) => {
    setPatterns(prev => {
      const exists = prev.some(p => p.id === patternId);
      if (!exists) {
        return [...prev, createEmptyPattern(patternId)];
      }
      return prev;
    });
    setCurrentPatternId(patternId);
  }, []);

  // Create a new pattern and return its ID
  const createPattern = useCallback(() => {
    const maxId = Math.max(...patterns.map(p => p.id), 0);
    const newId = maxId + 1;
    setPatterns(prev => [...prev, createEmptyPattern(newId)]);
    setCurrentPatternId(newId);
    return newId;
  }, [patterns]);

  // Wrapper for setChannels that updates channelMeta
  const setChannels: React.Dispatch<React.SetStateAction<Channel[]>> = useCallback((action) => {
    if (typeof action === 'function') {
      setChannelMeta(prev => action(prev));
    } else {
      setChannelMeta(action);
    }
  }, []);

  // Add a note to a channel in the current pattern
  const addNote = useCallback((channelIndex: number, pitch: number, startStep: number, duration: number) => {
    const noteId = `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
    setPatterns(prev => prev.map(pattern => {
      if (pattern.id !== currentPatternId) return pattern;
      const newNotes = pattern.notes.map((channelNotes, idx) => {
        if (idx !== channelIndex) return channelNotes;
        return [...channelNotes, { id: noteId, pitch, startStep, duration }];
      });
      return { ...pattern, notes: newNotes };
    }));
  }, [currentPatternId]);

  // Remove a note from a channel in the current pattern
  const removeNote = useCallback((channelIndex: number, noteId: string) => {
    setPatterns(prev => prev.map(pattern => {
      if (pattern.id !== currentPatternId) return pattern;
      const newNotes = pattern.notes.map((channelNotes, idx) => {
        if (idx !== channelIndex) return channelNotes;
        return channelNotes.filter(note => note.id !== noteId);
      });
      return { ...pattern, notes: newNotes };
    }));
  }, [currentPatternId]);

  // Update a note's properties (for nudging, resizing, etc.)
  const updateNote = useCallback((channelIndex: number, noteId: string, updates: Partial<Pick<Note, 'startStep' | 'duration' | 'pitch'>>) => {
    setPatterns(prev => prev.map(pattern => {
      if (pattern.id !== currentPatternId) return pattern;
      const newNotes = pattern.notes.map((channelNotes, idx) => {
        if (idx !== channelIndex) return channelNotes;
        return channelNotes.map(note => {
          if (note.id !== noteId) return note;
          return { ...note, ...updates };
        });
      });
      return { ...pattern, notes: newNotes };
    }));
  }, [currentPatternId]);

  // Toggle a note at position (add if not exists, remove if exists)
  const toggleNote = useCallback((channelIndex: number, pitch: number, startStep: number, duration: number) => {
    setPatterns(prev => prev.map(pattern => {
      if (pattern.id !== currentPatternId) return pattern;

      // Ensure notes array exists (for backwards compatibility)
      const patternNotes = pattern.notes || Array.from({ length: NUM_CHANNELS }, () => []);
      const channelNotes = patternNotes[channelIndex] || [];

      // Find existing note at this exact position
      const existingNote = channelNotes.find(n => n.pitch === pitch && n.startStep === startStep);

      const newNotes = patternNotes.map((notes, idx) => {
        if (idx !== channelIndex) return notes || [];
        if (existingNote) {
          // Remove the note
          return (notes || []).filter(n => n.id !== existingNote.id);
        } else {
          // Add a new note
          const noteId = `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
          return [...(notes || []), { id: noteId, pitch, startStep, duration }];
        }
      });
      return { ...pattern, notes: newNotes };
    }));
  }, [currentPatternId]);

  // Set channel type (sample vs synth)
  const setChannelType = useCallback((channelIndex: number, type: ChannelType) => {
    setChannelMeta(prev => prev.map((channel, idx) => {
      if (idx !== channelIndex) return channel;
      return { ...channel, type };
    }));
  }, []);

  // Set channel synth patch
  const setChannelSynthPatch = useCallback((channelIndex: number, patch: SynthPatch) => {
    setChannelMeta(prev => prev.map((channel, idx) => {
      if (idx !== channelIndex) return channel;
      return { ...channel, synthPatch: patch };
    }));
  }, []);

  return (
    <SequencerContext.Provider value={{
      channels,
      setChannels,
      isPlaying,
      setIsPlaying,
      playheadStep,
      bpm,
      setBpm,
      toggleStep,
      toggleMute,
      cycleMuteState,
      setChannelSample,
      clearChannel,
      clearStepRange,
      setStepsAt,
      patterns,
      currentPatternId,
      switchPattern,
      createPattern,
      selectedChannel,
      setSelectedChannel,
      addNote,
      removeNote,
      updateNote,
      toggleNote,
      setChannelType,
      setChannelSynthPatch,
    }}>
      {children}
    </SequencerContext.Provider>
  );
}

export function useSequencer() {
  const context = useContext(SequencerContext);
  if (!context) {
    throw new Error('useSequencer must be used within SequencerProvider');
  }
  return context;
}
