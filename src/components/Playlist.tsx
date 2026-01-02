import { useState, useCallback } from "react";
import { Box, Text, useInput } from "ink";
import { useIsFocused } from "../context/FocusContext.js";
import {
  useSequencer,
  type PlaylistClip,
} from "../context/SequencerContext.js";
import { useVim } from "../hooks/useVim.js";
import type { Position, Range, Key } from "../lib/vim/types.js";

// Data type for yank/paste operations
interface ClipData {
  patternId: number;
  barOffset: number;
}

const NUM_BARS = 16;
const NUM_TRACKS = 99;
// Internal header rows: bar header (1) + separator (1)
const INTERNAL_HEADER_ROWS = 2;

// Fixed widths for non-bar columns
const TRACK_NAME_WIDTH = 10;
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
    playlistTracks: tracks,
    setPlaylistTracks: setTracks,
    currentPatternId,
  } = useSequencer();
  const [cursorTrack, setCursorTrack] = useState(0);
  const [cursorBar, setCursorBar] = useState(0);
  const [viewportTop, setViewportTop] = useState(0);
  const [viewportLeft, setViewportLeft] = useState(0);
  const [playheadBar] = useState(0);

  // Calculate viewport dimensions
  const viewportHeight = Math.max(5, availableHeight - INTERNAL_HEADER_ROWS);
  const fixedWidth = TRACK_NAME_WIDTH + MUTE_WIDTH + PADDING;
  const viewportCols = Math.max(
    4,
    Math.min(NUM_BARS, Math.floor((availableWidth - fixedWidth) / BAR_WIDTH)),
  );

  const moveCursor = useCallback(
    (newTrack: number) => {
      const clampedTrack = Math.max(0, Math.min(tracks.length - 1, newTrack));
      setCursorTrack(clampedTrack);
      setViewportTop((prev) => {
        if (clampedTrack < prev) {
          return clampedTrack;
        } else if (clampedTrack >= prev + viewportHeight) {
          return clampedTrack - viewportHeight + 1;
        }
        return prev;
      });
    },
    [tracks.length, viewportHeight],
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

  // Vim hook - library handles all motions via defaults
  const vim = useVim<ClipData[]>({
    dimensions: { rows: NUM_TRACKS, cols: NUM_BARS },

    getCursor: () => ({ row: cursorTrack, col: cursorBar }),

    setCursor: (pos: Position) => {
      moveCursor(pos.row);
      const newBar = Math.max(0, Math.min(NUM_BARS - 1, pos.col));
      setCursorBar(newBar);
      updateHorizontalViewport(newBar);
    },

    // No gridSemantics needed - default motions work for simple grid

    getDataInRange: (range: Range) => {
      const minRow = Math.min(range.start.row, range.end.row);
      const minCol = Math.min(range.start.col, range.end.col);
      const maxCol = Math.max(range.start.col, range.end.col);

      const track = tracks[minRow];
      if (!track) return [];

      return track.clips
        .filter(
          (clip: PlaylistClip) =>
            clip.startBar >= minCol && clip.startBar <= maxCol,
        )
        .map((clip: PlaylistClip) => ({
          patternId: clip.patternId,
          barOffset: clip.startBar - minCol,
        }));
    },

    deleteRange: (range: Range) => {
      const minRow = Math.min(range.start.row, range.end.row);
      const maxRow = Math.max(range.start.row, range.end.row);
      const minCol = Math.min(range.start.col, range.end.col);
      const maxCol = Math.max(range.start.col, range.end.col);

      const deleted: ClipData[] = [];

      setTracks((prev) =>
        prev.map((track, idx) => {
          if (idx < minRow || idx > maxRow) return track;

          const toDelete = track.clips.filter(
            (clip: PlaylistClip) =>
              clip.startBar >= minCol && clip.startBar <= maxCol,
          );

          for (const clip of toDelete) {
            deleted.push({
              patternId: clip.patternId,
              barOffset: clip.startBar - minCol,
            });
          }

          const remaining = track.clips.filter(
            (clip: PlaylistClip) =>
              clip.startBar < minCol || clip.startBar > maxCol,
          );

          return { ...track, clips: remaining };
        }),
      );

      return deleted;
    },

    insertData: (pos: Position, data: ClipData[]) => {
      setTracks((prev) =>
        prev.map((track, idx) => {
          if (idx !== pos.row) return track;

          const newClips = [...track.clips];
          for (const item of data) {
            const bar = pos.col + item.barOffset;
            if (bar >= 0 && bar < NUM_BARS) {
              // Remove any existing clip at this position
              const existingIdx = newClips.findIndex((c) => c.startBar === bar);
              if (existingIdx >= 0) {
                newClips.splice(existingIdx, 1);
              }
              newClips.push({
                patternId: item.patternId,
                startBar: bar,
                length: 1,
              });
            }
          }
          return { ...track, clips: newClips };
        }),
      );
    },

    onCustomAction: (char: string, key: Key, _count: number) => {
      // Place/remove clip
      if (key.return || char === "x") {
        setTracks((prev) =>
          prev.map((track, idx) => {
            if (idx !== cursorTrack) return track;

            const existingClipIndex = track.clips.findIndex(
              (clip: PlaylistClip) => clip.startBar === cursorBar,
            );

            if (existingClipIndex >= 0) {
              const newClips = [...track.clips];
              newClips.splice(existingClipIndex, 1);
              return { ...track, clips: newClips };
            } else {
              return {
                ...track,
                clips: [
                  ...track.clips,
                  {
                    patternId: currentPatternId,
                    startBar: cursorBar,
                    length: 1,
                  },
                ],
              };
            }
          }),
        );
        return true;
      }

      // Mute track
      if (char === "m") {
        setTracks((prev) =>
          prev.map((track, idx) => {
            if (idx !== cursorTrack) return track;
            return { ...track, muted: !track.muted };
          }),
        );
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

  // Helper to get clip at position
  const getClipAt = (
    trackIndex: number,
    bar: number,
  ): PlaylistClip | undefined => {
    return tracks[trackIndex]?.clips.find(
      (clip: PlaylistClip) =>
        clip.startBar <= bar && bar < clip.startBar + clip.length,
    );
  };

  // Check if a cell is in visual selection
  const isInVisualSelection = (trackIdx: number, barIdx: number) => {
    if (!vim.visualRange) return false;
    const { start, end } = vim.visualRange;
    const minRow = Math.min(start.row, end.row);
    const maxRow = Math.max(start.row, end.row);
    const minCol = Math.min(start.col, end.col);
    const maxCol = Math.max(start.col, end.col);
    return (
      trackIdx >= minRow &&
      trackIdx <= maxRow &&
      barIdx >= minCol &&
      barIdx <= maxCol
    );
  };

  // Helper to get bar styling
  const getBarStyle = (
    trackIndex: number,
    barIndex: number,
    track: { muted: boolean; clips: PlaylistClip[] },
    isEmpty: boolean,
  ) => {
    const clip = getClipAt(trackIndex, barIndex);
    const isCursor =
      trackIndex === cursorTrack && barIndex === cursorBar && isFocused;
    const isPlayhead = barIndex === playheadBar;
    const isBeat = barIndex % 4 === 0;
    const isVisualSelected = isInVisualSelection(trackIndex, barIndex);

    let bgColor: string | undefined;
    let fgColor = "gray";
    let char = isBeat ? "┃" : "│";

    if (clip) {
      bgColor = track.muted ? "gray" : "magenta";
      fgColor = "white";
      char = `P${clip.patternId}`;
    }

    if (isCursor) {
      bgColor = "blue";
      fgColor = "white";
    } else if (isVisualSelected) {
      bgColor = "yellow";
      fgColor = "black";
    } else if (isPlayhead) {
      bgColor = clip ? "green" : undefined;
      fgColor = clip ? "black" : "green";
    }

    const bold = !!clip;
    const dimColor = isEmpty && !isCursor && !isPlayhead && !isVisualSelected;
    const displayChar = clip
      ? char.slice(0, 3).padEnd(BAR_WIDTH)
      : (char + " ").padEnd(BAR_WIDTH);

    return { bgColor, fgColor, char: displayChar, bold, dimColor };
  };

  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Bar number header */}
      <Text wrap="truncate">
        <Text dimColor>{"Track".padEnd(TRACK_NAME_WIDTH - 2)}</Text>
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
          TRACK_NAME_WIDTH + MUTE_WIDTH - 2 + viewportCols * BAR_WIDTH,
        )}
      </Text>

      {/* Track rows - only render viewport */}
      {tracks
        .slice(viewportTop, viewportTop + viewportHeight)
        .map((track, viewIndex) => {
          const trackIndex = viewportTop + viewIndex;
          const isEmpty = track.clips.length === 0;
          const isCurrentTrack = trackIndex === cursorTrack && isFocused;

          // Track name styling
          const trackColor =
            isEmpty && !isCurrentTrack
              ? "gray"
              : track.muted
                ? "gray"
                : isCurrentTrack
                  ? "cyan"
                  : "white";

          // Mute icon
          const muteIcon = isEmpty ? "·" : track.muted ? "M" : "○";
          const muteColor = isEmpty ? "gray" : track.muted ? "red" : "green";

          return (
            <Text key={`track-${trackIndex}`} wrap="truncate">
              {/* Track name */}
              <Text
                color={trackColor}
                bold={isCurrentTrack && !isEmpty}
                dimColor={(track.muted || isEmpty) && !isCurrentTrack}
              >
                {(isEmpty ? "(empty)" : track.name.slice(0, 8)).padEnd(
                  TRACK_NAME_WIDTH - 2,
                )}
              </Text>

              {/* Mute indicator */}
              <Text color={muteColor} dimColor={isEmpty}>
                {muteIcon.padEnd(MUTE_WIDTH)}
              </Text>

              {/* Bars - only render visible viewport */}
              {Array.from({ length: viewportCols }, (_, i) => {
                const barIndex = viewportLeft + i;
                if (barIndex >= NUM_BARS) return null;
                const style = getBarStyle(trackIndex, barIndex, track, isEmpty);

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
