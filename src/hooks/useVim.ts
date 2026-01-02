import { useCallback, useRef, useEffect } from "react";
import { useMachine } from "@xstate/react";
import { vimMachine } from "../lib/vim/VimMachine";
import { registers } from "../lib/vim/registers";
import { jumplist } from "../lib/vim/jumplist";
import type {
  VimConfig,
  VimState,
  VimMode,
  Position,
  Range,
  Key,
  Operator,
  RecordedAction,
  MotionResult,
  Zone,
  GridSemantics,
  GridDimensions,
  Motions,
} from "../lib/vim/types";

// ============================================================================
// Zone Helper Functions
// ============================================================================

/**
 * Find the zone that contains the given column.
 * Returns undefined if no zone contains this column.
 */
function getZoneAtCol(
  zones: Zone[] | undefined,
  col: number,
): Zone | undefined {
  if (!zones || zones.length === 0) return undefined;
  return zones.find((z) => col >= z.colRange[0] && col <= z.colRange[1]);
}

/**
 * Get the main zone (where 0/$ go).
 * Falls back to first zone, or undefined if no zones.
 */
function getMainZone(zones: Zone[] | undefined): Zone | undefined {
  if (!zones || zones.length === 0) return undefined;
  return zones.find((z) => z.isMain) ?? zones[0];
}

/**
 * Get the zone to the left of the current position's zone.
 */
function getPrevZone(
  zones: Zone[] | undefined,
  currentCol: number,
): Zone | undefined {
  if (!zones || zones.length === 0) return undefined;
  const currentZone = getZoneAtCol(zones, currentCol);
  if (!currentZone) return undefined;

  // Find zone whose end is just before current zone's start
  return zones.find((z) => z.colRange[1] === currentZone.colRange[0] - 1);
}

/**
 * Get the zone to the right of the current position's zone.
 */
function getNextZone(
  zones: Zone[] | undefined,
  currentCol: number,
): Zone | undefined {
  if (!zones || zones.length === 0) return undefined;
  const currentZone = getZoneAtCol(zones, currentCol);
  if (!currentZone) return undefined;

  // Find zone whose start is just after current zone's end
  return zones.find((z) => z.colRange[0] === currentZone.colRange[1] + 1);
}

// ============================================================================
// Default Motion Implementations
// ============================================================================

/**
 * Create default motions using dimensions and gridSemantics.
 * These handle all standard vim navigation with zone awareness.
 */
