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
} from "../lib/vim/types";

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
 */
function executeMotion(
  motionName: string,
  count: number,
  cursor: Position,
  motions: VimConfig["motions"],
): MotionResult | null {
  switch (motionName) {
    case "h":
      return motions.h(count, cursor);
    case "j":
      return motions.j(count, cursor);
    case "k":
      return motions.k(count, cursor);
    case "l":
      return motions.l(count, cursor);
    case "w":
      return motions.w?.(count, cursor) ?? null;
    case "b":
      return motions.b?.(count, cursor) ?? null;
    case "e":
      return motions.e?.(count, cursor) ?? null;
    case "gg":
      return motions.gg?.(count, cursor) ?? null;
    case "G":
      return motions.G?.(count, cursor) ?? null;
    case "zero":
      return motions.zero?.(count, cursor) ?? null;
    case "dollar":
      return motions.dollar?.(count, cursor) ?? null;
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

      // Escape always resets
      if (key.escape) {
        visualStartRef.current = null;
        send({ type: "ESCAPE" });
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

      // Visual block mode (Ctrl+v)
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
        const count = state.context.count || 1;
        const motionResult = executeMotion(
          motionName,
          count,
          cursor,
          config.motions,
        );

        if (!motionResult) {
          // Motion not implemented by this component
          send({ type: "RESET" });
          return false;
        }

        if (mode === "operator-pending" && state.context.operator) {
          // Execute operator with motion
          const range = calculateOperatorRange(cursor, motionResult);
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
            count,
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
          const motionResult = executeMotion(
            action.motion,
            count,
            cursor,
            config.motions,
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
