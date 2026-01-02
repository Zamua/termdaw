import type { Command, CommandContext, CommandPosition } from './Command.js';
import type { SynthPatch } from '../synth.js';

// Types matching SequencerContext (defined here to avoid circular deps)
type ChannelType = 'sample' | 'synth';

interface Note {
  id: string;
  pitch: number;
  startStep: number;
  duration: number;
}

interface Pattern {
  id: number;
  name: string;
  steps: boolean[][];
  notes: Note[][];
}

interface Channel {
  name: string;
  type: ChannelType;
  sample: string;
  synthPatch: SynthPatch;
  muted: boolean;
  solo: boolean;
  volume: number;
}

// State accessors passed to commands
export interface SequencerStateAccessors {
  getPatterns: () => Pattern[];
  setPatterns: React.Dispatch<React.SetStateAction<Pattern[]>>;
  getCurrentPatternId: () => number;
  getChannelMeta: () => Channel[];
  setChannelMeta: React.Dispatch<React.SetStateAction<Channel[]>>;
}

// Optional cursor info for undo/redo navigation
export interface CommandCursorInfo {
  context: CommandContext;
  position: CommandPosition;
}

/**
 * Toggle a step on/off in the channel rack.
 */
export class ToggleStepCommand implements Command {
  readonly type = 'toggleStep';
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  private previousValue: boolean | null = null;

  constructor(
    private state: SequencerStateAccessors,
    private patternId: number,
    private channelIndex: number,
    private stepIndex: number,
    cursorInfo?: CommandCursorInfo
  ) {
    this.description = `Toggle step ${stepIndex + 1} on channel ${channelIndex + 1}`;
    this.context = cursorInfo?.context;
    this.position = cursorInfo?.position;
  }

  execute(): void {
    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const channelSteps = pattern.steps[this.channelIndex];
        if (!channelSteps) return pattern;

        // Capture previous value on first execute
        if (this.previousValue === null) {
          this.previousValue = channelSteps[this.stepIndex] ?? false;
        }

        const newSteps = pattern.steps.map((steps, idx) => {
          if (idx !== this.channelIndex) return steps;
          const updated = [...steps];
          updated[this.stepIndex] = !this.previousValue;
          return updated;
        });
        return { ...pattern, steps: newSteps };
      })
    );
  }

  undo(): void {
    if (this.previousValue === null) return;

    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const newSteps = pattern.steps.map((steps, idx) => {
          if (idx !== this.channelIndex) return steps;
          const updated = [...steps];
          updated[this.stepIndex] = this.previousValue!;
          return updated;
        });
        return { ...pattern, steps: newSteps };
      })
    );
  }
}

/**
 * Set steps at a position (for paste operations).
 */
export class SetStepsCommand implements Command {
  readonly type = 'setSteps';
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  private previousSteps: boolean[] | null = null;

  constructor(
    private state: SequencerStateAccessors,
    private patternId: number,
    private channelIndex: number,
    private startStep: number,
    private steps: boolean[],
    cursorInfo?: CommandCursorInfo
  ) {
    this.description = `Paste ${steps.filter(Boolean).length} steps`;
    this.context = cursorInfo?.context;
    this.position = cursorInfo?.position;
  }

  execute(): void {
    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const channelSteps = pattern.steps[this.channelIndex];
        if (!channelSteps) return pattern;

        // Capture previous values on first execute
        if (this.previousSteps === null) {
          this.previousSteps = channelSteps.slice(
            this.startStep,
            this.startStep + this.steps.length
          );
        }

        const newSteps = pattern.steps.map((steps, idx) => {
          if (idx !== this.channelIndex) return steps;
          const updated = [...steps];
          for (let i = 0; i < this.steps.length && this.startStep + i < updated.length; i++) {
            updated[this.startStep + i] = this.steps[i]!;
          }
          return updated;
        });
        return { ...pattern, steps: newSteps };
      })
    );
  }

  undo(): void {
    if (this.previousSteps === null) return;

    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const newSteps = pattern.steps.map((steps, idx) => {
          if (idx !== this.channelIndex) return steps;
          const updated = [...steps];
          for (let i = 0; i < this.previousSteps!.length && this.startStep + i < updated.length; i++) {
            updated[this.startStep + i] = this.previousSteps![i]!;
          }
          return updated;
        });
        return { ...pattern, steps: newSteps };
      })
    );
  }
}

/**
 * Clear a range of steps (for delete operations).
 */
export class ClearStepRangeCommand implements Command {
  readonly type = 'clearStepRange';
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  private previousSteps: boolean[] | null = null;