function createDefaultMotions(
  dimensions: GridDimensions,
  gridSemantics?: GridSemantics,
): Motions {
  const zones = gridSemantics?.zones;

  // If no zones defined, treat entire grid as one main zone
  const effectiveZones: Zone[] = zones ?? [
    {
      name: "default",
      colRange: [0, dimensions.cols - 1],
      isMain: true,
    },
  ];

  return {
    h: (count: number, cursor: Position): MotionResult => {
      let col = cursor.col;

      for (let i = 0; i < count; i++) {
        const zone = getZoneAtCol(effectiveZones, col);
        if (!zone) break;

        if (col > zone.colRange[0]) {
          // Move within zone
          col--;
        } else {
          // At zone boundary - try to enter previous zone
          const prevZone = getPrevZone(effectiveZones, col);
          if (prevZone) {
            col = prevZone.colRange[1]; // Enter at rightmost column
          }
          // Else stay at boundary
        }
      }

      return { position: { row: cursor.row, col } };
    },

    l: (count: number, cursor: Position): MotionResult => {
      let col = cursor.col;

      for (let i = 0; i < count; i++) {
        const zone = getZoneAtCol(effectiveZones, col);
        if (!zone) break;

        if (col < zone.colRange[1]) {
          // Move within zone
          col++;
        } else {
          // At zone boundary - try to enter next zone
          const nextZone = getNextZone(effectiveZones, col);
          if (nextZone) {
            col = nextZone.colRange[0]; // Enter at leftmost column
          }
          // Else stay at boundary
        }
      }

      return { position: { row: cursor.row, col } };
    },

    j: (count: number, cursor: Position): MotionResult => {
      const newRow = Math.min(cursor.row + count, dimensions.rows - 1);
      return { position: { row: newRow, col: cursor.col }, linewise: true };
    },

    k: (count: number, cursor: Position): MotionResult => {
      const newRow = Math.max(cursor.row - count, 0);
      return { position: { row: newRow, col: cursor.col }, linewise: true };
    },

    w: (count: number, cursor: Position): MotionResult => {
      const zone = getZoneAtCol(effectiveZones, cursor.col);
      if (!zone) return { position: cursor };

      let pos = { ...cursor };

      for (let i = 0; i < count; i++) {
        const nextPos = findNextWordInZone(pos, zone, cursor.row);
        if (!nextPos) {
          // No more words - go to end of zone (vim behavior)
          pos = { row: cursor.row, col: zone.colRange[1] };
          break;
        }
        pos = nextPos;
      }

      return { position: pos };
    },

    b: (count: number, cursor: Position): MotionResult => {
      const zone = getZoneAtCol(effectiveZones, cursor.col);
      if (!zone) return { position: cursor };

      let pos = { ...cursor };

      for (let i = 0; i < count; i++) {
        const prevPos = findPrevWordInZone(pos, zone, cursor.row);
        if (!prevPos) {
          // No more words - go to start of zone (vim behavior)
          pos = { row: cursor.row, col: zone.colRange[0] };
          break;
        }
        pos = prevPos;
      }

      return { position: pos };
    },

    e: (count: number, cursor: Position): MotionResult => {
      // e goes to end of current word, or end of next word if already at end
      const zone = getZoneAtCol(effectiveZones, cursor.col);
      if (!zone) return { position: cursor };

      let pos = { ...cursor };

      for (let i = 0; i < count; i++) {
        const endPos = findWordEndInZone(pos, zone, cursor.row);
        if (!endPos) {
          pos = { row: cursor.row, col: zone.colRange[1] };
          break;
        }
        pos = endPos;
      }

      return { position: pos, inclusive: true };
    },

    zero: (_count: number, cursor: Position): MotionResult => {
      // 0 goes to start of main zone
      const mainZone = getMainZone(effectiveZones);
      const col = mainZone?.colRange[0] ?? 0;
      return { position: { row: cursor.row, col } };
    },

    dollar: (_count: number, cursor: Position): MotionResult => {
      // $ goes to end of main zone
      const mainZone = getMainZone(effectiveZones);
      const col = mainZone?.colRange[1] ?? dimensions.cols - 1;
      return { position: { row: cursor.row, col }, inclusive: true };
    },

    gg: (count: number, cursor: Position): MotionResult => {
      // gg with no count goes to row 0, with count goes to row count-1
      const row = count === 0 ? 0 : Math.min(count - 1, dimensions.rows - 1);
      return { position: { row, col: cursor.col }, linewise: true };
    },

    G: (count: number, cursor: Position): MotionResult => {
      // G with no count goes to last row, with count goes to row count-1
      const row =
        count === 0
          ? dimensions.rows - 1
          : Math.min(count - 1, dimensions.rows - 1);
      return { position: { row, col: cursor.col }, linewise: true };
    },
  };
}

/**
 * Find the next word position within a zone.
 * Uses zone's hasContent and wordInterval to find word boundaries.
 */
