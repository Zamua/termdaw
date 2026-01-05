//! Mouse input handling - fully encapsulated state machine
//!
//! Design principles (matching vim.rs):
//! - All mouse gesture logic is encapsulated here
//! - MouseState processes events and returns MouseAction for component to execute
//! - MouseState doesn't know about App, patterns, channels, etc.

#![allow(dead_code)] // Will be used as mouse handling is wired up

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use std::time::Instant;

// ============================================================================
// Core Types
// ============================================================================

/// Information about a click for double-click detection
#[derive(Debug, Clone)]
pub struct ClickInfo {
    pub x: u16,
    pub y: u16,
    pub time: Instant,
    pub button: MouseButton,
}

// ============================================================================
// Actions - What mouse tells the component to do
// ============================================================================

/// Actions that mouse returns for the component to execute
/// Mirrors VimAction in design
#[derive(Debug, Clone, PartialEq)]
pub enum MouseAction {
    /// Single click at screen position
    Click {
        x: u16,
        y: u16,
        button: MouseButton,
    },

    /// Double click at screen position (left button only)
    DoubleClick { x: u16, y: u16 },

    /// Right-click for context menu
    RightClick { x: u16, y: u16 },

    /// Drag started (button down with intent to drag)
    DragStart {
        x: u16,
        y: u16,
        button: MouseButton,
    },

    /// Drag moved (includes start position for context)
    DragMove {
        start_x: u16,
        start_y: u16,
        x: u16,
        y: u16,
    },

    /// Drag ended (button released after drag)
    DragEnd {
        start_x: u16,
        start_y: u16,
        x: u16,
        y: u16,
    },

    /// Scroll wheel
    Scroll { x: u16, y: u16, delta: i32 },
}

impl MouseAction {
    /// Get the current position of this action
    pub fn position(&self) -> (u16, u16) {
        match self {
            MouseAction::Click { x, y, .. } => (*x, *y),
            MouseAction::DoubleClick { x, y } => (*x, *y),
            MouseAction::RightClick { x, y } => (*x, *y),
            MouseAction::DragStart { x, y, .. } => (*x, *y),
            MouseAction::DragMove { x, y, .. } => (*x, *y),
            MouseAction::DragEnd { x, y, .. } => (*x, *y),
            MouseAction::Scroll { x, y, .. } => (*x, *y),
        }
    }
}

// ============================================================================
// Mouse State Machine
// ============================================================================

/// The main mouse state machine - encapsulated gesture tracking
///
/// Tracks drag state, double-click timing, and converts raw mouse events
/// into higher-level MouseAction enum values for components to execute.
#[derive(Debug, Clone, Default)]
pub struct MouseState {
    /// Position where drag started (if dragging)
    drag_start: Option<(u16, u16)>,

    /// Button currently held for drag
    drag_button: Option<MouseButton>,

    /// Whether we've moved enough to consider this a drag (vs a click)
    is_dragging: bool,

    /// Last click info for double-click detection
    last_click: Option<ClickInfo>,

    /// Current mouse position
    current_pos: (u16, u16),
}

