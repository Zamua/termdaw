import { useState, useEffect, useCallback } from "react";
import { Box, Text, useInput, useStdout } from "ink";
import { useIsFocused } from "../context/FocusContext.js";
import { useVim } from "../hooks/useVim.js";
import type { Position, Range, Key } from "../lib/vim/types.js";

interface Clip {
  patternId: number;
  startBar: number;
  length: number;
}

interface Track {
  name: string;
  clips: Clip[];
  muted: boolean;
}

// Data type for yank/paste operations
interface ClipData {
  patternId: number;
  barOffset: number;
}

const NUM_BARS = 16;
const NUM_TRACKS = 99;
const HEADER_ROWS = 5;

const createDefaultTracks = (): Track[] =>
  Array.from({ length: NUM_TRACKS }, (_, i) => ({
    name: `Track ${i + 1}`,
    clips: [],
    muted: false,
  }));

export default function Playlist() {
  const isFocused = useIsFocused("playlist");
  const { stdout } = useStdout();
  const [termHeight, setTermHeight] = useState(stdout?.rows || 24);
  const [tracks, setTracks] = useState<Track[]>(createDefaultTracks);
  const [cursorTrack, setCursorTrack] = useState(0);
  const [cursorBar, setCursorBar] = useState(0);
  const [viewportTop, setViewportTop] = useState(0);
  const [playheadBar] = useState(0);
  const [selectedPattern] = useState(1);

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

  // Vim hook
  const vim = useVim<ClipData[]>({
    dimensions: { rows: NUM_TRACKS, cols: NUM_BARS },

    getCursor: () => ({ row: cursorTrack, col: cursorBar }),

    setCursor: (pos: Position) => {
      moveCursor(pos.row);
      setCursorBar(Math.max(0, Math.min(NUM_BARS - 1, pos.col)));
    },

    motions: {
      h: (count, cursor) => ({
        position: { row: cursor.row, col: Math.max(0, cursor.col - count) },
      }),
      l: (count, cursor) => ({
        position: {
          row: cursor.row,
          col: Math.min(NUM_BARS - 1, cursor.col + count),
        },
      }),
      j: (count, cursor) => ({
        position: {
          row: Math.min(NUM_TRACKS - 1, cursor.row + count),
          col: cursor.col,
        },
        linewise: true,
      }),
      k: (count, cursor) => ({
        position: { row: Math.max(0, cursor.row - count), col: cursor.col },
        linewise: true,
      }),
      zero: (_count, cursor) => ({
        position: { row: cursor.row, col: 0 },
      }),
      dollar: (_count, cursor) => ({
        position: { row: cursor.row, col: NUM_BARS - 1 },
        inclusive: true,
      }),
      gg: (_count, cursor) => ({
        position: { row: 0, col: cursor.col },
      }),
      G: (_count, cursor) => ({
        position: { row: NUM_TRACKS - 1, col: cursor.col },
      }),
    },

    getDataInRange: (range: Range) => {
      const minRow = Math.min(range.start.row, range.end.row);
      const minCol = Math.min(range.start.col, range.end.col);
      const maxCol = Math.max(range.start.col, range.end.col);

      const track = tracks[minRow];
      if (!track) return [];

      return track.clips
        .filter((clip) => clip.startBar >= minCol && clip.startBar <= maxCol)
        .map((clip) => ({
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
            (clip) => clip.startBar >= minCol && clip.startBar <= maxCol,
          );

          for (const clip of toDelete) {
            deleted.push({
              patternId: clip.patternId,
              barOffset: clip.startBar - minCol,
            });
          }

          const remaining = track.clips.filter(
            (clip) => clip.startBar < minCol || clip.startBar > maxCol,
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
              (clip) => clip.startBar === cursorBar,
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
                    patternId: selectedPattern,
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
  const getClipAt = (trackIndex: number, bar: number): Clip | undefined => {
    return tracks[trackIndex]?.clips.find(
      (clip) => clip.startBar <= bar && bar < clip.startBar + clip.length,
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

  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Bar number header */}
      <Box>
        <Box width={10}>
          <Text dimColor>Track</Text>
        </Box>
        <Box width={3}>
          <Text dimColor>M</Text>
        </Box>
        {Array.from({ length: NUM_BARS }, (_, i) => (
          <Box key={`bar-header-${i}`} width={4}>
            <Text
              color={
                i === playheadBar ? "green" : i % 4 === 0 ? "yellow" : "gray"
              }
              bold={i === cursorBar && isFocused}
            >
              {String(i + 1).padStart(2, " ")}
            </Text>
          </Box>
        ))}
      </Box>

      {/* Separator */}
      <Box>
        <Text dimColor>{"─".repeat(10 + 3 + NUM_BARS * 4)}</Text>
      </Box>

      {/* Track rows - only render viewport */}
      {tracks
        .slice(viewportTop, viewportTop + viewportHeight)
        .map((track, viewIndex) => {
          const trackIndex = viewportTop + viewIndex;
          const isEmpty = track.clips.length === 0;
          const isCurrentTrack = trackIndex === cursorTrack && isFocused;

          return (
            <Box key={`track-${trackIndex}`}>
              {/* Track name */}
              <Box width={10}>
                <Text
                  color={
                    isEmpty && !isCurrentTrack
                      ? "gray"
                      : track.muted
                        ? "gray"
                        : isCurrentTrack
                          ? "cyan"
                          : "white"
                  }
                  bold={isCurrentTrack && !isEmpty}
                  dimColor={(track.muted || isEmpty) && !isCurrentTrack}
                >
                  {isEmpty ? "(empty)".padEnd(8) : track.name.slice(0, 8)}
                </Text>
              </Box>

              {/* Mute indicator */}
              <Box width={3}>
                <Text
                  color={isEmpty ? "gray" : track.muted ? "red" : "green"}
                  dimColor={isEmpty}
                >
                  {isEmpty ? "·" : track.muted ? "M" : "○"}
                </Text>
              </Box>

              {/* Bars */}
              {Array.from({ length: NUM_BARS }, (_, barIndex) => {
                const clip = getClipAt(trackIndex, barIndex);
                const isCursor =
                  trackIndex === cursorTrack &&
                  barIndex === cursorBar &&
                  isFocused;
                const isPlayhead = barIndex === playheadBar;
                const isBeat = barIndex % 4 === 0;
                const isVisualSelected = isInVisualSelection(
                  trackIndex,
                  barIndex,
                );

                let bgColor: string | undefined;
                let fgColor = isEmpty ? "gray" : "gray";
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

                return (
                  <Box key={`bar-${trackIndex}-${barIndex}`} width={4}>
                    <Text
                      backgroundColor={bgColor}
                      color={fgColor}
                      bold={!!clip}
                      dimColor={
                        isEmpty && !isCursor && !isPlayhead && !isVisualSelected
                      }
                    >
                      {clip ? char.slice(0, 3).padEnd(3, " ") : char + "  "}
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
