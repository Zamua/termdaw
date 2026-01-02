import type { Position } from './types';

/**
 * Vim-style jumplist for navigation history.
 *
 * Tracks positions when making "jumps" (large movements) and allows
 * navigating back/forward through history with Ctrl+o and Ctrl+i.
 */
class Jumplist {
  private list: Position[] = [];
  private index = -1;  // Current position in the list
  private readonly maxSize = 100;

  /**
   * Push a position onto the jumplist.
   * Called before making a "jump" (gg, G, search, etc.)
   */
  push(position: Position): void {
    // Don't add duplicate of current position
    if (this.index >= 0 && this.list[this.index]) {
      const current = this.list[this.index]!;
      if (current.row === position.row && current.col === position.col) {
        return;
      }
    }

    // If we're not at the end of the list, truncate forward history
    if (this.index < this.list.length - 1) {
      this.list = this.list.slice(0, this.index + 1);
    }

    // Add new position
    this.list.push({ ...position });
    this.index = this.list.length - 1;

    // Trim to max size (remove oldest entries)
    if (this.list.length > this.maxSize) {
      const overflow = this.list.length - this.maxSize;
      this.list = this.list.slice(overflow);
      this.index -= overflow;
    }
  }

  /**
   * Navigate backward in jumplist (Ctrl+o in vim).
   * Returns the position to jump to, or null if at start.
   */
  back(): Position | null {
    if (this.index <= 0) {
      return null;
    }
    this.index--;
    const pos = this.list[this.index]!;
    return { row: pos.row, col: pos.col };
  }

  /**
   * Navigate forward in jumplist (Ctrl+i in vim).
   * Returns the position to jump to, or null if at end.
   */
  forward(): Position | null {
    if (this.index >= this.list.length - 1) {
      return null;
    }
    this.index++;
    const pos = this.list[this.index]!;
    return { row: pos.row, col: pos.col };
  }

  /**
   * Get current position in jumplist.
   */
  current(): Position | null {
    if (this.index < 0 || this.index >= this.list.length) {
      return null;
    }
    const pos = this.list[this.index]!;
    return { row: pos.row, col: pos.col };
  }

  /**
   * Check if we can go back.
   */
  canGoBack(): boolean {
    return this.index > 0;
  }

  /**
   * Check if we can go forward.
   */
  canGoForward(): boolean {
    return this.index < this.list.length - 1;
  }

  /**
   * Clear the jumplist.
   */
  clear(): void {
    this.list = [];
    this.index = -1;
  }

  /**
   * Get the full jumplist for debugging.
   */
  getList(): readonly Position[] {
    return this.list;
  }

  /**
   * Get current index for debugging.
   */
  getIndex(): number {
    return this.index;
  }
}

// Global jumplist instance shared across all vim-enabled components
export const jumplist = new Jumplist();

// Hook for React components
export function useJumplist() {
  return jumplist;
}
