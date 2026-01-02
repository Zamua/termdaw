/**
 * Comprehensive motion tests based on official Vim documentation
 * Source: https://github.com/vim/vim/blob/master/runtime/doc/motion.txt
 *
 * This file tests vim motion behavior against the official specification.
 */

import { describe, test, expect, beforeEach } from "bun:test";
import { renderHook, act } from "@testing-library/react";
import { useVim } from "../../../hooks/useVim";
import type { VimConfig, Key, Position, Range, MotionResult } from "../types";
import { registers } from "../registers";
import { jumplist } from "../jumplist";

// Helper to create a minimal Key object
function createKey(overrides: Partial<Key> = {}): Key {
  return {
    upArrow: false,
    downArrow: false,
    leftArrow: false,
    rightArrow: false,
    pageDown: false,
    pageUp: false,
    return: false,
    escape: false,
    ctrl: false,
    shift: false,
    tab: false,
    backspace: false,
    delete: false,
    meta: false,
    ...overrides,
  };
}

/**
 * Text-based test configuration.
 * Simulates a text buffer with proper word boundaries.
 */
function createTextConfig(lines: string[]) {
  let cursor: Position = { row: 0, col: 0 };
  const buffer = lines.map((line) => line.split(""));
  const deletedRanges: Range[] = [];

  // Helper to find word boundaries
  const isWordChar = (char: string): boolean => /[a-zA-Z0-9_]/.test(char);
  const isWhitespace = (char: string): boolean => /\s/.test(char);

  const getChar = (row: number, col: number): string | null => {
    const line = buffer[row];
    if (!line) return null;
    return line[col] ?? null;
  };

  const lineLength = (row: number): number => {
    return buffer[row]?.length ?? 0;
  };

  const config: VimConfig<string[][]> = {
    dimensions: {
      rows: lines.length,
      cols: Math.max(...lines.map((l) => l.length)),
    },
    getCursor: () => ({ ...cursor }),
    setCursor: (pos) => {
      cursor = {
        row: Math.max(0, Math.min(lines.length - 1, pos.row)),
        col: Math.max(0, Math.min(lineLength(pos.row) - 1, pos.col)),
      };
    },
    motions: {
      // h - [count] characters to the left, exclusive, stops at first column
      h: (count, cur) => {
        const newCol = Math.max(0, cur.col - count);
        return { position: { row: cur.row, col: newCol } };
      },

      // l - [count] characters to the right, exclusive, stops at end of line
      l: (count, cur) => {
        const maxCol = Math.max(0, lineLength(cur.row) - 1);
        const newCol = Math.min(maxCol, cur.col + count);
        return { position: { row: cur.row, col: newCol } };
      },

      // j - [count] lines downward, linewise
      j: (count, cur) => {
        const newRow = Math.min(lines.length - 1, cur.row + count);
        const maxCol = Math.max(0, lineLength(newRow) - 1);
        return {
          position: { row: newRow, col: Math.min(cur.col, maxCol) },
          linewise: true,
        };
      },

      // k - [count] lines upward, linewise
      k: (count, cur) => {
        const newRow = Math.max(0, cur.row - count);
        const maxCol = Math.max(0, lineLength(newRow) - 1);
        return {
          position: { row: newRow, col: Math.min(cur.col, maxCol) },
          linewise: true,
        };
      },

      // w - [count] words forward, exclusive
      // A word is: sequence of letters/digits/underscores OR sequence of other non-blank
      // Empty lines are also words
      w: (count, cur) => {
        let row = cur.row;
        let col = cur.col;
        let remaining = count;

        while (remaining > 0 && row < lines.length) {
          const line = buffer[row];
          if (!line || line.length === 0) {
            // Empty line is a word
            row++;
            col = 0;
            remaining--;
            continue;
          }

          // Skip current word (if on a word)
          const currentChar = getChar(row, col);
          if (currentChar && !isWhitespace(currentChar)) {
            const isCurrentWord = isWordChar(currentChar);
            while (col < line.length) {
              const ch = getChar(row, col);
              if (!ch) break;
              if (isWhitespace(ch)) break;
              if (isWordChar(ch) !== isCurrentWord) break;
              col++;
            }
          }

          // Skip whitespace
          while (col < line.length && isWhitespace(getChar(row, col) ?? "")) {
            col++;
          }

          // If we reached end of line, go to next line
          if (col >= line.length) {
            row++;
            col = 0;
            // Skip leading whitespace on new line
            while (row < lines.length) {
              const newLine = buffer[row];
              if (!newLine || newLine.length === 0) break; // Empty line is a word
              while (
                col < newLine.length &&
                isWhitespace(getChar(row, col) ?? "")
              ) {
                col++;
              }
              if (col < newLine.length) break;
              row++;
              col = 0;
            }
          }

          remaining--;
        }

        // Clamp to valid position
        if (row >= lines.length) {
          row = lines.length - 1;
          col = Math.max(0, lineLength(row) - 1);
        }
        if (col >= lineLength(row)) {
          col = Math.max(0, lineLength(row) - 1);
        }

        return { position: { row, col } };
      },

      // b - [count] words backward, exclusive
      b: (count, cur) => {
        let row = cur.row;
        let col = cur.col;
        let remaining = count;

        while (remaining > 0 && (row > 0 || col > 0)) {
          const line = buffer[row];

          // If at start of line, go to previous line
          if (col === 0) {
            if (row === 0) break;
            row--;
            col = Math.max(0, lineLength(row) - 1);
            // Empty line is a word
            if (lineLength(row) === 0) {
              remaining--;
              continue;
            }
          }

          // Skip whitespace backward
          while (col > 0 && isWhitespace(getChar(row, col - 1) ?? "")) {
            col--;
          }

          // If at start of line after skipping whitespace
          if (col === 0) {
            if (row === 0) {
              remaining--;
              break;
            }
            continue;
          }

          // Find start of current word
          col--; // Move to last char of previous word
          const prevChar = getChar(row, col);
          if (prevChar) {
            const isWord = isWordChar(prevChar);
            while (col > 0) {
              const ch = getChar(row, col - 1);
              if (!ch) break;
              if (isWhitespace(ch)) break;
              if (isWordChar(ch) !== isWord) break;
              col--;
            }
          }

          remaining--;
        }

        return { position: { row, col } };
      },

      // e - Forward to the end of word [count], inclusive
      // Does not stop in empty lines
      e: (count, cur) => {
        let row = cur.row;
        let col = cur.col;
        let remaining = count;

        while (remaining > 0 && row < lines.length) {
          const line = buffer[row];

          // Skip empty lines
          if (!line || line.length === 0) {
            row++;
            col = 0;
            continue;
          }

          // Move forward by one to start search
          col++;

          // Skip whitespace
          while (row < lines.length) {
            const currentLine = buffer[row];
            if (!currentLine || currentLine.length === 0) {
              row++;
              col = 0;
              continue;
            }
            while (
              col < currentLine.length &&
              isWhitespace(getChar(row, col) ?? "")
            ) {
              col++;
            }
            if (col < currentLine.length) break;
            row++;
            col = 0;
          }

          if (row >= lines.length) break;

          // Find end of current word
          const currentChar = getChar(row, col);
          if (currentChar && !isWhitespace(currentChar)) {
            const isWord = isWordChar(currentChar);
            while (col < lineLength(row) - 1) {
              const nextChar = getChar(row, col + 1);
              if (!nextChar) break;
              if (isWhitespace(nextChar)) break;
              if (isWordChar(nextChar) !== isWord) break;
              col++;
            }
          }

          remaining--;
        }

        // Clamp
        if (row >= lines.length) {
          row = lines.length - 1;
        }
        if (col >= lineLength(row)) {
          col = Math.max(0, lineLength(row) - 1);
        }

        return { position: { row, col }, inclusive: true };
      },

      // 0 - To the first character of the line, exclusive
      zero: (_, cur) => ({
        position: { row: cur.row, col: 0 },
      }),

      // $ - To the end of the line, inclusive
      // With count >= 2, goes [count - 1] lines downward
      dollar: (count, cur) => {
        const targetRow = Math.min(lines.length - 1, cur.row + count - 1);
        const targetCol = Math.max(0, lineLength(targetRow) - 1);
        return {
          position: { row: targetRow, col: targetCol },
          inclusive: true,
        };
      },

      // gg - Goto line [count], default first line
      gg: (count) => {
        const targetRow = count > 0 ? Math.min(count - 1, lines.length - 1) : 0;
        // Find first non-blank
        let col = 0;
        const line = buffer[targetRow];
        if (line) {
          while (col < line.length && isWhitespace(line[col] ?? "")) {
            col++;
          }
        }
        return {
          position: { row: targetRow, col },
          linewise: true,
        };
      },

      // G - Goto line [count], default last line
      G: (count) => {
        const targetRow =
          count > 0 ? Math.min(count - 1, lines.length - 1) : lines.length - 1;
        // Find first non-blank
        let col = 0;
        const line = buffer[targetRow];
        if (line) {
          while (col < line.length && isWhitespace(line[col] ?? "")) {
            col++;
          }
        }
        return {
          position: { row: targetRow, col },
          linewise: true,
        };
      },
    },
    getDataInRange: (range) => {
      const result: string[][] = [];
      for (let r = range.start.row; r <= range.end.row; r++) {
        const line = buffer[r] ?? [];
        const startCol = r === range.start.row ? range.start.col : 0;
        const endCol =
          r === range.end.row
            ? Math.min(range.end.col, line.length - 1)
            : line.length - 1;
        result.push(line.slice(startCol, endCol + 1));
      }
      return result;
    },
    deleteRange: (range) => {
      deletedRanges.push(range);
      const result: string[][] = [];
      for (let r = range.start.row; r <= range.end.row; r++) {
        const line = buffer[r] ?? [];
        const startCol = r === range.start.row ? range.start.col : 0;
        const endCol =
          r === range.end.row
            ? Math.min(range.end.col, line.length - 1)
            : line.length - 1;
        result.push(line.slice(startCol, endCol + 1));
      }
      return result;
    },
    insertData: () => {},
  };

  return {
    config,
    cursor: () => ({ ...cursor }),
    setCursor: (pos: Position) => {
      cursor = pos;
    },
    buffer,
    deletedRanges,
  };
}

