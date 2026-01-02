import { setup, assign } from "xstate";
import type { Operator, Position, RecordedAction } from "./types";

// Events the vim machine can receive
type VimEvent =
  | { type: "DIGIT"; digit: number }
  | { type: "OPERATOR"; operator: Operator }
  | {
      type: "MOTION";
      motion: string;
      position: Position;
      linewise?: boolean;
      inclusive?: boolean;
    }
  | { type: "VISUAL" }
  | { type: "VISUAL_BLOCK" }
  | { type: "EXECUTE_VISUAL_OP"; operator: Operator }
  | { type: "ESCAPE" }
  | { type: "RESET" };

// Context stored in the state machine
interface VimContext {
  count: number;
  operator: Operator;
  visualStart: Position | null;
  lastAction: RecordedAction | null;
}

// Create the vim state machine
export const vimMachine = setup({
  types: {
    context: {} as VimContext,
    events: {} as VimEvent,
  },
  actions: {
    accumulateCount: assign({
      count: ({ context, event }) => {
        if (event.type !== "DIGIT") return context.count;
        return context.count * 10 + event.digit;
      },
    }),
    setOperator: assign({
      operator: ({ event }) => {
        if (event.type !== "OPERATOR") return null;
        return event.operator;
      },
    }),
    startVisual: assign({
      visualStart: () => {
        // Visual start position will be set by the hook based on current cursor
        // This is a placeholder that gets overwritten
        return null;
      },
    }),
    clearOperator: assign({
      operator: () => null,
    }),
    clearCount: assign({
      count: () => 0,
    }),
    clearVisual: assign({
      visualStart: () => null,
    }),
    reset: assign({
      count: () => 0,
      operator: () => null,
      visualStart: () => null,
    }),
    recordAction: assign({
      lastAction: ({ context, event }) => {
        if (event.type !== "MOTION") return context.lastAction;
        if (!context.operator) return context.lastAction;
        return {
          type: "operator" as const,
          operator: context.operator,
          motion: event.motion,
          count: context.count || 1,
        };
      },
    }),
  },
  guards: {
    isSameOperator: ({ context, event }) => {
      if (event.type !== "OPERATOR") return false;
      return context.operator === event.operator;
    },
    hasOperator: ({ context }) => context.operator !== null,
  },
}).createMachine({
  id: "vim",
  initial: "normal",
  context: {
    count: 0,
    operator: null,
    visualStart: null,
    lastAction: null,
  },
  states: {
    normal: {
      on: {
        DIGIT: {
          actions: "accumulateCount",
        },
        OPERATOR: {
          target: "operator",
          actions: ["setOperator"],
        },
        VISUAL: {
          target: "visual",
          actions: ["startVisual"],
        },
        VISUAL_BLOCK: {
          target: "visualBlock",
          actions: ["startVisual"],
        },
        MOTION: {
          actions: ["clearCount"],
        },
        ESCAPE: {
          actions: "reset",
        },
        RESET: {
          actions: "reset",
        },
      },
    },
    operator: {
      on: {
        DIGIT: {
          actions: "accumulateCount",
        },
        MOTION: {
          target: "normal",
          actions: ["recordAction", "reset"],
        },
        OPERATOR: [
          {
            // dd, yy, cc - operate on current line
            guard: "isSameOperator",
            target: "normal",
            actions: ["recordAction", "reset"],
          },
          {
            // Different operator - switch to new one
            actions: ["setOperator", "clearCount"],
          },
        ],
        ESCAPE: {
          target: "normal",
          actions: "reset",
        },
        RESET: {
          target: "normal",
          actions: "reset",
        },
      },
    },
    visual: {
      on: {
        DIGIT: {
          actions: "accumulateCount",
        },
        MOTION: {
          actions: ["clearCount"],
        },
        OPERATOR: {
          // d, y, c in visual mode executes immediately
          target: "normal",
          actions: ["reset"],
        },
        EXECUTE_VISUAL_OP: {
          target: "normal",
          actions: ["reset"],
        },
        VISUAL_BLOCK: {
          target: "visualBlock",
        },
        VISUAL: {
          // v in visual mode exits visual
          target: "normal",
          actions: ["clearVisual", "clearCount"],
        },
        ESCAPE: {
          target: "normal",
          actions: "reset",
        },
        RESET: {
          target: "normal",
          actions: "reset",
        },
      },
    },
    visualBlock: {
      on: {
        DIGIT: {
          actions: "accumulateCount",
        },
        MOTION: {
          actions: ["clearCount"],
        },
        OPERATOR: {
          target: "normal",
          actions: ["reset"],
        },
        EXECUTE_VISUAL_OP: {
          target: "normal",
          actions: ["reset"],
        },
        VISUAL: {
          target: "visual",
        },
        VISUAL_BLOCK: {
          // Ctrl+v in visual-block exits
          target: "normal",
          actions: ["clearVisual", "clearCount"],
        },
        ESCAPE: {
          target: "normal",
          actions: "reset",
        },
        RESET: {
          target: "normal",
          actions: "reset",
        },
      },
    },
  },
});

export type VimMachineState = typeof vimMachine;
