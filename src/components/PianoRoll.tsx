import { useState, useCallback, useRef } from 'react';
import { Box, Text, useInput } from 'ink';
import { useIsFocused, useFocusContext } from '../context/FocusContext.js';
import { useSequencer, type Note } from '../context/SequencerContext.js';
import { previewSamplePitched, getSamplePath } from '../lib/audio.js';
import { previewSynthNote } from '../lib/synth.js';

const NUM_STEPS = 16;
const MIN_PITCH = 36;  // C2
const MAX_PITCH = 84;  // C6
const VIEWPORT_HEIGHT = 16; // Visible rows

const PITCH_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];

function getPitchName(pitch: number): string {
  const note = PITCH_NAMES[pitch % 12];
  const octave = Math.floor(pitch / 12);
  return `${note}${octave}`;
}

function isBlackKey(pitch: number): boolean {
  const semitone = pitch % 12;
  return [1, 3, 6, 8, 10].includes(semitone);
}

type Operator = 'd' | 'y' | null;
type VisualMode = 'none' | 'char' | 'block';

interface YankedNote {
  pitchOffset: number;  // Relative to anchor pitch
  stepOffset: number;   // Relative to anchor step
  duration: number;
}

export default function PianoRoll() {
  const isFocused = useIsFocused('pianoRoll');
  const { exitPianoRoll } = useFocusContext();
  const { channels, selectedChannel, playheadStep, isPlaying, addNote, removeNote, updateNote } = useSequencer();

  const [cursorPitch, setCursorPitch] = useState(60);  // C4
  const [cursorStep, setCursorStep] = useState(0);
  const [viewportTop, setViewportTop] = useState(67);  // Show around C4-C5
  const [countPrefix, setCountPrefix] = useState('');
  const [pendingOperator, setPendingOperator] = useState<Operator>(null);

  // Note placement mode: when placing, we're setting the endpoint
  const [placingNote, setPlacingNote] = useState<{ startStep: number } | null>(null);

  // Visual selection mode
  const [visualMode, setVisualMode] = useState<VisualMode>('none');
  const [visualStart, setVisualStart] = useState<{ pitch: number; step: number } | null>(null);

  // Yank buffer for copy/paste
  const yankBuffer = useRef<YankedNote[]>([]);

  const channel = channels[selectedChannel];
  const notes: Note[] = channel?.notes || [];

  // Get the count from prefix or default to 1
  const getCount = useCallback(() => {
    const count = countPrefix ? parseInt(countPrefix, 10) : 1;
    setCountPrefix('');
    return count;
  }, [countPrefix]);

  // Find note at cursor position (note that starts at this step)
  const getNoteStartingAt = useCallback((pitch: number, step: number): Note | undefined => {
    if (!notes || !Array.isArray(notes)) return undefined;
    return notes.find(n => n && n.pitch === pitch && n.startStep === step);
  }, [notes]);

  // Find note covering cursor position (cursor is within note's range)
  const getNoteCovering = useCallback((pitch: number, step: number): Note | undefined => {
    if (!notes || !Array.isArray(notes)) return undefined;
    return notes.find(n =>
      n && n.pitch === pitch &&
      step >= n.startStep &&
      step < n.startStep + n.duration
    );
  }, [notes]);

  // Check if pitch/step has a note (for rendering)
  const hasNoteAt = useCallback((pitch: number, step: number): Note | undefined => {
    return getNoteCovering(pitch, step);
  }, [getNoteCovering]);

  // Is this the start of a note?
  const isNoteStart = useCallback((pitch: number, step: number): boolean => {
    return !!getNoteStartingAt(pitch, step);
  }, [getNoteStartingAt]);

  // Find next note on current pitch, or next bar line if no note found
  const findNextNote = useCallback((fromStep: number): number => {
    // First try to find next note
    for (let i = fromStep + 1; i < NUM_STEPS; i++) {
      if (getNoteStartingAt(cursorPitch, i)) return i;
    }
    // No note found, jump to next bar line (0, 4, 8, 12)
    const nextBar = Math.ceil((fromStep + 1) / 4) * 4;
    if (nextBar < NUM_STEPS) {
      return nextBar;
    }
    // Wrap to beginning
    return 0;
  }, [cursorPitch, getNoteStartingAt]);

  // Find previous note on current pitch, or previous bar line if no note found
  const findPrevNote = useCallback((fromStep: number): number => {
    // First try to find previous note
    for (let i = fromStep - 1; i >= 0; i--) {
      if (getNoteStartingAt(cursorPitch, i)) return i;
    }
    // No note found, jump to previous bar line (0, 4, 8, 12)
    const prevBar = Math.floor((fromStep - 1) / 4) * 4;
    if (prevBar >= 0) {
      return prevBar;
    }
    // Wrap to last bar line
    return 12;
  }, [cursorPitch, getNoteStartingAt]);

  // Find end of current/next note
  const findNoteEnd = useCallback((fromStep: number): number => {
    const note = getNoteCovering(cursorPitch, fromStep);
    if (note) {
      return note.startStep + note.duration - 1;
    }
    // Find next note and go to its end
    const nextStart = findNextNote(fromStep);
    const nextNote = getNoteStartingAt(cursorPitch, nextStart);
    if (nextNote) {
      return nextNote.startStep + nextNote.duration - 1;
    }
    return fromStep;
  }, [cursorPitch, getNoteCovering, findNextNote, getNoteStartingAt]);

  // Auto-scroll viewport when cursor moves
  const scrollToCursor = useCallback((pitch: number) => {
    if (pitch > viewportTop) {
      setViewportTop(pitch);
    } else if (pitch < viewportTop - VIEWPORT_HEIGHT + 1) {
      setViewportTop(pitch + VIEWPORT_HEIGHT - 1);
    }
  }, [viewportTop]);

  // Get visual selection bounds
  const getVisualBounds = useCallback(() => {
    if (!visualStart || visualMode === 'none') return null;
    const minPitch = Math.min(visualStart.pitch, cursorPitch);
    const maxPitch = Math.max(visualStart.pitch, cursorPitch);
    const minStep = Math.min(visualStart.step, cursorStep);
    const maxStep = Math.max(visualStart.step, cursorStep);
    return { minPitch, maxPitch, minStep, maxStep };
  }, [visualStart, visualMode, cursorPitch, cursorStep]);

  // Check if a cell is in visual selection
  const isInVisualSelection = useCallback((pitch: number, step: number): boolean => {
    const bounds = getVisualBounds();
    if (!bounds) return false;
    if (visualMode === 'char') {
      // Char mode: only current pitch row, between steps
      return pitch === cursorPitch && step >= bounds.minStep && step <= bounds.maxStep;
    } else if (visualMode === 'block') {
      // Block mode: rectangular selection
      return pitch >= bounds.minPitch && pitch <= bounds.maxPitch &&
             step >= bounds.minStep && step <= bounds.maxStep;
    }
    return false;
  }, [getVisualBounds, visualMode, cursorPitch]);

  // Get notes in visual selection
  const getNotesInSelection = useCallback((): Note[] => {
    const bounds = getVisualBounds();
    if (!bounds) return [];
    return notes.filter(n => {
      if (visualMode === 'char') {
        return n.pitch === cursorPitch &&
               n.startStep >= bounds.minStep &&
               n.startStep <= bounds.maxStep;
      } else {
        return n.pitch >= bounds.minPitch && n.pitch <= bounds.maxPitch &&
               n.startStep >= bounds.minStep && n.startStep <= bounds.maxStep;
      }
    });
  }, [getVisualBounds, notes, visualMode, cursorPitch]);

  // Yank notes in selection
  const yankSelection = useCallback(() => {
    const bounds = getVisualBounds();
    if (!bounds) return;
    const selectedNotes = getNotesInSelection();
    // Store relative to anchor (min corner)
    yankBuffer.current = selectedNotes.map(n => ({
      pitchOffset: n.pitch - bounds.minPitch,
      stepOffset: n.startStep - bounds.minStep,
      duration: n.duration,
    }));
  }, [getVisualBounds, getNotesInSelection]);

  // Delete notes in selection
  const deleteSelection = useCallback(() => {
    const selectedNotes = getNotesInSelection();
    for (const note of selectedNotes) {
      removeNote(selectedChannel, note.id);
    }
  }, [getNotesInSelection, removeNote, selectedChannel]);

  // Paste yanked notes at cursor
  const pasteNotes = useCallback(() => {
    for (const yanked of yankBuffer.current) {
      const pitch = cursorPitch + yanked.pitchOffset;
      const step = cursorStep + yanked.stepOffset;
      if (pitch >= MIN_PITCH && pitch <= MAX_PITCH && step >= 0 && step + yanked.duration <= NUM_STEPS) {
        addNote(selectedChannel, pitch, step, yanked.duration);
      }
    }
  }, [cursorPitch, cursorStep, addNote, selectedChannel]);

  // Exit visual mode
  const exitVisualMode = useCallback(() => {
    setVisualMode('none');
    setVisualStart(null);
  }, []);

  // Preview note at pitch (handles both sample and synth channels)
  const previewAtPitch = useCallback((pitch: number) => {
    if (!channel) return;
    if (channel.type === 'synth') {
      previewSynthNote(channel.synthPatch, pitch);
    } else if (channel.sample) {
      previewSamplePitched(getSamplePath(channel.sample), pitch);
    }
  }, [channel]);

  // Delete note at or covering cursor
  const deleteNoteAtCursor = useCallback(() => {
    const note = getNoteCovering(cursorPitch, cursorStep);
    if (note) {
      removeNote(selectedChannel, note.id);
    }
  }, [cursorPitch, cursorStep, getNoteCovering, removeNote, selectedChannel]);

  // Execute operator
  const executeOperator = useCallback((op: Operator, targetStep: number) => {
    if (!op) return;
    if (op === 'd') {
      // Delete all notes from cursor to target on current pitch
      const minStep = Math.min(cursorStep, targetStep);
      const maxStep = Math.max(cursorStep, targetStep);
      for (const note of notes) {
        if (note.pitch === cursorPitch &&
            ((note.startStep >= minStep && note.startStep <= maxStep) ||
             (note.startStep < minStep && note.startStep + note.duration > minStep))) {
          removeNote(selectedChannel, note.id);
        }
      }
    }
    setPendingOperator(null);
  }, [cursorStep, cursorPitch, notes, removeNote, selectedChannel]);

  useInput((input, key) => {
    if (!isFocused) return;

    // Escape cancels placement mode, visual mode, or pending operator; or exits piano roll
    if (key.escape) {
      if (placingNote) {
        setPlacingNote(null);
        return;
      }
      if (visualMode !== 'none') {
        exitVisualMode();
        return;
      }
      if (pendingOperator) {
        setPendingOperator(null);
        return;
      }
      if (countPrefix) {
        setCountPrefix('');
        return;
      }
      // Nothing to cancel, exit piano roll
      exitPianoRoll();
      return;
    }

    // Visual mode: v for char, Ctrl+v for block
    if (input === 'v' && !key.ctrl && visualMode === 'none') {
      setVisualMode('char');
      setVisualStart({ pitch: cursorPitch, step: cursorStep });
      return;
    }
    if (key.ctrl && input === 'v') {
      if (visualMode === 'none') {
        setVisualMode('block');
        setVisualStart({ pitch: cursorPitch, step: cursorStep });
      } else {
        // Toggle to block mode from char mode
        setVisualMode('block');
      }
      return;
    }

    // In visual mode: y to yank, d to delete
    if (visualMode !== 'none') {
      if (input === 'y') {
        yankSelection();
        exitVisualMode();
        return;
      }
      if (input === 'd') {
        yankSelection();  // Also yank before delete
        deleteSelection();
        exitVisualMode();
        return;
      }
      if (input === 'x') {
        // Delete selection
        deleteSelection();
        exitVisualMode();
        return;
      }
    }

    // Paste yanked notes
    if (input === 'p') {
      pasteNotes();
      return;
    }

    // Number prefix accumulation
    if (/^[0-9]$/.test(input) && (countPrefix || input !== '0')) {
      setCountPrefix(prev => prev + input);
      return;
    }

    const count = getCount();

    // Navigation
    if (key.leftArrow || input === 'h') {
      const target = Math.max(0, cursorStep - count);
      if (pendingOperator) {
        // h is exclusive for delete - delete to the left, don't move cursor
        executeOperator(pendingOperator, cursorStep - count);
      } else {
        setCursorStep(target);
      }
      return;
    }
    if (key.rightArrow || input === 'l') {
      if (pendingOperator) {
        // l is exclusive - dl deletes just current position, don't move cursor
        executeOperator(pendingOperator, cursorStep + count - 1);
      } else {
        const target = Math.min(NUM_STEPS - 1, cursorStep + count);
        setCursorStep(target);
      }
      return;
    }
    if (key.upArrow || input === 'k') {
      // Cancel placement if moving vertically
      if (placingNote) setPlacingNote(null);
      setPendingOperator(null);
      setCursorPitch(prev => {
        const next = Math.min(MAX_PITCH, prev + count);
        scrollToCursor(next);
        return next;
      });
      return;
    }
    if (key.downArrow || input === 'j') {
      if (placingNote) setPlacingNote(null);
      setPendingOperator(null);
      setCursorPitch(prev => {
        const next = Math.max(MIN_PITCH, prev - count);
        scrollToCursor(next);
        return next;
      });
      return;
    }

    // Octave jumps
    if (input === 'K') {
      if (placingNote) setPlacingNote(null);
      setPendingOperator(null);
      setCursorPitch(prev => {
        const next = Math.min(MAX_PITCH, prev + 12);
        scrollToCursor(next);
        return next;
      });
      return;
    }
    if (input === 'J') {
      if (placingNote) setPlacingNote(null);
      setPendingOperator(null);
      setCursorPitch(prev => {
        const next = Math.max(MIN_PITCH, prev - 12);
        scrollToCursor(next);
        return next;
      });
      return;
    }

    // Jump to start/end of row
    if (input === '0' && !countPrefix) {
      if (pendingOperator) {
        executeOperator(pendingOperator, 0);
      }
      setCursorStep(0);
      return;
    }
    if (input === '$') {
      if (pendingOperator) {
        executeOperator(pendingOperator, NUM_STEPS - 1);
      }
      setCursorStep(NUM_STEPS - 1);
      return;
    }

    // Vim motions: w (next note), b (previous note), e (end of note)
    if (input === 'w') {
      let step = cursorStep;
      for (let i = 0; i < count; i++) {
        step = findNextNote(step);
      }
      if (pendingOperator) {
        executeOperator(pendingOperator, step);
      }
      setCursorStep(step);
      return;
    }
    if (input === 'b') {
      let step = cursorStep;
      for (let i = 0; i < count; i++) {
        step = findPrevNote(step);
      }
      if (pendingOperator) {
        executeOperator(pendingOperator, step);
      }
      setCursorStep(step);
      return;
    }
    if (input === 'e') {
      // Go to end of current measure (step 3, 7, 11, or 15)
      // If already at end of measure, go to end of next measure
      const currentBar = Math.floor(cursorStep / 4);
      const endOfCurrentBar = currentBar * 4 + 3;
      let step: number;
      if (cursorStep === endOfCurrentBar && endOfCurrentBar < NUM_STEPS - 1) {
        // Already at end of bar, go to next bar's end
        step = Math.min(endOfCurrentBar + 4, NUM_STEPS - 1);
      } else {
        step = Math.min(endOfCurrentBar, NUM_STEPS - 1);
      }
      if (pendingOperator) {
        executeOperator(pendingOperator, step);
      }
      setCursorStep(step);
      return;
    }

    // Jump to top/bottom pitch
    if (input === 'g') {
      if (placingNote) setPlacingNote(null);
      setPendingOperator(null);
      setCursorPitch(MAX_PITCH);
      setViewportTop(MAX_PITCH);
      return;
    }
    if (input === 'G') {
      if (placingNote) setPlacingNote(null);
      setPendingOperator(null);
      setCursorPitch(MIN_PITCH);
      setViewportTop(MIN_PITCH + VIEWPORT_HEIGHT - 1);
      return;
    }

    // Page up/down (Ctrl+u/d)
    if (key.ctrl && input === 'u') {
      if (placingNote) setPlacingNote(null);
      const halfPage = Math.floor(VIEWPORT_HEIGHT / 2);
      setCursorPitch(prev => {
        const next = Math.min(MAX_PITCH, prev + halfPage);
        setViewportTop(vt => Math.min(MAX_PITCH, vt + halfPage));
        return next;
      });
      return;
    }
    if (key.ctrl && input === 'd') {
      if (placingNote) setPlacingNote(null);
      const halfPage = Math.floor(VIEWPORT_HEIGHT / 2);
      setCursorPitch(prev => {
        const next = Math.max(MIN_PITCH, prev - halfPage);
        setViewportTop(vt => Math.max(MIN_PITCH + VIEWPORT_HEIGHT - 1, vt - halfPage));
        return next;
      });
      return;
    }

    // Delete operator
    if (input === 'd') {
      if (pendingOperator === 'd') {
        // dd - delete all notes on current pitch (or just note at cursor)
        deleteNoteAtCursor();
        setPendingOperator(null);
        return;
      }
      setPendingOperator('d');
      return;
    }

    // x - Note placement/editing
    if (key.return || input === 'x') {
      if (placingNote) {
        // Finish placing note
        const startStep = Math.min(placingNote.startStep, cursorStep);
        const endStep = Math.max(placingNote.startStep, cursorStep);
        const duration = endStep - startStep + 1;
        addNote(selectedChannel, cursorPitch, startStep, duration);
        setPlacingNote(null);
      } else {
        // Check if there's a note at cursor
        const existingNote = getNoteCovering(cursorPitch, cursorStep);
        if (existingNote) {
          // Edit existing note: delete it and enter placement mode from its start
          removeNote(selectedChannel, existingNote.id);
          setPlacingNote({ startStep: existingNote.startStep });
        } else {
          // Start placing new note
          setPlacingNote({ startStep: cursorStep });
        }
      }
      return;
    }

    // Nudge notes with < and >
    if (input === '<') {
      const note = getNoteCovering(cursorPitch, cursorStep);
      if (note && note.startStep > 0) {
        updateNote(selectedChannel, note.id, { startStep: note.startStep - 1 });
        setCursorStep(prev => Math.max(0, prev - 1));
      }
      return;
    }
    if (input === '>') {
      const note = getNoteCovering(cursorPitch, cursorStep);
      if (note && note.startStep + note.duration < NUM_STEPS) {
        updateNote(selectedChannel, note.id, { startStep: note.startStep + 1 });
        setCursorStep(prev => Math.min(NUM_STEPS - 1, prev + 1));
      }
      return;
    }

    // Preview at cursor pitch
    if (input === 's') {
      previewAtPitch(cursorPitch);
      return;
    }
  });

  // Calculate visible pitch range (higher pitches at top)
  const pitchRange: number[] = [];
  for (let p = viewportTop; p > viewportTop - VIEWPORT_HEIGHT && p >= MIN_PITCH; p--) {
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
              color={i === playheadStep && isPlaying ? 'green' : i % 4 === 0 ? 'yellow' : 'gray'}
              bold={i === cursorStep && isFocused}
            >
              {(i + 1).toString(16).toUpperCase()}
            </Text>
          </Box>
        ))}
        <Box marginLeft={1}>
          <Text dimColor>
            {placingNote ? 'PLACE' : visualMode === 'char' ? 'VISUAL' : visualMode === 'block' ? 'V-BLOCK' : pendingOperator ? `d...` : ''}
          </Text>
        </Box>
      </Box>

      {/* Separator */}
      <Box>
        <Text dimColor>{'─'.repeat(5 + NUM_STEPS * 2 + 6)}</Text>
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
                color={isCursorRow ? 'cyan' : isBlack ? 'gray' : 'white'}
                bold={isCursorRow}
                dimColor={isBlack && !isCursorRow}
              >
                {getPitchName(pitch).padStart(4, ' ')}
              </Text>
            </Box>

            {/* Steps */}
            {Array.from({ length: NUM_STEPS }, (_, stepIndex) => {
              const note = hasNoteAt(pitch, stepIndex);
              const isStart = isNoteStart(pitch, stepIndex);
              const isCursor = pitch === cursorPitch && stepIndex === cursorStep && isFocused;
              const isPlayhead = stepIndex === playheadStep && isPlaying;
              const isBeat = stepIndex % 4 === 0;
              const isInPlacement = placementRange &&
                pitch === cursorPitch &&
                stepIndex >= placementRange.start &&
                stepIndex <= placementRange.end;
              const isVisualSelected = isInVisualSelection(pitch, stepIndex);

              let bgColor: string | undefined;
              let fgColor = isBlack ? 'gray' : 'white';
              let char = isBeat ? '┃' : '│';

              if (note) {
                if (isStart) {
                  char = '█';
                  fgColor = 'magenta';
                } else {
                  char = '─';
                  fgColor = 'magenta';
                }
              }

              // Placement preview
              if (isInPlacement && !note) {
                char = '░';
                fgColor = 'cyan';
              }

              if (isCursor && isPlayhead) {
                bgColor = 'greenBright';
                fgColor = 'black';
              } else if (isCursor) {
                bgColor = 'blue';
                fgColor = 'white';
              } else if (isVisualSelected) {
                bgColor = 'yellow';
                fgColor = 'black';
              } else if (isPlayhead) {
                bgColor = 'green';
                fgColor = 'black';
              } else if (isInPlacement) {
                bgColor = 'cyan';
                fgColor = 'black';
              }

              return (
                <Box key={`step-${pitch}-${stepIndex}`} width={2}>
                  <Text
                    backgroundColor={bgColor}
                    color={fgColor}
                    bold={!!note || isPlayhead || !!isInPlacement}
                    dimColor={isBlack && !note && !isCursor && !isPlayhead && !isInPlacement}
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
          hjkl:Move x:Place/Edit {'<>'}:Nudge v:Visual ^v:Block y:Yank p:Paste d:Del
        </Text>
      </Box>
    </Box>
  );
}