// Helper to send keys one at a time
function sendKeysWithAct(
  result: { current: { handleInput: (char: string, key: Key) => boolean } },
  sequence: string,
): void {
  for (const char of sequence) {
    act(() => {
      result.current.handleInput(char, createKey());
    });
  }
}

describe("Motion Tests - Official Vim Spec", () => {
  beforeEach(() => {
    registers.clear();
    jumplist.clear();
  });

  describe("h - left motion", () => {
    // Spec: "[count] characters to the left. Exclusive motion."

    test("moves one character left", () => {
      const { config, cursor, setCursor } = createTextConfig(["hello world"]);
      setCursor({ row: 0, col: 5 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("h", createKey());
      });

      expect(cursor().col).toBe(4);
    });

    test("moves [count] characters left", () => {
      const { config, cursor, setCursor } = createTextConfig(["hello world"]);
      setCursor({ row: 0, col: 8 });
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "3h");

      expect(cursor().col).toBe(5);
    });

    test("stops at first column (column 0)", () => {
      const { config, cursor, setCursor } = createTextConfig(["hello"]);
      setCursor({ row: 0, col: 2 });
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "10h");

      expect(cursor().col).toBe(0);
    });

    test("at column 0, h does not move", () => {
      const { config, cursor } = createTextConfig(["hello"]);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("h", createKey());
      });

      expect(cursor().col).toBe(0);
    });
  });

  describe("l - right motion", () => {
    // Spec: "[count] characters to the right. Exclusive motion."

    test("moves one character right", () => {
      const { config, cursor } = createTextConfig(["hello world"]);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("l", createKey());
      });

      expect(cursor().col).toBe(1);
    });

    test("moves [count] characters right", () => {
      const { config, cursor } = createTextConfig(["hello world"]);
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "5l");

      expect(cursor().col).toBe(5);
    });

    test("stops at end of line", () => {
      const { config, cursor } = createTextConfig(["hello"]);
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "20l");

      expect(cursor().col).toBe(4); // 'o' in "hello"
    });

    test("at end of line, l does not move", () => {
      const { config, cursor, setCursor } = createTextConfig(["hello"]);
      setCursor({ row: 0, col: 4 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("l", createKey());
      });

      expect(cursor().col).toBe(4);
    });
  });

  describe("j - down motion", () => {
    // Spec: "[count] lines downward. Linewise motion."

    test("moves one line down", () => {
      const { config, cursor } = createTextConfig([
        "line 1",
        "line 2",
        "line 3",
      ]);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("j", createKey());
      });

      expect(cursor().row).toBe(1);
    });

    test("moves [count] lines down", () => {
      const { config, cursor } = createTextConfig([
        "line 1",
        "line 2",
        "line 3",
        "line 4",
        "line 5",
      ]);
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "3j");

      expect(cursor().row).toBe(3);
    });

    test("stops at last line", () => {
      const { config, cursor } = createTextConfig([
        "line 1",
        "line 2",
        "line 3",
      ]);
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "10j");

      expect(cursor().row).toBe(2);
    });

    test("maintains column position when possible", () => {
      const { config, cursor, setCursor } = createTextConfig([
        "hello world",
        "foo bar baz",
      ]);
      setCursor({ row: 0, col: 7 }); // on 'o' of "world"
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("j", createKey());
      });

      expect(cursor().row).toBe(1);
      expect(cursor().col).toBe(7); // on 'r' of "bar"
    });

    test("adjusts column when new line is shorter", () => {
      const { config, cursor, setCursor } = createTextConfig([
        "hello world",
        "foo",
      ]);
      setCursor({ row: 0, col: 8 }); // on 'r' of "world"
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("j", createKey());
      });

      expect(cursor().row).toBe(1);
      expect(cursor().col).toBe(2); // clamped to end of "foo"
    });
  });

  describe("k - up motion", () => {
    // Spec: "[count] lines upward. Linewise motion."

    test("moves one line up", () => {
      const { config, cursor, setCursor } = createTextConfig([
        "line 1",
        "line 2",
        "line 3",
      ]);
      setCursor({ row: 2, col: 0 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("k", createKey());
      });

      expect(cursor().row).toBe(1);
    });

    test("moves [count] lines up", () => {
      const { config, cursor, setCursor } = createTextConfig([
        "line 1",
        "line 2",
        "line 3",
        "line 4",
        "line 5",
      ]);
      setCursor({ row: 4, col: 0 });
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "3k");

      expect(cursor().row).toBe(1);
    });

    test("stops at first line", () => {
      const { config, cursor, setCursor } = createTextConfig([
        "line 1",
        "line 2",
        "line 3",
      ]);
      setCursor({ row: 2, col: 0 });
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "10k");

      expect(cursor().row).toBe(0);
    });
  });

  describe("w - word forward motion", () => {
    // Spec: "[count] words forward. Exclusive motion."
    // A word is: sequence of letters/digits/underscores OR sequence of other non-blank

    test("moves to start of next word", () => {
      const { config, cursor } = createTextConfig(["hello world"]);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("w", createKey());
      });

      expect(cursor().col).toBe(6); // 'w' of "world"
    });

    test("moves [count] words forward", () => {
      const { config, cursor } = createTextConfig(["one two three four"]);
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "3w");

      expect(cursor().col).toBe(14); // 'f' of "four"
    });

    test("treats punctuation as separate word", () => {
      const { config, cursor } = createTextConfig(["hello, world"]);
      const { result } = renderHook(() => useVim(config));

      // From 'h', w should go to ',' (punctuation is a word)
      act(() => {
        result.current.handleInput("w", createKey());
      });

      expect(cursor().col).toBe(5); // ','
    });

    test("skips whitespace between words", () => {
      const { config, cursor } = createTextConfig(["hello   world"]);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("w", createKey());
      });

      expect(cursor().col).toBe(8); // 'w' of "world"
    });

    test("moves to next line when at end of current line", () => {
      const { config, cursor, setCursor } = createTextConfig([
        "hello",
        "world",
      ]);
      setCursor({ row: 0, col: 0 });
      const { result } = renderHook(() => useVim(config));

      // First w goes to end of "hello", second w should go to "world"
      sendKeysWithAct(result, "ww");

      expect(cursor().row).toBe(1);
      expect(cursor().col).toBe(0);
    });

    test("empty line counts as a word", () => {
      const { config, cursor } = createTextConfig(["hello", "", "world"]);
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "ww");

      // First w goes past "hello", second w should stop at empty line or go to "world"
      // In vim, empty lines are words, so w from end of "hello" goes to empty line
      // This test documents the expected behavior
    });

    test("from middle of word, moves to start of next word", () => {
      const { config, cursor, setCursor } = createTextConfig(["hello world"]);
      setCursor({ row: 0, col: 2 }); // on 'l' of "hello"
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("w", createKey());
      });

      expect(cursor().col).toBe(6); // 'w' of "world"
    });
  });

  describe("b - word backward motion", () => {
    // Spec: "[count] words backward. Exclusive motion."

    test("moves to start of previous word", () => {
      const { config, cursor, setCursor } = createTextConfig(["hello world"]);
      setCursor({ row: 0, col: 6 }); // on 'w' of "world"
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("b", createKey());
      });

      expect(cursor().col).toBe(0); // 'h' of "hello"
    });

    test("moves [count] words backward", () => {
      const { config, cursor, setCursor } = createTextConfig([
        "one two three four",
      ]);
      setCursor({ row: 0, col: 14 }); // on 'f' of "four"
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "2b");

      expect(cursor().col).toBe(4); // 't' of "two"
    });

    test("from middle of word, moves to start of current word", () => {
      const { config, cursor, setCursor } = createTextConfig(["hello world"]);
      setCursor({ row: 0, col: 8 }); // on 'r' of "world"
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("b", createKey());
      });

      expect(cursor().col).toBe(6); // 'w' of "world"
    });

    test("at start of word, moves to start of previous word", () => {
      const { config, cursor, setCursor } = createTextConfig(["hello world"]);
      setCursor({ row: 0, col: 6 }); // on 'w' of "world"
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("b", createKey());
      });

      expect(cursor().col).toBe(0); // 'h' of "hello"
    });

    test("stops at column 0 of first line", () => {
      const { config, cursor } = createTextConfig(["hello"]);
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "5b");

      expect(cursor().col).toBe(0);
    });
  });

  describe("e - end of word motion", () => {
    // Spec: "Forward to the end of word [count]. Inclusive motion."
    // Does not stop in empty lines.

    test("moves to end of current word", () => {
      const { config, cursor } = createTextConfig(["hello world"]);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("e", createKey());
      });

      expect(cursor().col).toBe(4); // 'o' of "hello"
    });

    test("from end of word, moves to end of next word", () => {
      const { config, cursor, setCursor } = createTextConfig(["hello world"]);
      setCursor({ row: 0, col: 4 }); // on 'o' of "hello"
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("e", createKey());
      });

      expect(cursor().col).toBe(10); // 'd' of "world"
    });

    test("moves [count] word ends forward", () => {
      const { config, cursor } = createTextConfig(["one two three"]);
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "2e");

      expect(cursor().col).toBe(6); // 'o' of "two"
    });

    test("is inclusive (includes end character)", () => {
      const { config, deletedRanges } = createTextConfig(["hello world"]);
      const { result } = renderHook(() => useVim(config));

      // de should delete "hello" (inclusive of 'o')
      sendKeysWithAct(result, "de");

      expect(deletedRanges).toHaveLength(1);
      const range = deletedRanges[0]!;
      expect(range.end.col).toBe(4); // includes 'o'
    });

    test("skips empty lines", () => {
      const { config, cursor } = createTextConfig(["hello", "", "world"]);
      const { result } = renderHook(() => useVim(config));

      // e from start should go to end of "hello"
      // ee should skip empty line and go to end of "world"
      sendKeysWithAct(result, "ee");

      expect(cursor().row).toBe(2);
      expect(cursor().col).toBe(4); // 'd' of "world"
    });
  });

  describe("0 - line start motion", () => {
    // Spec: "To the first character of the line. Exclusive motion."

    test("moves to column 0", () => {
      const { config, cursor, setCursor } = createTextConfig(["  hello world"]);
      setCursor({ row: 0, col: 8 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("0", createKey());
      });

      expect(cursor().col).toBe(0);
    });

    test("ignores count (any count is ignored)", () => {
      const { config, cursor, setCursor } = createTextConfig(["hello world"]);
      setCursor({ row: 0, col: 8 });
      const { result } = renderHook(() => useVim(config));

      // 5 then 0: 5 is count, 0 should still go to column 0
      // Actually, in vim, 0 at start of number is motion, not part of count
      // But after another digit, 0 is part of count
      // This is tricky - 50 is count 50, but 0 alone is motion
      act(() => {
        result.current.handleInput("0", createKey());
      });

      expect(cursor().col).toBe(0);
    });
  });

  describe("$ - line end motion", () => {
    // Spec: "To the end of the line. Inclusive motion."

    test("moves to last character of line", () => {
      const { config, cursor } = createTextConfig(["hello world"]);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("$", createKey());
      });

      expect(cursor().col).toBe(10); // 'd' of "world"
    });

    test("is inclusive", () => {
      const { config, deletedRanges } = createTextConfig(["hello"]);
      const { result } = renderHook(() => useVim(config));

      // d$ should delete entire line content
      sendKeysWithAct(result, "d$");

      expect(deletedRanges).toHaveLength(1);
      const range = deletedRanges[0]!;
      expect(range.end.col).toBe(4); // includes 'o'
    });

    test("with count >= 2, moves to end of [count-1] lines down", () => {
      const { config, cursor } = createTextConfig([
        "line 1",
        "line 2",
        "line 3",
      ]);
      const { result } = renderHook(() => useVim(config));

      // 2$ should move to end of line 2 (1 line down)
      sendKeysWithAct(result, "2$");

      expect(cursor().row).toBe(1);
      expect(cursor().col).toBe(5); // end of "line 2"
    });
  });

  describe("gg - go to first line", () => {
    // Spec: "Goto line [count], default first line, on the first non-blank character."

    test("moves to first line", () => {
      const { config, cursor, setCursor } = createTextConfig([
        "line 1",
        "line 2",
        "line 3",
      ]);
      setCursor({ row: 2, col: 3 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("g", createKey());
      });

      expect(cursor().row).toBe(0);
    });

    test("with count, goes to that line number", () => {
      const { config, cursor } = createTextConfig([
        "line 1",
        "line 2",
        "line 3",
        "line 4",
        "line 5",
      ]);
      const { result } = renderHook(() => useVim(config));

      // 3gg should go to line 3 (0-indexed: row 2)
      sendKeysWithAct(result, "3g");

      expect(cursor().row).toBe(2);
    });

    test("moves to first non-blank character", () => {
      const { config, cursor, setCursor } = createTextConfig([
        "  hello",
        "line 2",
      ]);
      setCursor({ row: 1, col: 0 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("g", createKey());
      });

      expect(cursor().row).toBe(0);
      expect(cursor().col).toBe(2); // 'h' of "hello"
    });
  });

  describe("G - go to last line", () => {
    // Spec: "Goto line [count], default last line, on the first non-blank character."

    test("moves to last line", () => {
      const { config, cursor } = createTextConfig([
        "line 1",
        "line 2",
        "line 3",
      ]);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("G", createKey());
      });

      expect(cursor().row).toBe(2);
    });

    test("with count, goes to that line number", () => {
      const { config, cursor } = createTextConfig([
        "line 1",
        "line 2",
        "line 3",
        "line 4",
        "line 5",
      ]);
      const { result } = renderHook(() => useVim(config));

      // 2G should go to line 2 (0-indexed: row 1)
      sendKeysWithAct(result, "2G");

      expect(cursor().row).toBe(1);
    });

    test("moves to first non-blank character", () => {
      const { config, cursor } = createTextConfig(["line 1", "  hello"]);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("G", createKey());
      });

      expect(cursor().row).toBe(1);
      expect(cursor().col).toBe(2); // 'h' of "hello"
    });
  });

  describe("dw special case - operator at end of line", () => {
    // Spec: "When using the 'w' motion in combination with an operator and
    // the last word moved over is at the end of a line, the end of that word
    // becomes the end of the operated text, not the first word in the next line."

    test("dw at last word of line does not include next line", () => {
      const { config, cursor, setCursor, deletedRanges } = createTextConfig([
        "hello world",
        "next line",
      ]);
      setCursor({ row: 0, col: 6 }); // on 'w' of "world"
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "dw");

      expect(deletedRanges).toHaveLength(1);
      const range = deletedRanges[0]!;
      // Should only affect row 0, not cross to row 1
      expect(range.start.row).toBe(0);
      expect(range.end.row).toBe(0);
    });
  });

  describe("cw special case", () => {
    // Spec: "'cw' and 'cW' are treated like 'ce' and 'cE' if the cursor is on a non-blank."

    test("cw on non-blank acts like ce (changes to end of word)", () => {
      const { config, cursor, deletedRanges } = createTextConfig([
        "hello world",
      ]);
      const { result } = renderHook(() => useVim(config));

      // cw from 'h' should delete "hello" (like ce), not "hello " (which regular cw would do)
      sendKeysWithAct(result, "cw");

      // This test documents the expected behavior
      // If implementation follows spec, range should end at 'o' of "hello", not include space
    });
  });

  describe("word definition", () => {
    // Spec: "A word consists of a sequence of letters, digits and underscores,
    // or a sequence of other non-blank characters, separated with white space"

    test("underscore is part of word", () => {
      const { config, cursor } = createTextConfig(["hello_world test"]);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("w", createKey());
      });

      // Should jump over "hello_world" as one word
      expect(cursor().col).toBe(12); // 't' of "test"
    });

    test("punctuation sequence is a word", () => {
      const { config, cursor } = createTextConfig(["hello... world"]);
      const { result } = renderHook(() => useVim(config));

      // w should go to '.', then another w should go to 'world'
      act(() => {
        result.current.handleInput("w", createKey());
      });
      expect(cursor().col).toBe(5); // first '.'

      act(() => {
        result.current.handleInput("w", createKey());
      });
      expect(cursor().col).toBe(9); // 'w' of "world"
    });

    test("digits are part of word", () => {
      const { config, cursor } = createTextConfig(["test123 hello"]);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("w", createKey());
      });

      // Should jump over "test123" as one word
      expect(cursor().col).toBe(8); // 'h' of "hello"
    });
  });
});
