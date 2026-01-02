import { useState, useCallback, useEffect } from "react";
import { Box, Text, useInput } from "ink";
import { useIsFocused, useFocusContext } from "../context/FocusContext.js";
import { useSequencer } from "../context/SequencerContext.js";
import { useCommands } from "../context/CommandContext.js";
import { previewSample, getSamplePath } from "../lib/audio.js";
import { playSynthNote } from "../lib/synth.js";
import { useVim } from "../hooks/useVim.js";
import type { Position, Range, Key } from "../lib/vim/types.js";

const NUM_STEPS = 16;
// Internal header rows: pattern indicator (1) + column header (1) + separator (1)
const INTERNAL_HEADER_ROWS = 3;

// Fixed widths for non-step columns
const CHANNEL_NAME_WIDTH = 10;
const TYPE_WIDTH = 2;
const MUTE_WIDTH = 3;
const STEP_WIDTH = 2;
const PADDING = 2; // paddingX={1} = 2 chars

interface ChannelRackProps {
  availableHeight: number;
  availableWidth: number;
}

// Virtual column mapping:
// -2 = sample zone
// -1 = mute zone
// 0-15 = steps zone
const SAMPLE_ZONE_COL = -2;
const MUTE_ZONE_COL = -1;

export default function ChannelRack({
  availableHeight,
  availableWidth,
}: ChannelRackProps) {
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
  const [cursorChannel, setCursorChannel] = useState(selectedChannel);
  const [cursorCol, setCursorCol] = useState(0); // Virtual column: -2, -1, or 0-15
  const [viewportTop, setViewportTop] = useState(0);
  const [viewportLeft, setViewportLeft] = useState(0);

  // Derive zone from virtual column
  const cursorZone =
    cursorCol === SAMPLE_ZONE_COL
      ? "sample"
      : cursorCol === MUTE_ZONE_COL
        ? "mute"
        : "steps";
  const cursorStep = cursorCol >= 0 ? cursorCol : 0;

  // Calculate viewport dimensions
  const viewportHeight = Math.max(5, availableHeight - INTERNAL_HEADER_ROWS);
  const fixedWidth = CHANNEL_NAME_WIDTH + TYPE_WIDTH + MUTE_WIDTH + PADDING;
  const viewportCols = Math.max(
    4,
    Math.min(NUM_STEPS, Math.floor((availableWidth - fixedWidth) / STEP_WIDTH)),
  );

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

  // Handle horizontal viewport scrolling when cursor moves
  const updateHorizontalViewport = useCallback(
    (newCol: number) => {
      // Only scroll for steps zone (col >= 0)
      if (newCol >= 0) {
        setViewportLeft((prev) => {
          if (newCol < prev) {
            return newCol;
          } else if (newCol >= prev + viewportCols) {
            return newCol - viewportCols + 1;
          }
          return prev;
        });
      }
    },
    [viewportCols],
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
      const newCol = pos.col - 2;
      setCursorCol(newCol);
      updateHorizontalViewport(newCol);
    });
    return () => unregisterCursorSetter("channelRack");
  }, [
    registerCursorSetter,
    unregisterCursorSetter,
    moveCursor,
    updateHorizontalViewport,
  ]);

  // Vim hook - uses virtual column space including zones
  // Column mapping: 0=sample, 1=mute, 2-17=steps
  const vim = useVim<boolean[]>({
    dimensions: { rows: channels.length, cols: NUM_STEPS + 2 }, // +2 for sample and mute zones

    getCursor: () => ({ row: cursorChannel, col: cursorCol + 2 }), // Shift by 2 so -2 becomes 0

    setCursor: (pos: Position) => {
      moveCursor(pos.row);
      const newCol = Math.max(
        SAMPLE_ZONE_COL,
        Math.min(NUM_STEPS - 1, pos.col - 2),
      );
      setCursorCol(newCol);
      updateHorizontalViewport(newCol);
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
          if (channel.type === "synth") {
            // Preview synth with C4 note for 0.5 seconds
            playSynthNote(channel.synthPatch, 60, 0.5);
          } else if (channel.sample) {
            previewSample(getSamplePath(channel.sample));
          }
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

  // Helper to get step styling
  const getStepStyle = (
    channelIndex: number,
    stepIndex: number,
    active: boolean,
    isEffectivelyMuted: boolean,
    isEmpty: boolean,
  ) => {
    const isCursor =
      channelIndex === cursorChannel &&
      stepIndex === cursorStep &&
      isFocused &&
      cursorZone === "steps";
    const isPlayheadHere = stepIndex === playheadStep && isPlaying;
    const isBeat = stepIndex % 4 === 0;
    const isVisualSelected = isInVisualSelection(channelIndex, stepIndex);

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

    if (active && !isCursor && !isPlayheadHere && !isVisualSelected) {
      fgColor = isEffectivelyMuted || isEmpty ? "gray" : "magenta";
    } else if (active && (isCursor || isVisualSelected)) {
      fgColor = "black";
    } else if (isEmpty && !isCursor) {
      fgColor = "gray";
    }

    const char = active ? "●" : isBeat ? "┃" : "│";
    const bold = active || isPlayheadHere;
    const dimColor = isEmpty && !isCursor && !isPlayheadHere;

    return { bgColor, fgColor, char, bold, dimColor };
  };

  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Pattern indicator */}
      <Text wrap="truncate">
        <Text color="cyan" bold>
          {patternName}
        </Text>
        <Text dimColor> [ ] to switch</Text>
      </Text>

      {/* Header row */}
      <Text wrap="truncate">
        <Text dimColor>{"Channel".padEnd(CHANNEL_NAME_WIDTH - 2)}</Text>
        <Text dimColor>{"T".padEnd(TYPE_WIDTH)}</Text>
        <Text dimColor>{"M".padEnd(MUTE_WIDTH)}</Text>
        {Array.from({ length: viewportCols }, (_, i) => {
          const stepNum = viewportLeft + i;
          if (stepNum >= NUM_STEPS) return null;
          const isPlayhead = stepNum === playheadStep && isPlaying;
          const isBeat = stepNum % 4 === 0;
          const isCursorCol =
            stepNum === cursorStep && isFocused && cursorZone === "steps";
          return (
            <Text
              key={stepNum}
              color={isPlayhead ? "green" : isBeat ? "yellow" : "gray"}
              bold={isCursorCol}
            >
              {(stepNum + 1).toString(16).toUpperCase().padEnd(STEP_WIDTH)}
            </Text>
          );
        })}
      </Text>

      {/* Separator */}
      <Text wrap="truncate" dimColor>
        {"─".repeat(
          CHANNEL_NAME_WIDTH +
            TYPE_WIDTH +
            MUTE_WIDTH -
            2 +
            viewportCols * STEP_WIDTH,
        )}
      </Text>

      {/* Channel rows */}
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

          // Channel name styling
          const channelColor = isSampleCursor
            ? "white"
            : isEmpty
              ? "gray"
              : isEffectivelyMuted
                ? "gray"
                : isCurrentChannel
                  ? "cyan"
                  : "white";

          // Type icon
          const typeIcon = isEmpty ? "·" : channel.type === "synth" ? "♪" : "◌";
          const typeColor = isEmpty
            ? "gray"
            : channel.type === "synth"
              ? "cyan"
              : "gray";

          // Mute icon
          const muteIcon = isEmpty
            ? "·"
            : channel.solo
              ? "S"
              : channel.muted
                ? "M"
                : "○";
          const muteColor = isMuteCursor
            ? "white"
            : isEmpty
              ? "gray"
              : channel.solo
                ? "yellow"
                : channel.muted
                  ? "red"
                  : "green";

          return (
            <Text key={`channel-${channelIndex}`} wrap="truncate">
              {/* Channel name */}
              <Text
                color={channelColor}
                backgroundColor={isSampleCursor ? "blue" : undefined}
                bold={isCurrentChannel && !isEmpty}
                dimColor={(isEffectivelyMuted || isEmpty) && !isSampleCursor}
              >
                {(isEmpty ? "(empty)" : channel.name.slice(0, 8)).padEnd(
                  CHANNEL_NAME_WIDTH - 2,
                )}
              </Text>

              {/* Type indicator */}
              <Text color={typeColor} dimColor={isEffectivelyMuted || isEmpty}>
                {typeIcon.padEnd(TYPE_WIDTH)}
              </Text>

              {/* Mute indicator */}
              <Text
                color={muteColor}
                backgroundColor={isMuteCursor ? "blue" : undefined}
                bold={isMuteCursor}
                dimColor={isEmpty && !isMuteCursor}
              >
                {muteIcon.padEnd(MUTE_WIDTH)}
              </Text>

              {/* Steps - only render visible viewport */}
              {Array.from({ length: viewportCols }, (_, i) => {
                const stepIndex = viewportLeft + i;
                if (stepIndex >= NUM_STEPS) return null;
                const active = channel.steps[stepIndex] ?? false;
                const style = getStepStyle(
                  channelIndex,
                  stepIndex,
                  active,
                  isEffectivelyMuted,
                  isEmpty,
                );

                return (
                  <Text
                    key={stepIndex}
                    backgroundColor={style.bgColor}
                    color={style.fgColor}
                    bold={style.bold}
                    dimColor={style.dimColor}
                  >
                    {style.char.padEnd(STEP_WIDTH)}
                  </Text>
                );
              })}
            </Text>
          );
        })}
    </Box>
  );
}