  constructor(
    private state: SequencerStateAccessors,
    private patternId: number,
    private channelIndex: number,
    private startStep: number,
    private endStep: number,
    cursorInfo?: CommandCursorInfo
  ) {
    const count = Math.abs(endStep - startStep) + 1;
    this.description = `Delete ${count} step${count > 1 ? 's' : ''}`;
    this.context = cursorInfo?.context;
    this.position = cursorInfo?.position;
  }

  execute(): void {
    const minStep = Math.min(this.startStep, this.endStep);
    const maxStep = Math.max(this.startStep, this.endStep);

    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const channelSteps = pattern.steps[this.channelIndex];
        if (!channelSteps) return pattern;

        // Capture previous values on first execute
        if (this.previousSteps === null) {
          this.previousSteps = channelSteps.slice(minStep, maxStep + 1);
        }

        const newSteps = pattern.steps.map((steps, idx) => {
          if (idx !== this.channelIndex) return steps;
          const updated = [...steps];
          for (let i = minStep; i <= maxStep; i++) {
            updated[i] = false;
          }
          return updated;
        });
        return { ...pattern, steps: newSteps };
      })
    );
  }

  undo(): void {
    if (this.previousSteps === null) return;

    const minStep = Math.min(this.startStep, this.endStep);

    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const newSteps = pattern.steps.map((steps, idx) => {
          if (idx !== this.channelIndex) return steps;
          const updated = [...steps];
          for (let i = 0; i < this.previousSteps!.length; i++) {
            updated[minStep + i] = this.previousSteps![i]!;
          }
          return updated;
        });
        return { ...pattern, steps: newSteps };
      })
    );
  }
}

/**
 * Clear all steps in a channel.
 */
export class ClearChannelCommand implements Command {
  readonly type = 'clearChannel';
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  private previousSteps: boolean[] | null = null;

  constructor(
    private state: SequencerStateAccessors,
    private patternId: number,
    private channelIndex: number,
    cursorInfo?: CommandCursorInfo
  ) {
    this.description = `Clear channel ${channelIndex + 1}`;
    this.context = cursorInfo?.context;
    this.position = cursorInfo?.position;
  }

  execute(): void {
    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const channelSteps = pattern.steps[this.channelIndex];
        if (!channelSteps) return pattern;

        // Capture previous values on first execute
        if (this.previousSteps === null) {
          this.previousSteps = [...channelSteps];
        }

        const newSteps = pattern.steps.map((steps, idx) => {
          if (idx !== this.channelIndex) return steps;
          return steps.map(() => false);
        });
        return { ...pattern, steps: newSteps };
      })
    );
  }

  undo(): void {
    if (this.previousSteps === null) return;

    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const newSteps = pattern.steps.map((steps, idx) => {
          if (idx !== this.channelIndex) return steps;
          return [...this.previousSteps!];
        });
        return { ...pattern, steps: newSteps };
      })
    );
  }
}

/**
 * Toggle mute state on a channel.
 */
export class ToggleMuteCommand implements Command {
  readonly type = 'toggleMute';
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  private previousMuted: boolean | null = null;

  constructor(
    private state: SequencerStateAccessors,
    private channelIndex: number,
    cursorInfo?: CommandCursorInfo
  ) {
    this.description = `Toggle mute on channel ${channelIndex + 1}`;
    this.context = cursorInfo?.context;
    this.position = cursorInfo?.position;
  }

  execute(): void {
    this.state.setChannelMeta(prev =>
      prev.map((channel, idx) => {
        if (idx !== this.channelIndex) return channel;

        // Capture previous value on first execute
        if (this.previousMuted === null) {
          this.previousMuted = channel.muted;
        }

        return { ...channel, muted: !this.previousMuted };
      })
    );
  }

  undo(): void {
    if (this.previousMuted === null) return;

    this.state.setChannelMeta(prev =>
      prev.map((channel, idx) => {
        if (idx !== this.channelIndex) return channel;
        return { ...channel, muted: this.previousMuted! };
      })
    );
  }
}

/**
 * Cycle mute state (unmuted -> muted -> solo -> unmuted).
 */
export class CycleMuteStateCommand implements Command {
  readonly type = 'cycleMuteState';
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  private previousState: { muted: boolean; solo: boolean } | null = null;
  private previousSoloChannel: number | null = null;

  constructor(
    private state: SequencerStateAccessors,
    private channelIndex: number,
    cursorInfo?: CommandCursorInfo
  ) {
    this.description = `Cycle mute state on channel ${channelIndex + 1}`;
    this.context = cursorInfo?.context;
    this.position = cursorInfo?.position;
  }

