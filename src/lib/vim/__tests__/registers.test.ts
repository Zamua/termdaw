import { describe, test, expect, beforeEach } from "bun:test";
import { registers } from "../registers";
import type { RegisterName } from "../types";

describe("RegisterManager", () => {
  beforeEach(() => {
    registers.clear();
  });

  describe("yank", () => {
    test("stores data in unnamed register", () => {
      registers.yank("hello", "char");
      const content = registers.peek<string>('"');
      expect(content?.data).toBe("hello");
    });

    test("stores data in yank register 0", () => {
      registers.yank("hello", "char");
      const content = registers.peek<string>("0");
      expect(content?.data).toBe("hello");
    });

    test("stores correct type (char)", () => {
      registers.yank("hello", "char");
      expect(registers.peek('"')?.type).toBe("char");
    });

    test("stores correct type (line)", () => {
      registers.yank("hello", "line");
      expect(registers.peek('"')?.type).toBe("line");
    });

    test("stores correct type (block)", () => {
      registers.yank("hello", "block");
      expect(registers.peek('"')?.type).toBe("block");
    });

    test("with selected register stores in that register too", () => {
      registers.setRegister("3");
      registers.yank("hello", "char");
      expect(registers.peek<string>("3")?.data).toBe("hello");
    });

    test("resets currentRegister to unnamed after use", () => {
      registers.setRegister("5");
      registers.yank("hello", "char");
      expect(registers.getSelectedRegister()).toBe('"');
    });

    test("overwrites previous yank in register 0", () => {
      registers.yank("first", "char");
      registers.yank("second", "char");
      expect(registers.peek<string>("0")?.data).toBe("second");
    });

    test("stores complex data types", () => {
      const data = [
        { pitch: 60, step: 0 },
        { pitch: 64, step: 2 },
      ];
      registers.yank(data, "char");
      expect(registers.peek('"')?.data).toEqual(data);
    });
  });

  describe("delete", () => {
    test("stores data in unnamed register", () => {
      registers.delete("deleted", "char");
      expect(registers.peek<string>('"')?.data).toBe("deleted");
    });

    test("stores data in register 1", () => {
      registers.delete("deleted", "char");
      expect(registers.peek<string>("1")?.data).toBe("deleted");
    });

    test("does NOT overwrite register 0 (yank register)", () => {
      registers.yank("yanked", "char");
      registers.delete("deleted", "char");
      expect(registers.peek<string>("0")?.data).toBe("yanked");
    });

    test("shifts history: content in 1 moves to 2", () => {
      registers.delete("first", "char");
      registers.delete("second", "char");
      expect(registers.peek<string>("2")?.data).toBe("first");
      expect(registers.peek<string>("1")?.data).toBe("second");
    });

    test("shifts full history 1->2, 2->3, ..., 8->9", () => {
      for (let i = 1; i <= 9; i++) {
        registers.delete(`delete${i}`, "char");
      }
      // delete1 should now be in register 9
      // delete9 should be in register 1
      expect(registers.peek<string>("9")?.data).toBe("delete1");
      expect(registers.peek<string>("1")?.data).toBe("delete9");
    });

    test("10th delete pushes first one out (only keeps 9 in history)", () => {
      for (let i = 1; i <= 10; i++) {
        registers.delete(`delete${i}`, "char");
      }
      // delete1 should be lost, delete2 in register 9
      expect(registers.peek<string>("9")?.data).toBe("delete2");
      expect(registers.peek<string>("1")?.data).toBe("delete10");
    });

    test("with selected register stores in that register too", () => {
      registers.setRegister("7");
      registers.delete("deleted", "char");
      expect(registers.peek<string>("7")?.data).toBe("deleted");
    });

    test("resets currentRegister to unnamed after use", () => {
      registers.setRegister("8");
      registers.delete("deleted", "char");
      expect(registers.getSelectedRegister()).toBe('"');
    });

    test("stores correct type", () => {
      registers.delete("deleted", "line");
      expect(registers.peek('"')?.type).toBe("line");
      expect(registers.peek("1")?.type).toBe("line");
    });
  });

  describe("get", () => {
    test("retrieves from unnamed register by default", () => {
      registers.yank("hello", "char");
      const content = registers.get<string>();
      expect(content?.data).toBe("hello");
    });

    test("retrieves from selected register when set", () => {
      registers.yank("hello", "char");
      registers.setRegister("0");
      registers.yank("world", "char");
      registers.setRegister("0");
      const content = registers.get<string>();
      expect(content?.data).toBe("world");
    });

    test("resets currentRegister after retrieval", () => {
      registers.setRegister("3");
      registers.yank("hello", "char");
      registers.setRegister("3");
      registers.get();
      expect(registers.getSelectedRegister()).toBe('"');
    });

    test("returns null/undefined for empty register", () => {
      expect(registers.get()).toBeFalsy();
    });

    test("returns content with type info", () => {
      registers.yank("hello", "line");
      const content = registers.get<string>();
      expect(content).toEqual({ data: "hello", type: "line" });
    });
  });

  describe("peek", () => {
    test("retrieves without changing selection", () => {
      registers.yank("hello", "char");
      registers.setRegister("5");
      registers.peek('"');
      expect(registers.getSelectedRegister()).toBe("5");
    });

    test("returns null/undefined for empty register", () => {
      expect(registers.peek("3")).toBeFalsy();
    });

    test("can peek at any named register", () => {
      registers.delete("d1", "char");
      registers.delete("d2", "char");
      expect(registers.peek<string>("1")?.data).toBe("d2");
      expect(registers.peek<string>("2")?.data).toBe("d1");
    });
  });

  describe("register selection", () => {
    test("setRegister changes current register", () => {
      registers.setRegister("5");
      expect(registers.getSelectedRegister()).toBe("5");
    });

    test("getSelectedRegister returns current register", () => {
      expect(registers.getSelectedRegister()).toBe('"');
      registers.setRegister("0");
      expect(registers.getSelectedRegister()).toBe("0");
    });

    test("register selection persists until used by yank", () => {
      registers.setRegister("3");
      expect(registers.getSelectedRegister()).toBe("3");
      registers.yank("hello", "char");
      expect(registers.getSelectedRegister()).toBe('"');
    });

    test("register selection persists until used by delete", () => {
      registers.setRegister("4");
      expect(registers.getSelectedRegister()).toBe("4");
      registers.delete("hello", "char");
      expect(registers.getSelectedRegister()).toBe('"');
    });

    test("register selection persists until used by get", () => {
      registers.yank("hello", "char");
      registers.setRegister("0");
      expect(registers.getSelectedRegister()).toBe("0");
      registers.get();
      expect(registers.getSelectedRegister()).toBe('"');
    });
  });

  describe("clear", () => {
    test("empties all registers", () => {
      registers.yank("yanked", "char");
      registers.delete("deleted", "char");
      registers.clear();
      expect(registers.peek('"')).toBeFalsy();
      expect(registers.peek("0")).toBeFalsy();
      expect(registers.peek("1")).toBeFalsy();
    });

    test("resets currentRegister to unnamed", () => {
      registers.setRegister("5");
      registers.clear();
      expect(registers.getSelectedRegister()).toBe('"');
    });
  });

  describe("vim behavior edge cases", () => {
    test("yank then delete: register 0 preserves yank, unnamed has delete", () => {
      registers.yank("yanked_content", "char");
      registers.delete("deleted_content", "char");

      // Unnamed register should have the delete (most recent operation)
      expect(registers.peek<string>('"')?.data).toBe("deleted_content");

      // Register 0 should still have the yank
      expect(registers.peek<string>("0")?.data).toBe("yanked_content");

      // Register 1 should have the delete
      expect(registers.peek<string>("1")?.data).toBe("deleted_content");
    });

    test("multiple yanks only affect register 0, not 1-9", () => {
      registers.yank("y1", "char");
      registers.yank("y2", "char");
      registers.yank("y3", "char");

      expect(registers.peek<string>("0")?.data).toBe("y3");
      expect(registers.peek("1")).toBeFalsy(); // No deletes, so 1-9 empty
    });

    test("getting from register 0 after delete returns last yank", () => {
      registers.yank("yanked", "char");
      registers.delete("deleted", "char");
      registers.setRegister("0");
      const content = registers.get<string>();
      expect(content?.data).toBe("yanked");
    });

    test("type preservation through delete history", () => {
      registers.delete("line1", "line");
      registers.delete("char2", "char");
      registers.delete("block3", "block");

      expect(registers.peek("1")?.type).toBe("block");
      expect(registers.peek("2")?.type).toBe("char");
      expect(registers.peek("3")?.type).toBe("line");
    });

    test("all valid register names work", () => {
      const names: RegisterName[] = [
        '"',
        "0",
        "1",
        "2",
        "3",
        "4",
        "5",
        "6",
        "7",
        "8",
        "9",
      ];
      for (const name of names) {
        registers.setRegister(name);
        expect(registers.getSelectedRegister()).toBe(name);
      }
    });
  });
});
