import { useState, useCallback, useEffect, useMemo } from "react";
import { Box, Text, useInput } from "ink";
import { useIsFocused, useFocusContext } from "../context/FocusContext.js";
import { useSequencer, type Note } from "../context/SequencerContext.js";
import { useCommands } from "../context/CommandContext.js";
import { previewSamplePitched, getSamplePath } from "../lib/audio.js";
import { previewSynthNote } from "../lib/synth.js";
import { useVim } from "../hooks/useVim.js";
import type { Position, Range, Key } from "../lib/vim/types.js";

const NUM_STEPS = 16;
const MIN_PITCH = 36; // C2
const MAX_PITCH = 84; // C6
const PITCH_RANGE = MAX_PITCH - MIN_PITCH + 1; // 49 pitches
const VIEWPORT_HEIGHT = 16;

const PITCH_NAMES = [
  "C",
  "C#",
  "D",
  "D#",
  "E",
  "F",
  "F#",
  "G",
  "G#",
  "A",
  "A#",
  "B",
];

function getPitchName(pitch: number): string {
  const note = PITCH_NAMES[pitch % 12];
  const octave = Math.floor(pitch / 12);
  return `${note}${octave}`;
}

function isBlackKey(pitch: number): boolean {
  const semitone = pitch % 12;
  return [1, 3, 6, 8, 10].includes(semitone);
}

// Convert between pitch and row (row 0 = MAX_PITCH, row increases as pitch decreases)
function pitchToRow(pitch: number): number {
  return MAX_PITCH - pitch;
}

function rowToPitch(row: number): number {
  return MAX_PITCH - row;
}

// Yanked note representation (relative positions)
interface YankedNote {
  pitchOffset: number;
  stepOffset: number;
  duration: number;
}

