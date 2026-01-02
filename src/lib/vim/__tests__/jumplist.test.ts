import { describe, test, expect, beforeEach } from "bun:test";
import { jumplist } from "../jumplist";

describe("Jumplist", () => {
  beforeEach(() => {
    jumplist.clear();
  });

  describe("push", () => {
    test("adds position to list", () => {
      jumplist.push({ row: 0, col: 0 });
      expect(jumplist.getList()).toHaveLength(1);
      expect(jumplist.getList()[0]).toEqual({ row: 0, col: 0 });
    });

    test("increments index after push", () => {
      expect(jumplist.getIndex()).toBe(-1);
      jumplist.push({ row: 0, col: 0 });
      expect(jumplist.getIndex()).toBe(0);
      jumplist.push({ row: 1, col: 0 });
      expect(jumplist.getIndex()).toBe(1);
    });

    test("does not add duplicate of current position", () => {
      jumplist.push({ row: 5, col: 10 });
      jumplist.push({ row: 5, col: 10 });
      expect(jumplist.getList()).toHaveLength(1);
    });

    test("adds position even if same row but different col", () => {
      jumplist.push({ row: 5, col: 10 });
      jumplist.push({ row: 5, col: 15 });
      expect(jumplist.getList()).toHaveLength(2);
    });

    test("truncates forward history when not at end", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.push({ row: 2, col: 0 });
      jumplist.back(); // index now at 1
      jumplist.push({ row: 10, col: 0 }); // should truncate position at index 2
      expect(jumplist.getList()).toHaveLength(3);
      expect(jumplist.getList()[2]).toEqual({ row: 10, col: 0 });
    });

    test("respects maxSize of 100", () => {
      for (let i = 0; i < 110; i++) {
        jumplist.push({ row: i, col: 0 });
      }
      expect(jumplist.getList()).toHaveLength(100);
    });

    test("removes oldest entries when over maxSize", () => {
      for (let i = 0; i < 110; i++) {
        jumplist.push({ row: i, col: 0 });
      }
      // Oldest entries (rows 0-9) should be removed
      expect(jumplist.getList()[0]).toEqual({ row: 10, col: 0 });
      expect(jumplist.getList()[99]).toEqual({ row: 109, col: 0 });
    });

    test("adjusts index when removing old entries", () => {
      for (let i = 0; i < 110; i++) {
        jumplist.push({ row: i, col: 0 });
      }
      // Index should still point to last element
      expect(jumplist.getIndex()).toBe(99);
    });

    test("creates a copy of the position object", () => {
      const pos = { row: 5, col: 10 };
      jumplist.push(pos);
      pos.row = 99;
      expect(jumplist.getList()[0]).toEqual({ row: 5, col: 10 });
    });
  });

  describe("back", () => {
    test("decrements index and returns previous position", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.push({ row: 2, col: 0 });
      const pos = jumplist.back();
      expect(pos).toEqual({ row: 1, col: 0 });
      expect(jumplist.getIndex()).toBe(1);
    });

    test("returns null when at start (index=0)", () => {
      jumplist.push({ row: 0, col: 0 });
      expect(jumplist.back()).toBeNull();
      expect(jumplist.getIndex()).toBe(0);
    });

    test("returns null when list is empty", () => {
      expect(jumplist.back()).toBeNull();
    });

    test("can navigate back multiple times", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.push({ row: 2, col: 0 });
      jumplist.push({ row: 3, col: 0 });
      expect(jumplist.back()).toEqual({ row: 2, col: 0 });
      expect(jumplist.back()).toEqual({ row: 1, col: 0 });
      expect(jumplist.back()).toEqual({ row: 0, col: 0 });
      expect(jumplist.back()).toBeNull();
    });
  });

  describe("forward", () => {
    test("increments index and returns next position", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.push({ row: 2, col: 0 });
      jumplist.back();
      jumplist.back();
      const pos = jumplist.forward();
      expect(pos).toEqual({ row: 1, col: 0 });
      expect(jumplist.getIndex()).toBe(1);
    });

    test("returns null when at end", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      expect(jumplist.forward()).toBeNull();
    });

    test("returns null when list is empty", () => {
      expect(jumplist.forward()).toBeNull();
    });

    test("can navigate forward after going back", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.push({ row: 2, col: 0 });
      jumplist.back();
      jumplist.back();
      expect(jumplist.forward()).toEqual({ row: 1, col: 0 });
      expect(jumplist.forward()).toEqual({ row: 2, col: 0 });
      expect(jumplist.forward()).toBeNull();
    });
  });

  describe("current", () => {
    test("returns position at current index", () => {
      jumplist.push({ row: 5, col: 10 });
      expect(jumplist.current()).toEqual({ row: 5, col: 10 });
    });

    test("returns null when list is empty", () => {
      expect(jumplist.current()).toBeNull();
    });

    test("returns correct position after navigation", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.push({ row: 2, col: 0 });
      jumplist.back();
      expect(jumplist.current()).toEqual({ row: 1, col: 0 });
    });
  });

  describe("canGoBack", () => {
    test("returns false when list is empty", () => {
      expect(jumplist.canGoBack()).toBe(false);
    });

    test("returns false when at first position (index=0)", () => {
      jumplist.push({ row: 0, col: 0 });
      expect(jumplist.canGoBack()).toBe(false);
    });

    test("returns true when not at first position", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      expect(jumplist.canGoBack()).toBe(true);
    });

    test("returns false after going back to start", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.back();
      expect(jumplist.canGoBack()).toBe(false);
    });
  });

  describe("canGoForward", () => {
    test("returns false when list is empty", () => {
      expect(jumplist.canGoForward()).toBe(false);
    });

    test("returns false when at end of list", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      expect(jumplist.canGoForward()).toBe(false);
    });

    test("returns true after going back", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.back();
      expect(jumplist.canGoForward()).toBe(true);
    });

    test("returns false after going forward to end", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.back();
      jumplist.forward();
      expect(jumplist.canGoForward()).toBe(false);
    });
  });

  describe("clear", () => {
    test("empties the list", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.clear();
      expect(jumplist.getList()).toHaveLength(0);
    });

    test("resets index to -1", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.clear();
      expect(jumplist.getIndex()).toBe(-1);
    });
  });

  describe("edge cases", () => {
    test("single item jumplist - cannot go back", () => {
      jumplist.push({ row: 5, col: 5 });
      expect(jumplist.canGoBack()).toBe(false);
      expect(jumplist.canGoForward()).toBe(false);
    });

    test("navigation sequence: push, push, back, push truncates forward", () => {
      jumplist.push({ row: 0, col: 0 });
      jumplist.push({ row: 1, col: 0 });
      jumplist.push({ row: 2, col: 0 });
      jumplist.back(); // at row 1
      jumplist.push({ row: 10, col: 0 }); // should remove row 2, add row 10
      expect(jumplist.getList()).toHaveLength(3);
      expect(jumplist.getList().map((p) => p.row)).toEqual([0, 1, 10]);
      expect(jumplist.canGoForward()).toBe(false);
    });

    test("back and forward preserve positions", () => {
      jumplist.push({ row: 0, col: 5 });
      jumplist.push({ row: 10, col: 15 });
      jumplist.push({ row: 20, col: 25 });

      jumplist.back();
      jumplist.back();
      expect(jumplist.current()).toEqual({ row: 0, col: 5 });

      jumplist.forward();
      expect(jumplist.current()).toEqual({ row: 10, col: 15 });

      jumplist.forward();
      expect(jumplist.current()).toEqual({ row: 20, col: 25 });
    });
  });
});