function findNextWordInZone(
  pos: Position,
  zone: Zone,
  row: number,
): Position | null {
  const { hasContent, wordInterval, colRange } = zone;

  // Start searching from next column
  for (let col = pos.col + 1; col <= colRange[1]; col++) {
    // Check word interval boundaries (e.g., bar lines every 4 steps)
    if (wordInterval && col > colRange[0]) {
      const relativeCol = col - colRange[0];
      if (relativeCol % wordInterval === 0) {
        return { row, col };
      }
    }

    // Check for content (if hasContent is defined)
    if (hasContent) {
      const prevCol = col - 1;
      const prevHasContent =
        prevCol >= colRange[0] && hasContent({ row, col: prevCol });
      const currHasContent = hasContent({ row, col });

      // Word boundary: transition from no content to content
      if (!prevHasContent && currHasContent) {
        return { row, col };
      }
    }
  }

  return null;
}

/**
 * Find the previous word position within a zone.
 */
function findPrevWordInZone(
  pos: Position,
  zone: Zone,
  row: number,
): Position | null {
  const { hasContent, wordInterval, colRange } = zone;

  // Start searching from previous column
  for (let col = pos.col - 1; col >= colRange[0]; col--) {
    // Check word interval boundaries
    if (wordInterval) {
      const relativeCol = col - colRange[0];
      if (relativeCol % wordInterval === 0) {
        return { row, col };
      }
    }

    // Check for content
    if (hasContent) {
      const currHasContent = hasContent({ row, col });

      // Find start of a word (content position)
      if (currHasContent) {
        // Walk back to find start of this word
        let wordStart = col;
        while (
          wordStart > colRange[0] &&
          hasContent({ row, col: wordStart - 1 })
        ) {
          wordStart--;
        }
        if (wordStart < pos.col) {
          return { row, col: wordStart };
        }
      }
    }
  }

  return null;
}

/**
 * Find the end of the current or next word within a zone.
 */
function findWordEndInZone(
  pos: Position,
  zone: Zone,
  row: number,
): Position | null {
  const { hasContent, colRange } = zone;

  if (!hasContent) {
    // Without hasContent, just go to end of zone
    return pos.col < colRange[1] ? { row, col: colRange[1] } : null;
  }

  let col = pos.col;

  // If on content, move past it first (to find end of next word if at end of current)
  if (hasContent({ row, col })) {
    // Find end of current word
    while (col < colRange[1] && hasContent({ row, col: col + 1 })) {
      col++;
    }
    if (col > pos.col) {
      return { row, col };
    }
    // Already at end of word, find next word
    col++;
  }

  // Skip non-content
  while (col <= colRange[1] && !hasContent({ row, col })) {
    col++;
  }

  if (col > colRange[1]) return null;

  // Find end of this word
  while (col < colRange[1] && hasContent({ row, col: col + 1 })) {
    col++;
  }

  return { row, col };
}

/**
 * Parse a character/key into a motion name if it's a motion key.
 * Returns null if it's not a motion.
 */
function parseMotionKey(char: string, key: Key): string | null {
  // Arrow keys
  if (key.leftArrow) return "h";
  if (key.downArrow) return "j";
  if (key.upArrow) return "k";
  if (key.rightArrow) return "l";

  // hjkl
  if (char === "h" || char === "j" || char === "k" || char === "l") {
    return char;
  }

  // Word motions
  if (char === "w" || char === "b" || char === "e") {
    return char;
  }

  // Line motions
  if (char === "0") return "zero";
  if (char === "$") return "dollar";

  // Document motions
  if (char === "g") return "gg";
  if (char === "G") return "G";

  return null;
}

/**
 * Execute a motion and return the result.
 *
 * Motion resolution order:
 * 1. customMotions (for truly special behavior like Browser folder expand/collapse)
 * 2. default motions from gridSemantics (zone-aware navigation)
 * 3. legacy motions (backward compatibility)
 * 4. wordBoundary for w/b (deprecated, backward compatibility)
 */
