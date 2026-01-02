// Vim mode types
export type VimMode = 'normal' | 'visual' | 'visual-block' | 'operator-pending';

// Operators that wait for a motion
export type Operator = 'd' | 'y' | 'c' | null;

// Generic 2D position
export interface Position {
  row: number;
  col: number;
}

// Range of positions for visual selection or operator execution
export interface Range {
  start: Position;
  end: Position;
  type: 'char' | 'line' | 'block';
}

// Named registers for yank/paste
export type RegisterName = '"' | '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9';

// What's stored in a register
export interface RegisterContent<T = unknown> {
  data: T;
  type: 'char' | 'line' | 'block';
}

// Motion result - where we move and optional range info
export interface MotionResult {
  position: Position;
  // If the motion is linewise (like j/k), affects how operators work
  linewise?: boolean;
  // If the motion is inclusive (like e), affects range end
  inclusive?: boolean;
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
  zero?: MotionFn;    // 0
  dollar?: MotionFn;  // $
}

// Action that can be repeated with .
export interface RecordedAction {
  type: 'operator' | 'simple';
  operator?: Operator;
  motion?: string;
  count?: number;
  data?: unknown;  // For things like what was yanked for paste
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

  // Motion implementations (component-specific navigation)
  motions: Motions;

  // Data operations for operators
  getDataInRange: (range: Range) => T;
  deleteRange: (range: Range) => T;  // Returns deleted data
  insertData: (pos: Position, data: T) => void;

  // Optional: handle custom keys not covered by vim
  onCustomAction?: (char: string, key: Key, count: number) => boolean;

  // Optional: callback when mode changes
  onModeChange?: (mode: VimMode) => void;

  // Optional: callback when visual range changes
  onVisualRangeChange?: (range: Range | null) => void;
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
  yank: (data: T, type: 'char' | 'line' | 'block') => void;
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
