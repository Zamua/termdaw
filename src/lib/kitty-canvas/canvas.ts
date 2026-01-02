/**
 * KittyCanvas - Canvas-like API for Kitty graphics protocol
 */

import { Terminal } from "./terminal.js";

export interface KittyCanvasOptions {
  width: number;
  height: number;
  x?: number; // Position in terminal cells
  y?: number;
  id?: number; // Image ID for updates
}

/**
 * RGBA pixel buffer with drawing primitives and Kitty protocol output
 */
export class KittyCanvas {
  readonly width: number;
  readonly height: number;
  readonly data: Uint8Array; // RGBA buffer

  private id: number;
  private x: number;
  private y: number;
  private placed: boolean = false;

  constructor(options: KittyCanvasOptions) {
    this.width = options.width;
    this.height = options.height;
    this.x = options.x ?? 0;
    this.y = options.y ?? 0;
    this.id = options.id ?? Math.floor(Math.random() * 1000000);

    // RGBA = 4 bytes per pixel
    this.data = new Uint8Array(this.width * this.height * 4);
    this.clear();
  }

  /**
   * Clear canvas to a color (default: transparent black)
   */
  clear(r = 0, g = 0, b = 0, a = 255): void {
    for (let i = 0; i < this.data.length; i += 4) {
      this.data[i] = r;
      this.data[i + 1] = g;
      this.data[i + 2] = b;
      this.data[i + 3] = a;
    }
  }

  /**
   * Set a single pixel
   */
  setPixel(
    x: number,
    y: number,
    r: number,
    g: number,
    b: number,
    a = 255,
  ): void {
    if (x < 0 || x >= this.width || y < 0 || y >= this.height) return;
    const i = (y * this.width + x) * 4;
    this.data[i] = r;
    this.data[i + 1] = g;
    this.data[i + 2] = b;
    this.data[i + 3] = a;
  }

  /**
   * Get pixel color at position
   */
  getPixel(x: number, y: number): [number, number, number, number] {
    if (x < 0 || x >= this.width || y < 0 || y >= this.height) {
      return [0, 0, 0, 0];
    }
    const i = (y * this.width + x) * 4;
    return [
      this.data[i] ?? 0,
      this.data[i + 1] ?? 0,
      this.data[i + 2] ?? 0,
      this.data[i + 3] ?? 0,
    ];
  }

  /**
   * Fill a rectangle
   */
  fillRect(
    x: number,
    y: number,
    w: number,
    h: number,
    r: number,
    g: number,
    b: number,
    a = 255,
  ): void {
    const x1 = Math.max(0, x);
    const y1 = Math.max(0, y);
    const x2 = Math.min(this.width, x + w);
    const y2 = Math.min(this.height, y + h);

    for (let py = y1; py < y2; py++) {
      for (let px = x1; px < x2; px++) {
        this.setPixel(px, py, r, g, b, a);
      }
    }
  }

  /**
   * Stroke a rectangle (outline only)
   */
  strokeRect(
    x: number,
    y: number,
    w: number,
    h: number,
    r: number,
    g: number,
    b: number,
    a = 255,
  ): void {
    this.hline(x, x + w - 1, y, r, g, b, a); // Top
    this.hline(x, x + w - 1, y + h - 1, r, g, b, a); // Bottom
    this.vline(x, y, y + h - 1, r, g, b, a); // Left
    this.vline(x + w - 1, y, y + h - 1, r, g, b, a); // Right
  }

  /**
   * Draw a horizontal line
   */
  hline(
    x1: number,
    x2: number,
    y: number,
    r: number,
    g: number,
    b: number,
    a = 255,
  ): void {
    const start = Math.max(0, Math.min(x1, x2));
    const end = Math.min(this.width - 1, Math.max(x1, x2));
    for (let x = start; x <= end; x++) {
      this.setPixel(x, y, r, g, b, a);
    }
  }

  /**
   * Draw a vertical line
   */
  vline(
    x: number,
    y1: number,
    y2: number,
    r: number,
    g: number,
    b: number,
    a = 255,
  ): void {
    const start = Math.max(0, Math.min(y1, y2));
    const end = Math.min(this.height - 1, Math.max(y1, y2));
    for (let y = start; y <= end; y++) {
      this.setPixel(x, y, r, g, b, a);
    }
  }

