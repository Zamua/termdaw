import {
  createContext,
  useContext,
  useState,
  useCallback,
  useRef,
  type ReactNode,
} from "react";

export type FocusPanel =
  | "browser"
  | "channelRack"
  | "playlist"
  | "mixer"
  | "transport"
  | "pianoRoll";
export type ViewMode = "channelRack" | "playlist" | "pianoRoll" | "mixer";

// Cursor position for undo/redo restoration
export interface CursorPosition {
  row: number;
  col: number;
}

// Context identifier matching CommandContext
export type CursorContext = "channelRack" | "pianoRoll" | "playlist";

interface SampleSelectionState {
  isSelecting: boolean;
  channelIndex: number | null;
  returnPanel: FocusPanel | null;
}

// Cursor setter function signature
type CursorSetter = (pos: CursorPosition) => void;

interface FocusContextType {
  focusedPanel: FocusPanel;
  setFocusedPanel: (panel: FocusPanel) => void;
  viewMode: ViewMode;
  setViewMode: (mode: ViewMode) => void;
  enterPianoRoll: () => void;
  exitPianoRoll: () => void;
  sampleSelection: SampleSelectionState;
  startSampleSelection: (channelIndex: number, returnPanel: FocusPanel) => void;
  cancelSampleSelection: () => void;
  completeSampleSelection: () => number | null;
  // Cursor restoration for undo/redo
  registerCursorSetter: (context: CursorContext, setter: CursorSetter) => void;
  unregisterCursorSetter: (context: CursorContext) => void;
  restoreCursor: (context: CursorContext, position: CursorPosition) => void;
}

const FocusContext = createContext<FocusContextType | null>(null);

export function FocusProvider({ children }: { children: ReactNode }) {
  const [focusedPanel, setFocusedPanel] = useState<FocusPanel>("channelRack");
  const [viewMode, setViewMode] = useState<ViewMode>("channelRack");
  const [sampleSelection, setSampleSelection] = useState<SampleSelectionState>({
    isSelecting: false,
    channelIndex: null,
    returnPanel: null,
  });

  // Cursor setters registered by components
  const cursorSettersRef = useRef<Map<CursorContext, CursorSetter>>(new Map());

  const registerCursorSetter = useCallback(
    (context: CursorContext, setter: CursorSetter) => {
      cursorSettersRef.current.set(context, setter);
    },
    [],
  );

  const unregisterCursorSetter = useCallback((context: CursorContext) => {
    cursorSettersRef.current.delete(context);
  }, []);

  const restoreCursor = useCallback(
    (context: CursorContext, position: CursorPosition) => {
      // Switch to the appropriate view/panel
      if (context === "channelRack") {
        setViewMode("channelRack");
        setFocusedPanel("channelRack");
      } else if (context === "pianoRoll") {
        setViewMode("pianoRoll");
        setFocusedPanel("pianoRoll");
      } else if (context === "playlist") {
        setViewMode("playlist");
        setFocusedPanel("playlist");
      }

      // Call the registered cursor setter
      const setter = cursorSettersRef.current.get(context);
      if (setter) {
        setter(position);
      }
    },
    [],
  );

  const enterPianoRoll = useCallback(() => {
    setViewMode("pianoRoll");
    setFocusedPanel("pianoRoll");
  }, []);

  const exitPianoRoll = useCallback(() => {
    setViewMode("channelRack");
    setFocusedPanel("channelRack");
  }, []);

  const startSampleSelection = useCallback(
    (channelIndex: number, returnPanel: FocusPanel) => {
      setSampleSelection({
        isSelecting: true,
        channelIndex,
        returnPanel,
      });
      setFocusedPanel("browser");
    },
    [],
  );

  const cancelSampleSelection = useCallback(() => {
    const returnPanel = sampleSelection.returnPanel || "channelRack";
    setSampleSelection({
      isSelecting: false,
      channelIndex: null,
      returnPanel: null,
    });
    setFocusedPanel(returnPanel);
  }, [sampleSelection.returnPanel]);

  const completeSampleSelection = useCallback(() => {
    const channelIndex = sampleSelection.channelIndex;
    const returnPanel = sampleSelection.returnPanel || "channelRack";
    setSampleSelection({
      isSelecting: false,
      channelIndex: null,
      returnPanel: null,
    });
    setFocusedPanel(returnPanel);
    return channelIndex;
  }, [sampleSelection.channelIndex, sampleSelection.returnPanel]);

  return (
    <FocusContext.Provider
      value={{
        focusedPanel,
        setFocusedPanel,
        viewMode,
        setViewMode,
        enterPianoRoll,
        exitPianoRoll,
        sampleSelection,
        startSampleSelection,
        cancelSampleSelection,
        completeSampleSelection,
        registerCursorSetter,
        unregisterCursorSetter,
        restoreCursor,
      }}
    >
      {children}
    </FocusContext.Provider>
  );
}

export function useFocusContext() {
  const context = useContext(FocusContext);
  if (!context) {
    throw new Error("useFocusContext must be used within FocusProvider");
  }
  return context;
}

// Hook to check if this panel is focused
export function useIsFocused(panel: FocusPanel): boolean {
  const { focusedPanel } = useFocusContext();
  return focusedPanel === panel;
}
