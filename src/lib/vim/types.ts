// Vim mode types
export type VimMode = "normal" | "visual" | "visual-block" | "operator-pending";

// Operators that wait for a motion
export type Operator = "d" | "y" | "c" | null;

// Generic 2D position
export interface Position {
  row: number;
  col: number;
}

// Range of positions for visual selection or operator execution
export interface Range {
  start: Position;
  end: Position;
  type: "char" | "line" | "block";
}

// Named registers for yank/paste
export type RegisterName =
  | '"'
  | "0"
  | "1"
  | "2"
  | "3"
  | "4"
  | "5"
  | "6"
  | "7"
  | "8"
  | "9";

// What's stored in a register
export interface RegisterContent<T = unknown> {
  data: T;
  type: "char" | "line" | "block";
}

// Motion result - where we move and optional range info
export interface MotionResult {
  position: Position;
  // If the motion is linewise (like j/k), affects how operators work
  linewise?: boolean;
  // If the motion is inclusive (like e), affects range end
  inclusive?: boolean;
}

// Word boundary definition for w/b motions
// Components define "what is a word" and the library handles vim semantics
// @deprecated Use GridSemantics zones with hasContent instead
export interface WordBoundary {
  // Find next "word" position from current position
  // Return null if at end (no more words) - cursor will stay in place
  findNext: (pos: Position) => Position | null;

  // Find previous "word" position from current position
  // Return null if at beginning - cursor will stay in place
  findPrev: (pos: Position) => Position | null;
}

// Zone within a grid row - defines a region with specific navigation semantics
export interface Zone {
  name: string;
  colRange: [number, number]; // [start, end] inclusive
  isMain?: boolean; // Where 0/$ go, default zone for w/b
  hasContent?: (pos: Position) => boolean; // For word navigation
  wordInterval?: number; // Additional word boundaries (e.g., bar lines every 4 steps)
}

// Semantic description of the grid
export interface GridSemantics {
  zones?: Zone[]; // If not provided, entire grid is one main zone
}

// Motion function signature
export type MotionFn = (count: number, cursor: Position) => MotionResult;

// Component provides these motion implementations
export interface Motions {
  h: MotionFn;
  j: MotionFn;
  k: MotionFn;
  l: MotionFn;
  w?: MotionFn;
  b?: MotionFn;
  e?: MotionFn;
  gg?: MotionFn;
  G?: MotionFn;
  zero?: MotionFn; // 0
  dollar?: MotionFn; // $
}

// Action that can be repeated with .
export interface RecordedAction {
  type: "operator" | "simple";
  operator?: Operator;
  motion?: string;
  count?: number;
  data?: unknown; // For things like what was yanked for paste
}

// Key event from Ink
export interface Key {
  upArrow: boolean;
  downArrow: boolean;
  leftArrow: boolean;
  rightArrow: boolean;
  pageDown: boolean;
  pageUp: boolean;
  return: boolean;
  escape: boolean;
  ctrl: boolean;
  shift: boolean;
  tab: boolean;
  backspace: boolean;
  delete: boolean;
  meta: boolean;
}

// Dimensions of the grid being navigated
export interface GridDimensions {
  rows: number;
  cols: number;
}

// Config provided by the component using vim
export interface VimConfig<T = unknown> {
  // Grid dimensions for boundary checking
  dimensions: GridDimensions;

  // Current cursor position
  getCursor: () => Position;
  setCursor: (pos: Position) => void;

  // Semantic description of the grid (zones, word boundaries, etc.)
  // If provided, library uses this to implement default motions
  gridSemantics?: GridSemantics;

  // Custom motion overrides for truly special behavior (e.g., Browser folder expand/collapse)
  // Only needed when default zone-based navigation doesn't fit
  customMotions?: Partial<Motions>;

  // @deprecated Use gridSemantics instead - will be removed in future version
  // Motion implementations (component-specific navigation)
  motions?: Motions;

  // @deprecated Use gridSemantics zones with hasContent instead
  // Optional: word boundary definitions for w/b motions
  wordBoundary?: WordBoundary;

  // Data operations for operators
  getDataInRange: (range: Range) => T;
  deleteRange: (range: Range) => T; // Returns deleted data
  insertData: (pos: Position, data: T) => void;

  // Optional: handle custom keys not covered by vim
  onCustomAction?: (char: string, key: Key, count: number) => boolean;

  // Optional: callback when mode changes
  onModeChange?: (mode: VimMode) => void;

  // Optional: callback when visual range changes
  onVisualRangeChange?: (range: Range | null) => void;

  // Optional: callback when escape is pressed (after vim resets state)
  // Receives the mode that was active before escape (useful for deciding behavior)
  // e.g., only exit view if already in normal mode, not when escaping from visual
  onEscape?: (prevMode: VimMode) => void;
}

// State returned by useVim hook
export interface VimState<T = unknown> {
  mode: VimMode;
  count: number;
  operator: Operator;
  visualRange: Range | null;

  // Input handling - returns true if handled
  handleInput: (char: string, key: Key) => boolean;

  // Register operations
  yank: (data: T, type: "char" | "line" | "block") => void;
  paste: () => RegisterContent<T> | null;

  // Jumplist navigation
  pushJump: () => void;
  jumpBack: () => Position | null;
  jumpForward: () => Position | null;

  // Repeat functionality
  getLastAction: () => RecordedAction | null;
  setLastAction: (action: RecordedAction) => void;

  // Force reset to normal mode
  reset: () => void;
}
