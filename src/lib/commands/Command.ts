/**
 * Position where a command was executed.
 */
export interface CommandPosition {
  row: number;
  col: number;
}

/**
 * Context identifier for where a command applies.
 */
export type CommandContext = "channelRack" | "pianoRoll" | "playlist";

/**
 * Base interface for all commands.
 * Commands encapsulate a single undoable action.
 */
export interface Command {
  readonly type: string;
  readonly description: string;

  /** Which component this command applies to (optional for legacy) */
  readonly context?: CommandContext;

  /** Cursor position when command was created (optional for legacy) */
  readonly position?: CommandPosition;

  /** Execute the command (do/redo) */
  execute(): void;

  /** Undo the command */
  undo(): void;
}

/**
 * Batch multiple commands into a single undoable unit.
 */
export class BatchCommand implements Command {
  readonly type = "batch";
  readonly description: string;
  readonly context?: CommandContext;
  readonly position?: CommandPosition;

  constructor(
    private commands: Command[],
    description: string,
  ) {
    this.description = description;
    // Use first command's context and position
    const first = commands[0];
    this.context = first?.context;
    this.position = first?.position;
  }

  execute(): void {
    for (const cmd of this.commands) {
      cmd.execute();
    }
  }

  undo(): void {
    // Undo in reverse order
    for (let i = this.commands.length - 1; i >= 0; i--) {
      this.commands[i]!.undo();
    }
  }
}