  execute(): void {
    this.state.setChannelMeta(prev => {
      const channel = prev[this.channelIndex];
      if (!channel) return prev;

      // Capture previous state on first execute
      if (this.previousState === null) {
        this.previousState = { muted: channel.muted, solo: channel.solo };
        // Find any other solo channel
        const soloIdx = prev.findIndex((ch, idx) => idx !== this.channelIndex && ch.solo);
        this.previousSoloChannel = soloIdx >= 0 ? soloIdx : null;
      }

      let nextMuted = false;
      let nextSolo = false;

      if (!channel.muted && !channel.solo) {
        nextMuted = true;
      } else if (channel.muted && !channel.solo) {
        nextSolo = true;
      }

      return prev.map((ch, idx) => {
        if (idx === this.channelIndex) {
          return { ...ch, muted: nextMuted, solo: nextSolo };
        }
        // If this channel is going solo, unsolo others
        if (nextSolo && ch.solo) {
          return { ...ch, solo: false };
        }
        return ch;
      });
    });
  }

  undo(): void {
    if (this.previousState === null) return;

    this.state.setChannelMeta(prev =>
      prev.map((ch, idx) => {
        if (idx === this.channelIndex) {
          return { ...ch, muted: this.previousState!.muted, solo: this.previousState!.solo };
        }
        // Restore previous solo channel if there was one
        if (idx === this.previousSoloChannel) {
          return { ...ch, solo: true };
        }
        return ch;
      })
    );
  }
}

/**
 * Set channel sample.
 */
export class SetChannelSampleCommand implements Command {
  readonly type = 'setChannelSample';
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  private previousSample: string | null = null;
  private previousName: string | null = null;

  constructor(
    private state: SequencerStateAccessors,
    private channelIndex: number,
    private samplePath: string,
    cursorInfo?: CommandCursorInfo
  ) {
    const name = samplePath.split('/').pop()?.replace(/\.[^/.]+$/, '') || 'Sample';
    this.description = `Set sample to ${name}`;
    this.context = cursorInfo?.context;
    this.position = cursorInfo?.position;
  }

  execute(): void {
    const name = this.samplePath.split('/').pop()?.replace(/\.[^/.]+$/, '') || 'Sample';

    this.state.setChannelMeta(prev =>
      prev.map((channel, idx) => {
        if (idx !== this.channelIndex) return channel;

        // Capture previous values on first execute
        if (this.previousSample === null) {
          this.previousSample = channel.sample;
          this.previousName = channel.name;
        }

        return { ...channel, sample: this.samplePath, name };
      })
    );
  }

  undo(): void {
    if (this.previousSample === null || this.previousName === null) return;

    this.state.setChannelMeta(prev =>
      prev.map((channel, idx) => {
        if (idx !== this.channelIndex) return channel;
        return { ...channel, sample: this.previousSample!, name: this.previousName! };
      })
    );
  }
}

/**
 * Add a note to the piano roll.
 */
export class AddNoteCommand implements Command {
  readonly type = 'addNote';
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  private noteId: string | null = null;

  constructor(
    private state: SequencerStateAccessors,
    private patternId: number,
    private channelIndex: number,
    private pitch: number,
    private startStep: number,
    private duration: number,
    cursorInfo?: CommandCursorInfo
  ) {
    this.description = `Add note at step ${startStep + 1}`;
    this.context = cursorInfo?.context;
    this.position = cursorInfo?.position;
  }

