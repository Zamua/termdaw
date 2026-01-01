import { createContext, useContext, useState, useCallback, type ReactNode } from 'react';

export type FocusPanel = 'browser' | 'channelRack' | 'playlist' | 'mixer' | 'transport' | 'pianoRoll';
export type ViewMode = 'channelRack' | 'playlist' | 'pianoRoll' | 'mixer';

interface SampleSelectionState {
  isSelecting: boolean;
  channelIndex: number | null;
  returnPanel: FocusPanel | null;
}

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
}

const FocusContext = createContext<FocusContextType | null>(null);

export function FocusProvider({ children }: { children: ReactNode }) {
  const [focusedPanel, setFocusedPanel] = useState<FocusPanel>('channelRack');
  const [viewMode, setViewMode] = useState<ViewMode>('channelRack');
  const [sampleSelection, setSampleSelection] = useState<SampleSelectionState>({
    isSelecting: false,
    channelIndex: null,
    returnPanel: null,
  });

  const enterPianoRoll = useCallback(() => {
    setViewMode('pianoRoll');
    setFocusedPanel('pianoRoll');
  }, []);

  const exitPianoRoll = useCallback(() => {
    setViewMode('channelRack');
    setFocusedPanel('channelRack');
  }, []);

  const startSampleSelection = useCallback((channelIndex: number, returnPanel: FocusPanel) => {
    setSampleSelection({
      isSelecting: true,
      channelIndex,
      returnPanel,
    });
    setFocusedPanel('browser');
  }, []);

  const cancelSampleSelection = useCallback(() => {
    const returnPanel = sampleSelection.returnPanel || 'channelRack';
    setSampleSelection({
      isSelecting: false,
      channelIndex: null,
      returnPanel: null,
    });
    setFocusedPanel(returnPanel);
  }, [sampleSelection.returnPanel]);

  const completeSampleSelection = useCallback(() => {
    const channelIndex = sampleSelection.channelIndex;
    const returnPanel = sampleSelection.returnPanel || 'channelRack';
    setSampleSelection({
      isSelecting: false,
      channelIndex: null,
      returnPanel: null,
    });
    setFocusedPanel(returnPanel);
    return channelIndex;
  }, [sampleSelection.channelIndex, sampleSelection.returnPanel]);

  return (
    <FocusContext.Provider value={{
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
    }}>
      {children}
    </FocusContext.Provider>
  );
}

export function useFocusContext() {
  const context = useContext(FocusContext);
  if (!context) {
    throw new Error('useFocusContext must be used within FocusProvider');
  }
  return context;
}

// Hook to check if this panel is focused
export function useIsFocused(panel: FocusPanel): boolean {
  const { focusedPanel } = useFocusContext();
  return focusedPanel === panel;
}
