/**
 * Terminal utilities and escape sequences
 */

// Escape sequences
const ESC = '\x1b';
const CSI = `${ESC}[`;
const APC = `${ESC}_G`;  // Application Program Command for graphics
const ST = `${ESC}\\`;   // String Terminator

export const Terminal = {
  ESC,
  CSI,
  APC,
  ST,

  /**
   * Move cursor to position (1-indexed)
   */
  moveCursor(x: number, y: number): void {
    process.stdout.write(`${CSI}${y + 1};${x + 1}H`);
  },

  /**
   * Hide cursor
   */
  hideCursor(): void {
    process.stdout.write(`${CSI}?25l`);
  },

  /**
   * Show cursor
   */
  showCursor(): void {
    process.stdout.write(`${CSI}?25h`);
  },

  /**
   * Clear screen
   */
  clearScreen(): void {
    process.stdout.write(`${CSI}2J${CSI}H`);
  },

  /**
   * Check if terminal likely supports Kitty graphics protocol
   */
  isKittySupported(): boolean {
    const term = process.env.TERM?.toLowerCase() || '';
    const termProgram = process.env.TERM_PROGRAM?.toLowerCase() || '';

    return (
      term.includes('kitty') ||
      term.includes('ghostty') ||
      termProgram.includes('kitty') ||
      termProgram.includes('ghostty') ||
      termProgram.includes('wezterm')
    );
  },

  /**
   * Write raw data to stdout
   */
  write(data: string): void {
    process.stdout.write(data);
  },
};
