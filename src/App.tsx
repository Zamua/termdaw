import { useState, useEffect } from 'react';
import { Box, Text, useInput, useApp, useStdout } from 'ink';
import { TitledBox } from '@mishieck/ink-titled-box';
import { FocusProvider, useFocusContext, type FocusPanel, type ViewMode } from './context/FocusContext.js';
import { SequencerProvider, useSequencer } from './context/SequencerContext.js';
import { CommandProvider, useCommands } from './context/CommandContext.js';
import Transport from './components/Transport.js';
import Browser from './components/Browser.js';
import ChannelRack from './components/ChannelRack.js';
import Playlist from './components/Playlist.js';
import Mixer from './components/Mixer.js';
import PianoRoll from './components/PianoRoll.js';

function AppContent() {
  const { exit } = useApp();
  const { stdout } = useStdout();
  const { focusedPanel, setFocusedPanel, viewMode, setViewMode } = useFocusContext();
  const { isPlaying, setIsPlaying, bpm, setBpm, currentPatternId, switchPattern, patterns, selectedChannel, channels } = useSequencer();
  const { undo, redo, canUndo, canRedo } = useCommands();
  const [showBrowser, setShowBrowser] = useState(true);
  const [showMixer, setShowMixer] = useState(false);

  // Terminal dimensions with resize support
  const [dimensions, setDimensions] = useState({
    width: stdout?.columns || 80,
    height: stdout?.rows || 24,
  });

  // Handle terminal resize
  useEffect(() => {
    const handleResize = () => {
      if (stdout) {
        // Clear screen to prevent rendering artifacts
        stdout.write('\x1b[2J\x1b[H');
        setDimensions({
          width: stdout.columns,
          height: stdout.rows,
        });
      }
    };

    stdout?.on('resize', handleResize);
    return () => {
      stdout?.off('resize', handleResize);
    };
  }, [stdout]);

  const terminalHeight = dimensions.height;
  const terminalWidth = dimensions.width;

  // Global keybindings (always active)
  useInput((input, key) => {
    // q to quit (changed from Escape to allow vim-style Escape in editors)
    if (input === 'q') {
      exit();
      return;
    }

    // Tab to cycle focus between panels
    if (key.tab) {
      const panels: FocusPanel[] = [];
      if (showBrowser) panels.push('browser');
      if (viewMode === 'playlist') {
        panels.push('playlist');
      } else if (viewMode === 'pianoRoll') {
        panels.push('pianoRoll');
      } else {
        panels.push('channelRack');
      }
      if (showMixer) panels.push('mixer');

      const currentIndex = panels.indexOf(focusedPanel);
      const nextIndex = (currentIndex + 1) % panels.length;
      const nextPanel = panels[nextIndex];
      if (nextPanel) {
        setFocusedPanel(nextPanel);
      }
      return;
    }

    // Alt+hjkl for directional focus switching (Ctrl+hjkl are terminal control codes)
    const mainPanel: FocusPanel = viewMode === 'playlist' ? 'playlist' : viewMode === 'pianoRoll' ? 'pianoRoll' : 'channelRack';
    if (key.meta && input === 'h') {
      // Focus left (browser)
      if (showBrowser) {
        setFocusedPanel('browser');
      }
      return;
    }
    if (key.meta && input === 'l') {
      // Focus right (main view)
      setFocusedPanel(mainPanel);
      return;
    }
    if (key.meta && input === 'j') {
      // Focus down (mixer)
      if (showMixer) {
        setFocusedPanel('mixer');
      }
      return;
    }
    if (key.meta && input === 'k') {
      // Focus up (main view from mixer, or stay)
      if (focusedPanel === 'mixer') {
        setFocusedPanel(mainPanel);
      }
      return;
    }

    // Space - Play/Pause (global)
    if (input === ' ') {
      setIsPlaying(!isPlaying);
      return;
    }

    // Number keys for view switching (F5=5, F6=6, etc.) - global
    if (input === '5') {
      setViewMode('playlist');
      setFocusedPanel('playlist');
      return;
    }

    if (input === '6') {
      setViewMode('channelRack');
      setFocusedPanel('channelRack');
      return;
    }

    if (input === '7') {
      setViewMode('pianoRoll');
      setFocusedPanel('pianoRoll');
      return;
    }

    if (input === '9') {
      setShowMixer(prev => !prev);
      return;
    }

    if (input === '8') {
      setShowBrowser(prev => {
        const newVal = !prev;
        // If hiding browser and it was focused, move focus to main view
        if (!newVal && focusedPanel === 'browser') {
          setFocusedPanel(viewMode === 'playlist' ? 'playlist' : 'channelRack');
        }
        return newVal;
      });
      return;
    }

    // +/- for BPM (global)
    if (input === '+' || input === '=') {
      setBpm(Math.min(999, bpm + 1));
      return;
    }
    if (input === '-' || input === '_') {
      setBpm(Math.max(20, bpm - 1));
      return;
    }

    // Pattern selection with Ctrl+number
    if (key.ctrl && input >= '1' && input <= '9') {
      switchPattern(parseInt(input));
      return;
    }

    // Undo/Redo (vim-style: u for undo, Ctrl+r for redo)
    if (input === 'u' && !key.ctrl && !key.meta) {
      undo();
      return;
    }
    if (key.ctrl && input === 'r') {
      redo();
      return;
    }
  });

  const renderMainView = () => {
    switch (viewMode) {
      case 'playlist':
        return <Playlist />;
      case 'channelRack':
        return <ChannelRack />;
      case 'pianoRoll':
        return <PianoRoll />;
      case 'mixer':
        return <Mixer />;
      default:
        return <ChannelRack />;
    }
  };

  const getMainViewTitle = () => {
    const mainPanel: FocusPanel = viewMode === 'playlist' ? 'playlist' : viewMode === 'pianoRoll' ? 'pianoRoll' : 'channelRack';
    const isFocused = focusedPanel === mainPanel;
    const focusIndicator = isFocused ? ' *' : '';

    switch (viewMode) {
      case 'playlist':
        return `Playlist${focusIndicator}`;
      case 'channelRack':
        return `Channel Rack P${currentPatternId}${focusIndicator}`;
      case 'pianoRoll':
        const channelName = channels[selectedChannel]?.name || 'Channel';
        return `Piano Roll - ${channelName} P${currentPatternId}${focusIndicator}`;
      default:
        return `Channel Rack P${currentPatternId}${focusIndicator}`;
    }
  };

  // Get focus indicator for panel borders
  const getBorderColor = (panel: FocusPanel): string => {
    return focusedPanel === panel ? 'cyan' : 'gray';
  };

  // Calculate heights for layout (account for borders: 2 lines each for top/bottom)
  const transportHeight = 5; // 4 content + border
  const statusHeight = 1;
  const mixerHeight = showMixer ? 12 : 0; // 10 content + 2 border
  const mainContentHeight = Math.max(10, terminalHeight - transportHeight - statusHeight - mixerHeight);

  return (
    <Box key={`${terminalWidth}x${terminalHeight}-${viewMode}`} flexDirection="column" width={terminalWidth} height={terminalHeight}>
      {/* Transport Bar */}
      <Box borderStyle="round" borderColor="gray">
        <Transport
          isPlaying={isPlaying}
          bpm={bpm}
        />
      </Box>

      {/* Main Content Area */}
      <Box height={mainContentHeight} flexDirection="row" flexGrow={1}>
        {/* Browser Panel */}
        {showBrowser && (
          <TitledBox
            width={30}
            flexDirection="column"
            borderStyle="round"
            borderColor={getBorderColor('browser')}
            titles={[`Browser${focusedPanel === 'browser' ? ' *' : ''}`]}
            overflow="hidden"
          >
            <Browser />
          </TitledBox>
        )}

        {/* Main View (Channel Rack / Playlist / Piano Roll) */}
        <TitledBox
          flexGrow={1}
          flexDirection="column"
          borderStyle="round"
          borderColor={getBorderColor(viewMode === 'playlist' ? 'playlist' : viewMode === 'pianoRoll' ? 'pianoRoll' : 'channelRack')}
          titles={[getMainViewTitle()]}
          overflow="hidden"
        >
          {renderMainView()}
        </TitledBox>
      </Box>

      {/* Mixer (bottom panel) */}
      {showMixer && (
        <TitledBox
          height={mixerHeight}
          borderStyle="round"
          borderColor={getBorderColor('mixer')}
          titles={[`Mixer${focusedPanel === 'mixer' ? ' *' : ''}`]}
        >
          <Mixer />
        </TitledBox>
      )}

      {/* Status Bar */}
      <Box paddingX={1} justifyContent="space-between">
        <Text dimColor>
          6:Rack 5:Playlist 7:Piano 9:Mixer 8:Browser | Space:Play | u:Undo | ^r:Redo | q:Quit
        </Text>
        <Box>
          {(canUndo || canRedo) && (
            <Text dimColor>
              {canUndo ? 'u' : '-'}/{canRedo ? '^r' : '-'}{' '}
            </Text>
          )}
          <Text color="cyan">
            [{focusedPanel.toUpperCase()}]
          </Text>
        </Box>
      </Box>
    </Box>
  );
}

export default function App() {
  return (
    <CommandProvider>
      <SequencerProvider>
        <FocusProvider>
          <AppContent />
        </FocusProvider>
      </SequencerProvider>
    </CommandProvider>
  );
}