  execute(): void {
    // Generate ID only on first execute
    if (this.noteId === null) {
      this.noteId = `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
    }

    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const newNotes = pattern.notes.map((channelNotes, idx) => {
          if (idx !== this.channelIndex) return channelNotes;
          return [
            ...channelNotes,
            {
              id: this.noteId!,
              pitch: this.pitch,
              startStep: this.startStep,
              duration: this.duration,
            },
          ];
        });
        return { ...pattern, notes: newNotes };
      })
    );
  }

  undo(): void {
    if (this.noteId === null) return;

    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const newNotes = pattern.notes.map((channelNotes, idx) => {
          if (idx !== this.channelIndex) return channelNotes;
          return channelNotes.filter(note => note.id !== this.noteId);
        });
        return { ...pattern, notes: newNotes };
      })
    );
  }
}

/**
 * Remove a note from the piano roll.
 */
export class RemoveNoteCommand implements Command {
  readonly type = 'removeNote';
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  private removedNote: Note | null = null;

  constructor(
    private state: SequencerStateAccessors,
    private patternId: number,
    private channelIndex: number,
    private noteId: string,
    cursorInfo?: CommandCursorInfo
  ) {
    this.description = `Remove note`;
    this.context = cursorInfo?.context;
    this.position = cursorInfo?.position;
  }

  execute(): void {
    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const newNotes = pattern.notes.map((channelNotes, idx) => {
          if (idx !== this.channelIndex) return channelNotes;

          // Capture removed note on first execute
          if (this.removedNote === null) {
            const note = channelNotes.find(n => n.id === this.noteId);
            if (note) {
              this.removedNote = { ...note };
            }
          }

          return channelNotes.filter(note => note.id !== this.noteId);
        });
        return { ...pattern, notes: newNotes };
      })
    );
  }

  undo(): void {
    if (this.removedNote === null) return;

    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const newNotes = pattern.notes.map((channelNotes, idx) => {
          if (idx !== this.channelIndex) return channelNotes;
          return [...channelNotes, this.removedNote!];
        });
        return { ...pattern, notes: newNotes };
      })
    );
  }
}

/**
 * Update a note's properties (position, duration, pitch).
 */
export class UpdateNoteCommand implements Command {
  readonly type = 'updateNote';
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  private previousValues: Partial<Pick<Note, 'startStep' | 'duration' | 'pitch'>> | null = null;

  constructor(
    private state: SequencerStateAccessors,
    private patternId: number,
    private channelIndex: number,
    private noteId: string,
    private updates: Partial<Pick<Note, 'startStep' | 'duration' | 'pitch'>>,
    cursorInfo?: CommandCursorInfo
  ) {
    this.description = `Update note`;
    this.context = cursorInfo?.context;
    this.position = cursorInfo?.position;
  }

  execute(): void {
    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const newNotes = pattern.notes.map((channelNotes, idx) => {
          if (idx !== this.channelIndex) return channelNotes;
          return channelNotes.map(note => {
            if (note.id !== this.noteId) return note;

            // Capture previous values on first execute
            if (this.previousValues === null) {
              this.previousValues = {};
              if ('startStep' in this.updates) this.previousValues.startStep = note.startStep;
              if ('duration' in this.updates) this.previousValues.duration = note.duration;
              if ('pitch' in this.updates) this.previousValues.pitch = note.pitch;
            }

            return { ...note, ...this.updates };
          });
        });
        return { ...pattern, notes: newNotes };
      })
    );
  }

  undo(): void {
    if (this.previousValues === null) return;

    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;
        const newNotes = pattern.notes.map((channelNotes, idx) => {
          if (idx !== this.channelIndex) return channelNotes;
          return channelNotes.map(note => {
            if (note.id !== this.noteId) return note;
            return { ...note, ...this.previousValues! };
          });
        });
        return { ...pattern, notes: newNotes };
      })
    );
  }
}

/**
 * Toggle a note at a position (add if not exists, remove if exists).
 */
export class ToggleNoteCommand implements Command {
  readonly type = 'toggleNote';
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  private addedNoteId: string | null = null;
  private removedNote: Note | null = null;

  constructor(
    private state: SequencerStateAccessors,
    private patternId: number,
    private channelIndex: number,
    private pitch: number,
    private startStep: number,
    private duration: number,
    cursorInfo?: CommandCursorInfo
  ) {
    this.description = `Toggle note at step ${startStep + 1}`;
    this.context = cursorInfo?.context;
    this.position = cursorInfo?.position;
  }

  execute(): void {
    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;

        const patternNotes = pattern.notes || [];
        const channelNotes = patternNotes[this.channelIndex] || [];
        const existingNote = channelNotes.find(
          n => n.pitch === this.pitch && n.startStep === this.startStep
        );

        const newNotes = patternNotes.map((notes, idx) => {
          if (idx !== this.channelIndex) return notes || [];
          if (existingNote) {
            // Remove the note
            if (this.removedNote === null) {
              this.removedNote = { ...existingNote };
            }
            return (notes || []).filter(n => n.id !== existingNote.id);
          } else {
            // Add a new note
            if (this.addedNoteId === null) {
              this.addedNoteId = `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
            }
            return [
              ...(notes || []),
              {
                id: this.addedNoteId,
                pitch: this.pitch,
                startStep: this.startStep,
                duration: this.duration,
              },
            ];
          }
        });
        return { ...pattern, notes: newNotes };
      })
    );
  }

  undo(): void {
    this.state.setPatterns(prev =>
      prev.map(pattern => {
        if (pattern.id !== this.patternId) return pattern;

        const newNotes = pattern.notes.map((notes, idx) => {
          if (idx !== this.channelIndex) return notes;
          if (this.removedNote) {
            // Re-add the removed note
            return [...notes, this.removedNote];
          } else if (this.addedNoteId) {
            // Remove the added note
            return notes.filter(n => n.id !== this.addedNoteId);
          }
          return notes;
        });
        return { ...pattern, notes: newNotes };
      })
    );
  }
}
