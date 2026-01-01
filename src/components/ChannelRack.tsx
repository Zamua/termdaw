import { useState, useCallback, useRef, useEffect } from 'react';
import { Box, Text, useInput, useStdout } from 'ink';
import { useIsFocused, useFocusContext } from '../context/FocusContext.js';
import { useSequencer } from '../context/SequencerContext.js';
import { previewSample, getSamplePath } from '../lib/audio.js';

const NUM_STEPS = 16;
const HEADER_ROWS = 5; // Transport, title bar, header, separator, some padding

type Mode = 'normal' | 'visual' | 'operator';
type Operator = 'd' | 'y' | null;
type CursorZone = 'sample' | 'mute' | 'steps';

interface YankBuffer {
  steps: boolean[];
  channelIndex: number;
}

interface LastAction {
  type: 'toggle' | 'paste' | 'delete';
  steps?: boolean[];
}

export default function ChannelRack() {
  const isFocused = useIsFocused('channelRack');
  const { startSampleSelection, enterPianoRoll } = useFocusContext();
  const { channels, playheadStep, isPlaying, toggleStep, cycleMuteState, clearChannel, clearStepRange, setStepsAt, currentPatternId, switchPattern, patterns, selectedChannel, setSelectedChannel } = useSequencer();
  const { stdout } = useStdout();
  const [termHeight, setTermHeight] = useState(stdout?.rows || 24);
  const [cursorChannel, setCursorChannel] = useState(selectedChannel);
  const [cursorStep, setCursorStep] = useState(0);
  const [cursorZone, setCursorZone] = useState<CursorZone>('steps');
  const [mode, setMode] = useState<Mode>('normal');
  const [visualStart, setVisualStart] = useState<{ channel: number; step: number } | null>(null);
  const [countPrefix, setCountPrefix] = useState('');
  const [pendingOperator, setPendingOperator] = useState<Operator>(null);
  const [viewportTop, setViewportTop] = useState(0);
  const yankBuffer = useRef<YankBuffer | null>(null);
  const lastAction = useRef<LastAction | null>(null);

  // Track terminal height changes
  useEffect(() => {
    const handleResize = () => {
      setTermHeight(stdout?.rows || 24);
    };
    stdout?.on('resize', handleResize);
    return () => {
      stdout?.off('resize', handleResize);
    };
  }, [stdout]);

  // Calculate viewport height based on terminal size
  const viewportHeight = Math.max(5, termHeight - HEADER_ROWS);

  // Combined cursor movement with viewport scrolling (avoids extra re-renders)
  const moveCursor = useCallback((newChannel: number) => {
    const clampedChannel = Math.max(0, Math.min(channels.length - 1, newChannel));
    setCursorChannel(clampedChannel);
    // Update viewport in same tick to avoid double render
    setViewportTop(prev => {
      if (clampedChannel < prev) {
        return clampedChannel;
      } else if (clampedChannel >= prev + viewportHeight) {
        return clampedChannel - viewportHeight + 1;
      }
      return prev;
    });
  }, [channels.length, viewportHeight]);

  // Sync cursor channel to global selected channel
  useEffect(() => {
    setSelectedChannel(cursorChannel);
  }, [cursorChannel, setSelectedChannel]);

  // Sync from global selected channel when returning to channel rack
  useEffect(() => {
    if (isFocused) {
      setCursorChannel(selectedChannel);
    }
  }, [isFocused, selectedChannel]);

  // Get the count from prefix or default to 1
  const getCount = useCallback(() => {
    const count = countPrefix ? parseInt(countPrefix, 10) : 1;
    setCountPrefix('');
    return count;
  }, [countPrefix]);

  // Get visual selection range (sorted)
  const getVisualRange = useCallback(() => {
    if (!visualStart) return null;
    const startStep = Math.min(visualStart.step, cursorStep);
    const endStep = Math.max(visualStart.step, cursorStep);
    return { startStep, endStep, channel: cursorChannel };
  }, [visualStart, cursorStep, cursorChannel]);

  // Find next note on current channel, or next bar line if no note found
  const findNextNote = useCallback((fromStep: number, channel: number) => {
    const steps = channels[channel]?.steps || [];
    // First try to find the next note
    for (let i = fromStep + 1; i < NUM_STEPS; i++) {
      if (steps[i]) return i;
    }
    // No note found, jump to next bar line (0, 4, 8, 12)
    const nextBar = Math.ceil((fromStep + 1) / 4) * 4;
    if (nextBar < NUM_STEPS) {
      return nextBar;
    }
    // Wrap to beginning
    return 0;
  }, [channels]);

  // Find previous note on current channel, or previous bar line if no note found
  const findPrevNote = useCallback((fromStep: number, channel: number) => {
    const steps = channels[channel]?.steps || [];
    // First try to find the previous note
    for (let i = fromStep - 1; i >= 0; i--) {
      if (steps[i]) return i;
    }
    // No note found, jump to previous bar line (0, 4, 8, 12)
    const prevBar = Math.floor((fromStep - 1) / 4) * 4;
    if (prevBar >= 0) {
      return prevBar;
    }
    // Wrap to last bar line
    return 12;
  }, [channels]);

  // Delete steps in a range (inclusive)
  const deleteRange = useCallback((startStep: number, endStep: number) => {
    clearStepRange(cursorChannel, startStep, endStep);
    lastAction.current = { type: 'delete' };
  }, [cursorChannel, clearStepRange]);

  // Yank steps in a range (inclusive)
  const yankRange = useCallback((startStep: number, endStep: number) => {
    const minStep = Math.min(startStep, endStep);
    const maxStep = Math.max(startStep, endStep);
    const channel = channels[cursorChannel];
    if (!channel) return;
    const copiedSteps = channel.steps.slice(minStep, maxStep + 1);
    yankBuffer.current = { steps: copiedSteps, channelIndex: cursorChannel };
  }, [cursorChannel, channels]);

  // Execute operator with motion target
  // If endStep is provided, use startStep->endStep; otherwise use cursorStep->startStep
  const executeOperator = useCallback((op: Operator, startStep: number, endStep?: number) => {
    if (!op) return;
    const from = endStep !== undefined ? startStep : cursorStep;
    const to = endStep !== undefined ? endStep : startStep;
    if (op === 'd') {
      deleteRange(from, to);
    } else if (op === 'y') {
      yankRange(from, to);
    }
    setPendingOperator(null);
  }, [cursorStep, deleteRange, yankRange]);

  // Yank (copy) steps in visual selection
  const yankSelection = useCallback(() => {
    const range = getVisualRange();
    if (!range) return;
    const channel = channels[range.channel];
    if (!channel) return;
    const copiedSteps = channel.steps.slice(range.startStep, range.endStep + 1);
    yankBuffer.current = { steps: copiedSteps, channelIndex: range.channel };
    setMode('normal');
    setVisualStart(null);
  }, [getVisualRange, channels]);

  // Paste yanked steps at cursor
  const pasteSteps = useCallback(() => {
    if (!yankBuffer.current) return;
    const { steps: copiedSteps } = yankBuffer.current;
    setStepsAt(cursorChannel, cursorStep, copiedSteps);
    lastAction.current = { type: 'paste', steps: copiedSteps };
  }, [cursorChannel, cursorStep, setStepsAt]);

  // Delete (clear) steps in visual selection
  const deleteSelection = useCallback(() => {
    const range = getVisualRange();
    if (!range) return;
    clearStepRange(range.channel, range.startStep, range.endStep);
    lastAction.current = { type: 'delete' };
    setMode('normal');
    setVisualStart(null);
  }, [getVisualRange, clearStepRange]);

  // Repeat last action
  const repeatLastAction = useCallback(() => {
    if (!lastAction.current) return;
    switch (lastAction.current.type) {
      case 'toggle':
        toggleStep(cursorChannel, cursorStep);
        break;
      case 'paste':
        pasteSteps();
        break;
    }
  }, [cursorChannel, cursorStep, toggleStep, pasteSteps]);

  useInput((input, key) => {
    // Only handle input when this panel is focused
    if (!isFocused) return;

    // Escape exits visual mode or cancels pending operator
    if (key.escape) {
      if (mode === 'visual') {
        setMode('normal');
        setVisualStart(null);
        setCountPrefix('');
      }
      if (pendingOperator) {
        setPendingOperator(null);
        setCountPrefix('');
      }
      return;
    }

    // Number prefix accumulation (1-9, then 0-9)
    if (/^[0-9]$/.test(input) && (countPrefix || input !== '0')) {
      setCountPrefix(prev => prev + input);
      return;
    }

    const count = getCount();

    // Handle horizontal motions (zone-aware)
    if (key.leftArrow || input === 'h') {
      if (cursorZone === 'steps') {
        if (cursorStep === 0) {
          // Move from steps to mute zone
          setCursorZone('mute');
        } else {
          const target = Math.max(0, cursorStep - count);
          if (pendingOperator) {
            // h is exclusive - delete steps BEFORE cursor (not including cursor)
            // dh deletes the step to the left, d2h deletes 2 steps to the left
            executeOperator(pendingOperator, target, cursorStep - 1);
          } else {
            setCursorStep(target);
          }
        }
      } else if (cursorZone === 'mute') {
        // Move from mute to sample zone
        setCursorZone('sample');
      }
      // If already in sample zone, do nothing (leftmost)
      return;
    }
    if (key.rightArrow || input === 'l') {
      if (cursorZone === 'sample') {
        // Move from sample to mute zone
        setCursorZone('mute');
      } else if (cursorZone === 'mute') {
        // Move from mute to steps zone
        setCursorZone('steps');
        setCursorStep(0);
      } else {
        if (pendingOperator) {
          // l is exclusive - dl deletes just current step, d2l deletes current + next
          const endStep = Math.min(NUM_STEPS - 1, cursorStep + count - 1);
          executeOperator(pendingOperator, cursorStep, endStep);
        } else {
          const target = Math.min(NUM_STEPS - 1, cursorStep + count);
          setCursorStep(target);
        }
      }
      return;
    }

    // Jump to start/end of row (works with operators)
    if (input === '0' && !countPrefix) {
      if (pendingOperator) {
        executeOperator(pendingOperator, 0);
      } else {
        setCursorStep(0);
      }
      return;
    }
    if (input === '$') {
      if (pendingOperator) {
        executeOperator(pendingOperator, NUM_STEPS - 1);
      } else {
        setCursorStep(NUM_STEPS - 1);
      }
      return;
    }

    // w - next note, b - previous note (works with operators)
    if (input === 'w') {
      let step = cursorStep;
      for (let i = 0; i < count; i++) {
        step = findNextNote(step, cursorChannel);
      }
      if (pendingOperator) {
        executeOperator(pendingOperator, step);
      } else {
        setCursorStep(step);
      }
      return;
    }
    if (input === 'b') {
      let step = cursorStep;
      for (let i = 0; i < count; i++) {
        step = findPrevNote(step, cursorChannel);
      }
      if (pendingOperator) {
        executeOperator(pendingOperator, step);
      } else {
        setCursorStep(step);
      }
      return;
    }

    // Vertical navigation (cancel operator if pending)
    if (key.upArrow || input === 'k') {
      setPendingOperator(null);
      moveCursor(cursorChannel - count);
      return;
    }
    if (key.downArrow || input === 'j') {
      setPendingOperator(null);
      moveCursor(cursorChannel + count);
      return;
    }

    // gg - go to first channel, G - go to last channel
    if (input === 'g') {
      setPendingOperator(null);
      moveCursor(0);
      return;
    }
    if (input === 'G') {
      setPendingOperator(null);
      moveCursor(channels.length - 1);
      return;
    }

    // Visual mode
    if (input === 'v') {
      if (mode === 'normal') {
        setMode('visual');
        setVisualStart({ channel: cursorChannel, step: cursorStep });
      } else {
        setMode('normal');
        setVisualStart(null);
      }
      return;
    }

    // Yank - in visual mode or as operator
    if (input === 'y') {
      if (mode === 'visual') {
        yankSelection();
        return;
      }
      // yy yanks entire channel row
      if (pendingOperator === 'y') {
        yankRange(0, NUM_STEPS - 1);
        setPendingOperator(null);
        return;
      }
      // Enter operator-pending mode
      setPendingOperator('y');
      return;
    }

    // Delete - in visual mode or as operator
    if (input === 'd') {
      if (mode === 'visual') {
        deleteSelection();
        return;
      }
      // dd clears entire channel
      if (pendingOperator === 'd') {
        clearChannel(cursorChannel);
        setPendingOperator(null);
        return;
      }
      // Enter operator-pending mode
      setPendingOperator('d');
      return;
    }

    // Paste
    if (input === 'p') {
      pasteSteps();
      return;
    }

    // Repeat last action
    if (input === '.') {
      repeatLastAction();
      return;
    }

    // Zone-aware actions for Enter/x
    if (key.return || input === 'x') {
      if (cursorZone === 'sample') {
        // Open browser to select sample
        startSampleSelection(cursorChannel, 'channelRack');
        return;
      } else if (cursorZone === 'mute') {
        // Cycle mute state: normal -> muted -> solo -> normal
        cycleMuteState(cursorChannel);
        return;
      } else {
        // Toggle step in steps zone
        toggleStep(cursorChannel, cursorStep);
        lastAction.current = { type: 'toggle' };
        return;
      }
    }

    // Preview sample with 's'
    if (input === 's') {
      const channel = channels[cursorChannel];
      if (channel) {
        previewSample(getSamplePath(channel.sample));
      }
      return;
    }

    // 'm' as shortcut to go to mute zone and cycle (quick mute toggle)
    if (input === 'm') {
      cycleMuteState(cursorChannel);
      return;
    }

    // 'i' to enter piano roll for this channel
    if (input === 'i') {
      enterPianoRoll();
      return;
    }

    // Pattern navigation with [ and ]
    if (input === '[') {
      // Previous pattern
      const currentIdx = patterns.findIndex(p => p.id === currentPatternId);
      if (currentIdx > 0) {
        const prevPattern = patterns[currentIdx - 1];
        if (prevPattern) {
          switchPattern(prevPattern.id);
        }
      }
      return;
    }
    if (input === ']') {
      // Next pattern (or create new one)
      const currentIdx = patterns.findIndex(p => p.id === currentPatternId);
      if (currentIdx < patterns.length - 1) {
        const nextPattern = patterns[currentIdx + 1];
        if (nextPattern) {
          switchPattern(nextPattern.id);
        }
      } else {
        // At last pattern, switch to next ID (creates it)
        switchPattern(currentPatternId + 1);
      }
      return;
    }

    // Clear channel
    if (input === 'c' && mode === 'normal') {
      clearChannel(cursorChannel);
      return;
    }
  });

  // Check if a step is in visual selection
  const isInVisualSelection = (channelIdx: number, stepIdx: number) => {
    if (mode !== 'visual' || !visualStart) return false;
    if (channelIdx !== cursorChannel) return false;
    const minStep = Math.min(visualStart.step, cursorStep);
    const maxStep = Math.max(visualStart.step, cursorStep);
    return stepIdx >= minStep && stepIdx <= maxStep;
  };

  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Step number header */}
      <Box>
        <Box width={10}>
          <Text dimColor>Channel</Text>
        </Box>
        <Box width={2}>
          <Text dimColor>T</Text>
        </Box>
        <Box width={3}>
          <Text dimColor>M</Text>
        </Box>
        {Array.from({ length: NUM_STEPS }, (_, i) => (
          <Box key={`header-${i}`} width={2}>
            <Text
              color={i === playheadStep && isPlaying ? 'green' : i % 4 === 0 ? 'yellow' : 'gray'}
              bold={i === cursorStep && isFocused}
            >
              {(i + 1).toString(16).toUpperCase()}
            </Text>
          </Box>
        ))}
      </Box>

      {/* Separator */}
      <Box>
        <Text dimColor>{'─'.repeat(10 + 2 + 3 + NUM_STEPS * 2)}</Text>
      </Box>

      {/* Channel rows - only render viewport */}
      {channels.slice(viewportTop, viewportTop + viewportHeight).map((channel, viewIndex) => {
        const channelIndex = viewportTop + viewIndex;
        const isCurrentChannel = channelIndex === cursorChannel && isFocused;
        const isSampleCursor = isCurrentChannel && cursorZone === 'sample';
        const isMuteCursor = isCurrentChannel && cursorZone === 'mute';
        // Check if any channel is solo'd (affects display)
        const hasSolo = channels.some(ch => ch.solo);
        // Is this channel effectively muted? (muted, or not solo when something else is)
        const isEffectivelyMuted = channel.muted || (hasSolo && !channel.solo);
        // Is this channel empty? (sample type with no sample assigned)
        const isEmpty = channel.type === 'sample' && !channel.sample;

        return (
        <Box key={`channel-${channelIndex}`}>
          {/* Channel name / Sample zone */}
          <Box width={10}>
            <Text
              color={isSampleCursor ? 'white' : isEmpty ? 'gray' : isEffectivelyMuted ? 'gray' : isCurrentChannel ? 'cyan' : 'white'}
              backgroundColor={isSampleCursor ? 'blue' : undefined}
              bold={isCurrentChannel && !isEmpty}
              dimColor={(isEffectivelyMuted || isEmpty) && !isSampleCursor}
            >
              {isEmpty ? '(empty)'.padEnd(8, ' ') : channel.name.slice(0, 8).padEnd(8, ' ')}
            </Text>
          </Box>

          {/* Channel type indicator */}
          <Box width={2}>
            <Text
              color={isEmpty ? 'gray' : channel.type === 'synth' ? 'cyan' : 'gray'}
              dimColor={isEffectivelyMuted || isEmpty}
            >
              {isEmpty ? '·' : channel.type === 'synth' ? '♪' : '◌'}
            </Text>
          </Box>

          {/* Mute indicator */}
          <Box width={3}>
            <Text
              color={isMuteCursor ? 'white' : isEmpty ? 'gray' : channel.solo ? 'yellow' : channel.muted ? 'red' : 'green'}
              backgroundColor={isMuteCursor ? 'blue' : undefined}
              bold={isMuteCursor}
              dimColor={isEmpty && !isMuteCursor}
            >
              {isEmpty ? '·' : channel.solo ? 'S' : channel.muted ? 'M' : '○'}
            </Text>
          </Box>

          {/* Steps */}
          {channel.steps.map((active, stepIndex) => {
            const isCursor = channelIndex === cursorChannel && stepIndex === cursorStep && isFocused && cursorZone === 'steps';
            const isPlayheadHere = stepIndex === playheadStep && isPlaying;
            const isBeat = stepIndex % 4 === 0;
            const isVisualSelected = isInVisualSelection(channelIndex, stepIndex);

            let bgColor: string | undefined;
            let fgColor = 'gray';

            if (isCursor && isPlayheadHere) {
              bgColor = 'greenBright';
              fgColor = 'black';
            } else if (isCursor) {
              bgColor = 'blue';
              fgColor = 'white';
            } else if (isVisualSelected) {
              bgColor = 'yellow';
              fgColor = 'black';
            } else if (isPlayheadHere) {
              bgColor = 'green';
              fgColor = 'black';
            }

            if (active && !isCursor && !isPlayheadHere && !isVisualSelected) {
              fgColor = isEffectivelyMuted || isEmpty ? 'gray' : 'magenta';
            } else if (active && (isCursor || isVisualSelected)) {
              fgColor = 'black';
            } else if (isEmpty && !isCursor) {
              fgColor = 'gray';
            }

            return (
              <Box key={`step-${channelIndex}-${stepIndex}`} width={2}>
                <Text
                  backgroundColor={bgColor}
                  color={fgColor}
                  bold={active || isPlayheadHere}
                  dimColor={isEmpty && !isCursor && !isPlayheadHere}
                >
                  {active ? '●' : isBeat ? '┃' : '│'}
                </Text>
              </Box>
            );
          })}
        </Box>
        );
      })}
    </Box>
  );
}