function executeMotion(
  motionName: string,
  count: number,
  cursor: Position,
  config: Pick<
    VimConfig,
    | "motions"
    | "customMotions"
    | "gridSemantics"
    | "wordBoundary"
    | "dimensions"
  >,
): MotionResult | null {
  const { customMotions, gridSemantics, motions, wordBoundary, dimensions } =
    config;

  // Create default motions using gridSemantics
  const defaultMotions = createDefaultMotions(dimensions, gridSemantics);

  // Helper to get motion from appropriate source
  const getMotion = (
    name: keyof Motions,
  ): ((count: number, cursor: Position) => MotionResult) | undefined => {
    // 1. Check customMotions first (explicit overrides)
    if (customMotions?.[name]) {
      return customMotions[name];
    }

    // 2. Use default motions (from gridSemantics) if no legacy motions provided
    // or if the legacy motion doesn't exist for this key
    if (!motions || !motions[name]) {
      return defaultMotions[name];
    }

    // 3. Legacy motions (backward compatibility)
    return motions[name];
  };

  switch (motionName) {
    case "h": {
      const motion = getMotion("h");
      return motion ? motion(count, cursor) : null;
    }
    case "j": {
      const motion = getMotion("j");
      return motion ? motion(count, cursor) : null;
    }
    case "k": {
      const motion = getMotion("k");
      return motion ? motion(count, cursor) : null;
    }
    case "l": {
      const motion = getMotion("l");
      return motion ? motion(count, cursor) : null;
    }
    case "w": {
      // Check customMotions first
      if (customMotions?.w) {
        return customMotions.w(count, cursor);
      }
      // Prefer gridSemantics default if available (uses hasContent + wordInterval)
      if (gridSemantics || !motions) {
        return defaultMotions.w!(count, cursor);
      }
      // Legacy: wordBoundary interface (deprecated)
      if (wordBoundary) {
        let pos = cursor;
        for (let i = 0; i < count; i++) {
          const next = wordBoundary.findNext(pos);
          if (next === null) break;
          pos = next;
        }
        return { position: pos };
      }
      // Legacy: component-provided motion
      return motions.w?.(count, cursor) ?? null;
    }
    case "b": {
      // Check customMotions first
      if (customMotions?.b) {
        return customMotions.b(count, cursor);
      }
      // Prefer gridSemantics default if available
      if (gridSemantics || !motions) {
        return defaultMotions.b!(count, cursor);
      }
      // Legacy: wordBoundary interface (deprecated)
      if (wordBoundary) {
        let pos = cursor;
        for (let i = 0; i < count; i++) {
          const prev = wordBoundary.findPrev(pos);
          if (prev === null) break;
          pos = prev;
        }
        return { position: pos };
      }
      // Legacy: component-provided motion
      return motions.b?.(count, cursor) ?? null;
    }
    case "e": {
      const motion = getMotion("e");
      return motion ? motion(count, cursor) : null;
    }
    case "gg": {
      const motion = getMotion("gg");
      return motion ? motion(count, cursor) : null;
    }
    case "G": {
      const motion = getMotion("G");
      return motion ? motion(count, cursor) : null;
    }
    case "zero": {
      const motion = getMotion("zero");
      return motion ? motion(count, cursor) : null;
    }
    case "dollar": {
      const motion = getMotion("dollar");
      return motion ? motion(count, cursor) : null;
    }
    default:
      return null;
  }
}

/**
 * Calculate the visual range given the visual start and current cursor.
 */
function calculateVisualRange(
  visualStart: Position | null,
  cursor: Position,
  mode: VimMode,
): Range | null {
  if (!visualStart) return null;

  const minRow = Math.min(visualStart.row, cursor.row);
  const maxRow = Math.max(visualStart.row, cursor.row);
  const minCol = Math.min(visualStart.col, cursor.col);
  const maxCol = Math.max(visualStart.col, cursor.col);

  if (mode === "visual-block") {
    return {
      start: { row: minRow, col: minCol },
      end: { row: maxRow, col: maxCol },
      type: "block",
    };
  }

  // Character-wise visual mode
  return {
    start: {
      row: minRow,
      col: visualStart.row <= cursor.row ? visualStart.col : minCol,
    },
    end: {
      row: maxRow,
      col: visualStart.row <= cursor.row ? cursor.col : maxCol,
    },
    type: "char",
  };
}

