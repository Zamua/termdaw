import type { RegisterContent, RegisterName } from './types';

/**
 * Vim-style register manager.
 *
 * Registers:
 * - " (unnamed): Default register for yank/delete
 * - 0: Last yank (not affected by delete)
 * - 1-9: Delete history (1 is most recent)
 */
class RegisterManager {
  private registers = new Map<RegisterName, RegisterContent>();
  private currentRegister: RegisterName = '"';

  /**
   * Set the register to use for the next yank/paste operation.
   */
  setRegister(name: RegisterName): void {
    this.currentRegister = name;
  }

  /**
   * Get the currently selected register name.
   */
  getSelectedRegister(): RegisterName {
    return this.currentRegister;
  }

  /**
   * Store data in a register after yank.
   * Also copies to register 0 (last yank).
   */
  yank<T>(data: T, type: 'char' | 'line' | 'block'): void {
    const content: RegisterContent<T> = { data, type };

    // Store in unnamed register
    this.registers.set('"', content);

    // Store in yank register (not affected by delete)
    this.registers.set('0', content);

    // If a specific register was selected, store there too
    if (this.currentRegister !== '"') {
      this.registers.set(this.currentRegister, content);
    }

    // Reset to default register
    this.currentRegister = '"';
  }

  /**
   * Store data in registers after delete.
   * Shifts delete history (1-9) and stores in unnamed register.
   */
  delete<T>(data: T, type: 'char' | 'line' | 'block'): void {
    const content: RegisterContent<T> = { data, type };

    // Shift delete history: 8->9, 7->8, ... 1->2
    for (let i = 8; i >= 1; i--) {
      const from = i.toString() as RegisterName;
      const to = (i + 1).toString() as RegisterName;
      const existing = this.registers.get(from);
      if (existing) {
        this.registers.set(to, existing);
      }
    }

    // Store new delete in register 1
    this.registers.set('1', content);

    // Also store in unnamed register
    this.registers.set('"', content);

    // If a specific register was selected, store there too
    if (this.currentRegister !== '"') {
      this.registers.set(this.currentRegister, content);
    }

    // Reset to default register
    this.currentRegister = '"';
  }

  /**
   * Get content from the current register.
   */
  get<T>(): RegisterContent<T> | null {
    const content = this.registers.get(this.currentRegister);
    this.currentRegister = '"';  // Reset after use
    return content as RegisterContent<T> | null;
  }

  /**
   * Peek at a specific register without changing current selection.
   */
  peek<T>(name: RegisterName): RegisterContent<T> | null {
    return this.registers.get(name) as RegisterContent<T> | null;
  }

  /**
   * Clear all registers.
   */
  clear(): void {
    this.registers.clear();
    this.currentRegister = '"';
  }
}

// Global register instance shared across all vim-enabled components
export const registers = new RegisterManager();

// Hook for React components
export function useRegisters() {
  return registers;
}
