import { createContext, useContext, useCallback, useSyncExternalStore, type ReactNode } from 'react';
import {
  commandManager,
  type Command,
  type SequencerStateAccessors,
  ToggleStepCommand,
  SetStepsCommand,
  ClearStepRangeCommand,
  ClearChannelCommand,
  ToggleMuteCommand,
  CycleMuteStateCommand,
  SetChannelSampleCommand,
  AddNoteCommand,
  RemoveNoteCommand,
  UpdateNoteCommand,
  ToggleNoteCommand,
} from '../lib/commands/index.js';

// Local type to avoid circular dependency with SequencerContext
interface NoteUpdates {
  startStep?: number;
  duration?: number;
  pitch?: number;
}

interface CommandContextType {
  // Core command operations
  execute: (command: Command) => void;
  undo: () => boolean;
  redo: () => boolean;
  canUndo: boolean;
  canRedo: boolean;

  // State accessors for creating commands
  stateAccessors: SequencerStateAccessors | null;
  setStateAccessors: (accessors: SequencerStateAccessors) => void;

  // Convenience methods for common commands
  toggleStep: (patternId: number, channelIndex: number, stepIndex: number) => void;
  setSteps: (patternId: number, channelIndex: number, startStep: number, steps: boolean[]) => void;
  clearStepRange: (patternId: number, channelIndex: number, startStep: number, endStep: number) => void;
  clearChannel: (patternId: number, channelIndex: number) => void;
  toggleMute: (channelIndex: number) => void;
  cycleMuteState: (channelIndex: number) => void;
  setChannelSample: (channelIndex: number, samplePath: string) => void;
  addNote: (patternId: number, channelIndex: number, pitch: number, startStep: number, duration: number) => void;
  removeNote: (patternId: number, channelIndex: number, noteId: string) => void;
  updateNote: (patternId: number, channelIndex: number, noteId: string, updates: NoteUpdates) => void;
  toggleNote: (patternId: number, channelIndex: number, pitch: number, startStep: number, duration: number) => void;
}

const CommandContext = createContext<CommandContextType | null>(null);

// Store for state accessors (updated by SequencerProvider)
let currentStateAccessors: SequencerStateAccessors | null = null;

export function CommandProvider({ children }: { children: ReactNode }) {
  // Subscribe to command manager for canUndo/canRedo updates
  const canUndo = useSyncExternalStore(
    commandManager.subscribe.bind(commandManager),
    commandManager.canUndo.bind(commandManager),
    commandManager.canUndo.bind(commandManager)
  );

  const canRedo = useSyncExternalStore(
    commandManager.subscribe.bind(commandManager),
    commandManager.canRedo.bind(commandManager),
    commandManager.canRedo.bind(commandManager)
  );

  const setStateAccessors = useCallback((accessors: SequencerStateAccessors) => {
    currentStateAccessors = accessors;
  }, []);

  const execute = useCallback((command: Command) => {
    commandManager.execute(command);
  }, []);

  const undo = useCallback(() => {
    return commandManager.undo();
  }, []);

  const redo = useCallback(() => {
    return commandManager.redo();
  }, []);

  // Convenience methods that create and execute commands
  const toggleStep = useCallback((patternId: number, channelIndex: number, stepIndex: number) => {
    if (!currentStateAccessors) return;
    execute(new ToggleStepCommand(currentStateAccessors, patternId, channelIndex, stepIndex));
  }, [execute]);

  const setSteps = useCallback((patternId: number, channelIndex: number, startStep: number, steps: boolean[]) => {
    if (!currentStateAccessors) return;
    execute(new SetStepsCommand(currentStateAccessors, patternId, channelIndex, startStep, steps));
  }, [execute]);

  const clearStepRange = useCallback((patternId: number, channelIndex: number, startStep: number, endStep: number) => {
    if (!currentStateAccessors) return;
    execute(new ClearStepRangeCommand(currentStateAccessors, patternId, channelIndex, startStep, endStep));
  }, [execute]);

  const clearChannel = useCallback((patternId: number, channelIndex: number) => {
    if (!currentStateAccessors) return;
    execute(new ClearChannelCommand(currentStateAccessors, patternId, channelIndex));
  }, [execute]);

  const toggleMute = useCallback((channelIndex: number) => {
    if (!currentStateAccessors) return;
    execute(new ToggleMuteCommand(currentStateAccessors, channelIndex));
  }, [execute]);

  const cycleMuteState = useCallback((channelIndex: number) => {
    if (!currentStateAccessors) return;
    execute(new CycleMuteStateCommand(currentStateAccessors, channelIndex));
  }, [execute]);

  const setChannelSample = useCallback((channelIndex: number, samplePath: string) => {
    if (!currentStateAccessors) return;
    execute(new SetChannelSampleCommand(currentStateAccessors, channelIndex, samplePath));
  }, [execute]);

  const addNote = useCallback((patternId: number, channelIndex: number, pitch: number, startStep: number, duration: number) => {
    if (!currentStateAccessors) return;
    execute(new AddNoteCommand(currentStateAccessors, patternId, channelIndex, pitch, startStep, duration));
  }, [execute]);

  const removeNote = useCallback((patternId: number, channelIndex: number, noteId: string) => {
    if (!currentStateAccessors) return;
    execute(new RemoveNoteCommand(currentStateAccessors, patternId, channelIndex, noteId));
  }, [execute]);

  const updateNote = useCallback((patternId: number, channelIndex: number, noteId: string, updates: NoteUpdates) => {
    if (!currentStateAccessors) return;
    execute(new UpdateNoteCommand(currentStateAccessors, patternId, channelIndex, noteId, updates));
  }, [execute]);

  const toggleNote = useCallback((patternId: number, channelIndex: number, pitch: number, startStep: number, duration: number) => {
    if (!currentStateAccessors) return;
    execute(new ToggleNoteCommand(currentStateAccessors, patternId, channelIndex, pitch, startStep, duration));
  }, [execute]);

  return (
    <CommandContext.Provider
      value={{
        execute,
        undo,
        redo,
        canUndo,
        canRedo,
        stateAccessors: currentStateAccessors,
        setStateAccessors,
        toggleStep,
        setSteps,
        clearStepRange,
        clearChannel,
        toggleMute,
        cycleMuteState,
        setChannelSample,
        addNote,
        removeNote,
        updateNote,
        toggleNote,
      }}
    >
      {children}
    </CommandContext.Provider>
  );
}

export function useCommands() {
  const context = useContext(CommandContext);
  if (!context) {
    throw new Error('useCommands must be used within CommandProvider');
  }
  return context;
}
