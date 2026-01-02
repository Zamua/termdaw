//! Plugin editor modal UI
//!
//! A popup modal for editing plugin parameters with vim-style navigation.
//! Features:
//! - High-quality ADSR envelope visualization using tiny-skia + kitty graphics
//! - Horizontal faders for parameters
//! - Waveform selector

use std::time::Instant;

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::envelope::{EnvelopeParams, EnvelopeRenderer};
use crate::app::App;
use crate::plugin_host::PluginParam;

/// Plugin editor state
#[derive(Default)]
pub struct PluginEditorState {
    /// Whether the editor is visible
    pub visible: bool,
    /// Channel index being edited
    pub channel_idx: usize,
    /// Currently selected parameter index
    pub selected_param: usize,
    /// Plugin name
    pub plugin_name: String,
    /// Plugin parameters
    pub params: Vec<PluginParam>,
    /// Envelope renderer (handles caching and graphics protocol)
    envelope_renderer: EnvelopeRenderer,
    /// When the preview started (note on)
    preview_start: Option<Instant>,
    /// When the preview key was released (note off)
    preview_release: Option<Instant>,
}

impl std::fmt::Debug for PluginEditorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginEditorState")
            .field("visible", &self.visible)
            .field("channel_idx", &self.channel_idx)
            .field("selected_param", &self.selected_param)
            .field("plugin_name", &self.plugin_name)
            .field("params", &self.params)
            .finish()
    }
}

#[allow(dead_code)]
impl PluginEditorState {
    /// Create a new plugin editor state
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the editor for a channel
    pub fn open(&mut self, channel_idx: usize, plugin_name: &str, params: Vec<PluginParam>) {
        // If reopening the same channel, preserve existing param values
        if self.channel_idx == channel_idx && !self.params.is_empty() {
            // Just make it visible again, keep current params
            self.visible = true;
            return;
        }

        self.visible = true;
        self.channel_idx = channel_idx;
        self.selected_param = 0;
        self.plugin_name = plugin_name.to_string();
        self.params = params;
        // Invalidate envelope cache so it regenerates with new params
        self.envelope_renderer.invalidate();
    }

