/**
 * kitty-canvas - Terminal Canvas with Kitty Graphics Protocol
 *
 * A canvas-like drawing API that renders to terminals supporting the Kitty graphics protocol.
 * Works with Ghostty, Kitty, WezTerm, and other compatible terminals.
 *
 * @example
 * ```typescript
 * import { KittyCanvas } from 'kitty-canvas';
 *
 * const canvas = new KittyCanvas({ width: 200, height: 100 });
 * canvas.fillRect(10, 10, 50, 50, 255, 0, 0);  // Red rectangle
 * canvas.line(0, 0, 200, 100, 0, 255, 0);      // Green diagonal
 * canvas.render();
 * ```
 *
 * @see https://sw.kovidgoyal.net/kitty/graphics-protocol/
 */

export { KittyCanvas, type KittyCanvasOptions } from "./canvas.js";
export {
  hexToRgb,
  hslToRgb,
  type Color,
  type RGB,
  type RGBA,
} from "./color.js";
export { Terminal } from "./terminal.js";