impl MouseState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a raw mouse event and return action(s) for component to execute
    ///
    /// Returns a Vec because some events might trigger multiple actions
    /// (though typically just one). Mirrors VimState::process_key pattern.
    pub fn process_event(&mut self, event: MouseEvent) -> Vec<MouseAction> {
        let x = event.column;
        let y = event.row;
        self.current_pos = (x, y);

        let mut actions = Vec::new();

        match event.kind {
            MouseEventKind::Down(button) => {
                self.handle_down(x, y, button, &mut actions);
            }
            MouseEventKind::Up(button) => {
                self.handle_up(x, y, button, &mut actions);
            }
            MouseEventKind::Drag(button) => {
                self.handle_drag(x, y, button, &mut actions);
            }
            MouseEventKind::ScrollUp => {
                actions.push(MouseAction::Scroll { x, y, delta: -3 });
            }
            MouseEventKind::ScrollDown => {
                actions.push(MouseAction::Scroll { x, y, delta: 3 });
            }
            MouseEventKind::ScrollLeft => {
                // Could add horizontal scroll support later
            }
            MouseEventKind::ScrollRight => {
                // Could add horizontal scroll support later
            }
            MouseEventKind::Moved => {
                // Could track hover state if needed
            }
        }

        actions
    }

    /// Handle mouse button down
    fn handle_down(&mut self, x: u16, y: u16, button: MouseButton, actions: &mut Vec<MouseAction>) {
        match button {
            MouseButton::Left => {
                // Check for double-click
                if self.is_double_click(x, y, button) {
                    actions.push(MouseAction::DoubleClick { x, y });
                    self.last_click = None; // Reset after double-click
                    return;
                }

                // Record this click for future double-click detection
                self.record_click(x, y, button);

                // Start potential drag
                self.drag_start = Some((x, y));
                self.drag_button = Some(button);
                self.is_dragging = false;
            }
            MouseButton::Right => {
                actions.push(MouseAction::RightClick { x, y });
            }
            MouseButton::Middle => {
                // Could handle middle click if needed
            }
        }
    }

    /// Handle mouse button up
    fn handle_up(&mut self, x: u16, y: u16, button: MouseButton, actions: &mut Vec<MouseAction>) {
        if self.drag_button == Some(button) {
            if self.is_dragging {
                // End drag
                if let Some((start_x, start_y)) = self.drag_start {
                    actions.push(MouseAction::DragEnd {
                        start_x,
                        start_y,
                        x,
                        y,
                    });
                }
            } else {
                // Was just a click (no drag movement)
                actions.push(MouseAction::Click { x, y, button });
            }

            // Reset drag state
            self.drag_start = None;
            self.drag_button = None;
            self.is_dragging = false;
        }
    }

    /// Handle mouse drag (movement while button held)
    fn handle_drag(&mut self, x: u16, y: u16, button: MouseButton, actions: &mut Vec<MouseAction>) {
        if self.drag_button != Some(button) {
            return;
        }

        if let Some((start_x, start_y)) = self.drag_start {
            // Check if we've moved enough to consider this a drag
            let dx = (x as i32 - start_x as i32).abs();
            let dy = (y as i32 - start_y as i32).abs();
            let threshold = 1; // Minimum movement to trigger drag

            if !self.is_dragging && (dx > threshold || dy > threshold) {
                // First movement past threshold - emit DragStart
                self.is_dragging = true;
                actions.push(MouseAction::DragStart {
                    x: start_x,
                    y: start_y,
                    button,
                });
            }

            if self.is_dragging {
                // Continue drag
                actions.push(MouseAction::DragMove {
                    start_x,
                    start_y,
                    x,
                    y,
                });
            }
        }
    }

    /// Check if this click qualifies as a double-click
    fn is_double_click(&self, x: u16, y: u16, button: MouseButton) -> bool {
        if let Some(last) = &self.last_click {
            // Same button
            if last.button != button {
                return false;
            }

            // Close enough in space (within 2 pixels)
            let dx = (x as i32 - last.x as i32).abs();
            let dy = (y as i32 - last.y as i32).abs();
            if dx > 2 || dy > 2 {
                return false;
            }

            // Close enough in time (within 500ms)
            if last.time.elapsed().as_millis() > 500 {
                return false;
            }

            true
        } else {
            false
        }
    }

    /// Record a click for future double-click detection
    fn record_click(&mut self, x: u16, y: u16, button: MouseButton) {
        self.last_click = Some(ClickInfo {
            x,
            y,
            time: Instant::now(),
            button,
        });
    }

    /// Get current mouse position
    pub fn current_position(&self) -> (u16, u16) {
        self.current_pos
    }

    /// Check if currently in a drag operation
    pub fn is_dragging(&self) -> bool {
        self.is_dragging
    }

    /// Get drag start position if dragging
    pub fn drag_start_position(&self) -> Option<(u16, u16)> {
        if self.is_dragging {
            self.drag_start
        } else {
            None
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{MouseEvent, MouseEventKind};

    fn make_event(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: crossterm::event::KeyModifiers::empty(),
        }
    }

    #[test]
    fn test_simple_click() {
        let mut mouse = MouseState::new();

        // Mouse down
        let actions = mouse.process_event(make_event(MouseEventKind::Down(MouseButton::Left), 10, 20));
        assert!(actions.is_empty()); // Down doesn't emit action yet

        // Mouse up without moving = click
        let actions = mouse.process_event(make_event(MouseEventKind::Up(MouseButton::Left), 10, 20));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], MouseAction::Click { x: 10, y: 20, button: MouseButton::Left }));
    }

    #[test]
    fn test_drag() {
        let mut mouse = MouseState::new();

        // Mouse down
        let actions = mouse.process_event(make_event(MouseEventKind::Down(MouseButton::Left), 10, 20));
        assert!(actions.is_empty());

        // Drag (move while button held)
        let actions = mouse.process_event(make_event(MouseEventKind::Drag(MouseButton::Left), 15, 25));
        assert_eq!(actions.len(), 2); // DragStart + DragMove
        assert!(matches!(actions[0], MouseAction::DragStart { x: 10, y: 20, .. }));
        assert!(matches!(actions[1], MouseAction::DragMove { start_x: 10, start_y: 20, x: 15, y: 25 }));

        // Continue drag
        let actions = mouse.process_event(make_event(MouseEventKind::Drag(MouseButton::Left), 20, 30));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], MouseAction::DragMove { x: 20, y: 30, .. }));

        // Release
        let actions = mouse.process_event(make_event(MouseEventKind::Up(MouseButton::Left), 20, 30));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], MouseAction::DragEnd { start_x: 10, start_y: 20, x: 20, y: 30 }));
    }

    #[test]
    fn test_right_click() {
        let mut mouse = MouseState::new();

        let actions = mouse.process_event(make_event(MouseEventKind::Down(MouseButton::Right), 10, 20));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], MouseAction::RightClick { x: 10, y: 20 }));
    }

    #[test]
    fn test_scroll() {
        let mut mouse = MouseState::new();

        let actions = mouse.process_event(make_event(MouseEventKind::ScrollUp, 10, 20));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], MouseAction::Scroll { x: 10, y: 20, delta: -3 }));

        let actions = mouse.process_event(make_event(MouseEventKind::ScrollDown, 10, 20));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], MouseAction::Scroll { x: 10, y: 20, delta: 3 }));
    }
}