    /// Close the editor
    pub fn close(&mut self) {
        self.visible = false;
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if self.selected_param > 0 {
            self.selected_param -= 1;
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if !self.params.is_empty() && self.selected_param < self.params.len() - 1 {
            self.selected_param += 1;
        }
    }

    /// Adjust selected parameter value (delta is -1 to 1)
    pub fn adjust_value(&mut self, delta: f32) {
        if let Some(param) = self.params.get_mut(self.selected_param) {
            // Handle discrete parameters (like Waveform) differently
            if param.name == "Waveform" {
                // Cycle through discrete values
                let current = param.value.round() as i32;
                let max = param.max as i32;
                let new_val = if delta > 0.0 {
                    (current + 1).min(max)
                } else {
                    (current - 1).max(0)
                };
                param.value = new_val as f32;
            } else {
                // Continuous parameters: 1% of range per step
                let range = param.max - param.min;
                let step = range * 0.01;
                param.value = (param.value + delta * step).clamp(param.min, param.max);
            }
            // Note: envelope renderer handles parameter changes via set_source()
            // No need to invalidate - that would cause flicker by recreating the protocol
        }
    }

    /// Get the currently selected parameter
    pub fn selected_param(&self) -> Option<&PluginParam> {
        self.params.get(self.selected_param)
    }

    /// Get parameter value by name
    pub fn get_param_value(&self, name: &str) -> Option<f32> {
        self.params.iter().find(|p| p.name == name).map(|p| p.value)
    }

    /// Get normalized parameter value (0.0-1.0) by name
    pub fn get_param_normalized(&self, name: &str) -> Option<f32> {
        self.params.iter().find(|p| p.name == name).map(|p| {
            let range = p.max - p.min;
            if range > 0.0 {
                (p.value - p.min) / range
            } else {
                0.0
            }
        })
    }

    /// Get the CLAP param ID for the currently selected parameter.
    /// Maps the param name to nih-plug's Rabin fingerprint hash.
    pub fn get_selected_clap_param_id(&self) -> Option<u32> {
        self.params.get(self.selected_param).and_then(|p| {
            // nih-plug uses Rabin fingerprint: hash = hash * 31 + byte
            // These are pre-computed for the simple-synth params
            match p.name.as_str() {
                "Attack" => Some(96920),     // hash of "atk"
                "Decay" => Some(99330),      // hash of "dec"
                "Sustain" => Some(114257),   // hash of "sus"
                "Release" => Some(112793),   // hash of "rel"
                "Gain" => Some(3165055),     // hash of "gain"
                "Waveform" => Some(3642105), // hash of "wave"
                _ => None,
            }
        })
    }

    /// Get the selected parameter's normalized value (0.0-1.0) for sending to the plugin.
    /// CLAP/nih-plug expects normalized values for continuous params.
    pub fn get_selected_param_value(&self) -> Option<f64> {
        self.params.get(self.selected_param).map(|p| {
            match p.name.as_str() {
                // Waveform is discrete - use the enum index directly
                "Waveform" => p.value.round() as f64,
                // All other params need to be normalized to 0.0-1.0
                _ => {
                    let range = p.max - p.min;
                    if range > 0.0 {
                        ((p.value - p.min) / range) as f64
                    } else {
                        0.0
                    }
                }
            }
        })
    }

    /// Get current ADSR envelope parameters
    fn get_envelope_params(&self) -> EnvelopeParams {
        EnvelopeParams {
            attack: self.get_param_normalized("Attack").unwrap_or(0.1),
            decay: self.get_param_normalized("Decay").unwrap_or(0.2),
            sustain: self.get_param_normalized("Sustain").unwrap_or(0.7),
            release: self.get_param_normalized("Release").unwrap_or(0.3),
        }
    }

    /// Start the preview animation (called when 's' is pressed)
    pub fn start_preview_animation(&mut self) {
        self.preview_start = Some(Instant::now());
        self.preview_release = None;
    }

    /// Stop the preview animation (called when 's' is released)
    pub fn stop_preview_animation(&mut self) {
        if self.preview_start.is_some() {
            self.preview_release = Some(Instant::now());
        }
    }

    /// Clear preview animation state
    pub fn clear_preview_animation(&mut self) {
        self.preview_start = None;
        self.preview_release = None;
    }

    /// Calculate the current playhead position as a fraction (0.0 to 1.0) across the envelope.
    /// Returns None if not currently previewing or if the envelope has completed.
    pub fn get_playhead_position(&self) -> Option<f32> {
        let start = self.preview_start?;

        // Get ADSR times in milliseconds
        let attack_ms = self.get_param_value("Attack").unwrap_or(10.0);
        let decay_ms = self.get_param_value("Decay").unwrap_or(100.0);
        let release_ms = self.get_param_value("Release").unwrap_or(200.0);

        // Calculate total envelope width (same as in envelope.rs)
        // We use a fixed sustain display width of 25% of total
        let sustain_ratio = 0.25;
        let total_time = attack_ms
            + decay_ms
            + (sustain_ratio * (attack_ms + decay_ms + release_ms) / (1.0 - sustain_ratio))
            + release_ms;

        let attack_frac = attack_ms / total_time;
        let decay_frac = decay_ms / total_time;
        let sustain_frac = sustain_ratio;
        let release_frac = release_ms / total_time;

        // Calculate positions on the envelope (cumulative)
        let attack_end = attack_frac;
        let decay_end = attack_end + decay_frac;
        let sustain_end = decay_end + sustain_frac;

        if let Some(release_time) = self.preview_release {
            // Key has been released - we're in the release phase
            let time_since_release = release_time.elapsed().as_secs_f32() * 1000.0;

            if time_since_release >= release_ms {
                // Envelope has completed
                return None;
            }

            // Position in release phase
            let release_progress = time_since_release / release_ms;
            Some(sustain_end + release_progress * release_frac)
        } else {
            // Key is still held - calculate position in attack/decay/sustain
            let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;

            if elapsed_ms < attack_ms {
                // Attack phase
                let attack_progress = elapsed_ms / attack_ms;
                Some(attack_progress * attack_frac)
            } else if elapsed_ms < attack_ms + decay_ms {
                // Decay phase
                let decay_elapsed = elapsed_ms - attack_ms;
                let decay_progress = decay_elapsed / decay_ms;
                Some(attack_end + decay_progress * decay_frac)
            } else {
                // Sustain phase - hold at end of sustain
                Some(sustain_end)
            }
        }
    }
}

/// Render the plugin editor modal
pub fn render(frame: &mut Frame, app: &mut App) {
    if !app.plugin_editor.visible {
        return;
    }

    let area = frame.area();

    // Calculate popup dimensions (70% width, 80% height)
    let popup_width = (area.width as f32 * 0.7).min(70.0) as u16;
    let popup_height = (area.height as f32 * 0.8).min(28.0) as u16;
    let popup_area = centered_rect(popup_width, popup_height, area);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Render the popup border
    let title = format!(" {} ", app.plugin_editor.plugin_name);
    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Render content
    if app.plugin_editor.params.is_empty() {
        let no_params = Paragraph::new("No parameters available")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(no_params, inner);
    } else {
        // Give the envelope more space - use about 40% of the modal height
        let envelope_height = (inner.height as f32 * 0.4).max(8.0) as u16;

        // Render ADSR envelope at the top - use full width
        let envelope_area = Rect {
            x: inner.x + 1,
            y: inner.y + 1,
            width: inner.width.saturating_sub(2),
            height: envelope_height.saturating_sub(1),
        };
        render_envelope(frame, envelope_area, &mut app.plugin_editor);

        // Render parameters below the envelope
        let params_area = Rect {
            x: inner.x,
            y: inner.y + envelope_height + 1,
            width: inner.width,
            height: inner.height.saturating_sub(envelope_height + 2),
        };
        render_params(frame, params_area, &app.plugin_editor);
    }

    // Render help footer
    render_footer(frame, popup_area);
}

/// Render the ADSR envelope visualization
fn render_envelope(frame: &mut Frame, area: Rect, state: &mut PluginEditorState) {
    if area.height < 2 || area.width < 10 {
        return;
    }

    // Render title above the envelope
    let title_area = Rect {
        x: area.x,
        y: area.y.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let title_line = Line::from(vec![
        Span::styled("─── ", Style::default().fg(Color::DarkGray)),
        Span::styled("ENVELOPE", Style::default().fg(Color::White)),
        Span::styled(" ───", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(title_line), title_area);

    // Get envelope parameters and playhead position
    let params = state.get_envelope_params();
    let playhead = state.get_playhead_position();

    if state.envelope_renderer.is_available() {
        state
            .envelope_renderer
            .render(frame, area, params, playhead);
    } else {
        // Fallback for terminals without graphics support
        render_envelope_fallback(frame, area, params);
    }
}

/// ASCII fallback for terminals without graphics protocol support
fn render_envelope_fallback(frame: &mut Frame, area: Rect, params: EnvelopeParams) {
    // Simple text-based representation
    let env_width = area.width as usize;
    let env_height = area.height as usize;

    if env_height < 2 || env_width < 10 {
        return;
    }

    // Calculate segment widths
    let total_time = params.attack + params.decay + 0.3 + params.release;
    let attack_w = ((params.attack / total_time) * env_width as f32) as usize;
    let decay_w = ((params.decay / total_time) * env_width as f32) as usize;
    let sustain_w = ((0.3 / total_time) * env_width as f32) as usize;
    let release_w = env_width.saturating_sub(attack_w + decay_w + sustain_w);

    // Calculate heights at each column
    let mut heights: Vec<f32> = Vec::with_capacity(env_width);

    // Attack: 0 -> 1
    for i in 0..attack_w {
        let t = if attack_w > 0 {
            i as f32 / attack_w as f32
        } else {
            1.0
        };
        heights.push(t);
    }

    // Decay: 1 -> sustain
    for i in 0..decay_w {
        let t = if decay_w > 0 {
            i as f32 / decay_w as f32
        } else {
            1.0
        };
        heights.push(1.0 - t * (1.0 - params.sustain));
    }

    // Sustain: hold
    for _ in 0..sustain_w {
        heights.push(params.sustain);
    }

    // Release: sustain -> 0
    for i in 0..release_w {
        let t = if release_w > 0 {
            i as f32 / release_w as f32
        } else {
            1.0
        };
        heights.push(params.sustain * (1.0 - t));
    }

    // Convert to row indices
    let height_to_row: Vec<usize> = heights
        .iter()
        .map(|&h| ((1.0 - h) * (env_height - 1) as f32).round() as usize)
        .collect();

    // Render each row
    for row in 0..env_height {
        let y = area.y + row as u16;
        let mut spans: Vec<Span> = Vec::new();

        for (col, &target_row) in height_to_row.iter().enumerate() {
            let ch = if row == target_row { "█" } else { " " };
            let style = if row == target_row {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            spans.push(Span::styled(ch, style));

            if col >= area.width as usize - 1 {
                break;
            }
        }

        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));
    }
}

/// Render parameter list with horizontal faders
fn render_params(frame: &mut Frame, area: Rect, state: &PluginEditorState) {
    let fader_width = 30usize;
    let mut y = area.y;

    for (i, param) in state.params.iter().enumerate() {
        if y >= area.y + area.height - 2 {
            break;
        }

        let is_selected = i == state.selected_param;

        // Calculate fill percentage
        let range = param.max - param.min;
        let normalized = if range > 0.0 {
            ((param.value - param.min) / range).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fill_count = (normalized * fader_width as f32) as usize;

        // Build the fader line
        let prefix = if is_selected { "▸ " } else { "  " };
        let name_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        // Format value based on parameter type
        let value_str = if param.name == "Sustain" || param.name == "Gain" {
            format!("{:>5.0}%", param.value * 100.0)
        } else if param.name == "Waveform" {
            // Order matches synth's Waveform enum: Sine, Square, Saw, Triangle
            match param.value.round() as u8 {
                0 => "  SINE".to_string(),
                1 => "   SQR".to_string(),
                2 => "   SAW".to_string(),
                _ => "   TRI".to_string(),
            }
        } else {
            format!("{:>5.0}ms", param.value)
        };

        // Build fader string
        let mut fader_spans: Vec<Span> = Vec::new();
        fader_spans.push(Span::styled(prefix, name_style));
        fader_spans.push(Span::styled(format!("{:<10}", param.name), name_style));

        // Fader track with colored start marker
        if param.name != "Waveform" {
            // Start marker (vertex color)
            fader_spans.push(Span::styled("░", Style::default().fg(Color::Cyan)));

            // Filled portion
            if fill_count > 1 {
                fader_spans.push(Span::styled(
                    "█".repeat(fill_count - 1),
                    Style::default().fg(Color::Yellow),
                ));
            }

            // Empty portion
            let empty_count = fader_width.saturating_sub(fill_count);
            if empty_count > 0 {
                fader_spans.push(Span::styled(
                    "░".repeat(empty_count),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            fader_spans.push(Span::styled("▏", Style::default().fg(Color::DarkGray)));
        } else {
            // Waveform selector - show options with proper spacing
            // Order: Sine(0), Square(1), Saw(2), Triangle(3) - matches synth enum
            let wave_val = param.value.round() as u8;
            let waves = [("∿", 0u8), ("⊓", 1), ("⩘", 2), ("△", 3)];

            for (symbol, val) in waves {
                if wave_val == val {
                    fader_spans.push(Span::styled(
                        format!(" ◉ {} ", symbol),
                        Style::default().fg(Color::Yellow),
                    ));
                } else {
                    fader_spans.push(Span::styled(
                        format!(" ○ {} ", symbol),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
        }

        // Value display
        fader_spans.push(Span::styled(
            format!("  {}", value_str),
            if is_selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Gray)
            },
        ));

        let line = Line::from(fader_spans);
        frame.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));

        y += 2; // Add spacing between parameters
    }
}

/// Render footer with keybindings
fn render_footer(frame: &mut Frame, popup_area: Rect) {
    let footer_area = Rect {
        x: popup_area.x + 1,
        y: popup_area.y + popup_area.height - 2,
        width: popup_area.width - 2,
        height: 1,
    };

    let footer = Paragraph::new(Line::from(vec![
        Span::styled("j/k", Style::default().fg(Color::Cyan)),
        Span::styled(" navigate  ", Style::default().fg(Color::DarkGray)),
        Span::styled("h/l", Style::default().fg(Color::Cyan)),
        Span::styled(" adjust  ", Style::default().fg(Color::DarkGray)),
        Span::styled("H/L", Style::default().fg(Color::Cyan)),
        Span::styled(" fine  ", Style::default().fg(Color::DarkGray)),
        Span::styled("s", Style::default().fg(Color::Cyan)),
        Span::styled(" preview  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled(" close", Style::default().fg(Color::DarkGray)),
    ]))
    .alignment(Alignment::Center);

    frame.render_widget(footer, footer_area);
}

/// Create a centered rect of given size within the parent area
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;

    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
