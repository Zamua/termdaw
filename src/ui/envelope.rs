//! ADSR Envelope visualization module
//!
//! Draws envelope using Braille characters for smooth, high-resolution rendering.
//! Braille gives us 2x4 sub-cell resolution (8 dots per character cell).
//! Supports vertices for future mouse drag interaction.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::collections::HashMap;

/// ADSR envelope parameters (all normalized 0.0-1.0)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvelopeParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for EnvelopeParams {
    fn default() -> Self {
        Self {
            attack: 0.1,
            decay: 0.2,
            sustain: 0.7,
            release: 0.3,
        }
    }
}

/// A vertex point on the envelope (for mouse interaction)
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Vertex {
    /// X position in cell coordinates
    pub x: u16,
    /// Y position in cell coordinates
    pub y: u16,
    /// Which part of the envelope this vertex controls
    pub kind: VertexKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VertexKind {
    Start,
    AttackPeak,
    DecayEnd,
    SustainEnd,
    ReleaseEnd,
}

/// Braille dot positions within a cell (2 wide x 4 tall)
/// Bit values for each dot position:
/// 1  8
/// 2  16
/// 4  32
/// 64 128
const BRAILLE_BASE: u32 = 0x2800;

fn braille_dot(x: u8, y: u8) -> u8 {
    match (x, y) {
        (0, 0) => 1,
        (0, 1) => 2,
        (0, 2) => 4,
        (0, 3) => 64,
        (1, 0) => 8,
        (1, 1) => 16,
        (1, 2) => 32,
        (1, 3) => 128,
        _ => 0,
    }
}

/// Widget that draws an ADSR envelope with Braille characters
pub struct EnvelopeWidget {
    params: EnvelopeParams,
    line_color: Color,
    vertex_color: Color,
    playhead_color: Color,
    /// Playhead position as fraction 0.0-1.0 across the envelope
    playhead: Option<f32>,
}

impl EnvelopeWidget {
    pub fn new(params: EnvelopeParams) -> Self {
        Self {
            params,
            line_color: Color::Yellow,
            vertex_color: Color::Cyan,
            playhead_color: Color::White,
            playhead: None,
        }
    }

    /// Set the playhead position (0.0 to 1.0)
    pub fn with_playhead(mut self, position: Option<f32>) -> Self {
        self.playhead = position;
        self
    }

    /// Calculate vertex positions for the given area (in cell coordinates)
    pub fn vertices(&self, area: Rect) -> Vec<Vertex> {
        if area.width < 4 || area.height < 2 {
            return vec![];
        }

        let w = area.width as f32;
        let h = area.height as f32;

        // Calculate segment widths proportionally
        let total_time = self.params.attack + self.params.decay + 0.25 + self.params.release;
        let attack_w = (self.params.attack / total_time) * w;
        let decay_w = (self.params.decay / total_time) * w;
        let sustain_w = (0.25 / total_time) * w;

        let start_x = 0.0;
        let peak_x = attack_w;
        let decay_end_x = peak_x + decay_w;
        let sustain_end_x = decay_end_x + sustain_w;
        let release_end_x = w - 1.0;

        let bottom_y = h - 1.0;
        let top_y = 0.0;
        let sustain_y = (1.0 - self.params.sustain) * (h - 1.0);

        vec![
            Vertex {
                x: area.x + start_x as u16,
                y: area.y + bottom_y as u16,
                kind: VertexKind::Start,
            },
            Vertex {
                x: area.x + peak_x.round() as u16,
                y: area.y + top_y as u16,
                kind: VertexKind::AttackPeak,
            },
            Vertex {
                x: area.x + decay_end_x.round() as u16,
                y: area.y + sustain_y.round() as u16,
                kind: VertexKind::DecayEnd,
            },
            Vertex {
                x: area.x + sustain_end_x.round() as u16,
                y: area.y + sustain_y.round() as u16,
                kind: VertexKind::SustainEnd,
            },
            Vertex {
                x: area.x + release_end_x.min(area.x as f32 + w - 1.0) as u16,
                y: area.y + bottom_y as u16,
                kind: VertexKind::ReleaseEnd,
            },
        ]
    }

    /// Calculate points in braille coordinates (2x resolution for x, 4x for y)
    fn braille_points(&self, area: Rect) -> Vec<(f32, f32)> {
        let bw = (area.width as f32) * 2.0; // Braille width (2 dots per cell)
        let bh = (area.height as f32) * 4.0; // Braille height (4 dots per cell)

        // Calculate segment widths proportionally
        let total_time = self.params.attack + self.params.decay + 0.25 + self.params.release;
        let attack_w = (self.params.attack / total_time) * bw;
        let decay_w = (self.params.decay / total_time) * bw;
        let sustain_w = (0.25 / total_time) * bw;

        let start = (0.0, bh - 1.0);
        let peak = (attack_w, 0.0);
        let decay_end = (attack_w + decay_w, (1.0 - self.params.sustain) * (bh - 1.0));
        let sustain_end = (attack_w + decay_w + sustain_w, decay_end.1);
        let release_end = (bw - 1.0, bh - 1.0);

        vec![start, peak, decay_end, sustain_end, release_end]
    }
}

impl Widget for EnvelopeWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 2 {
            return;
        }

        // Build a map of cell -> braille dots
        let mut braille_map: HashMap<(u16, u16), u8> = HashMap::new();

        let points = self.braille_points(area);

        // Draw lines between points using braille
        for i in 0..points.len() - 1 {
            let (x1, y1) = points[i];
            let (x2, y2) = points[i + 1];
            self.draw_braille_line(&mut braille_map, x1, y1, x2, y2);
        }

        // Render braille characters
        let line_style = Style::default().fg(self.line_color);
        for ((cell_x, cell_y), dots) in &braille_map {
            let x = area.x + cell_x;
            let y = area.y + cell_y;
            if x < area.x + area.width && y < area.y + area.height {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    let ch = char::from_u32(BRAILLE_BASE + *dots as u32).unwrap_or('?');
                    cell.set_char(ch);
                    cell.set_style(line_style);
                }
            }
        }

        // Draw vertices on top
        let vertices = self.vertices(area);
        let vertex_style = Style::default().fg(self.vertex_color);
        for vertex in &vertices {
            if vertex.x >= area.x
                && vertex.x < area.x + area.width
                && vertex.y >= area.y
                && vertex.y < area.y + area.height
            {
                if let Some(cell) = buf.cell_mut((vertex.x, vertex.y)) {
                    cell.set_char('●');
                    cell.set_style(vertex_style);
                }
            }
        }

        // Draw playhead bar if active
        if let Some(pos) = self.playhead {
            let playhead_x = area.x + ((pos * (area.width - 1) as f32).round() as u16);
            if playhead_x >= area.x && playhead_x < area.x + area.width {
                let playhead_style = Style::default().fg(self.playhead_color);
                for y in area.y..area.y + area.height {
                    if let Some(cell) = buf.cell_mut((playhead_x, y)) {
                        cell.set_char('│');
                        cell.set_style(playhead_style);
                    }
                }
            }
        }
    }
}

