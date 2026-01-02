//! Waveform visualizer using Kitty graphics protocol
//!
//! Renders audio samples as a waveform image displayed directly in the terminal.

use base64::Engine;
use image::{ImageBuffer, Rgba};
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
use std::io::{Cursor, Write};
use std::sync::{Arc, Mutex};

/// Number of samples in the waveform buffer
const WAVEFORM_SAMPLES: usize = 512;

/// Fixed image ID for the waveform (so we can replace it)
const WAVEFORM_IMAGE_ID: u32 = 1;

/// Waveform widget that renders audio samples using Kitty graphics
pub struct WaveformWidget {
    /// Reference to the audio waveform buffer
    samples: Vec<f32>,
    /// Width in pixels
    width: u32,
    /// Height in pixels
    height: u32,
    /// Waveform color (R, G, B)
    color: (u8, u8, u8),
    /// Background color (R, G, B, A)
    bg_color: (u8, u8, u8, u8),
}

impl WaveformWidget {
    /// Create a new waveform widget from a buffer reference
    pub fn new(waveform_buffer: &Arc<Mutex<Vec<f32>>>) -> Self {
        // Copy samples from the shared buffer
        let samples = if let Ok(buffer) = waveform_buffer.try_lock() {
            buffer.clone()
        } else {
            vec![0.0; WAVEFORM_SAMPLES]
        };

        Self {
            samples,
            width: 200,
            height: 40,
            color: (255, 200, 0),   // Yellow/gold
            bg_color: (0, 0, 0, 0), // Transparent background
        }
    }

    /// Set the image dimensions
    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Set the waveform color
    #[allow(dead_code)]
    pub fn with_color(mut self, r: u8, g: u8, b: u8) -> Self {
        self.color = (r, g, b);
        self
    }

    /// Generate the waveform image as PNG bytes
    fn generate_png(&self) -> Vec<u8> {
        let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(self.width, self.height);

        // Fill with background
        for pixel in img.pixels_mut() {
            *pixel = Rgba([
                self.bg_color.0,
                self.bg_color.1,
                self.bg_color.2,
                self.bg_color.3,
            ]);
        }

        let (r, g, b) = self.color;
        let center_y = self.height as f32 / 2.0;

        // Amplitude gain for more visual impact (audio samples are often quiet)
        let gain = 3.0;

        // Draw waveform - map samples to pixel columns
        let samples_per_pixel = self.samples.len() as f32 / self.width as f32;

        for x in 0..self.width {
            // Get the sample for this pixel (average if multiple samples per pixel)
            let sample_start = (x as f32 * samples_per_pixel) as usize;
            let sample_end = ((x + 1) as f32 * samples_per_pixel) as usize;
            let sample_end = sample_end.min(self.samples.len());

            if sample_start >= self.samples.len() {
                continue;
            }

            // Find min and max in this range for better visualization
            let mut min_val = 0.0f32;
            let mut max_val = 0.0f32;
            for i in sample_start..sample_end {
                let s = (self.samples[i] * gain).clamp(-1.0, 1.0);
                min_val = min_val.min(s);
                max_val = max_val.max(s);
            }

            // Convert to y coordinates
            let y_top = (center_y - max_val * center_y).max(0.0) as u32;
            let y_bottom = (center_y - min_val * center_y).min(self.height as f32 - 1.0) as u32;

            // Draw vertical line from y_top to y_bottom
            for y in y_top..=y_bottom {
                if y < self.height {
                    img.put_pixel(x, y, Rgba([r, g, b, 255]));
                }
            }
        }

        // Draw center line (dimmer)
        let center_y_int = center_y as u32;
        if center_y_int < self.height {
            for x in 0..self.width {
                let pixel = img.get_pixel_mut(x, center_y_int);
                // Only draw if not already drawn by waveform
                if pixel[3] == 0 {
                    *pixel = Rgba([40, 40, 40, 255]);
                }
            }
        }

        // Encode as PNG
        let mut png_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut png_bytes);
        img.write_to(&mut cursor, image::ImageFormat::Png)
            .expect("Failed to encode PNG");

        png_bytes
    }

    /// Generate Kitty graphics escape sequence
    fn kitty_escape_sequence(&self, x: u16, y: u16, cols: u16, rows: u16) -> String {
        let png_bytes = self.generate_png();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

        // Kitty graphics protocol:
        // a=T - transmit and display
        // f=100 - PNG format
        // t=d - direct transmission
        // i=<id> - image ID (for replacement)
        // p=<id> - placement ID
        // c=<cols>,r=<rows> - cell size to display in
        // q=2 - suppress response
        //
        // First delete any existing image with this ID, then display new one

        let chunk_size = 4096;
        let mut result = String::new();

        // Delete previous image with this ID
        result.push_str(&format!("\x1b_Ga=d,d=i,i={},q=2;\x1b\\", WAVEFORM_IMAGE_ID));

        // Move cursor to position
        result.push_str(&format!("\x1b[{};{}H", y + 1, x + 1));

        let chunks: Vec<&str> = b64
            .as_bytes()
            .chunks(chunk_size)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect();

        for (i, chunk) in chunks.iter().enumerate() {
            let is_last = i == chunks.len() - 1;
            let m = if is_last { 0 } else { 1 }; // m=1 means more chunks coming

            if i == 0 {
                // First chunk includes all parameters
                // c=cols, r=rows tells Kitty how many cells to use for display
                result.push_str(&format!(
                    "\x1b_Ga=T,f=100,t=d,i={},p=1,c={},r={},q=2,m={};{}\x1b\\",
                    WAVEFORM_IMAGE_ID, cols, rows, m, chunk
                ));
            } else {
                // Subsequent chunks only need m parameter
                result.push_str(&format!("\x1b_Gm={};{}\x1b\\", m, chunk));
            }
        }

        result
    }
}

impl Widget for WaveformWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Calculate pixel dimensions based on cell area
        // Typical terminal cells are ~7x14 pixels, but we'll use a ratio
        let pixel_width = (area.width as u32) * 8;
        let pixel_height = (area.height as u32) * 16;

        // Create widget with calculated size
        let widget = Self {
            samples: self.samples,
            width: pixel_width.min(600), // Cap at reasonable size
            height: pixel_height.min(100),
            color: self.color,
            bg_color: self.bg_color,
        };

        // Generate and write the escape sequence
        let escape_seq = widget.kitty_escape_sequence(area.x, area.y, area.width, area.height);

        // Write escape sequence to the first cell
        // The terminal will handle the actual image display
        if let Some(cell) = buf.cell_mut((area.x, area.y)) {
            // Store the escape sequence in the cell's symbol
            // This is a bit of a hack - we're putting the escape sequence in the cell
            // and relying on the terminal to interpret it
            cell.set_symbol(&escape_seq);
        }
    }
}

/// Render waveform directly to stdout (bypassing ratatui buffer)
/// This is more reliable for Kitty graphics
pub fn render_waveform_direct(
    waveform_buffer: &Arc<Mutex<Vec<f32>>>,
    x: u16,
    y: u16,
    cols: u16,
    rows: u16,
) {
    // Calculate pixel dimensions - use higher resolution for better quality
    let pixel_width = (cols as u32) * 20;
    let pixel_height = (rows as u32) * 40;

    let widget = WaveformWidget::new(waveform_buffer).with_size(pixel_width, pixel_height);
    let escape_seq = widget.kitty_escape_sequence(x, y, cols, rows);

    // Write directly to stdout
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(escape_seq.as_bytes());
    let _ = stdout.flush();
}
