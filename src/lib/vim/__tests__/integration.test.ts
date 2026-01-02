import { describe, test, expect, beforeEach } from "bun:test";
import { renderHook, act } from "@testing-library/react";
import { useVim } from "../../../hooks/useVim";
import type { VimConfig, Key, Position, Range } from "../types";
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

// Grid-based test setup (like a step sequencer)
function createGridConfig(rows = 10, cols = 16) {
  let cursor: Position = { row: 0, col: 0 };
  // 2D array of booleans (like step sequencer on/off states)
  const grid: boolean[][] = Array.from({ length: rows }, () =>
    Array.from({ length: cols }, () => false),
  );

  // Track deleted ranges for verification
  const deletedRanges: Range[] = [];
  const yankedRanges: Range[] = [];

  const config: VimConfig<boolean[][]> = {
    dimensions: { rows, cols },
    getCursor: () => ({ ...cursor }),
    setCursor: (pos) => {
      cursor = {
        row: Math.max(0, Math.min(rows - 1, pos.row)),
        col: Math.max(0, Math.min(cols - 1, pos.col)),
      };
    },
    motions: {
      h: (count, cur) => ({
        position: { row: cur.row, col: Math.max(0, cur.col - count) },
      }),
      j: (count, cur) => ({
        position: { row: Math.min(rows - 1, cur.row + count), col: cur.col },
        linewise: true,
      }),
      k: (count, cur) => ({
        position: { row: Math.max(0, cur.row - count), col: cur.col },
        linewise: true,
      }),
      l: (count, cur) => ({
        position: { row: cur.row, col: Math.min(cols - 1, cur.col + count) },
      }),
      w: (count, cur) => ({
        // Simplified: move count columns right
        position: { row: cur.row, col: Math.min(cols - 1, cur.col + count) },
      }),
      b: (count, cur) => ({
        // Simplified: move count columns left
        position: { row: cur.row, col: Math.max(0, cur.col - count) },
      }),
      e: (count, cur) => ({
        position: { row: cur.row, col: Math.min(cols - 1, cur.col + count) },
        inclusive: true,
      }),
      gg: (count) => ({
        position: { row: Math.max(0, (count || 1) - 1), col: 0 },
        linewise: true,
      }),
      G: (count) => ({
        position: { row: count ? count - 1 : rows - 1, col: 0 },
        linewise: true,
      }),
      zero: (_, cur) => ({
        position: { row: cur.row, col: 0 },
      }),
      dollar: (_, cur) => ({
        position: { row: cur.row, col: cols - 1 },
        inclusive: true,
      }),
    },
    getDataInRange: (range) => {
      yankedRanges.push(range);
      const result: boolean[][] = [];
      for (let r = range.start.row; r <= range.end.row; r++) {
        const row: boolean[] = [];
        const startCol = r === range.start.row ? range.start.col : 0;
        const endCol =
          r === range.end.row ? Math.min(range.end.col, cols - 1) : cols - 1;
        for (let c = startCol; c <= endCol; c++) {
          row.push(grid[r]?.[c] ?? false);
        }
        result.push(row);
      }
      return result;
    },
    deleteRange: (range) => {
      deletedRanges.push(range);
      const result: boolean[][] = [];
      for (let r = range.start.row; r <= range.end.row; r++) {
        const row: boolean[] = [];
        const startCol = r === range.start.row ? range.start.col : 0;
        const endCol =
          r === range.end.row ? Math.min(range.end.col, cols - 1) : cols - 1;
        for (let c = startCol; c <= endCol; c++) {
          const gridRow = grid[r];
          if (gridRow) {
            row.push(gridRow[c] ?? false);
            gridRow[c] = false;
          }
        }
        result.push(row);
      }
      return result;
    },
    insertData: (pos, data) => {
      for (let r = 0; r < data.length; r++) {
        const dataRow = data[r];
        if (!dataRow) continue;
        for (let c = 0; c < dataRow.length; c++) {
          const targetRow = pos.row + r;
          const targetCol = pos.col + c;
          const gridRow = grid[targetRow];
          if (gridRow && targetCol < cols) {
            gridRow[targetCol] = dataRow[c] ?? false;
          }
        }
      }
    },
  };

  return {
    config,
    cursor: () => ({ ...cursor }),
    grid,
    deletedRanges,
    yankedRanges,
    setCursor: (pos: Position) => {
      cursor = pos;
    },
    setCell: (row: number, col: number, value: boolean) => {
      const gridRow = grid[row];
      if (gridRow) {
        gridRow[col] = value;
      }
    },
  };
}

