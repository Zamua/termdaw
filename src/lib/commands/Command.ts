/**
 * Base interface for all commands.
 * Commands encapsulate a single undoable action.
 */
export interface Command {
  readonly type: string;
  readonly description: string;

  /** Execute the command (do/redo) */
  execute(): void;

  /** Undo the command */
  undo(): void;
}

/**
 * Batch multiple commands into a single undoable unit.
 */
export class BatchCommand implements Command {
  readonly type = 'batch';
  readonly description: string;

  constructor(
    private commands: Command[],
    description: string
  ) {
    this.description = description;
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
