import type { Command, CommandContext, CommandPosition } from "./Command.js";
import { BatchCommand } from "./Command.js";

/**
 * Result of an undo/redo operation.
 */
export interface UndoRedoResult {
  success: boolean;
  context?: CommandContext;
  position?: CommandPosition;
}

/**
 * Manages undo/redo stacks for commands.
 *
 * Usage:
 * - execute(cmd) to run a command and add it to history
 * - undo() to undo the last command
 * - redo() to redo the last undone command
 * - batch(() => { ... }) to group multiple commands as one undo unit
 */
class CommandManager {
  private undoStack: Command[] = [];
  private redoStack: Command[] = [];
  private batchQueue: Command[] | null = null;
  private readonly maxSize = 100;

  // Listeners for state changes
  private listeners: Set<() => void> = new Set();

  /**
   * Execute a command and add it to the undo stack.
   */
  execute(command: Command): void {
    if (this.batchQueue) {
      // In batch mode, queue the command
      this.batchQueue.push(command);
      command.execute();
    } else {
      command.execute();
      this.undoStack.push(command);
      this.redoStack = []; // Clear redo on new action

      // Trim to max size
      if (this.undoStack.length > this.maxSize) {
        this.undoStack.shift();
      }

      this.notifyListeners();
    }
  }

  /**
   * Group multiple commands into a single undo unit.
   */
  batch(fn: () => void, description: string): void {
    this.batchQueue = [];
    fn();
    const commands = this.batchQueue;
    this.batchQueue = null;

    if (commands.length > 0) {
      const batch = new BatchCommand(commands, description);
      this.undoStack.push(batch);
      this.redoStack = [];

      // Trim to max size
      if (this.undoStack.length > this.maxSize) {
        this.undoStack.shift();
      }

      this.notifyListeners();
    }
  }

  /**
   * Undo the last command.
   * Returns result with success flag and cursor position to restore.
   */
  undo(): UndoRedoResult {
    const cmd = this.undoStack.pop();
    if (!cmd) return { success: false };

    cmd.undo();
    this.redoStack.push(cmd);
    this.notifyListeners();
    return {
      success: true,
      context: cmd.context,
      position: cmd.position,
    };
  }

  /**
   * Redo the last undone command.
   * Returns result with success flag and cursor position to restore.
   */
  redo(): UndoRedoResult {
    const cmd = this.redoStack.pop();
    if (!cmd) return { success: false };

    cmd.execute();
    this.undoStack.push(cmd);
    this.notifyListeners();
    return {
      success: true,
      context: cmd.context,
      position: cmd.position,
    };
  }

  /**
   * Check if undo is available.
   */
  canUndo(): boolean {
    return this.undoStack.length > 0;
  }

  /**
   * Check if redo is available.
   */
  canRedo(): boolean {
    return this.redoStack.length > 0;
  }

  /**
   * Get the description of the command that would be undone.
   */
  getUndoDescription(): string | null {
    const cmd = this.undoStack[this.undoStack.length - 1];
    return cmd?.description ?? null;
  }

  /**
   * Get the description of the command that would be redone.
   */
  getRedoDescription(): string | null {
    const cmd = this.redoStack[this.redoStack.length - 1];
    return cmd?.description ?? null;
  }

  /**
   * Clear all history.
   */
  clear(): void {
    this.undoStack = [];
    this.redoStack = [];
    this.notifyListeners();
  }

  /**
   * Subscribe to state changes.
   */
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notifyListeners(): void {
    for (const listener of this.listeners) {
      listener();
    }
  }
}

// Global singleton instance
export const commandManager = new CommandManager();

export function useCommandManager() {
  return commandManager;
}
