import {
  createContext,
  useContext,
  useCallback,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import {
  commandManager,
  type Command,
  type SequencerStateAccessors,
  type UndoRedoResult,
  type CommandCursorInfo,
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
} from "../lib/commands/index.js";

// Local type to avoid circular dependency with SequencerContext
interface NoteUpdates {
  startStep?: number;
  duration?: number;
  pitch?: number;
}

interface CommandContextType {
  // Core command operations
  execute: (command: Command) => void;
  undo: () => UndoRedoResult;
  redo: () => UndoRedoResult;
  canUndo: boolean;
  canRedo: boolean;

  // State accessors for creating commands
  stateAccessors: SequencerStateAccessors | null;
  setStateAccessors: (accessors: SequencerStateAccessors) => void;

  // Convenience methods for common commands (all accept optional cursorInfo for undo/redo navigation)
  toggleStep: (
    patternId: number,
    channelIndex: number,
    stepIndex: number,
    cursorInfo?: CommandCursorInfo,
  ) => void;
  setSteps: (
    patternId: number,
    channelIndex: number,
    startStep: number,
    steps: boolean[],
    cursorInfo?: CommandCursorInfo,
  ) => void;
  clearStepRange: (
    patternId: number,
    channelIndex: number,
    startStep: number,
    endStep: number,
    cursorInfo?: CommandCursorInfo,
  ) => void;
  clearChannel: (
    patternId: number,
    channelIndex: number,
    cursorInfo?: CommandCursorInfo,
  ) => void;
  toggleMute: (channelIndex: number, cursorInfo?: CommandCursorInfo) => void;
  cycleMuteState: (
    channelIndex: number,
    cursorInfo?: CommandCursorInfo,
  ) => void;
  setChannelSample: (
    channelIndex: number,
    samplePath: string,
    cursorInfo?: CommandCursorInfo,
  ) => void;
  addNote: (
    patternId: number,
    channelIndex: number,
    pitch: number,
    startStep: number,
    duration: number,
    cursorInfo?: CommandCursorInfo,
  ) => void;
  removeNote: (
    patternId: number,
    channelIndex: number,
    noteId: string,
    cursorInfo?: CommandCursorInfo,
  ) => void;
  updateNote: (
    patternId: number,
    channelIndex: number,
    noteId: string,
    updates: NoteUpdates,
    cursorInfo?: CommandCursorInfo,
  ) => void;
  toggleNote: (
    patternId: number,
    channelIndex: number,
    pitch: number,
    startStep: number,
    duration: number,
    cursorInfo?: CommandCursorInfo,
  ) => void;
}

const CommandContext = createContext<CommandContextType | null>(null);

// Store for state accessors (updated by SequencerProvider)
let currentStateAccessors: SequencerStateAccessors | null = null;

export function CommandProvider({ children }: { children: ReactNode }) {
  // Subscribe to command manager for canUndo/canRedo updates
  const canUndo = useSyncExternalStore(
    commandManager.subscribe.bind(commandManager),
    commandManager.canUndo.bind(commandManager),
    commandManager.canUndo.bind(commandManager),
  );

  const canRedo = useSyncExternalStore(
    commandManager.subscribe.bind(commandManager),
    commandManager.canRedo.bind(commandManager),
    commandManager.canRedo.bind(commandManager),
  );

  const setStateAccessors = useCallback(
    (accessors: SequencerStateAccessors) => {
      currentStateAccessors = accessors;
    },
    [],
  );

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
  const toggleStep = useCallback(
    (
      patternId: number,
      channelIndex: number,
      stepIndex: number,
      cursorInfo?: CommandCursorInfo,
    ) => {
      if (!currentStateAccessors) return;
      execute(
        new ToggleStepCommand(
          currentStateAccessors,
          patternId,
          channelIndex,
          stepIndex,
          cursorInfo,
        ),
      );
    },
    [execute],
  );

  const setSteps = useCallback(
    (
      patternId: number,
      channelIndex: number,
      startStep: number,
      steps: boolean[],
      cursorInfo?: CommandCursorInfo,
    ) => {
      if (!currentStateAccessors) return;
      execute(
        new SetStepsCommand(
          currentStateAccessors,
          patternId,
          channelIndex,
          startStep,
          steps,
          cursorInfo,
        ),
      );
    },
    [execute],
  );

  const clearStepRange = useCallback(
    (
      patternId: number,
      channelIndex: number,
      startStep: number,
      endStep: number,
      cursorInfo?: CommandCursorInfo,
    ) => {
      if (!currentStateAccessors) return;
      execute(
        new ClearStepRangeCommand(
          currentStateAccessors,
          patternId,
          channelIndex,
          startStep,
          endStep,
          cursorInfo,
        ),
      );
    },
    [execute],
  );

  const clearChannel = useCallback(
    (
      patternId: number,
      channelIndex: number,
      cursorInfo?: CommandCursorInfo,
    ) => {
      if (!currentStateAccessors) return;
      execute(
        new ClearChannelCommand(
          currentStateAccessors,
          patternId,
          channelIndex,
          cursorInfo,
        ),
      );
    },
    [execute],
  );

  const toggleMute = useCallback(
    (channelIndex: number, cursorInfo?: CommandCursorInfo) => {
      if (!currentStateAccessors) return;
      execute(
        new ToggleMuteCommand(currentStateAccessors, channelIndex, cursorInfo),
      );
    },
    [execute],
  );

  const cycleMuteState = useCallback(
    (channelIndex: number, cursorInfo?: CommandCursorInfo) => {
      if (!currentStateAccessors) return;
      execute(
        new CycleMuteStateCommand(
          currentStateAccessors,
          channelIndex,
          cursorInfo,
        ),
      );
    },
    [execute],
  );

  const setChannelSample = useCallback(
    (
      channelIndex: number,
      samplePath: string,
      cursorInfo?: CommandCursorInfo,
    ) => {
      if (!currentStateAccessors) return;
      execute(
        new SetChannelSampleCommand(
          currentStateAccessors,
          channelIndex,
          samplePath,
          cursorInfo,
        ),
      );
    },
    [execute],
  );

  const addNote = useCallback(
    (
      patternId: number,
      channelIndex: number,
      pitch: number,
      startStep: number,
      duration: number,
      cursorInfo?: CommandCursorInfo,
    ) => {
      if (!currentStateAccessors) return;
      execute(
        new AddNoteCommand(
          currentStateAccessors,
          patternId,
          channelIndex,
          pitch,
          startStep,
          duration,
          cursorInfo,
        ),
      );
    },
    [execute],
  );

  const removeNote = useCallback(
    (
      patternId: number,
      channelIndex: number,
      noteId: string,
      cursorInfo?: CommandCursorInfo,
    ) => {
      if (!currentStateAccessors) return;
      execute(
        new RemoveNoteCommand(
          currentStateAccessors,
          patternId,
          channelIndex,
          noteId,
          cursorInfo,
        ),
      );
    },
    [execute],
  );

  const updateNote = useCallback(
    (
      patternId: number,
      channelIndex: number,
      noteId: string,
      updates: NoteUpdates,
      cursorInfo?: CommandCursorInfo,
    ) => {
      if (!currentStateAccessors) return;
      execute(
        new UpdateNoteCommand(
          currentStateAccessors,
          patternId,
          channelIndex,
          noteId,
          updates,
          cursorInfo,
        ),
      );
    },
    [execute],
  );

  const toggleNote = useCallback(
    (
      patternId: number,
      channelIndex: number,
      pitch: number,
      startStep: number,
      duration: number,
      cursorInfo?: CommandCursorInfo,
    ) => {
      if (!currentStateAccessors) return;
      execute(
        new ToggleNoteCommand(
          currentStateAccessors,
          patternId,
          channelIndex,
          pitch,
          startStep,
          duration,
          cursorInfo,
        ),
      );
    },
    [execute],
  );

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
    throw new Error("useCommands must be used within CommandProvider");
  }
  return context;
}
