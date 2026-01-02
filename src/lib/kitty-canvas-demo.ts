#!/usr/bin/env bun
/**
 * Demo script for KittyCanvas
 * Run with: bun src/lib/kitty-canvas-demo.ts
 */

import { KittyCanvas, Terminal } from "./kitty-canvas/index.js";

async function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function demo() {
  console.log("Kitty Graphics Protocol Demo\n");
  console.log(`Terminal: ${process.env.TERM_PROGRAM || process.env.TERM}`);
  console.log(
    `Supported: ${Terminal.isKittySupported() ? "Yes" : "Maybe not"}\n`,
  );

  // Simple rectangle demo
  console.log("1. Drawing a gradient with shapes...\n");
  const canvas = new KittyCanvas({ width: 400, height: 200 });

  // Draw a gradient-ish pattern
  for (let y = 0; y < 200; y++) {
    for (let x = 0; x < 400; x++) {
      const r = Math.floor((x / 400) * 255);
      const g = Math.floor((y / 200) * 255);
      const b = 128;
      canvas.setPixel(x, y, r, g, b, 255);
    }
  }

  // Draw some shapes
  canvas.line(0, 0, 399, 199, 255, 255, 255, 255);
  canvas.line(0, 199, 399, 0, 255, 255, 255, 255);
  canvas.fillRect(160, 80, 80, 40, 255, 0, 0, 255);
  canvas.strokeRect(40, 40, 120, 80, 255, 255, 0, 255);
  canvas.fillCircle(320, 140, 40, 0, 255, 255, 255);
  canvas.circle(320, 140, 48, 255, 255, 255, 255);

  canvas.render();
  console.log("\n\n\n\n"); // Space for the image

  await sleep(2000);

  // Animation demo
  console.log("\n2. Animated bouncing ball...\n");
  const animCanvas = new KittyCanvas({ width: 600, height: 200 });

  Terminal.hideCursor();

  // Animate for 5 seconds
  const startTime = Date.now();
  let frame = 0;

  while (Date.now() - startTime < 5000) {
    // Clear
    animCanvas.clear(20, 20, 30, 255);

    // Bouncing ball
    const t = frame * 0.1;
    const ballX = Math.floor(300 + 250 * Math.sin(t));
    const ballY = Math.floor(100 + 80 * Math.sin(t * 1.3));

    // Draw trail
    for (let i = 1; i <= 8; i++) {
      const trailT = (frame - i * 2) * 0.1;
      const trailX = Math.floor(300 + 250 * Math.sin(trailT));
      const trailY = Math.floor(100 + 80 * Math.sin(trailT * 1.3));
      const alpha = Math.floor(255 * (1 - i / 9));
      animCanvas.fillCircle(trailX, trailY, 16, 0, 100, 200, alpha);
    }

    // Draw ball
    animCanvas.fillCircle(ballX, ballY, 24, 0, 200, 255, 255);
    animCanvas.circle(ballX, ballY, 24, 255, 255, 255, 255);

    // Move cursor and render
    Terminal.moveCursor(0, 10);
    animCanvas.render();

    frame++;
    await sleep(50);
  }

  Terminal.showCursor();
  animCanvas.destroy();
  canvas.destroy();

  console.log("\n\nDemo complete!");
}

demo().catch(console.error);