  /**
   * Draw a line using Bresenham's algorithm
   */
  line(
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    r: number,
    g: number,
    b: number,
    a = 255,
  ): void {
    const dx = Math.abs(x2 - x1);
    const dy = Math.abs(y2 - y1);
    const sx = x1 < x2 ? 1 : -1;
    const sy = y1 < y2 ? 1 : -1;
    let err = dx - dy;

    let x = x1;
    let y = y1;

    while (true) {
      this.setPixel(x, y, r, g, b, a);
      if (x === x2 && y === y2) break;
      const e2 = 2 * err;
      if (e2 > -dy) {
        err -= dy;
        x += sx;
      }
      if (e2 < dx) {
        err += dx;
        y += sy;
      }
    }
  }

  /**
   * Draw a circle using midpoint algorithm
   */
  circle(
    cx: number,
    cy: number,
    radius: number,
    r: number,
    g: number,
    b: number,
    a = 255,
  ): void {
    let x = radius;
    let y = 0;
    let err = 0;

    while (x >= y) {
      this.setPixel(cx + x, cy + y, r, g, b, a);
      this.setPixel(cx + y, cy + x, r, g, b, a);
      this.setPixel(cx - y, cy + x, r, g, b, a);
      this.setPixel(cx - x, cy + y, r, g, b, a);
      this.setPixel(cx - x, cy - y, r, g, b, a);
      this.setPixel(cx - y, cy - x, r, g, b, a);
      this.setPixel(cx + y, cy - x, r, g, b, a);
      this.setPixel(cx + x, cy - y, r, g, b, a);

      y += 1;
      err += 1 + 2 * y;
      if (2 * (err - x) + 1 > 0) {
        x -= 1;
        err += 1 - 2 * x;
      }
    }
  }

  /**
   * Fill a circle
   */
  fillCircle(
    cx: number,
    cy: number,
    radius: number,
    r: number,
    g: number,
    b: number,
    a = 255,
  ): void {
    for (let y = -radius; y <= radius; y++) {
      for (let x = -radius; x <= radius; x++) {
        if (x * x + y * y <= radius * radius) {
          this.setPixel(cx + x, cy + y, r, g, b, a);
        }
      }
    }
  }

  /**
   * Encode the canvas data for Kitty protocol
   */
  private encodeData(): string {
    return Buffer.from(this.data).toString("base64");
  }

  /**
   * Build the Kitty graphics command
   */
  private buildCommand(
    action: "t" | "T" | "p" | "d",
    payload?: string,
  ): string {
    const params: string[] = [];

    params.push(`a=${action}`);
    params.push(`i=${this.id}`);
    params.push("q=2"); // Quiet mode - suppress terminal responses

    if (action === "t" || action === "T") {
      params.push("f=32"); // RGBA format
      params.push(`s=${this.width}`);
      params.push(`v=${this.height}`);
    }

    if (action === "T" || action === "p") {
      params.push(`X=${this.x}`);
      params.push(`Y=${this.y}`);
    }

    const paramStr = params.join(",");

    if (payload) {
      const chunks: string[] = [];
      const chunkSize = 4096;

      for (let i = 0; i < payload.length; i += chunkSize) {
        const chunk = payload.slice(i, i + chunkSize);
        const isLast = i + chunkSize >= payload.length;
        const m = isLast ? 0 : 1;

        if (i === 0) {
          chunks.push(
            `${Terminal.APC}${paramStr},m=${m};${chunk}${Terminal.ST}`,
          );
        } else {
          chunks.push(`${Terminal.APC}m=${m};${chunk}${Terminal.ST}`);
        }
      }

      return chunks.join("");
    }

    return `${Terminal.APC}${paramStr}${Terminal.ST}`;
  }

  /**
   * Render the canvas to the terminal
   */
  render(): void {
    Terminal.write(this.toString());
  }

  /**
   * Get the Kitty graphics escape sequence as a string
   * Useful for integration with frameworks like Ink
   */
  toString(): string {
    const encoded = this.encodeData();

    if (!this.placed) {
      this.placed = true;
      return this.buildCommand("T", encoded);
    } else {
      // Delete and re-transmit for updates
      return this.buildCommand("d") + this.buildCommand("T", encoded);
    }
  }

  /**
   * Delete the image from terminal
   */
  destroy(): void {
    if (this.placed) {
      Terminal.write(this.buildCommand("d"));
      this.placed = false;
    }
  }

  /**
   * Get the delete command as a string
   */
  destroyString(): string {
    if (this.placed) {
      this.placed = false;
      return this.buildCommand("d");
    }
    return "";
  }

  /**
   * Get raw pixel data
   */
  getImageData(): { data: Uint8Array; width: number; height: number } {
    return {
      data: this.data,
      width: this.width,
      height: this.height,
    };
  }
}
