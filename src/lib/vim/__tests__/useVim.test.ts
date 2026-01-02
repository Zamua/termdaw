import { describe, test, expect, beforeEach, mock } from "bun:test";
import { renderHook, act } from "@testing-library/react";
import { useVim } from "../../../hooks/useVim";
import type { VimConfig, Key, Position } from "../types";
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

// Helper to create a test config
function createTestConfig(overrides: Partial<VimConfig<string[]>> = {}) {
  let cursor: Position = { row: 0, col: 0 };
  const data: string[][] = [
    ["a", "b", "c", "d", "e"],
    ["f", "g", "h", "i", "j"],
    ["k", "l", "m", "n", "o"],
  ];

  const defaultConfig: VimConfig<string[]> = {
    dimensions: { rows: 3, cols: 5 },
    getCursor: () => cursor,
    setCursor: (pos) => {
      cursor = pos;
    },
    motions: {
      h: (count, cur) => ({
        position: { row: cur.row, col: Math.max(0, cur.col - count) },
      }),
      j: (count, cur) => ({
        position: { row: Math.min(2, cur.row + count), col: cur.col },
        linewise: true,
      }),
      k: (count, cur) => ({
        position: { row: Math.max(0, cur.row - count), col: cur.col },
        linewise: true,
      }),
      l: (count, cur) => ({
        position: { row: cur.row, col: Math.min(4, cur.col + count) },
      }),
      w: (count, cur) => ({
        position: { row: cur.row, col: Math.min(4, cur.col + count) },
      }),
      b: (count, cur) => ({
        position: { row: cur.row, col: Math.max(0, cur.col - count) },
      }),
      e: (count, cur) => ({
        position: { row: cur.row, col: Math.min(4, cur.col + count) },
        inclusive: true,
      }),
      gg: () => ({
        position: { row: 0, col: 0 },
        linewise: true,
      }),
      G: () => ({
        position: { row: 2, col: 0 },
        linewise: true,
      }),
      zero: () => ({
        position: { row: cursor.row, col: 0 },
      }),
      dollar: () => ({
        position: { row: cursor.row, col: 4 },
        inclusive: true,
      }),
    },
    getDataInRange: (range) => {
      const result: string[] = [];
      for (let r = range.start.row; r <= range.end.row; r++) {
        const startCol = r === range.start.row ? range.start.col : 0;
        const endCol = r === range.end.row ? Math.min(range.end.col, 4) : 4;
        for (let c = startCol; c <= endCol; c++) {
          const row = data[r];
          const cell = row?.[c];
          if (cell) result.push(cell);
        }
      }
      return result;
    },
    deleteRange: (range) => {
      const result: string[] = [];
      for (let r = range.start.row; r <= range.end.row; r++) {
        const startCol = r === range.start.row ? range.start.col : 0;
        const endCol = r === range.end.row ? Math.min(range.end.col, 4) : 4;
        for (let c = startCol; c <= endCol; c++) {
          const row = data[r];
          const cell = row?.[c];
          if (row && cell) {
            result.push(cell);
            row[c] = "";
          }
        }
      }
      return result;
    },
    insertData: () => {},
    ...overrides,
  };

  return { config: defaultConfig, cursor: () => cursor, data };
}

