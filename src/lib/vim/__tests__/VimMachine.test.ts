import { describe, test, expect } from "bun:test";
import { createActor } from "xstate";
import { vimMachine } from "../VimMachine";

function createVimActor() {
  return createActor(vimMachine).start();
}

describe("VimMachine", () => {
  describe("initial state", () => {
    test("starts in normal mode", () => {
      const actor = createVimActor();
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("starts with count 0", () => {
      const actor = createVimActor();
      expect(actor.getSnapshot().context.count).toBe(0);
    });

    test("starts with operator null", () => {
      const actor = createVimActor();
      expect(actor.getSnapshot().context.operator).toBeNull();
    });

    test("starts with visualStart null", () => {
      const actor = createVimActor();
      expect(actor.getSnapshot().context.visualStart).toBeNull();
    });

    test("starts with lastAction null", () => {
      const actor = createVimActor();
      expect(actor.getSnapshot().context.lastAction).toBeNull();
    });
  });

  describe("mode transitions from normal", () => {
    test("d transitions to operator-pending", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      expect(actor.getSnapshot().value).toBe("operator");
    });

    test("y transitions to operator-pending", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "y" });
      expect(actor.getSnapshot().value).toBe("operator");
    });

    test("c transitions to operator-pending", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "c" });
      expect(actor.getSnapshot().value).toBe("operator");
    });

    test("v transitions to visual", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL" });
      expect(actor.getSnapshot().value).toBe("visual");
    });

    test("Ctrl+v transitions to visual-block", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL_BLOCK" });
      expect(actor.getSnapshot().value).toBe("visualBlock");
    });

    test("MOTION stays in normal but clears count", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 5 });
      actor.send({
        type: "MOTION",
        motion: "j",
        position: { row: 5, col: 0 },
      });
      expect(actor.getSnapshot().value).toBe("normal");
      expect(actor.getSnapshot().context.count).toBe(0);
    });

    test("ESCAPE stays in normal and resets", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 5 });
      actor.send({ type: "ESCAPE" });
      expect(actor.getSnapshot().value).toBe("normal");
      expect(actor.getSnapshot().context.count).toBe(0);
    });
  });

  describe("mode transitions from operator-pending", () => {
    test("MOTION returns to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({
        type: "MOTION",
        motion: "w",
        position: { row: 0, col: 5 },
      });
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("same OPERATOR (dd) returns to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({ type: "OPERATOR", operator: "d" });
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("same OPERATOR (yy) returns to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "y" });
      actor.send({ type: "OPERATOR", operator: "y" });
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("same OPERATOR (cc) returns to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "c" });
      actor.send({ type: "OPERATOR", operator: "c" });
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("different OPERATOR switches to new operator", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({ type: "OPERATOR", operator: "y" });
      expect(actor.getSnapshot().value).toBe("operator");
      expect(actor.getSnapshot().context.operator).toBe("y");
    });

    test("ESCAPE returns to normal and resets", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({ type: "ESCAPE" });
      expect(actor.getSnapshot().value).toBe("normal");
      expect(actor.getSnapshot().context.operator).toBeNull();
    });

    test("RESET returns to normal and resets", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({ type: "RESET" });
      expect(actor.getSnapshot().value).toBe("normal");
      expect(actor.getSnapshot().context.operator).toBeNull();
    });
  });

  describe("mode transitions from visual", () => {
    test("OPERATOR returns to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL" });
      actor.send({ type: "OPERATOR", operator: "d" });
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("VISUAL (toggle off) returns to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL" });
      actor.send({ type: "VISUAL" });
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("VISUAL_BLOCK switches to visual-block", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL" });
      actor.send({ type: "VISUAL_BLOCK" });
      expect(actor.getSnapshot().value).toBe("visualBlock");
    });

    test("MOTION stays in visual but clears count", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL" });
      actor.send({ type: "DIGIT", digit: 3 });
      actor.send({
        type: "MOTION",
        motion: "j",
        position: { row: 3, col: 0 },
      });
      expect(actor.getSnapshot().value).toBe("visual");
      expect(actor.getSnapshot().context.count).toBe(0);
    });

    test("ESCAPE returns to normal and resets", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL" });
      actor.send({ type: "ESCAPE" });
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("EXECUTE_VISUAL_OP returns to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL" });
      actor.send({ type: "EXECUTE_VISUAL_OP", operator: "d" });
      expect(actor.getSnapshot().value).toBe("normal");
    });
  });

  describe("mode transitions from visual-block", () => {
    test("OPERATOR returns to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL_BLOCK" });
      actor.send({ type: "OPERATOR", operator: "y" });
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("VISUAL_BLOCK (toggle off) returns to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL_BLOCK" });
      actor.send({ type: "VISUAL_BLOCK" });
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("VISUAL switches to visual", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL_BLOCK" });
      actor.send({ type: "VISUAL" });
      expect(actor.getSnapshot().value).toBe("visual");
    });

    test("ESCAPE returns to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL_BLOCK" });
      actor.send({ type: "ESCAPE" });
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("MOTION stays in visual-block", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL_BLOCK" });
      actor.send({
        type: "MOTION",
        motion: "l",
        position: { row: 0, col: 1 },
      });
      expect(actor.getSnapshot().value).toBe("visualBlock");
    });
  });

  describe("count accumulation", () => {
    test("single digit accumulates", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 5 });
      expect(actor.getSnapshot().context.count).toBe(5);
    });

    test("multiple digits accumulate (123)", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 1 });
      actor.send({ type: "DIGIT", digit: 2 });
      actor.send({ type: "DIGIT", digit: 3 });
      expect(actor.getSnapshot().context.count).toBe(123);
    });

    test("count preserved across operator entry", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 5 });
      actor.send({ type: "OPERATOR", operator: "d" });
      expect(actor.getSnapshot().context.count).toBe(5);
    });

    test("count in operator-pending accumulates", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({ type: "DIGIT", digit: 3 });
      expect(actor.getSnapshot().context.count).toBe(3);
    });

    test("count resets after MOTION in normal", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 5 });
      actor.send({
        type: "MOTION",
        motion: "j",
        position: { row: 5, col: 0 },
      });
      expect(actor.getSnapshot().context.count).toBe(0);
    });

    test("count resets after ESCAPE", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 5 });
      actor.send({ type: "ESCAPE" });
      expect(actor.getSnapshot().context.count).toBe(0);
    });

    test("count resets after RESET", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 5 });
      actor.send({ type: "RESET" });
      expect(actor.getSnapshot().context.count).toBe(0);
    });

    test("count accumulates in visual mode", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL" });
      actor.send({ type: "DIGIT", digit: 4 });
      expect(actor.getSnapshot().context.count).toBe(4);
    });

    test("large count accumulates correctly", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 9 });
      actor.send({ type: "DIGIT", digit: 9 });
      actor.send({ type: "DIGIT", digit: 9 });
      expect(actor.getSnapshot().context.count).toBe(999);
    });
  });

  describe("actions", () => {
    test("setOperator stores the operator", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      expect(actor.getSnapshot().context.operator).toBe("d");
    });

    test("setOperator with y", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "y" });
      expect(actor.getSnapshot().context.operator).toBe("y");
    });

    test("setOperator with c", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "c" });
      expect(actor.getSnapshot().context.operator).toBe("c");
    });

    test("clearOperator nullifies operator after motion", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({
        type: "MOTION",
        motion: "w",
        position: { row: 0, col: 5 },
      });
      expect(actor.getSnapshot().context.operator).toBeNull();
    });

    test("reset clears all context", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 5 });
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({ type: "ESCAPE" });
      const ctx = actor.getSnapshot().context;
      expect(ctx.count).toBe(0);
      expect(ctx.operator).toBeNull();
      expect(ctx.visualStart).toBeNull();
    });

    test("recordAction stores operator+motion combo", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 3 });
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({
        type: "MOTION",
        motion: "w",
        position: { row: 0, col: 10 },
      });
      const lastAction = actor.getSnapshot().context.lastAction;
      expect(lastAction?.type).toBe("operator");
      expect(lastAction?.operator).toBe("d");
      expect(lastAction?.motion).toBe("w");
      expect(lastAction?.count).toBe(3);
    });

    test("recordAction with count=0 records count as 1", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "y" });
      actor.send({
        type: "MOTION",
        motion: "w",
        position: { row: 0, col: 5 },
      });
      const lastAction = actor.getSnapshot().context.lastAction;
      expect(lastAction?.count).toBe(1);
    });

    test("recordAction does not record on motion without operator", () => {
      const actor = createVimActor();
      actor.send({
        type: "MOTION",
        motion: "j",
        position: { row: 1, col: 0 },
      });
      expect(actor.getSnapshot().context.lastAction).toBeNull();
    });
  });

  describe("guards", () => {
    test("isSameOperator allows dd to return to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({ type: "OPERATOR", operator: "d" });
      // If guard works, we should be back in normal
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("isSameOperator blocks different operators from returning to normal", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({ type: "OPERATOR", operator: "y" });
      // Should still be in operator mode with new operator
      expect(actor.getSnapshot().value).toBe("operator");
    });

    test("operator switch clears count", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 5 });
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({ type: "OPERATOR", operator: "y" });
      expect(actor.getSnapshot().context.count).toBe(0);
    });
  });

  describe("complex sequences", () => {
    test("5dw sequence: count, operator, motion", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 5 });
      expect(actor.getSnapshot().context.count).toBe(5);

      actor.send({ type: "OPERATOR", operator: "d" });
      expect(actor.getSnapshot().value).toBe("operator");
      expect(actor.getSnapshot().context.operator).toBe("d");

      actor.send({
        type: "MOTION",
        motion: "w",
        position: { row: 0, col: 25 },
      });
      expect(actor.getSnapshot().value).toBe("normal");
      expect(actor.getSnapshot().context.lastAction?.count).toBe(5);
    });

    test("d3w sequence: operator, count, motion", () => {
      const actor = createVimActor();
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({ type: "DIGIT", digit: 3 });
      actor.send({
        type: "MOTION",
        motion: "w",
        position: { row: 0, col: 15 },
      });
      expect(actor.getSnapshot().value).toBe("normal");
      expect(actor.getSnapshot().context.lastAction?.count).toBe(3);
    });

    test("visual mode with operator", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL" });
      expect(actor.getSnapshot().value).toBe("visual");

      actor.send({
        type: "MOTION",
        motion: "l",
        position: { row: 0, col: 5 },
      });
      expect(actor.getSnapshot().value).toBe("visual");

      actor.send({ type: "OPERATOR", operator: "d" });
      expect(actor.getSnapshot().value).toBe("normal");
    });

    test("switching visual modes", () => {
      const actor = createVimActor();
      actor.send({ type: "VISUAL" });
      expect(actor.getSnapshot().value).toBe("visual");

      actor.send({ type: "VISUAL_BLOCK" });
      expect(actor.getSnapshot().value).toBe("visualBlock");

      actor.send({ type: "VISUAL" });
      expect(actor.getSnapshot().value).toBe("visual");
    });

    test("escape during complex sequence", () => {
      const actor = createVimActor();
      actor.send({ type: "DIGIT", digit: 5 });
      actor.send({ type: "OPERATOR", operator: "d" });
      actor.send({ type: "DIGIT", digit: 3 });
      actor.send({ type: "ESCAPE" });

      expect(actor.getSnapshot().value).toBe("normal");
      expect(actor.getSnapshot().context.count).toBe(0);
      expect(actor.getSnapshot().context.operator).toBeNull();
    });
  });
});