/**
 * Calculate the range for an operator + motion.
 */
function calculateOperatorRange(
  cursor: Position,
  motionResult: MotionResult,
): Range {
  const minRow = Math.min(cursor.row, motionResult.position.row);
  const maxRow = Math.max(cursor.row, motionResult.position.row);

  if (motionResult.linewise) {
    return {
      start: { row: minRow, col: 0 },
      end: { row: maxRow, col: Infinity }, // Infinity means end of line
      type: "line",
    };
  }

  // Character-wise range
  const forwardMotion =
    motionResult.position.row > cursor.row ||
    (motionResult.position.row === cursor.row &&
      motionResult.position.col > cursor.col);

  if (forwardMotion) {
    return {
      start: cursor,
      end: motionResult.inclusive
        ? motionResult.position
        : {
            row: motionResult.position.row,
            col: motionResult.position.col - 1,
          },
      type: "char",
    };
  } else {
    return {
      start: motionResult.position,
      end: cursor,
      type: "char",
    };
  }
}

/**
 * Main vim hook for components.
 *
 * Usage:
 * ```
 * const vim = useVim({
 *   dimensions: { rows: 10, cols: 16 },
 *   getCursor: () => cursor,
 *   setCursor: (pos) => setCursor(pos),
 *   motions: { h, j, k, l, ... },
 *   getDataInRange: (range) => ...,
 *   deleteRange: (range) => ...,
 *   insertData: (pos, data) => ...,
 * });
 *
 * useInput((char, key) => {
 *   if (vim.handleInput(char, key)) return;
 *   // Handle component-specific keys
 * });
 * ```
 */