export default function PianoRoll() {
  const isFocused = useIsFocused("pianoRoll");
  const { exitPianoRoll, registerCursorSetter, unregisterCursorSetter } =
    useFocusContext();
  const {
    channels,
    selectedChannel,
    playheadStep,
    isPlaying,
    currentPatternId,
  } = useSequencer();
  const { addNote, removeNote, updateNote } = useCommands();

  const [cursorPitch, setCursorPitch] = useState(60); // C4
  const [cursorStep, setCursorStep] = useState(0);
  const [viewportTop, setViewportTop] = useState(67); // Show around C4-C5
  const [placingNote, setPlacingNote] = useState<{ startStep: number } | null>(
    null,
  );

  const channel = channels[selectedChannel];
  const notes: Note[] = useMemo(() => channel?.notes || [], [channel?.notes]);

  // Find note at cursor position (note that starts at this step)
  const getNoteStartingAt = useCallback(
    (pitch: number, step: number): Note | undefined => {
      if (!notes || !Array.isArray(notes)) return undefined;
      return notes.find((n) => n && n.pitch === pitch && n.startStep === step);
    },
    [notes],
  );

  // Find note covering cursor position
  const getNoteCovering = useCallback(
    (pitch: number, step: number): Note | undefined => {
      if (!notes || !Array.isArray(notes)) return undefined;
      return notes.find(
        (n) =>
          n &&
          n.pitch === pitch &&
          step >= n.startStep &&
          step < n.startStep + n.duration,
      );
    },
    [notes],
  );

  // Is this the start of a note?
  const isNoteStart = useCallback(
    (pitch: number, step: number): boolean => {
      return !!getNoteStartingAt(pitch, step);
    },
    [getNoteStartingAt],
  );

  // Auto-scroll viewport
  const scrollToCursor = useCallback(
    (pitch: number) => {
      if (pitch > viewportTop) {
        setViewportTop(pitch);
      } else if (pitch < viewportTop - VIEWPORT_HEIGHT + 1) {
        setViewportTop(pitch + VIEWPORT_HEIGHT - 1);
      }
    },
    [viewportTop],
  );

  // Register cursor setter for undo/redo restoration
  useEffect(() => {
    registerCursorSetter("pianoRoll", (pos) => {
      const newPitch = rowToPitch(pos.row);
      setCursorPitch(newPitch);
      setCursorStep(pos.col);
      scrollToCursor(newPitch);
    });
    return () => unregisterCursorSetter("pianoRoll");
  }, [registerCursorSetter, unregisterCursorSetter, scrollToCursor]);

  // Preview note at pitch
  const previewAtPitch = useCallback(
    (pitch: number) => {
      if (!channel) return;
      if (channel.type === "synth") {
        previewSynthNote(channel.synthPatch, pitch);
      } else if (channel.sample) {
        previewSamplePitched(getSamplePath(channel.sample), pitch);
      }
    },
    [channel],
  );

  // Vim hook
  const vim = useVim<YankedNote[]>({
    dimensions: { rows: PITCH_RANGE, cols: NUM_STEPS },

    getCursor: () => ({ row: pitchToRow(cursorPitch), col: cursorStep }),

    setCursor: (pos: Position) => {
      const newPitch = rowToPitch(
        Math.max(0, Math.min(PITCH_RANGE - 1, pos.row)),
      );
      setCursorPitch(newPitch);
      setCursorStep(Math.max(0, Math.min(NUM_STEPS - 1, pos.col)));
      scrollToCursor(newPitch);
    },

    motions: {
      h: (count, cursor) => ({
        position: { row: cursor.row, col: Math.max(0, cursor.col - count) },
      }),
      l: (count, cursor) => ({
        position: {
          row: cursor.row,
          col: Math.min(NUM_STEPS - 1, cursor.col + count),
        },
      }),
      // k = up = increase pitch = decrease row
      k: (count, cursor) => ({
        position: { row: Math.max(0, cursor.row - count), col: cursor.col },
        linewise: true,
      }),
      // j = down = decrease pitch = increase row
      j: (count, cursor) => ({
        position: {
          row: Math.min(PITCH_RANGE - 1, cursor.row + count),
          col: cursor.col,
        },
        linewise: true,
      }),
      e: (_count, cursor) => {
        // End of measure
        const currentBar = Math.floor(cursor.col / 4);
        const endOfCurrentBar = currentBar * 4 + 3;
        let step: number;
        if (cursor.col === endOfCurrentBar && endOfCurrentBar < NUM_STEPS - 1) {
          step = Math.min(endOfCurrentBar + 4, NUM_STEPS - 1);
        } else {
          step = Math.min(endOfCurrentBar, NUM_STEPS - 1);
        }
        return { position: { row: cursor.row, col: step }, inclusive: true };
      },
      zero: (_count, cursor) => ({
        position: { row: cursor.row, col: 0 },
      }),
      dollar: (_count, cursor) => ({
        position: { row: cursor.row, col: NUM_STEPS - 1 },
        inclusive: true,
      }),
      // g = top = highest pitch = row 0
      gg: (_count, cursor) => ({
        position: { row: 0, col: cursor.col },
      }),
      // G = bottom = lowest pitch = last row
      G: (_count, cursor) => ({
        position: { row: PITCH_RANGE - 1, col: cursor.col },
      }),
    },

    // Word boundary for w/b motions - library handles vim semantics
    wordBoundary: {
      findNext: (pos) => {
        const pitch = rowToPitch(pos.row);

        // Look for next note on this pitch
        for (let i = pos.col + 1; i < NUM_STEPS; i++) {
          if (getNoteStartingAt(pitch, i)) return { row: pos.row, col: i };
        }

        // Fallback to next bar boundary
        const nextBar = Math.ceil((pos.col + 1) / 4) * 4;
        if (nextBar < NUM_STEPS) return { row: pos.row, col: nextBar };

        // At end - return null to stay in place (vim behavior)
        return null;
      },

      findPrev: (pos) => {
        const pitch = rowToPitch(pos.row);

        // Look for previous note on this pitch
        for (let i = pos.col - 1; i >= 0; i--) {
          if (getNoteStartingAt(pitch, i)) return { row: pos.row, col: i };
        }

        // Fallback to previous bar boundary
        const prevBar = Math.floor((pos.col - 1) / 4) * 4;
        if (prevBar >= 0) return { row: pos.row, col: prevBar };

        // At beginning - return null to stay in place (vim behavior)
        return null;
      },
    },

    getDataInRange: (range: Range) => {
      // Get notes in range and convert to relative positions
      const minRow = Math.min(range.start.row, range.end.row);
      const maxRow = Math.max(range.start.row, range.end.row);
      const minCol = Math.min(range.start.col, range.end.col);
      const maxCol = Math.max(range.start.col, range.end.col);
      const minPitch = rowToPitch(maxRow);
      const maxPitch = rowToPitch(minRow);

      const selected = notes.filter(
        (n) =>
          n.pitch >= minPitch &&
          n.pitch <= maxPitch &&
          n.startStep >= minCol &&
          n.startStep <= maxCol,
      );

      return selected.map((n) => ({
        pitchOffset: n.pitch - minPitch,
        stepOffset: n.startStep - minCol,
        duration: n.duration,
      }));
    },

    deleteRange: (range: Range) => {
      const minRow = Math.min(range.start.row, range.end.row);
      const maxRow = Math.max(range.start.row, range.end.row);
      const minCol = Math.min(range.start.col, range.end.col);
      const maxCol = Math.max(range.start.col, range.end.col);
      const minPitch = rowToPitch(maxRow);
      const maxPitch = rowToPitch(minRow);

      const toDelete = notes.filter(
        (n) =>
          n.pitch >= minPitch &&
          n.pitch <= maxPitch &&
          ((n.startStep >= minCol && n.startStep <= maxCol) ||
            (n.startStep < minCol && n.startStep + n.duration > minCol)),
      );

      const yanked = toDelete.map((n) => ({
        pitchOffset: n.pitch - minPitch,
        stepOffset: n.startStep - minCol,
        duration: n.duration,
      }));

      const cursorInfo = {
        context: "pianoRoll" as const,
        position: { row: range.start.row, col: range.start.col },
      };
      for (const note of toDelete) {
        removeNote(currentPatternId, selectedChannel, note.id, cursorInfo);
      }

      return yanked;
    },

    insertData: (pos: Position, data: YankedNote[]) => {
      const basePitch = rowToPitch(pos.row);
      const baseStep = pos.col;
      const cursorInfo = {
        context: "pianoRoll" as const,
        position: { row: pos.row, col: pos.col },
      };
      for (const yanked of data) {
        const pitch = basePitch + yanked.pitchOffset;
        const step = baseStep + yanked.stepOffset;
        if (
          pitch >= MIN_PITCH &&
          pitch <= MAX_PITCH &&
          step >= 0 &&
          step + yanked.duration <= NUM_STEPS
        ) {
          addNote(
            currentPatternId,
            selectedChannel,
            pitch,
            step,
            yanked.duration,
            cursorInfo,
          );
        }
      }
    },

    onCustomAction: (char: string, key: Key, count: number) => {
      // Helper to get current cursor info for undo/redo
      const getCursorInfo = () => ({
        context: "pianoRoll" as const,
        position: { row: pitchToRow(cursorPitch), col: cursorStep },
      });

      // Escape behavior: cancel placement first, then let vim handle
      if (key.escape) {
        if (placingNote) {
          setPlacingNote(null);
          return true;
        }
        // If nothing special, exit piano roll on escape (after vim resets)
        if (vim.mode === "normal" && !vim.operator) {
          exitPianoRoll();
          return true;
        }
        return false; // Let vim handle escape for visual/operator modes
      }

      // Octave jumps (K and J, uppercase)
      if (char === "K") {
        if (placingNote) setPlacingNote(null);
        const newPitch = Math.min(MAX_PITCH, cursorPitch + 12 * count);
        setCursorPitch(newPitch);
        scrollToCursor(newPitch);
        return true;
      }
      if (char === "J") {
        if (placingNote) setPlacingNote(null);
        const newPitch = Math.max(MIN_PITCH, cursorPitch - 12 * count);
        setCursorPitch(newPitch);
        scrollToCursor(newPitch);
        return true;
      }

      // Page up/down (Ctrl+u/d)
      if (key.ctrl && char === "u") {
        if (placingNote) setPlacingNote(null);
        const halfPage = Math.floor(VIEWPORT_HEIGHT / 2);
        const newPitch = Math.min(MAX_PITCH, cursorPitch + halfPage);
        setCursorPitch(newPitch);
        setViewportTop((vt) => Math.min(MAX_PITCH, vt + halfPage));
        return true;
      }
      if (key.ctrl && char === "d") {
        if (placingNote) setPlacingNote(null);
        const halfPage = Math.floor(VIEWPORT_HEIGHT / 2);
        const newPitch = Math.max(MIN_PITCH, cursorPitch - halfPage);
        setCursorPitch(newPitch);
        setViewportTop((vt) =>
          Math.max(MIN_PITCH + VIEWPORT_HEIGHT - 1, vt - halfPage),
        );
        return true;
      }

      // Note placement with x or Enter
      if (key.return || char === "x") {
        if (placingNote) {
          // Finish placing note
          const startStep = Math.min(placingNote.startStep, cursorStep);
          const endStep = Math.max(placingNote.startStep, cursorStep);
          const duration = endStep - startStep + 1;
          addNote(
            currentPatternId,
            selectedChannel,
            cursorPitch,
            startStep,
            duration,
            getCursorInfo(),
          );
          setPlacingNote(null);
        } else {
          const existingNote = getNoteCovering(cursorPitch, cursorStep);
          if (existingNote) {
            removeNote(
              currentPatternId,
              selectedChannel,
              existingNote.id,
              getCursorInfo(),
            );
            setPlacingNote({ startStep: existingNote.startStep });
          } else {
            setPlacingNote({ startStep: cursorStep });
          }
        }
        return true;
      }

      // Nudge notes with < and >
      if (char === "<") {
        const note = getNoteCovering(cursorPitch, cursorStep);
        if (note && note.startStep > 0) {
          updateNote(
            currentPatternId,
            selectedChannel,
            note.id,
            { startStep: note.startStep - 1 },
            getCursorInfo(),
          );
          setCursorStep((prev) => Math.max(0, prev - 1));
        }
        return true;
      }
      if (char === ">") {
        const note = getNoteCovering(cursorPitch, cursorStep);
        if (note && note.startStep + note.duration < NUM_STEPS) {
          updateNote(
            currentPatternId,
            selectedChannel,
            note.id,
            { startStep: note.startStep + 1 },
            getCursorInfo(),
          );
          setCursorStep((prev) => Math.min(NUM_STEPS - 1, prev + 1));
        }
        return true;
      }

      // Preview at cursor pitch
      if (char === "s") {
        previewAtPitch(cursorPitch);
        return true;
      }

      return false;
    },

    onModeChange: (_mode) => {
      // Cancel placement when entering visual or operator mode
      if (placingNote) setPlacingNote(null);
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
  const isInVisualSelection = (pitch: number, step: number): boolean => {
    if (!vim.visualRange) return false;
    const { start, end } = vim.visualRange;
    const row = pitchToRow(pitch);
    const minRow = Math.min(start.row, end.row);
    const maxRow = Math.max(start.row, end.row);
    const minCol = Math.min(start.col, end.col);
    const maxCol = Math.max(start.col, end.col);
    return row >= minRow && row <= maxRow && step >= minCol && step <= maxCol;
  };

  // Calculate visible pitch range
  const pitchRange: number[] = [];
  for (
    let p = viewportTop;
    p > viewportTop - VIEWPORT_HEIGHT && p >= MIN_PITCH;
    p--
  ) {
    pitchRange.push(p);
  }

  // Calculate placement preview range
  const getPlacementRange = () => {
    if (!placingNote) return null;
    const start = Math.min(placingNote.startStep, cursorStep);
    const end = Math.max(placingNote.startStep, cursorStep);
    return { start, end };
  };
  const placementRange = getPlacementRange();

  // Mode indicator
  const getModeIndicator = () => {
    if (placingNote) return "PLACE";
    if (vim.mode === "visual") return "VISUAL";
    if (vim.mode === "visual-block") return "V-BLOCK";
    if (vim.mode === "operator-pending") return `${vim.operator}...`;
    return "";
  };

  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Header - step numbers */}
      <Box>
        <Box width={5}>
          <Text dimColor>Note</Text>
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
              bold={i === cursorStep && isFocused}
            >
              {(i + 1).toString(16).toUpperCase()}
            </Text>
          </Box>
        ))}
        <Box marginLeft={1}>
          <Text dimColor>{getModeIndicator()}</Text>
        </Box>
      </Box>

      {/* Separator */}
      <Box>
        <Text dimColor>{"─".repeat(5 + NUM_STEPS * 2 + 6)}</Text>
      </Box>

      {/* Piano roll grid */}
      {pitchRange.map((pitch) => {
        const isBlack = isBlackKey(pitch);
        const isCursorRow = pitch === cursorPitch && isFocused;

        return (
          <Box key={`pitch-${pitch}`}>
            {/* Pitch label */}
            <Box width={5}>
              <Text
                color={isCursorRow ? "cyan" : isBlack ? "gray" : "white"}
                bold={isCursorRow}
                dimColor={isBlack && !isCursorRow}
              >
                {getPitchName(pitch).padStart(4, " ")}
              </Text>
            </Box>

            {/* Steps */}
            {Array.from({ length: NUM_STEPS }, (_, stepIndex) => {
              const note = getNoteCovering(pitch, stepIndex);
              const isStart = isNoteStart(pitch, stepIndex);
              const isCursor =
                pitch === cursorPitch && stepIndex === cursorStep && isFocused;
              const isPlayhead = stepIndex === playheadStep && isPlaying;
              const isBeat = stepIndex % 4 === 0;
              const isInPlacement =
                placementRange &&
                pitch === cursorPitch &&
                stepIndex >= placementRange.start &&
                stepIndex <= placementRange.end;
              const isVisualSelected = isInVisualSelection(pitch, stepIndex);

              let bgColor: string | undefined;
              let fgColor = isBlack ? "gray" : "white";
              let char = isBeat ? "┃" : "│";

              if (note) {
                if (isStart) {
                  char = "█";
                  fgColor = "magenta";
                } else {
                  char = "─";
                  fgColor = "magenta";
                }
              }

              // Placement preview
              if (isInPlacement && !note) {
                char = "░";
                fgColor = "cyan";
              }

              if (isCursor && isPlayhead) {
                bgColor = "greenBright";
                fgColor = "black";
              } else if (isCursor) {
                bgColor = "blue";
                fgColor = "white";
              } else if (isVisualSelected) {
                bgColor = "yellow";
                fgColor = "black";
              } else if (isPlayhead) {
                bgColor = "green";
                fgColor = "black";
              } else if (isInPlacement) {
                bgColor = "cyan";
                fgColor = "black";
              }

              return (
                <Box key={`step-${pitch}-${stepIndex}`} width={2}>
                  <Text
                    backgroundColor={bgColor}
                    color={fgColor}
                    bold={!!note || isPlayhead || !!isInPlacement}
                    dimColor={
                      isBlack &&
                      !note &&
                      !isCursor &&
                      !isPlayhead &&
                      !isInPlacement
                    }
                  >
                    {char}
                  </Text>
                </Box>
              );
            })}
          </Box>
        );
      })}

      {/* Footer info */}
      <Box marginTop={1}>
        <Text dimColor>
          hjkl:Move x:Place/Edit {"<>"}:Nudge v:Visual ^v:Block y:Yank p:Paste
          d:Del
        </Text>
      </Box>
    </Box>
  );
}