describe("useVim", () => {
  beforeEach(() => {
    registers.clear();
    jumplist.clear();
  });

  describe("mode state", () => {
    test("starts in normal mode", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));
      expect(result.current.mode).toBe("normal");
    });

    test("enters operator-pending on d", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("d", createKey());
      });

      expect(result.current.mode).toBe("operator-pending");
    });

    test("enters visual mode on v", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });

      expect(result.current.mode).toBe("visual");
    });

    test("enters visual-block mode on Ctrl+v", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey({ ctrl: true }));
      });

      expect(result.current.mode).toBe("visual-block");
    });

    test("exits visual mode on v again", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });
      act(() => {
        result.current.handleInput("v", createKey());
      });

      expect(result.current.mode).toBe("normal");
    });

    test("escape returns to normal from any mode", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("d", createKey());
      });
      expect(result.current.mode).toBe("operator-pending");

      act(() => {
        result.current.handleInput("", createKey({ escape: true }));
      });
      expect(result.current.mode).toBe("normal");
    });
  });

  describe("motion key parsing", () => {
    test("h moves cursor left", () => {
      const { config, cursor } = createTestConfig();
      config.setCursor({ row: 0, col: 2 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("h", createKey());
      });

      expect(cursor().col).toBe(1);
    });

    test("j moves cursor down", () => {
      const { config, cursor } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("j", createKey());
      });

      expect(cursor().row).toBe(1);
    });

    test("k moves cursor up", () => {
      const { config, cursor } = createTestConfig();
      config.setCursor({ row: 2, col: 0 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("k", createKey());
      });

      expect(cursor().row).toBe(1);
    });

    test("l moves cursor right", () => {
      const { config, cursor } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("l", createKey());
      });

      expect(cursor().col).toBe(1);
    });

    test("arrow keys work like hjkl", () => {
      const { config, cursor } = createTestConfig();
      config.setCursor({ row: 1, col: 2 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("", createKey({ leftArrow: true }));
      });
      expect(cursor().col).toBe(1);

      act(() => {
        result.current.handleInput("", createKey({ rightArrow: true }));
      });
      expect(cursor().col).toBe(2);

      act(() => {
        result.current.handleInput("", createKey({ upArrow: true }));
      });
      expect(cursor().row).toBe(0);

      act(() => {
        result.current.handleInput("", createKey({ downArrow: true }));
      });
      expect(cursor().row).toBe(1);
    });

    test("w moves to next word", () => {
      const { config, cursor } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("w", createKey());
      });

      expect(cursor().col).toBe(1);
    });

    test("b moves to previous word", () => {
      const { config, cursor } = createTestConfig();
      config.setCursor({ row: 0, col: 3 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("b", createKey());
      });

      expect(cursor().col).toBe(2);
    });

    test("0 goes to first column", () => {
      const { config, cursor } = createTestConfig();
      config.setCursor({ row: 1, col: 3 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("0", createKey());
      });

      expect(cursor().col).toBe(0);
    });

    test("$ goes to last column", () => {
      const { config, cursor } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("$", createKey());
      });

      expect(cursor().col).toBe(4);
    });

    test("gg goes to first row", () => {
      const { config, cursor } = createTestConfig();
      config.setCursor({ row: 2, col: 3 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("g", createKey());
      });

      expect(cursor().row).toBe(0);
    });

    test("G goes to last row", () => {
      const { config, cursor } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("G", createKey());
      });

      expect(cursor().row).toBe(2);
    });
  });

  describe("count handling", () => {
    test("5j moves 5 rows down (clamped)", () => {
      const { config, cursor } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("5", createKey());
      });
      act(() => {
        result.current.handleInput("j", createKey());
      });

      expect(cursor().row).toBe(2); // Clamped to max row
    });

    test("3l moves 3 columns right", () => {
      const { config, cursor } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("3", createKey());
      });
      act(() => {
        result.current.handleInput("l", createKey());
      });

      expect(cursor().col).toBe(3);
    });

    test("count accumulates to form larger numbers", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("1", createKey());
      });
      expect(result.current.count).toBe(1);

      act(() => {
        result.current.handleInput("2", createKey());
      });
      expect(result.current.count).toBe(12);

      act(() => {
        result.current.handleInput("3", createKey());
      });
      expect(result.current.count).toBe(123);
    });

    test("0 at start is motion, not count", () => {
      const { config, cursor } = createTestConfig();
      config.setCursor({ row: 0, col: 3 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("0", createKey());
      });

      expect(cursor().col).toBe(0);
      expect(result.current.count).toBe(0);
    });

    test("10 is count 10 (0 after 1 is digit)", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("1", createKey());
      });
      act(() => {
        result.current.handleInput("0", createKey());
      });

      expect(result.current.count).toBe(10);
    });
  });

  describe("operator + motion", () => {
    test("dw deletes to next word and stores in register", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("d", createKey());
      });
      act(() => {
        result.current.handleInput("w", createKey());
      });

      // Check register has content
      const content = registers.peek<string[]>('"');
      expect(content).not.toBeNull();
    });

    test("yw yanks without deleting", () => {
      const { config, data } = createTestConfig();
      const originalData = JSON.parse(JSON.stringify(data));
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("y", createKey());
      });
      act(() => {
        result.current.handleInput("w", createKey());
      });

      // Data should not be modified
      expect(data[0]?.[0]).toBe(originalData[0]?.[0]);

      // But register should have content
      const content = registers.peek('"');
      expect(content).not.toBeNull();
    });

    test("dd deletes current line (linewise)", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("d", createKey());
      });
      act(() => {
        result.current.handleInput("d", createKey());
      });

      expect(result.current.mode).toBe("normal");
      const content = registers.peek<string[]>("1");
      expect(content?.type).toBe("line");
    });

    test("yy yanks current line (linewise)", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("y", createKey());
      });
      act(() => {
        result.current.handleInput("y", createKey());
      });

      const content = registers.peek('"');
      expect(content?.type).toBe("line");
    });

    test("3dw with count deletes 3 words", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("3", createKey());
      });
      act(() => {
        result.current.handleInput("d", createKey());
      });
      act(() => {
        result.current.handleInput("w", createKey());
      });

      expect(result.current.mode).toBe("normal");
    });

    test("d3w also deletes 3 words (count after operator)", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("d", createKey());
      });
      act(() => {
        result.current.handleInput("3", createKey());
      });
      act(() => {
        result.current.handleInput("w", createKey());
      });

      expect(result.current.mode).toBe("normal");
    });
  });

  describe("visual mode", () => {
    test("visual mode calculates range", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });

      expect(result.current.visualRange).not.toBeNull();
    });

    test("visual range expands with motion", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });
      const initialRange = result.current.visualRange;

      act(() => {
        result.current.handleInput("l", createKey());
      });

      expect(result.current.visualRange?.end.col).toBeGreaterThan(
        initialRange?.end.col ?? -1,
      );
    });

    test("d in visual mode deletes selection", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });
      act(() => {
        result.current.handleInput("l", createKey());
      });
      act(() => {
        result.current.handleInput("l", createKey());
      });
      act(() => {
        result.current.handleInput("d", createKey());
      });

      expect(result.current.mode).toBe("normal");
      expect(result.current.visualRange).toBeNull();
    });

    test("y in visual mode yanks selection", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });
      act(() => {
        result.current.handleInput("l", createKey());
      });
      act(() => {
        result.current.handleInput("y", createKey());
      });

      expect(result.current.mode).toBe("normal");
      const content = registers.peek('"');
      expect(content).not.toBeNull();
    });

    test("visual block mode has block type", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey({ ctrl: true }));
      });
      act(() => {
        result.current.handleInput("l", createKey());
      });

      expect(result.current.visualRange?.type).toBe("block");
    });
  });

  describe("paste operations", () => {
    test("p pastes after cursor", () => {
      const { config } = createTestConfig();
      let insertedPos: Position | null = null;
      config.insertData = (pos) => {
        insertedPos = pos;
      };
      const { result } = renderHook(() => useVim(config));

      // Yank something first
      registers.yank(["x", "y"], "char");

      act(() => {
        result.current.handleInput("p", createKey());
      });

      expect(insertedPos).not.toBeNull();
      expect(insertedPos!.col).toBe(1); // After cursor
    });

    test("P pastes at cursor", () => {
      const { config } = createTestConfig();
      let insertedPos: Position | null = null;
      config.insertData = (pos) => {
        insertedPos = pos;
      };
      const { result } = renderHook(() => useVim(config));

      registers.yank(["x", "y"], "char");

      act(() => {
        result.current.handleInput("P", createKey());
      });

      expect(insertedPos).not.toBeNull();
      expect(insertedPos!.col).toBe(0); // At cursor
    });
  });

  describe("jumplist operations", () => {
    test("gg adds to jumplist", () => {
      const { config } = createTestConfig();
      config.setCursor({ row: 2, col: 3 });
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("g", createKey());
      });

      expect(jumplist.getList()).toHaveLength(1);
      expect(jumplist.getList()[0]).toEqual({ row: 2, col: 3 });
    });

    test("G adds to jumplist", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("G", createKey());
      });

      expect(jumplist.getList()).toHaveLength(1);
    });

    test("Ctrl+o jumps back", () => {
      const { config, cursor } = createTestConfig();
      config.setCursor({ row: 0, col: 0 });
      const { result } = renderHook(() => useVim(config));

      // Add some positions to jumplist
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 2, col: 3 });

      act(() => {
        result.current.handleInput("o", createKey({ ctrl: true }));
      });

      expect(cursor().row).toBe(0);
    });

    test("Ctrl+i jumps forward", () => {
      const { config, cursor } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 2, col: 3 });
      jumplist.back();

      act(() => {
        result.current.handleInput("i", createKey({ ctrl: true }));
      });

      expect(cursor().row).toBe(2);
    });
  });

  describe("repeat (dot command)", () => {
    test(". repeats last operator+motion", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      // dw
      act(() => {
        result.current.handleInput("d", createKey());
      });
      act(() => {
        result.current.handleInput("w", createKey());
      });

      // . to repeat
      act(() => {
        result.current.handleInput(".", createKey());
      });

      expect(result.current.mode).toBe("normal");
    });

    test(". with no previous action does nothing harmful", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput(".", createKey());
      });

      expect(result.current.mode).toBe("normal");
    });
  });

  describe("handleInput return value", () => {
    test("returns true for handled vim keys", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      let handled = false;
      act(() => {
        handled = result.current.handleInput("j", createKey());
      });

      expect(handled).toBe(true);
    });

    test("returns false for unhandled keys", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      let handled = true;
      act(() => {
        handled = result.current.handleInput("z", createKey());
      });

      expect(handled).toBe(false);
    });

    test("returns true for escape", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      let handled = false;
      act(() => {
        handled = result.current.handleInput("", createKey({ escape: true }));
      });

      expect(handled).toBe(true);
    });
  });

  describe("custom actions", () => {
    test("onCustomAction is called for unhandled keys", () => {
      const { config } = createTestConfig();
      const customAction = mock(() => true);
      config.onCustomAction = customAction;
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("x", createKey());
      });

      expect(customAction).toHaveBeenCalledWith("x", expect.any(Object), 1);
    });

    test("onCustomAction receives count", () => {
      const { config } = createTestConfig();
      const customAction = mock(() => true);
      config.onCustomAction = customAction;
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("5", createKey());
      });
      act(() => {
        result.current.handleInput("x", createKey());
      });

      expect(customAction).toHaveBeenCalledWith("x", expect.any(Object), 5);
    });

    test("handleInput returns true when customAction returns true", () => {
      const { config } = createTestConfig();
      config.onCustomAction = () => true;
      const { result } = renderHook(() => useVim(config));

      let handled = false;
      act(() => {
        handled = result.current.handleInput("x", createKey());
      });

      expect(handled).toBe(true);
    });
  });

  describe("mode change callback", () => {
    test("onModeChange is called when mode changes", () => {
      const { config } = createTestConfig();
      const onModeChange = mock(() => {});
      config.onModeChange = onModeChange;
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });

      expect(onModeChange).toHaveBeenCalledWith("visual");
    });
  });

  describe("reset function", () => {
    test("reset returns to normal mode", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("d", createKey());
      });
      expect(result.current.mode).toBe("operator-pending");

      act(() => {
        result.current.reset();
      });
      expect(result.current.mode).toBe("normal");
    });

    test("reset clears visual range", () => {
      const { config } = createTestConfig();
      const { result } = renderHook(() => useVim(config));

      act(() => {
        result.current.handleInput("v", createKey());
      });
      expect(result.current.visualRange).not.toBeNull();

      act(() => {
        result.current.reset();
      });
      expect(result.current.visualRange).toBeNull();
    });
  });
});
