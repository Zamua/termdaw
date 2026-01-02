import { useState, useCallback, useMemo } from "react";
import { Box, Text, useInput } from "ink";
import { useIsFocused } from "../context/FocusContext.js";
import {
  useSequencer,
  type PatternPlacement,
} from "../context/SequencerContext.js";
import { useVim } from "../hooks/useVim.js";
import type { Position, Range, Key } from "../lib/vim/types.js";

// Data type for yank/paste operations
interface PlacementData {
  barOffset: number;
}

const NUM_BARS = 16;
// Internal header rows: bar header (1) + separator (1)
const INTERNAL_HEADER_ROWS = 2;

// Fixed widths for non-bar columns
const PATTERN_NAME_WIDTH = 12;
const MUTE_WIDTH = 3;
const BAR_WIDTH = 4;
const PADDING = 2; // paddingX={1} = 2 chars

interface PlaylistProps {
  availableHeight: number;
  availableWidth: number;
}

export default function Playlist({
  availableHeight,
  availableWidth,
}: PlaylistProps) {
  const isFocused = useIsFocused("playlist");
  const {
    getNonEmptyPatterns,
    arrangement,
    togglePatternPlacement,
    togglePatternMute,
    setArrangement,
    arrangementBar,
    playMode,
    isPlaying,
  } = useSequencer();
  const [cursorRow, setCursorRow] = useState(0);
  const [cursorBar, setCursorBar] = useState(0);
  const [viewportTop, setViewportTop] = useState(0);
  const [viewportLeft, setViewportLeft] = useState(0);

  // Only show playhead when playing in arrangement mode
  const playheadBar =
    isPlaying && playMode === "arrangement" ? arrangementBar : -1;

  // Get non-empty patterns for display
  const patterns = useMemo(() => getNonEmptyPatterns(), [getNonEmptyPatterns]);

  // Calculate viewport dimensions
  const viewportHeight = Math.max(5, availableHeight - INTERNAL_HEADER_ROWS);
  const fixedWidth = PATTERN_NAME_WIDTH + MUTE_WIDTH + PADDING;
  const viewportCols = Math.max(
    4,
    Math.min(NUM_BARS, Math.floor((availableWidth - fixedWidth) / BAR_WIDTH)),
  );

  // Get placements for a pattern at a specific bar
  const getPlacementAt = useCallback(
    (patternId: number, bar: number): PatternPlacement | undefined => {
      return arrangement.placements.find(
        (p) =>
          p.patternId === patternId &&
          p.startBar <= bar &&
          bar < p.startBar + p.length,
      );
    },
    [arrangement.placements],
  );

  const moveCursor = useCallback(
    (newRow: number) => {
      const maxRow = Math.max(0, patterns.length - 1);
      const clampedRow = Math.max(0, Math.min(maxRow, newRow));
      setCursorRow(clampedRow);
      setViewportTop((prev) => {
        if (clampedRow < prev) {
          return clampedRow;
        } else if (clampedRow >= prev + viewportHeight) {
          return clampedRow - viewportHeight + 1;
        }
        return prev;
      });
    },
    [patterns.length, viewportHeight],
  );

  // Handle horizontal viewport scrolling when cursor moves
  const updateHorizontalViewport = useCallback(
    (newBar: number) => {
      setViewportLeft((prev) => {
        if (newBar < prev) {
          return newBar;
        } else if (newBar >= prev + viewportCols) {
          return newBar - viewportCols + 1;
        }
        return prev;
      });
    },
    [viewportCols],
  );

  // Vim hook
  const vim = useVim<PlacementData[]>({
    dimensions: { rows: Math.max(1, patterns.length), cols: NUM_BARS },

    getCursor: () => ({ row: cursorRow, col: cursorBar }),

    setCursor: (pos: Position) => {
      moveCursor(pos.row);
      const newBar = Math.max(0, Math.min(NUM_BARS - 1, pos.col));
      setCursorBar(newBar);
      updateHorizontalViewport(newBar);
    },

    gridSemantics: {
      zones: [
        {
          name: "bars",
          colRange: [0, NUM_BARS - 1],
          isMain: true,
          hasContent: (pos) => {
            const pattern = patterns[pos.row];
            if (!pattern) return false;
            return getPlacementAt(pattern.id, pos.col) !== undefined;
          },
          wordInterval: 4,
        },
      ],
    },

    getDataInRange: (range: Range) => {
      const pattern = patterns[range.start.row];
      if (!pattern) return [];

      const minCol = Math.min(range.start.col, range.end.col);
      const maxCol = Math.max(range.start.col, range.end.col);

      return arrangement.placements
        .filter(
          (p) =>
            p.patternId === pattern.id &&
            p.startBar >= minCol &&
            p.startBar <= maxCol,
        )
        .map((p) => ({
          barOffset: p.startBar - minCol,
        }));
    },

    deleteRange: (range: Range) => {
      const pattern = patterns[range.start.row];
      if (!pattern) return [];

      const minCol = Math.min(range.start.col, range.end.col);
      const maxCol = Math.max(range.start.col, range.end.col);

      const toDelete = arrangement.placements.filter(
        (p) =>
          p.patternId === pattern.id &&
          p.startBar >= minCol &&
          p.startBar <= maxCol,
      );

      const deleted: PlacementData[] = toDelete.map((p) => ({
        barOffset: p.startBar - minCol,
      }));

      setArrangement((prev) => ({
        ...prev,
        placements: prev.placements.filter(
          (p) =>
            !(
              p.patternId === pattern.id &&
              p.startBar >= minCol &&
              p.startBar <= maxCol
            ),
        ),
      }));

      return deleted;
    },

    insertData: (pos: Position, data: PlacementData[]) => {
      const pattern = patterns[pos.row];
      if (!pattern) return;

      setArrangement((prev) => {
        const newPlacements = [...prev.placements];
        for (const item of data) {
          const bar = pos.col + item.barOffset;
          if (bar >= 0 && bar < NUM_BARS) {
            // Remove any existing placement at this position for this pattern
            const existingIdx = newPlacements.findIndex(
              (p) => p.patternId === pattern.id && p.startBar === bar,
            );
            if (existingIdx >= 0) {
              newPlacements.splice(existingIdx, 1);
            }
            const id = `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
            newPlacements.push({
              id,
              patternId: pattern.id,
              startBar: bar,
              length: 1,
            });
          }
        }
        return { ...prev, placements: newPlacements };
      });
    },

    onCustomAction: (char: string, key: Key, _count: number) => {
      const pattern = patterns[cursorRow];
      if (!pattern) return false;

      // Place/remove pattern placement
      if (key.return || char === "x") {
        togglePatternPlacement(pattern.id, cursorBar);
        return true;
      }

      // Mute pattern in arrangement
      if (char === "m") {
        togglePatternMute(pattern.id);
        return true;
      }

      return false;
    },
  });

  // All input goes through vim
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

  // Check if a cell is in visual selection
  const isInVisualSelection = (rowIdx: number, barIdx: number) => {
    if (!vim.visualRange) return false;
    const { start, end } = vim.visualRange;
    const minRow = Math.min(start.row, end.row);
    const maxRow = Math.max(start.row, end.row);
    const minCol = Math.min(start.col, end.col);
    const maxCol = Math.max(start.col, end.col);
    return (
      rowIdx >= minRow &&
      rowIdx <= maxRow &&
      barIdx >= minCol &&
      barIdx <= maxCol
    );
  };

  // Helper to get bar styling
  const getBarStyle = (
    rowIndex: number,
    barIndex: number,
    patternId: number,
    isMuted: boolean,
  ) => {
    const placement = getPlacementAt(patternId, barIndex);
    const isCursor =
      rowIndex === cursorRow && barIndex === cursorBar && isFocused;
    const isPlayhead = barIndex === playheadBar;
    const isBeat = barIndex % 4 === 0;
    const isVisualSelected = isInVisualSelection(rowIndex, barIndex);

    let bgColor: string | undefined;
    let fgColor = "gray";
    let char = isBeat ? "┃" : "│";

    if (placement) {
      bgColor = isMuted ? "gray" : "magenta";
      fgColor = "white";
      char = "■■■";
    }

    if (isCursor) {
      bgColor = "blue";
      fgColor = "white";
    } else if (isVisualSelected) {
      bgColor = "yellow";
      fgColor = "black";
    } else if (isPlayhead) {
      bgColor = placement ? "green" : undefined;
      fgColor = placement ? "black" : "green";
    }

    const bold = !!placement;
    const dimColor =
      !placement && !isCursor && !isPlayhead && !isVisualSelected;
    const displayChar = placement
      ? char.slice(0, 3).padEnd(BAR_WIDTH)
      : (char + " ").padEnd(BAR_WIDTH);

    return { bgColor, fgColor, char: displayChar, bold, dimColor };
  };

  // Show empty state if no patterns
  if (patterns.length === 0) {
    return (
      <Box flexDirection="column" paddingX={1}>
        <Text dimColor>No patterns with content.</Text>
        <Text dimColor>
          Add steps in Channel Rack (6) or notes in Piano Roll (7).
        </Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Bar number header */}
      <Text wrap="truncate">
        <Text dimColor>{"Pattern".padEnd(PATTERN_NAME_WIDTH - 2)}</Text>
        <Text dimColor>{"M".padEnd(MUTE_WIDTH)}</Text>
        {Array.from({ length: viewportCols }, (_, i) => {
          const barNum = viewportLeft + i;
          if (barNum >= NUM_BARS) return null;
          const isPlayhead = barNum === playheadBar;
          const isBeat = barNum % 4 === 0;
          const isCursorCol = barNum === cursorBar && isFocused;
          return (
            <Text
              key={barNum}
              color={isPlayhead ? "green" : isBeat ? "yellow" : "gray"}
              bold={isCursorCol}
            >
              {String(barNum + 1)
                .padStart(2, " ")
                .padEnd(BAR_WIDTH)}
            </Text>
          );
        })}
      </Text>

      {/* Separator */}
      <Text wrap="truncate" dimColor>
        {"─".repeat(
          PATTERN_NAME_WIDTH + MUTE_WIDTH - 2 + viewportCols * BAR_WIDTH,
        )}
      </Text>

      {/* Pattern rows - only render viewport */}
      {patterns
        .slice(viewportTop, viewportTop + viewportHeight)
        .map((pattern, viewIndex) => {
          const rowIndex = viewportTop + viewIndex;
          const isMuted = arrangement.mutedPatterns.has(pattern.id);
          const isCurrentRow = rowIndex === cursorRow && isFocused;
          const hasPlacements = arrangement.placements.some(
            (p) => p.patternId === pattern.id,
          );

          // Pattern name styling
          const patternColor = isMuted
            ? "gray"
            : isCurrentRow
              ? "cyan"
              : "white";

          // Mute icon
          const muteIcon = isMuted ? "M" : "○";
          const muteColor = isMuted ? "red" : "green";

          return (
            <Text key={`pattern-${pattern.id}`} wrap="truncate">
              {/* Pattern name */}
              <Text
                color={patternColor}
                bold={isCurrentRow}
                dimColor={isMuted && !isCurrentRow}
              >
                {pattern.name.slice(0, 10).padEnd(PATTERN_NAME_WIDTH - 2)}
              </Text>

              {/* Mute indicator */}
              <Text color={muteColor} dimColor={!hasPlacements}>
                {muteIcon.padEnd(MUTE_WIDTH)}
              </Text>

              {/* Bars - only render visible viewport */}
              {Array.from({ length: viewportCols }, (_, i) => {
                const barIndex = viewportLeft + i;
                if (barIndex >= NUM_BARS) return null;
                const style = getBarStyle(
                  rowIndex,
                  barIndex,
                  pattern.id,
                  isMuted,
                );

                return (
                  <Text
                    key={barIndex}
                    backgroundColor={style.bgColor}
                    color={style.fgColor}
                    bold={style.bold}
                    dimColor={style.dimColor}
                  >
                    {style.char}
                  </Text>
                );
              })}
            </Text>
          );
        })}
    </Box>
  );
}
