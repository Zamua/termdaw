/**
 * Kitty Graphics Waveform using Unicode Placeholders
 *
 * Uses Kitty's Unicode placeholder approach (U+10EEEE) which works
 * with text-based frameworks like Ink.
 */

import { useState, useEffect, useRef } from 'react';
import { Text } from 'ink';
import { KittyCanvas, Terminal } from '../lib/kitty-canvas/index.js';
import { getWaveformData } from '../lib/audio.js';

// Unicode placeholder character for Kitty graphics
const PLACEHOLDER = '\u{10EEEE}';

// Diacritics for row/column encoding (class 230)
// Row diacritics: U+0305 (row 0), U+030D (row 1), U+030E (row 2), etc.
const ROW_DIACRITICS = [
  '\u0305', '\u030D', '\u030E', '\u0310', '\u0312', '\u033D', '\u033E', '\u033F',
  '\u0346', '\u034A', '\u034B', '\u034C', '\u0350', '\u0351', '\u0352', '\u0357',
  '\u035B', '\u0363', '\u0364', '\u0365', '\u0366', '\u0367', '\u0368', '\u0369',
];

// Column diacritics
const COL_DIACRITICS = [
  '\u0305', '\u030D', '\u030E', '\u0310', '\u0312', '\u033D', '\u033E', '\u033F',
  '\u0346', '\u034A', '\u034B', '\u034C', '\u0350', '\u0351', '\u0352', '\u0357',
  '\u035B', '\u0363', '\u0364', '\u0365', '\u0366', '\u0367', '\u0368', '\u0369',
];

interface KittyWaveformProps {
  width?: number;      // Pixel width
  height?: number;     // Pixel height
  columns?: number;    // Terminal columns to occupy
  rows?: number;       // Terminal rows to occupy
  imageId?: number;
}

export function KittyWaveform({
  width = 200,
  height = 40,
  columns = 24,
  rows = 2,
  imageId = 42,  // Fixed ID for simplicity
}: KittyWaveformProps) {
  const [waveform, setWaveform] = useState<Uint8Array>(new Uint8Array(0));
  const canvasRef = useRef<KittyCanvas | null>(null);
  const placedRef = useRef(false);

  // Create canvas when dimensions change
  useEffect(() => {
    canvasRef.current = new KittyCanvas({ width, height, id: imageId });

    // Pre-initialize audio context
    getWaveformData();

    return () => {
      // Delete image on unmount
      if (placedRef.current) {
        const deleteCmd = `${Terminal.APC}a=d,d=I,i=${imageId},q=2${Terminal.ST}`;
        process.stdout.write(deleteCmd);
      }
    };
  }, [width, height, imageId]);

  // Animation loop - point-in-time waveform
  useEffect(() => {
    const interval = setInterval(() => {
      const data = getWaveformData();
      setWaveform(data);
    }, 16); // ~60fps

    return () => clearInterval(interval);
  }, []);

  // Render waveform to canvas and transmit
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || waveform.length === 0) return;

    const midY = Math.floor(height / 2);

    // Clear
    canvas.clear(20, 25, 30, 255);

    // Center line
    canvas.hline(0, width - 1, midY, 40, 50, 55, 255);

    // Draw point-in-time waveform
    // Downsample waveform data to fit canvas width
    const step = waveform.length / width;

    // Check if data has actual audio signal (not just silence at 128 or zeros)
    const hasSignal = waveform.some(v => v !== 0 && Math.abs(v - 128) > 2);

    for (let x = 0; x < width; x++) {
      const idx = Math.floor(x * step);
      // If no signal, treat as silence (128); otherwise use actual sample
      const sample = hasSignal ? (waveform[idx] ?? 128) : 128;

      // Convert from 0-255 (128 = center) to y position
      const normalized = (sample - 128) / 128;
      const y = Math.floor(midY - normalized * (midY - 1));

      // Draw vertical line from center to sample
      const intensity = Math.min(255, 100 + Math.floor(Math.abs(normalized) * 155));
      if (y < midY) {
        canvas.vline(x, y, midY, 0, intensity, 80, 255);
      } else if (y > midY) {
        canvas.vline(x, midY, y, 0, intensity, 80, 255);
      }
    }

    // Transmit image data - overwrite in place (no delete, reduces flicker)
    const encoded = Buffer.from(canvas.data).toString('base64');

    // Chunked transmission - use 'T' with U=1 for virtual placement
    const chunkSize = 4096;
    for (let i = 0; i < encoded.length; i += chunkSize) {
      const chunk = encoded.slice(i, i + chunkSize);
      const isFirst = i === 0;
      const isLast = i + chunkSize >= encoded.length;
      const m = isLast ? 0 : 1;

      if (isFirst) {
        const params = `a=T,U=1,i=${imageId},c=${columns},r=${rows},q=2,f=32,s=${width},v=${height},m=${m}`;
        process.stdout.write(`${Terminal.APC}${params};${chunk}${Terminal.ST}`);
      } else {
        process.stdout.write(`${Terminal.APC}m=${m};${chunk}${Terminal.ST}`);
      }
    }

    placedRef.current = true;
  }, [waveform, width, height, columns, rows, imageId]);

  // Generate placeholder string for Ink
  // Each cell needs: PLACEHOLDER + row_diacritic + col_diacritic
  const placeholderRows: string[] = [];
  for (let r = 0; r < rows; r++) {
    let row = '';
    for (let c = 0; c < columns; c++) {
      const rowDiac = ROW_DIACRITICS[r] || ROW_DIACRITICS[0];
      const colDiac = COL_DIACRITICS[c] || COL_DIACRITICS[0];
      row += PLACEHOLDER + rowDiac + colDiac;
    }
    placeholderRows.push(row);
  }

  // Image ID encoded in foreground color (8-bit mode: color index = imageId)
  // For 256-color mode, we use escape sequence 38;5;N
  return (
    <Text color={`#${imageId.toString(16).padStart(6, '0')}`}>
      {placeholderRows.join('\n')}
    </Text>
  );
}

export default KittyWaveform;