export function useVim<T = unknown>(config: VimConfig<T>): VimState<T> {
  const [state, send] = useMachine(vimMachine);
  const lastActionRef = useRef<RecordedAction | null>(null);
  const visualStartRef = useRef<Position | null>(null);

  // Map XState state to VimMode
  const mode: VimMode = (() => {
    switch (state.value) {
      case "normal":
        return "normal";
      case "operator":
        return "operator-pending";
      case "visual":
        return "visual";
      case "visualBlock":
        return "visual-block";
      default:
        return "normal";
    }
  })();

  // Calculate visual range
  const visualRange = calculateVisualRange(
    visualStartRef.current,
    config.getCursor(),
    mode,
  );

  // Notify on mode change
  const prevModeRef = useRef<VimMode>(mode);
  useEffect(() => {
    if (mode !== prevModeRef.current) {
      config.onModeChange?.(mode);
      prevModeRef.current = mode;
    }
  }, [mode, config]);

  // Notify on visual range change
  useEffect(() => {
    config.onVisualRangeChange?.(visualRange);
  }, [visualRange, config]);

  /**
   * Handle input - returns true if handled, false if component should handle it.
   */
  const handleInput = useCallback(
    (char: string, key: Key): boolean => {
      const cursor = config.getCursor();

      // Escape always resets, then notifies component
      if (key.escape) {
        const prevMode = mode;
        visualStartRef.current = null;
        send({ type: "ESCAPE" });
        config.onEscape?.(prevMode);
        return true;
      }

      // Count accumulation (digits, but not 0 at start)
      if (/^[0-9]$/.test(char)) {
        const digit = parseInt(char, 10);
        if (digit !== 0 || state.context.count > 0) {
          send({ type: "DIGIT", digit });
          return true;
        }
        // 0 at start is a motion (go to start of line)
      }

      // Operators
      if (char === "d" || char === "y" || char === "c") {
        const operator = char as Operator;

        // In visual mode, execute immediately
        if (mode === "visual" || mode === "visual-block") {
          if (visualRange) {
            if (operator === "d" || operator === "c") {
              const deleted = config.deleteRange(visualRange);
              registers.delete(deleted, visualRange.type);
            } else if (operator === "y") {
              const yanked = config.getDataInRange(visualRange);
              registers.yank(yanked, visualRange.type);
            }
          }
          visualStartRef.current = null;
          send({ type: "OPERATOR", operator });
          return true;
        }

        // In operator-pending mode, check for double operator (dd, yy, cc)
        if (
          mode === "operator-pending" &&
          state.context.operator === operator
        ) {
          // Linewise operation on current row
          const count = state.context.count || 1;
          const lineRange: Range = {
            start: { row: cursor.row, col: 0 },
            end: {
              row: Math.min(cursor.row + count - 1, config.dimensions.rows - 1),
              col: Infinity,
            },
            type: "line",
          };

          if (operator === "d" || operator === "c") {
            const deleted = config.deleteRange(lineRange);
            registers.delete(deleted, "line");
          } else if (operator === "y") {
            const yanked = config.getDataInRange(lineRange);
            registers.yank(yanked, "line");
          }

          lastActionRef.current = {
            type: "operator",
            operator,
            motion: operator ?? undefined, // dd, yy, cc represented as repeated operator
            count,
          };

          send({ type: "OPERATOR", operator });
          return true;
        }

        // Enter operator-pending mode
        send({ type: "OPERATOR", operator });
        return true;
      }

      // Visual block mode (Ctrl+v) - must check before regular v
      if (key.ctrl && char === "v") {
        if (mode === "visual-block") {
          visualStartRef.current = null;
          send({ type: "VISUAL_BLOCK" });
        } else {
          visualStartRef.current = { ...cursor };
          send({ type: "VISUAL_BLOCK" });
        }
        return true;
      }

      // Visual mode
      if (char === "v") {
        if (mode === "visual") {
          // Exit visual mode
          visualStartRef.current = null;
          send({ type: "VISUAL" });
        } else {
          // Enter visual mode
          visualStartRef.current = { ...cursor };
          send({ type: "VISUAL" });
        }
        return true;
      }

      // Paste
      if (char === "p" || char === "P") {
        const content = registers.get<T>();
        if (content) {
          const insertPos =
            char === "P" ? cursor : { row: cursor.row, col: cursor.col + 1 };
          config.insertData(insertPos, content.data);
        }
        return true;
      }

      // Jumplist navigation
      if (key.ctrl && char === "o") {
        const pos = jumplist.back();
        if (pos) {
          config.setCursor(pos);
        }
        return true;
      }

      if (key.ctrl && char === "i") {
        const pos = jumplist.forward();
        if (pos) {
          config.setCursor(pos);
        }
        return true;
      }

      // Motions
      const motionName = parseMotionKey(char, key);
      if (motionName) {
        // Pass raw count - motions like G need to distinguish "no count" (0) from "count of 1"
        const rawCount = state.context.count;
        const effectiveCount = rawCount || 1;
        const motionResult = executeMotion(
          motionName,
          motionName === "G" || motionName === "gg" ? rawCount : effectiveCount,
          cursor,
          config,
        );

        if (!motionResult) {
          // Motion not implemented by this component
          send({ type: "RESET" });
          return false;
        }

        if (mode === "operator-pending" && state.context.operator) {
          // Execute operator with motion
          let adjustedMotionResult = motionResult;

          // Special case: dw/yw at end of line should NOT cross to next line
          // Vim spec: "When using the 'w' motion in combination with an operator
          // and the last word moved over is at the end of a line, the end of
          // that word becomes the end of the operated text, not the first word
          // in the next line."
          if (motionName === "w" && motionResult.position.row > cursor.row) {
            adjustedMotionResult = {
              ...motionResult,
              position: {
                row: cursor.row,
                col: config.dimensions.cols - 1, // End of current line
              },
              inclusive: true,
            };
          }

          const range = calculateOperatorRange(cursor, adjustedMotionResult);
          const operator = state.context.operator;

          if (operator === "d" || operator === "c") {
            const deleted = config.deleteRange(range);
            registers.delete(deleted, range.type);
          } else if (operator === "y") {
            const yanked = config.getDataInRange(range);
            registers.yank(yanked, range.type);
          }

          lastActionRef.current = {
            type: "operator",
            operator,
            motion: motionName,
            count: effectiveCount,
          };

          // Move cursor to start of range after delete/yank
          config.setCursor(range.start);
          send({
            type: "MOTION",
            motion: motionName,
            position: motionResult.position,
          });
          return true;
        }

        // For jump motions, save to jumplist
        if (motionName === "gg" || motionName === "G") {
          jumplist.push(cursor);
        }

        // Just a motion - move cursor
        config.setCursor(motionResult.position);
        send({
          type: "MOTION",
          motion: motionName,
          position: motionResult.position,
        });
        return true;
      }

      // Repeat last action with .
      if (char === ".") {
        const action = lastActionRef.current;
        if (
          action &&
          action.type === "operator" &&
          action.operator &&
          action.motion
        ) {
          const count = state.context.count || action.count || 1;

          // Check if this is a linewise operation (dd, yy, cc)
          // where motion equals operator
          if (action.motion === action.operator) {
            // Linewise operation
            const lineRange: Range = {
              start: { row: cursor.row, col: 0 },
              end: {
                row: Math.min(
                  cursor.row + count - 1,
                  config.dimensions.rows - 1,
                ),
                col: Infinity,
              },
              type: "line",
            };

            if (action.operator === "d" || action.operator === "c") {
              const deleted = config.deleteRange(lineRange);
              registers.delete(deleted, "line");
            } else if (action.operator === "y") {
              const yanked = config.getDataInRange(lineRange);
              registers.yank(yanked, "line");
            }

            config.setCursor(lineRange.start);
          } else {
            // Regular operator + motion
            const motionResult = executeMotion(
              action.motion,
              count,
              cursor,
              config,
            );

            if (motionResult) {
              const range = calculateOperatorRange(cursor, motionResult);

              if (action.operator === "d" || action.operator === "c") {
                const deleted = config.deleteRange(range);
                registers.delete(deleted, range.type);
              } else if (action.operator === "y") {
                const yanked = config.getDataInRange(range);
                registers.yank(yanked, range.type);
              }

              config.setCursor(range.start);
            }
          }
        }
        send({ type: "RESET" });
        return true;
      }

      // Custom actions (component-specific)
      if (config.onCustomAction) {
        const count = state.context.count || 1;
        if (config.onCustomAction(char, key, count)) {
          send({ type: "RESET" });
          return true;
        }
      }

      // Not handled
      return false;
    },
    [
      config,
      mode,
      send,
      state.context.count,
      state.context.operator,
      visualRange,
    ],
  );

  // Yank function
  const yank = useCallback((data: T, type: "char" | "line" | "block") => {
    registers.yank(data, type);
  }, []);

  // Paste function
  const paste = useCallback(() => {
    return registers.get<T>();
  }, []);

  // Jumplist functions
  const pushJump = useCallback(() => {
    jumplist.push(config.getCursor());
  }, [config]);

  const jumpBack = useCallback(() => {
    return jumplist.back();
  }, []);

  const jumpForward = useCallback(() => {
    return jumplist.forward();
  }, []);

  // Last action functions
  const getLastAction = useCallback(() => {
    return lastActionRef.current;
  }, []);

  const setLastAction = useCallback((action: RecordedAction) => {
    lastActionRef.current = action;
  }, []);

  // Reset function
  const reset = useCallback(() => {
    visualStartRef.current = null;
    send({ type: "RESET" });
  }, [send]);

  return {
    mode,
    count: state.context.count,
    operator: state.context.operator,
    visualRange,
    handleInput,
    yank,
    paste,
    pushJump,
    jumpBack,
    jumpForward,
    getLastAction,
    setLastAction,
    reset,
  };
}
