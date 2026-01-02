import { useState, useCallback, useEffect } from "react";
import { Box, Text, useInput, useStdout } from "ink";
import { useIsFocused, useFocusContext } from "../context/FocusContext.js";
import { useSequencer } from "../context/SequencerContext.js";
import { useCommands } from "../context/CommandContext.js";
import { previewSample, getSamplePath } from "../lib/audio.js";
import { useVim } from "../hooks/useVim.js";
import type { Position, Range, Key } from "../lib/vim/types.js";

const NUM_STEPS = 16;
const HEADER_ROWS = 6;

// Virtual column mapping:
// -2 = sample zone
// -1 = mute zone
// 0-15 = steps zone
const SAMPLE_ZONE_COL = -2;
const MUTE_ZONE_COL = -1;

export default function ChannelRack() {
  const isFocused = useIsFocused("channelRack");
  const {
    startSampleSelection,
    enterPianoRoll,
    registerCursorSetter,
    unregisterCursorSetter,
  } = useFocusContext();
  const {
    channels,
    playheadStep,
    isPlaying,
    currentPatternId,
    switchPattern,
    patterns,
    selectedChannel,
    setSelectedChannel,
  } = useSequencer();
  const { toggleStep, cycleMuteState, clearChannel, clearStepRange, setSteps } =
    useCommands();
  const { stdout } = useStdout();
  const [termHeight, setTermHeight] = useState(stdout?.rows || 24);
  const [cursorChannel, setCursorChannel] = useState(selectedChannel);
  const [cursorCol, setCursorCol] = useState(0); // Virtual column: -2, -1, or 0-15
  const [viewportTop, setViewportTop] = useState(0);

  // Derive zone from virtual column
  const cursorZone =
    cursorCol === SAMPLE_ZONE_COL
      ? "sample"
      : cursorCol === MUTE_ZONE_COL
        ? "mute"
        : "steps";
  const cursorStep = cursorCol >= 0 ? cursorCol : 0;

  // Track terminal height changes
  useEffect(() => {
    const handleResize = () => {
      setTermHeight(stdout?.rows || 24);
    };
    stdout?.on("resize", handleResize);
    return () => {
      stdout?.off("resize", handleResize);
    };
  }, [stdout]);

  const viewportHeight = Math.max(5, termHeight - HEADER_ROWS);

  const moveCursor = useCallback(
    (newChannel: number) => {
      const clampedChannel = Math.max(
        0,
        Math.min(channels.length - 1, newChannel),
      );
      setCursorChannel(clampedChannel);
      setViewportTop((prev) => {
        if (clampedChannel < prev) {
          return clampedChannel;
        } else if (clampedChannel >= prev + viewportHeight) {
          return clampedChannel - viewportHeight + 1;
        }
        return prev;
      });
    },
    [channels.length, viewportHeight],
  );

  useEffect(() => {
    setSelectedChannel(cursorChannel);
  }, [cursorChannel, setSelectedChannel]);

  useEffect(() => {
    if (isFocused) {
      setCursorChannel(selectedChannel);
    }
  }, [isFocused, selectedChannel]);

  // Register cursor setter for undo/redo restoration
  useEffect(() => {
    registerCursorSetter("channelRack", (pos) => {
      moveCursor(pos.row);
      setCursorCol(pos.col - 2); // Convert from shifted space back to virtual column
    });
    return () => unregisterCursorSetter("channelRack");
  }, [registerCursorSetter, unregisterCursorSetter, moveCursor]);

  // Vim hook - uses virtual column space including zones
  // Column mapping: 0=sample, 1=mute, 2-17=steps
  const vim = useVim<boolean[]>({
    dimensions: { rows: channels.length, cols: NUM_STEPS + 2 }, // +2 for sample and mute zones

    getCursor: () => ({ row: cursorChannel, col: cursorCol + 2 }), // Shift by 2 so -2 becomes 0

    setCursor: (pos: Position) => {
      moveCursor(pos.row);
      setCursorCol(
        Math.max(SAMPLE_ZONE_COL, Math.min(NUM_STEPS - 1, pos.col - 2)),
      ); // Shift back
    },

    // Library handles all motions via gridSemantics
    gridSemantics: {
      zones: [
        { name: "sample", colRange: [0, 0] },
        { name: "mute", colRange: [1, 1] },
        {
          name: "steps",
          colRange: [2, NUM_STEPS + 1],
          isMain: true,
          hasContent: (pos) => channels[pos.row]?.steps[pos.col - 2] === true,
          wordInterval: 4,
        },
      ],
    },

    getDataInRange: (range: Range) => {
      // Convert from virtual to real columns, only steps zone has data
      const startRealCol = Math.max(0, range.start.col - 2);
      const endRealCol = Math.max(0, range.end.col - 2);
      const channel = channels[range.start.row];
      if (!channel) return [];
      return channel.steps.slice(
        Math.min(startRealCol, endRealCol),
        Math.max(startRealCol, endRealCol) + 1,
      );
    },

    deleteRange: (range: Range) => {
      const startRealCol = Math.max(0, range.start.col - 2);
      const endRealCol = Math.max(0, range.end.col - 2);
      const channel = channels[range.start.row];
      if (!channel) return [];
      const startCol = Math.min(startRealCol, endRealCol);
      const endCol = Math.max(startRealCol, endRealCol);
      const deleted = channel.steps.slice(startCol, endCol + 1);
      clearStepRange(currentPatternId, range.start.row, startCol, endCol, {
        context: "channelRack",
        position: { row: range.start.row, col: range.start.col },
      });
      return deleted;
    },

    insertData: (pos: Position, data: boolean[]) => {
      const realCol = Math.max(0, pos.col - 2);
      setSteps(currentPatternId, pos.row, realCol, data, {
        context: "channelRack",
        position: { row: pos.row, col: pos.col },
      });
    },

    onCustomAction: (char: string, key: Key, _count: number) => {
      // Helper to get current cursor info for undo/redo
      const getCursorInfo = () => ({
        context: "channelRack" as const,
        position: { row: cursorChannel, col: cursorCol + 2 },
      });

      // x/Enter actions depend on current zone
      if (key.return || char === "x") {
        if (cursorZone === "sample") {
          startSampleSelection(cursorChannel, "channelRack");
          return true;
        } else if (cursorZone === "mute") {
          cycleMuteState(cursorChannel, getCursorInfo());
          return true;
        } else {
          toggleStep(
            currentPatternId,
            cursorChannel,
            cursorStep,
            getCursorInfo(),
          );
          return true;
        }
      }

      if (char === "s") {
        const channel = channels[cursorChannel];
        if (channel) {
          previewSample(getSamplePath(channel.sample));
        }
        return true;
      }

      if (char === "m") {
        cycleMuteState(cursorChannel, getCursorInfo());
        return true;
      }

      if (char === "i") {
        enterPianoRoll();
        return true;
      }

      if (char === "[") {
        const currentIdx = patterns.findIndex((p) => p.id === currentPatternId);
        if (currentIdx > 0) {
          const prevPattern = patterns[currentIdx - 1];
          if (prevPattern) switchPattern(prevPattern.id);
        }
        return true;
      }

      if (char === "]") {
        const currentIdx = patterns.findIndex((p) => p.id === currentPatternId);
        if (currentIdx < patterns.length - 1) {
          const nextPattern = patterns[currentIdx + 1];
          if (nextPattern) switchPattern(nextPattern.id);
        } else {
          switchPattern(currentPatternId + 1);
        }
        return true;
      }

      if (char === "c" && vim.mode === "normal" && !vim.operator) {
        clearChannel(currentPatternId, cursorChannel, getCursorInfo());
        return true;
      }

      return false;
    },
  });

  // All input goes through vim - no custom logic here
  useInput((input, key) => {
    if (!isFocused) return;

    const inkKey: Key = {
      upArrow: key.upArrow,
      downArrow: key.downArrow,
      leftArrow: key.leftArrow,
      rightArrow: key.rightArrow,
      pageDown: key.pageDown,
      pageUp: key.pageUp,
      return: key.return,
      escape: key.escape,
      ctrl: key.ctrl,
      shift: key.shift,
      tab: key.tab,
      backspace: key.backspace,
      delete: key.delete,
      meta: key.meta,
    };

    vim.handleInput(input, inkKey);
  });

  const isInVisualSelection = (channelIdx: number, stepIdx: number) => {
    if (!vim.visualRange) return false;
    const { start, end } = vim.visualRange;
    if (channelIdx < start.row || channelIdx > end.row) return false;
    // Convert visual range to real columns
    const startRealCol = Math.max(0, start.col - 2);
    const endRealCol = Math.max(0, end.col - 2);
    if (stepIdx < Math.min(startRealCol, endRealCol)) return false;
    if (stepIdx > Math.max(startRealCol, endRealCol)) return false;
    return true;
  };

  // Find current pattern name
  const currentPattern = patterns.find((p) => p.id === currentPatternId);
  const patternName = currentPattern?.name || `Pattern ${currentPatternId}`;

  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Pattern indicator */}
      <Box>
        <Text color="cyan" bold>
          {patternName}
        </Text>
        <Text dimColor> [ ] to switch</Text>
      </Box>

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
              color={
                i === playheadStep && isPlaying
                  ? "green"
                  : i % 4 === 0
                    ? "yellow"
                    : "gray"
              }
              bold={i === cursorStep && isFocused && cursorZone === "steps"}
            >
              {(i + 1).toString(16).toUpperCase()}
            </Text>
          </Box>
        ))}
      </Box>

      <Box>
        <Text dimColor>{"─".repeat(10 + 2 + 3 + NUM_STEPS * 2)}</Text>
      </Box>

      {channels
        .slice(viewportTop, viewportTop + viewportHeight)
        .map((channel, viewIndex) => {
          const channelIndex = viewportTop + viewIndex;
          const isCurrentChannel = channelIndex === cursorChannel && isFocused;
          const isSampleCursor = isCurrentChannel && cursorZone === "sample";
          const isMuteCursor = isCurrentChannel && cursorZone === "mute";
          const hasSolo = channels.some((ch) => ch.solo);
          const isEffectivelyMuted =
            channel.muted || (hasSolo && !channel.solo);
          const isEmpty = channel.type === "sample" && !channel.sample;

          return (
            <Box key={`channel-${channelIndex}`}>
              <Box width={10}>
                <Text
                  color={
                    isSampleCursor
                      ? "white"
                      : isEmpty
                        ? "gray"
                        : isEffectivelyMuted
                          ? "gray"
                          : isCurrentChannel
                            ? "cyan"
                            : "white"
                  }
                  backgroundColor={isSampleCursor ? "blue" : undefined}
                  bold={isCurrentChannel && !isEmpty}
                  dimColor={(isEffectivelyMuted || isEmpty) && !isSampleCursor}
                >
                  {isEmpty
                    ? "(empty)".padEnd(8, " ")
                    : channel.name.slice(0, 8).padEnd(8, " ")}
                </Text>
              </Box>

              <Box width={2}>
                <Text
                  color={
                    isEmpty
                      ? "gray"
                      : channel.type === "synth"
                        ? "cyan"
                        : "gray"
                  }
                  dimColor={isEffectivelyMuted || isEmpty}
                >
                  {isEmpty ? "·" : channel.type === "synth" ? "♪" : "◌"}
                </Text>
              </Box>

              <Box width={3}>
                <Text
                  color={
                    isMuteCursor
                      ? "white"
                      : isEmpty
                        ? "gray"
                        : channel.solo
                          ? "yellow"
                          : channel.muted
                            ? "red"
                            : "green"
                  }
                  backgroundColor={isMuteCursor ? "blue" : undefined}
                  bold={isMuteCursor}
                  dimColor={isEmpty && !isMuteCursor}
                >
                  {isEmpty
                    ? "·"
                    : channel.solo
                      ? "S"
                      : channel.muted
                        ? "M"
                        : "○"}
                </Text>
              </Box>

              {channel.steps.map((active, stepIndex) => {
                const isCursor =
                  channelIndex === cursorChannel &&
                  stepIndex === cursorStep &&
                  isFocused &&
                  cursorZone === "steps";
                const isPlayheadHere = stepIndex === playheadStep && isPlaying;
                const isBeat = stepIndex % 4 === 0;
                const isVisualSelected = isInVisualSelection(
                  channelIndex,
                  stepIndex,
                );

                let bgColor: string | undefined;
                let fgColor = "gray";

                if (isCursor && isPlayheadHere) {
                  bgColor = "greenBright";
                  fgColor = "black";
                } else if (isCursor) {
                  bgColor = "blue";
                  fgColor = "white";
                } else if (isVisualSelected) {
                  bgColor = "yellow";
                  fgColor = "black";
                } else if (isPlayheadHere) {
                  bgColor = "green";
                  fgColor = "black";
                }

                if (
                  active &&
                  !isCursor &&
                  !isPlayheadHere &&
                  !isVisualSelected
                ) {
                  fgColor = isEffectivelyMuted || isEmpty ? "gray" : "magenta";
                } else if (active && (isCursor || isVisualSelected)) {
                  fgColor = "black";
                } else if (isEmpty && !isCursor) {
                  fgColor = "gray";
                }

                return (
                  <Box key={`step-${channelIndex}-${stepIndex}`} width={2}>
                    <Text
                      backgroundColor={bgColor}
                      color={fgColor}
                      bold={active || isPlayheadHere}
                      dimColor={isEmpty && !isCursor && !isPlayheadHere}
                    >
                      {active ? "●" : isBeat ? "┃" : "│"}
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
