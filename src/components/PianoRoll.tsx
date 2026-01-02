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

// Fixed widths for non-step columns
const PITCH_LABEL_WIDTH = 5;
const STEP_WIDTH = 2;
const PADDING = 2; // paddingX={1} = 2 chars
const INTERNAL_HEADER_ROWS = 3; // header + separator + footer

interface PianoRollProps {
  availableHeight: number;
  availableWidth: number;
}

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

export default function PianoRoll({
  availableHeight,
  availableWidth,
}: PianoRollProps) {
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
  const [viewportLeft, setViewportLeft] = useState(0);
  const [placingNote, setPlacingNote] = useState<{ startStep: number } | null>(
    null,
  );

  // Calculate viewport dimensions
  const viewportHeight = Math.max(5, availableHeight - INTERNAL_HEADER_ROWS);
  const fixedWidth = PITCH_LABEL_WIDTH + PADDING;
  const viewportCols = Math.max(
    4,
    Math.min(NUM_STEPS, Math.floor((availableWidth - fixedWidth) / STEP_WIDTH)),
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

  // Auto-scroll viewport vertically
  const scrollToCursor = useCallback(
    (pitch: number) => {
      if (pitch > viewportTop) {
        setViewportTop(pitch);
      } else if (pitch < viewportTop - viewportHeight + 1) {
        setViewportTop(pitch + viewportHeight - 1);
      }
    },
    [viewportTop, viewportHeight],
  );

  // Handle horizontal viewport scrolling when cursor moves
  const updateHorizontalViewport = useCallback(
    (newStep: number) => {
      setViewportLeft((prev) => {
        if (newStep < prev) {
          return newStep;
        } else if (newStep >= prev + viewportCols) {
          return newStep - viewportCols + 1;
        }
        return prev;
      });
    },
    [viewportCols],
  );

  // Register cursor setter for undo/redo restoration
  useEffect(() => {
    registerCursorSetter("pianoRoll", (pos) => {
      const newPitch = rowToPitch(pos.row);
      setCursorPitch(newPitch);
      setCursorStep(pos.col);
      scrollToCursor(newPitch);
      updateHorizontalViewport(pos.col);
    });
    return () => unregisterCursorSetter("pianoRoll");
  }, [
    registerCursorSetter,
    unregisterCursorSetter,
    scrollToCursor,
    updateHorizontalViewport,
  ]);

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
      const newStep = Math.max(0, Math.min(NUM_STEPS - 1, pos.col));
      setCursorPitch(newPitch);
      setCursorStep(newStep);
      scrollToCursor(newPitch);
      updateHorizontalViewport(newStep);
    },

    // Library handles all motions via gridSemantics
    gridSemantics: {
      zones: [
        {
          name: "steps",
          colRange: [0, NUM_STEPS - 1],
          isMain: true,
          hasContent: (pos) =>
            !!getNoteStartingAt(rowToPitch(pos.row), pos.col),
          wordInterval: 4,
        },
      ],
    },

    // Custom e motion for "end of measure" behavior
    customMotions: {
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
        const halfPage = Math.floor(viewportHeight / 2);
        const newPitch = Math.min(MAX_PITCH, cursorPitch + halfPage);
        setCursorPitch(newPitch);
        setViewportTop((vt) => Math.min(MAX_PITCH, vt + halfPage));
        return true;
      }
      if (key.ctrl && char === "d") {
        if (placingNote) setPlacingNote(null);
        const halfPage = Math.floor(viewportHeight / 2);
        const newPitch = Math.max(MIN_PITCH, cursorPitch - halfPage);
        setCursorPitch(newPitch);
        setViewportTop((vt) =>
          Math.max(MIN_PITCH + viewportHeight - 1, vt - halfPage),
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

    onEscape: (prevMode) => {
      // Cancel placement first
      if (placingNote) {
        setPlacingNote(null);
        return;
      }
      // Only exit piano roll if we were already in normal mode
      // (not when escaping from visual/operator modes)
      if (prevMode === "normal") {
        exitPianoRoll();
      }
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
    p > viewportTop - viewportHeight && p >= MIN_PITCH;
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

  // Helper to get step styling
  const getStepStyle = (pitch: number, stepIndex: number) => {
    const note = getNoteCovering(pitch, stepIndex);
    const noteStart = isNoteStart(pitch, stepIndex);
    const isCursor =
      pitch === cursorPitch && stepIndex === cursorStep && isFocused;
    const isPlayhead = stepIndex === playheadStep && isPlaying;
    const isBeat = stepIndex % 4 === 0;
    const isBlack = isBlackKey(pitch);
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
      if (noteStart) {
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

    const bold = !!note || isPlayhead || !!isInPlacement;
    const dimColor =
      isBlack && !note && !isCursor && !isPlayhead && !isInPlacement;

    return { bgColor, fgColor, char, bold, dimColor };
  };

  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Header - step numbers */}
      <Text wrap="truncate">
        <Text dimColor>{"Note".padEnd(PITCH_LABEL_WIDTH)}</Text>
        {Array.from({ length: viewportCols }, (_, i) => {
          const stepNum = viewportLeft + i;
          if (stepNum >= NUM_STEPS) return null;
          const isPlayhead = stepNum === playheadStep && isPlaying;
          const isBeat = stepNum % 4 === 0;
          const isCursorCol = stepNum === cursorStep && isFocused;
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
        <Text dimColor> {getModeIndicator()}</Text>
      </Text>

      {/* Separator */}
      <Text wrap="truncate" dimColor>
        {"─".repeat(PITCH_LABEL_WIDTH + viewportCols * STEP_WIDTH)}
      </Text>

      {/* Piano roll grid */}
      {pitchRange.map((pitch) => {
        const isBlack = isBlackKey(pitch);
        const isCursorRow = pitch === cursorPitch && isFocused;

        return (
          <Text key={`pitch-${pitch}`} wrap="truncate">
            {/* Pitch label */}
            <Text
              color={isCursorRow ? "cyan" : isBlack ? "gray" : "white"}
              bold={isCursorRow}
              dimColor={isBlack && !isCursorRow}
            >
              {getPitchName(pitch).padStart(4, " ").padEnd(PITCH_LABEL_WIDTH)}
            </Text>

            {/* Steps - only render visible viewport */}
            {Array.from({ length: viewportCols }, (_, i) => {
              const stepIndex = viewportLeft + i;
              if (stepIndex >= NUM_STEPS) return null;
              const style = getStepStyle(pitch, stepIndex);

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

      {/* Footer info */}
      <Text wrap="truncate" dimColor>
        hjkl:Move x:Place/Edit {"<>"}:Nudge v:Visual ^v:Block y:Yank p:Paste
        d:Del
      </Text>
    </Box>
  );
}