impl EnvelopeWidget {
    /// Draw a line in braille coordinates
    fn draw_braille_line(
        &self,
        map: &mut HashMap<(u16, u16), u8>,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    ) {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let steps = dx.abs().max(dy.abs()).ceil() as i32;

        if steps == 0 {
            self.set_braille_dot(map, x1, y1);
            return;
        }

        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let x = x1 + t * dx;
            let y = y1 + t * dy;
            self.set_braille_dot(map, x, y);
        }
    }

    /// Set a single braille dot
    fn set_braille_dot(&self, map: &mut HashMap<(u16, u16), u8>, bx: f32, by: f32) {
        let cell_x = (bx / 2.0).floor() as u16;
        let cell_y = (by / 4.0).floor() as u16;
        let dot_x = (bx as u8) % 2;
        let dot_y = (by as u8) % 4;

        let dots = map.entry((cell_x, cell_y)).or_insert(0);
        *dots |= braille_dot(dot_x, dot_y);
    }
}

/// Simple envelope renderer that uses the widget
#[derive(Debug, Default)]
pub struct EnvelopeRenderer;

impl EnvelopeRenderer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }

    /// Check if graphics are available (always true for ASCII)
    pub fn is_available(&mut self) -> bool {
        true
    }

    /// Render the envelope to a frame area
    pub fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: Rect,
        params: EnvelopeParams,
        playhead: Option<f32>,
    ) {
        let widget = EnvelopeWidget::new(params).with_playhead(playhead);
        frame.render_widget(widget, area);
    }

    /// Invalidate the cache (no-op for ASCII renderer)
    pub fn invalidate(&mut self) {}
}