// Helper for key sequences that need separate act() per key
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

describe("Integration Tests", () => {
  beforeEach(() => {
    registers.clear();
    jumplist.clear();
  });

  describe("basic motions", () => {
    test("h moves cursor left", () => {
      const { config, cursor, setCursor } = createGridConfig();
      setCursor({ row: 0, col: 5 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("h", createKey());
      });

      expect(cursor().col).toBe(4);
    });

    test("l moves cursor right", () => {
      const { config, cursor } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("l", createKey());
      });

      expect(cursor().col).toBe(1);
    });

    test("j moves cursor down", () => {
      const { config, cursor } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("j", createKey());
      });

      expect(cursor().row).toBe(1);
    });

    test("k moves cursor up", () => {
      const { config, cursor, setCursor } = createGridConfig();
      setCursor({ row: 5, col: 0 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("k", createKey());
      });

      expect(cursor().row).toBe(4);
    });

    test("5j moves 5 rows down", () => {
      const { config, cursor } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "5j");

      expect(cursor().row).toBe(5);
    });

    test("10l moves 10 columns right (clamped to max)", () => {
      const { config, cursor } = createGridConfig(10, 16);
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "10l");

      expect(cursor().col).toBe(10);
    });

    test("gg goes to first row", () => {
      const { config, cursor, setCursor } = createGridConfig();
      setCursor({ row: 8, col: 5 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("g", createKey());
      });

      expect(cursor().row).toBe(0);
    });

    test("G goes to last row", () => {
      const { config, cursor } = createGridConfig(10, 16);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("G", createKey());
      });

      expect(cursor().row).toBe(9);
    });

    test("0 goes to first column", () => {
      const { config, cursor, setCursor } = createGridConfig();
      setCursor({ row: 3, col: 10 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("0", createKey());
      });

      expect(cursor().col).toBe(0);
    });

    test("$ goes to last column", () => {
      const { config, cursor } = createGridConfig(10, 16);
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("$", createKey());
      });

      expect(cursor().col).toBe(15);
    });

    test("w moves forward by count", () => {
      const { config, cursor } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "3w");

      expect(cursor().col).toBe(3);
    });

    test("b moves backward by count", () => {
      const { config, cursor, setCursor } = createGridConfig();
      setCursor({ row: 0, col: 10 });
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "3b");

      expect(cursor().col).toBe(7);
    });
  });

  describe("operators with motions", () => {
    test("dw deletes to next position", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "dw");

      expect(deletedRanges).toHaveLength(1);
      expect(result.current.mode).toBe("normal");
    });

    test("d$ deletes to end of line", () => {
      const { config, deletedRanges, setCursor } = createGridConfig();
      setCursor({ row: 0, col: 5 });
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "d$");

      expect(deletedRanges).toHaveLength(1);
      const range = deletedRanges[0]!;
      expect(range.end.col).toBe(15); // last column
    });

    test("d0 deletes to start of line", () => {
      const { config, deletedRanges, setCursor } = createGridConfig();
      setCursor({ row: 0, col: 5 });
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "d0");

      expect(deletedRanges).toHaveLength(1);
      const range = deletedRanges[0]!;
      expect(range.start.col).toBe(0);
    });

    test("dj deletes current and next line (linewise)", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "dj");

      expect(deletedRanges).toHaveLength(1);
      const range = deletedRanges[0]!;
      expect(range.type).toBe("line");
      expect(range.start.row).toBe(0);
      expect(range.end.row).toBe(1);
    });

    test("dk deletes current and previous line", () => {
      const { config, deletedRanges, setCursor } = createGridConfig();
      setCursor({ row: 2, col: 0 });
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "dk");

      expect(deletedRanges).toHaveLength(1);
      const range = deletedRanges[0]!;
      expect(range.start.row).toBe(1);
      expect(range.end.row).toBe(2);
    });

    test("yw yanks without deleting", () => {
      const { config, deletedRanges, yankedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "yw");

      expect(deletedRanges).toHaveLength(0);
      expect(yankedRanges).toHaveLength(1);
    });

    test("y$ yanks to end of line", () => {
      const { config, yankedRanges, setCursor } = createGridConfig();
      setCursor({ row: 0, col: 3 });
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "y$");

      expect(yankedRanges).toHaveLength(1);
      const range = yankedRanges[0]!;
      expect(range.end.col).toBe(15);
    });
  });

  describe("linewise operations (dd, yy, cc)", () => {
    test("dd deletes current line", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "dd");

      expect(deletedRanges).toHaveLength(1);
      expect(deletedRanges[0]?.type).toBe("line");
      expect(result.current.mode).toBe("normal");
    });

    test("yy yanks current line", () => {
      const { config, yankedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "yy");

      expect(yankedRanges).toHaveLength(1);
      expect(yankedRanges[0]?.type).toBe("line");
    });

    test("cc changes current line (deletes linewise)", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "cc");

      expect(deletedRanges).toHaveLength(1);
      expect(deletedRanges[0]?.type).toBe("line");
    });

    test("3dd deletes 3 lines", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "3dd");

      expect(deletedRanges).toHaveLength(1);
      const range = deletedRanges[0]!;
      expect(range.start.row).toBe(0);
      expect(range.end.row).toBe(2);
    });

    test("5yy yanks 5 lines", () => {
      const { config, yankedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "5yy");

      expect(yankedRanges).toHaveLength(1);
      const range = yankedRanges[0]!;
      expect(range.start.row).toBe(0);
      expect(range.end.row).toBe(4);
    });
  });

  describe("count combinations", () => {
    test("3dw deletes with count before operator", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "3dw");

      expect(deletedRanges).toHaveLength(1);
    });

    test("d3w deletes with count after operator", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "d3w");

      expect(deletedRanges).toHaveLength(1);
    });

    test("5j then 3k net moves 2 down", () => {
      const { config, cursor } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "5j");
      sendKeysWithAct(result, "3k");

      expect(cursor().row).toBe(2);
    });

    test("10h at column 5 stops at column 0", () => {
      const { config, cursor, setCursor } = createGridConfig();
      setCursor({ row: 0, col: 5 });
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "10h");

      expect(cursor().col).toBe(0);
    });
  });

  describe("visual mode", () => {
    test("v enters visual mode", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });

      expect(result.current.mode).toBe("visual");
    });

    test("v then movement extends selection", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });
      const initialEnd = result.current.visualRange?.end.col;

      act(() => {
        result.current.handleInput("l", createKey());
      });

      expect(result.current.visualRange!.end.col).toBeGreaterThan(
        initialEnd ?? -1,
      );
    });

    test("v then v exits visual mode", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });
      act(() => {
        result.current.handleInput("v", createKey());
      });

      expect(result.current.mode).toBe("normal");
      expect(result.current.visualRange).toBeNull();
    });

    test("vjjd deletes visual selection across lines", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "vjjd");

      expect(result.current.mode).toBe("normal");
      expect(deletedRanges).toHaveLength(1);
    });

    test("Ctrl+v enters visual block mode", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey({ ctrl: true }));
      });

      expect(result.current.mode).toBe("visual-block");
    });

    test("visual block selection has block type", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey({ ctrl: true }));
      });
      act(() => {
        result.current.handleInput("l", createKey());
      });

      expect(result.current.visualRange?.type).toBe("block");
    });

    test("d in visual mode deletes selection", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "vlld");

      expect(deletedRanges).toHaveLength(1);
      expect(result.current.mode).toBe("normal");
    });

    test("y in visual mode yanks selection", () => {
      const { config, yankedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "vlly");

      expect(yankedRanges).toHaveLength(1);
      expect(result.current.mode).toBe("normal");
    });
  });

  describe("register operations", () => {
    test("dd then p pastes deleted content", () => {
      const { config, setCell, setCursor } = createGridConfig(10, 16);
      // Set up some data on row 0
      setCell(0, 0, true);
      setCell(0, 1, true);

      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "dd");
      setCursor({ row: 1, col: 0 });
      act(() => {
        result.current.handleInput("p", createKey());
      });

      // Verify paste was attempted (register should have content)
      const content = registers.peek('"');
      expect(content).not.toBeNull();
    });

    test("yy then p pastes yanked content", () => {
      const { config, setCell, setCursor } = createGridConfig(10, 16);
      setCell(0, 0, true);

      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "yy");
      setCursor({ row: 1, col: 0 });
      act(() => {
        result.current.handleInput("p", createKey());
      });

      const content = registers.peek('"');
      expect(content).not.toBeNull();
    });

    test("multiple dd maintains history in registers 1-9", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      // Delete multiple times
      for (let i = 0; i < 5; i++) {
        sendKeysWithAct(result, "dd");
      }

      // Should have content in registers 1-5
      expect(registers.peek("1")).not.toBeNull();
      expect(registers.peek("2")).not.toBeNull();
      expect(registers.peek("3")).not.toBeNull();
    });

    test("yank does not overwrite register 0 on delete", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      // First yank
      sendKeysWithAct(result, "yy");
      const yankContent = registers.peek("0");

      // Then delete
      sendKeysWithAct(result, "dd");

      // Register 0 should still have yank content
      expect(registers.peek("0")).toEqual(yankContent);
    });
  });

  describe("jumplist", () => {
    test("gg adds current position to jumplist before jumping", () => {
      const { config, setCursor } = createGridConfig();
      setCursor({ row: 5, col: 3 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("g", createKey());
      });

      expect(jumplist.getList()).toHaveLength(1);
      expect(jumplist.getList()[0]).toEqual({ row: 5, col: 3 });
    });

    test("G adds current position to jumplist", () => {
      const { config, setCursor } = createGridConfig();
      setCursor({ row: 2, col: 4 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("G", createKey());
      });

      expect(jumplist.getList()).toHaveLength(1);
    });

    test("Ctrl+o jumps back in jumplist", () => {
      const { config, cursor, setCursor } = createGridConfig();

      // Manually add jumplist entries
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 5, col: 5 });
      setCursor({ row: 5, col: 5 });

      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("o", createKey({ ctrl: true }));
      });

      expect(cursor().row).toBe(0);
    });

    test("Ctrl+i jumps forward in jumplist", () => {
      const { config, cursor, setCursor } = createGridConfig();

      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 5, col: 5 });
      jumplist.back(); // Go back to first position
      setCursor({ row: 0, col: 0 });

      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("i", createKey({ ctrl: true }));
      });

      expect(cursor().row).toBe(5);
    });

    test("jump back/forward sequence", () => {
      const { config, cursor, setCursor } = createGridConfig();

      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 3, col: 3 });
      jumplist.push({ row: 6, col: 6 });
      setCursor({ row: 6, col: 6 });

      const { result } = renderHook(() => useVim(config));

      // Go back twice
      act(() => {
        result.current.handleInput("o", createKey({ ctrl: true }));
      });
      expect(cursor().row).toBe(3);

      act(() => {
        result.current.handleInput("o", createKey({ ctrl: true }));
      });
      expect(cursor().row).toBe(0);

      // Go forward once
      act(() => {
        result.current.handleInput("i", createKey({ ctrl: true }));
      });
      expect(cursor().row).toBe(3);
    });
  });

  describe("repeat (dot command)", () => {
    test("dw then . repeats delete", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "dw");
      act(() => {
        result.current.handleInput(".", createKey());
      });

      expect(deletedRanges).toHaveLength(2);
    });

    test(". works after dd", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "dd");
      act(() => {
        result.current.handleInput(".", createKey());
      });

      expect(deletedRanges).toHaveLength(2);
    });

    test(". with no previous action does nothing", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput(".", createKey());
      });

      expect(deletedRanges).toHaveLength(0);
      expect(result.current.mode).toBe("normal");
    });
  });

  describe("escape behavior", () => {
    test("escape in normal mode is harmless", () => {
      const { config, cursor } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      const initialPos = cursor();
      act(() => {
        result.current.handleInput("", createKey({ escape: true }));
      });

      expect(result.current.mode).toBe("normal");
      expect(cursor()).toEqual(initialPos);
    });

    test("escape cancels pending operator", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("d", createKey());
      });
      expect(result.current.mode).toBe("operator-pending");

      act(() => {
        result.current.handleInput("", createKey({ escape: true }));
      });
      expect(result.current.mode).toBe("normal");
      expect(result.current.operator).toBeNull();
    });

    test("escape exits visual mode", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });
      expect(result.current.mode).toBe("visual");

      act(() => {
        result.current.handleInput("", createKey({ escape: true }));
      });
      expect(result.current.mode).toBe("normal");
      expect(result.current.visualRange).toBeNull();
    });

    test("escape clears accumulated count", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "123");
      expect(result.current.count).toBe(123);

      act(() => {
        result.current.handleInput("", createKey({ escape: true }));
      });
      expect(result.current.count).toBe(0);
    });
  });

  describe("edge cases and vim quirks", () => {
    test("0 at start of count is motion to column 0", () => {
      const { config, cursor, setCursor } = createGridConfig();
      setCursor({ row: 0, col: 5 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("0", createKey());
      });

      expect(cursor().col).toBe(0);
      expect(result.current.count).toBe(0);
    });

    test("10 is count 10, 0 after 1 is part of count", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "10");

      expect(result.current.count).toBe(10);
    });

    test("motion with no movement still completes", () => {
      const { config, cursor } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      // h at column 0 shouldn't move but should still work
      act(() => {
        result.current.handleInput("h", createKey());
      });

      expect(cursor().col).toBe(0);
      expect(result.current.mode).toBe("normal");
    });

    test("handleInput returns true for vim keys", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      let handled = false;
      act(() => {
        handled = result.current.handleInput("j", createKey());
      });

      expect(handled).toBe(true);
    });

    test("handleInput returns false for unknown keys", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      let handled = true;
      act(() => {
        handled = result.current.handleInput("z", createKey());
      });

      expect(handled).toBe(false);
    });

    test("switching operators clears count and starts fresh", () => {
      const { config } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "5d");
      expect(result.current.operator).toBe("d");

      act(() => {
        result.current.handleInput("y", createKey());
      });
      expect(result.current.operator).toBe("y");
      expect(result.current.count).toBe(0);
    });
  });

  describe("complex sequences", () => {
    test("5dw deletes 5 words", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "5dw");

      expect(deletedRanges).toHaveLength(1);
      expect(result.current.mode).toBe("normal");
    });

    test("vjjjd then p pastes visual selection", () => {
      const { config, deletedRanges } = createGridConfig();
      const { result } = renderHook(() => useVim(config));

      sendKeysWithAct(result, "vjjjd");

      expect(deletedRanges).toHaveLength(1);

      // Content should be in register
      const content = registers.peek('"');
      expect(content).not.toBeNull();

      // Can paste
      act(() => {
        result.current.handleInput("p", createKey());
      });
    });

    test("ggVGd deletes entire document (visual line mode)", () => {
      const { config, setCursor, deletedRanges } = createGridConfig();
      setCursor({ row: 5, col: 5 });
      const { result } = renderHook(() => useVim(config));

      // gg - go to top
      act(() => {
        result.current.handleInput("g", createKey());
      });

      // V - visual line mode (we only have v, so use v for character)
      act(() => {
        result.current.handleInput("v", createKey());
      });

      // G - go to bottom
      act(() => {
        result.current.handleInput("G", createKey());
      });

      // d - delete
      act(() => {
        result.current.handleInput("d", createKey());
      });

      expect(deletedRanges).toHaveLength(1);
      expect(result.current.mode).toBe("normal");
    });
  });
});
