//! Vim-style modal editing - fully encapsulated state machine
//!
//! Design principles (matching TypeScript version):
//! - All vim logic is encapsulated here
//! - Components provide configuration via VimConfig trait
//! - Vim processes keys and returns VimAction for component to execute
//! - Vim doesn't know about pattern data, channels, etc.

use serde::{Deserialize, Serialize};

// ============================================================================
// Core Types
// ============================================================================

/// Vim editing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum VimMode {
    #[default]
    Normal,
    Visual,
    VisualBlock,
    OperatorPending,
}

impl VimMode {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            VimMode::Normal => "NORMAL",
            VimMode::Visual => "VISUAL",
            VimMode::VisualBlock => "V-BLOCK",
            VimMode::OperatorPending => "OP-PENDING",
        }
    }

    pub fn is_visual(&self) -> bool {
        matches!(self, VimMode::Visual | VimMode::VisualBlock)
    }
}

/// Pending operator waiting for a motion
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Yank,
    Change,
}

/// A 2D position in a grid
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

impl Position {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

// ============================================================================
// Grid Semantics - Zone-aware navigation
// ============================================================================

/// A zone within the grid (e.g., sample zone, mute zone, steps zone)
#[derive(Debug, Clone)]
pub struct Zone {
    /// Column range [start, end] inclusive
    pub col_range: (usize, usize),
    /// Whether this is the main zone (for 0/$ navigation)
    pub is_main: bool,
    /// Word interval for w/b/e motions (e.g., 4 for beat boundaries)
    pub word_interval: Option<usize>,
}

impl Zone {
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            col_range: (start, end),
            is_main: false,
            word_interval: None,
        }
    }

    pub fn main(mut self) -> Self {
        self.is_main = true;
        self
    }

    pub fn with_word_interval(mut self, interval: usize) -> Self {
        self.word_interval = Some(interval);
        self
    }

    /// Check if a column is within this zone
    pub fn contains_col(&self, col: usize) -> bool {
        col >= self.col_range.0 && col <= self.col_range.1
    }

    /// Get the start column
    pub fn start(&self) -> usize {
        self.col_range.0
    }

    /// Get the end column
    pub fn end(&self) -> usize {
        self.col_range.1
    }
}

/// Grid semantics configuration for zone-aware navigation
#[derive(Debug, Clone, Default)]
pub struct GridSemantics {
    /// Zones in left-to-right order
    pub zones: Vec<Zone>,
}

impl GridSemantics {
    pub fn with_zones(zones: Vec<Zone>) -> Self {
        Self { zones }
    }

    /// Get the zone containing the given column
    pub fn get_zone_at_col(&self, col: usize) -> Option<&Zone> {
        self.zones.iter().find(|z| z.contains_col(col))
    }

    /// Get the previous zone (for h motion at zone boundary)
    pub fn get_prev_zone(&self, current_col: usize) -> Option<&Zone> {
        let current_zone = self.get_zone_at_col(current_col)?;
        // Find zone whose end is just before current zone's start
        self.zones
            .iter()
            .find(|z| z.end() + 1 == current_zone.start())
    }

    /// Get the next zone (for l motion at zone boundary)
    pub fn get_next_zone(&self, current_col: usize) -> Option<&Zone> {
        let current_zone = self.get_zone_at_col(current_col)?;
        // Find zone whose start is just after current zone's end
        self.zones
            .iter()
            .find(|z| z.start() == current_zone.end() + 1)
    }
}

/// Selection range type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RangeType {
    #[default]
    Char,
    Line,
    Block,
}

/// A selection range
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
    pub range_type: RangeType,
}

#[allow(dead_code)]
impl Range {
    /// Get normalized range (start <= end)
    pub fn normalized(&self) -> (Position, Position) {
        let min_row = self.start.row.min(self.end.row);
        let max_row = self.start.row.max(self.end.row);
        let min_col = self.start.col.min(self.end.col);
        let max_col = self.start.col.max(self.end.col);

        (
            Position::new(min_row, min_col),
            Position::new(max_row, max_col),
        )
    }

    /// Check if position is within selection
    pub fn contains(&self, pos: Position) -> bool {
        match self.range_type {
            RangeType::Block => {
                // Block selections use normalized coordinates
                let (start, end) = self.normalized();
                pos.row >= start.row
                    && pos.row <= end.row
                    && pos.col >= start.col
                    && pos.col <= end.col
            }
            RangeType::Line => {
                // Line selections use normalized rows only
                let min_row = self.start.row.min(self.end.row);
                let max_row = self.start.row.max(self.end.row);
                pos.row >= min_row && pos.row <= max_row
            }
            RangeType::Char => {
                // Char selections are directional - normalize row order but preserve
                // column semantics based on direction
                let (start, end) = if self.start.row < self.end.row
                    || (self.start.row == self.end.row && self.start.col <= self.end.col)
                {
                    (self.start, self.end)
                } else {
                    (self.end, self.start)
                };

                if pos.row < start.row || pos.row > end.row {
                    return false;
                }
                if start.row == end.row {
                    pos.col >= start.col && pos.col <= end.col
                } else if pos.row == start.row {
                    pos.col >= start.col
                } else if pos.row == end.row {
                    pos.col <= end.col
                } else {
                    true
                }
            }
        }
    }
}

// ============================================================================
// Actions - What vim tells the component to do
// ============================================================================

/// Actions that vim returns for the component to execute
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum VimAction {
    /// No action needed
    None,

    /// Move cursor to position
    MoveCursor(Position),

    /// Toggle item at current position (like 'x' in normal mode)
    Toggle,

    /// Yank the given range (component should copy data and call vim.set_register)
    Yank(Range),

    /// Delete the given range (component should delete and call vim.set_register)
    Delete(Range),

    /// Paste after current position (p)
    Paste,

    /// Paste before current position (P)
    PasteBefore,

    /// Visual selection changed (for UI highlighting)
    SelectionChanged(Option<Range>),

    /// Mode changed (for status bar)
    ModeChanged(VimMode),

    /// Escape was pressed (component may want to do cleanup)
    Escape,

    /// Scroll viewport by N lines (positive = down, negative = up)
    /// Used by Ctrl+e (scroll down) and Ctrl+y (scroll up)
    ScrollViewport(i32),
}

// ============================================================================
// Register (clipboard)
// ============================================================================

/// Register content for yank/paste - generic over data type T
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RegisterContent<T: Clone> {
    /// Raw data - type determined by the component
    pub data: T,
    pub range_type: RangeType,
}

#[allow(dead_code)]
impl<T: Clone> RegisterContent<T> {
    pub fn new(data: T, range_type: RangeType) -> Self {
        Self { data, range_type }
    }
}

/// Multi-register system matching vim's behavior - generic over data type T
/// - Register 0: last yank
/// - Registers 1-9: delete history (1 = most recent, shifts down)
/// - Unnamed register: last operation (yank or delete)
#[derive(Debug, Clone)]
pub struct RegisterBank<T: Clone> {
    /// Register 0: last yank
    pub reg_0: Option<RegisterContent<T>>,
    /// Registers 1-9: delete history (index 0 = register 1)
    pub reg_1_9: [Option<RegisterContent<T>>; 9],
    /// Unnamed register (last operation)
    pub unnamed: Option<RegisterContent<T>>,
}

impl<T: Clone> Default for RegisterBank<T> {
    fn default() -> Self {
        Self {
            reg_0: None,
            reg_1_9: Default::default(),
            unnamed: None,
        }
    }
}

#[allow(dead_code)]
impl<T: Clone> RegisterBank<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a yank operation
    pub fn store_yank(&mut self, content: RegisterContent<T>) {
        self.reg_0 = Some(content.clone());
        self.unnamed = Some(content);
    }

    /// Store a delete operation (shifts history)
    pub fn store_delete(&mut self, content: RegisterContent<T>) {
        // Shift registers 1-9 down (9 is lost, 1 becomes 2, etc.)
        for i in (1..9).rev() {
            self.reg_1_9[i] = self.reg_1_9[i - 1].take();
        }
        // New delete goes into register 1
        self.reg_1_9[0] = Some(content.clone());
        self.unnamed = Some(content);
    }

    /// Get the default register (unnamed) for paste
    pub fn get_default(&self) -> Option<&RegisterContent<T>> {
        self.unnamed.as_ref()
    }

    /// Get register 0 (last yank)
    pub fn get_reg_0(&self) -> Option<&RegisterContent<T>> {
        self.reg_0.as_ref()
    }

    /// Get register 1-9 (delete history)
    pub fn get_reg_numbered(&self, n: usize) -> Option<&RegisterContent<T>> {
        if (1..=9).contains(&n) {
            self.reg_1_9[n - 1].as_ref()
        } else {
            None
        }
    }
}

// ============================================================================
// Jumplist
// ============================================================================

/// Jumplist for navigation history
#[derive(Debug, Clone, Default)]
pub struct Jumplist {
    /// Stack of previous positions
    positions: Vec<Position>,
    /// Current index in the stack (-1 means at current position)
    index: isize,
}

#[allow(dead_code)]
impl Jumplist {
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            index: -1,
        }
    }

    /// Push a new position to the jumplist
    pub fn push(&mut self, pos: Position) {
        // Remove any forward history when pushing a new position
        if self.index >= 0 {
            let keep = (self.index + 1) as usize;
            self.positions.truncate(keep);
        }
        self.positions.push(pos);
        self.index = -1; // Reset to current position
    }

    /// Go back in the jumplist
    pub fn go_back(&mut self, current: Position) -> Option<Position> {
        if self.positions.is_empty() {
            return None;
        }

        // If at current position, save it first
        if self.index == -1 {
            self.positions.push(current);
            self.index = (self.positions.len() as isize) - 2;
        } else if self.index > 0 {
            self.index -= 1;
        } else {
            return None; // Already at oldest
        }

        self.positions.get(self.index as usize).copied()
    }

    /// Go forward in the jumplist
    pub fn go_forward(&mut self) -> Option<Position> {
        if self.index < 0 || self.positions.is_empty() {
            return None;
        }

        if (self.index as usize) < self.positions.len() - 1 {
            self.index += 1;
            let pos = self.positions.get(self.index as usize).copied();
            if self.index as usize == self.positions.len() - 1 {
                // Back at the current position
                self.index = -1;
            }
            pos
        } else {
            None
        }
    }
}

// ============================================================================
// Dot Repeat
// ============================================================================

/// Last repeatable action for the `.` command
#[derive(Debug, Clone)]
pub enum RepeatableAction {
    /// Delete with operator+motion (stores keys pressed)
    OperatorMotion {
        operator: Operator,
        motion: char,
        count: Option<usize>,
    },
    /// Toggle at position
    Toggle,
    /// Paste
    Paste { before: bool },
}

// ============================================================================
// Vim State Machine
// ============================================================================

/// Grid dimensions for boundary checking
#[derive(Debug, Clone, Copy)]
pub struct GridDimensions {
    pub rows: usize,
    pub cols: usize,
}

/// The main vim state machine - generic over register data type T
/// Default type is Vec<Vec<bool>> for channel rack step data
#[derive(Debug, Clone)]
pub struct VimState<T: Clone = Vec<Vec<bool>>> {
    /// Current mode
    mode: VimMode,

    /// Pending operator (d, y, c)
    operator: Option<Operator>,

    /// Visual selection anchor
    visual_anchor: Option<Position>,

    /// Accumulated count (for 5j, 3x, etc.)
    count: Option<usize>,

    /// Register bank (multi-register clipboard)
    registers: RegisterBank<T>,

    /// Grid dimensions for bounds checking
    dimensions: GridDimensions,

    /// Grid semantics for zone-aware navigation
    grid_semantics: Option<GridSemantics>,

    /// Jumplist for Ctrl+o / Ctrl+i navigation
    jumplist: Jumplist,

    /// Last repeatable action for dot command
    last_action: Option<RepeatableAction>,
}

#[allow(dead_code)]
impl<T: Clone> VimState<T> {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            mode: VimMode::Normal,
            operator: None,
            visual_anchor: None,
            count: None,
            registers: RegisterBank::new(),
            dimensions: GridDimensions { rows, cols },
            grid_semantics: None,
            jumplist: Jumplist::new(),
            last_action: None,
        }
    }

    /// Create a new VimState with grid semantics (zones)
    pub fn with_grid_semantics(rows: usize, cols: usize, semantics: GridSemantics) -> Self {
        Self {
            mode: VimMode::Normal,
            operator: None,
            visual_anchor: None,
            count: None,
            registers: RegisterBank::new(),
            dimensions: GridDimensions { rows, cols },
            grid_semantics: Some(semantics),
            jumplist: Jumplist::new(),
            last_action: None,
        }
    }

    /// Set grid semantics for zone-aware navigation
    pub fn set_grid_semantics(&mut self, semantics: GridSemantics) {
        self.grid_semantics = Some(semantics);
    }

    /// Clear grid semantics
    pub fn clear_grid_semantics(&mut self) {
        self.grid_semantics = None;
    }

    // ========================================================================
    // Public getters
    // ========================================================================

    pub fn mode(&self) -> VimMode {
        self.mode
    }

    pub fn is_visual(&self) -> bool {
        self.mode.is_visual()
    }

    pub fn get_selection(&self, cursor: Position) -> Option<Range> {
        self.visual_anchor.map(|anchor| Range {
            start: anchor,
            end: cursor,
            range_type: match self.mode {
                VimMode::VisualBlock => RangeType::Block,
                VimMode::Visual => RangeType::Char,
                _ => RangeType::Char,
            },
        })
    }

    /// Get the default register for pasting (unnamed register)
    pub fn get_register(&self) -> Option<&RegisterContent<T>> {
        self.registers.get_default()
    }

    /// Get register 0 (last yank)
    pub fn get_register_0(&self) -> Option<&RegisterContent<T>> {
        self.registers.get_reg_0()
    }

    /// Get numbered register 1-9 (delete history)
    pub fn get_register_numbered(&self, n: usize) -> Option<&RegisterContent<T>> {
        self.registers.get_reg_numbered(n)
    }

    /// Store a yank operation (goes to register 0 and unnamed)
    pub fn store_yank(&mut self, data: T, range_type: RangeType) {
        let content = RegisterContent::new(data, range_type);
        self.registers.store_yank(content);
    }

    /// Store a delete operation (goes to register 1 and unnamed, shifts history)
    pub fn store_delete(&mut self, data: T, range_type: RangeType) {
        let content = RegisterContent::new(data, range_type);
        self.registers.store_delete(content);
    }

    /// Legacy method for compatibility - stores as yank
    pub fn set_register(&mut self, data: T, range_type: RangeType) {
        self.store_yank(data, range_type);
    }

    pub fn update_dimensions(&mut self, rows: usize, cols: usize) {
        self.dimensions = GridDimensions { rows, cols };
    }

    // ========================================================================
    // Key processing - the main entry point
    // ========================================================================

    /// Process a key and return action(s) for the component to execute
    ///
    /// Returns a Vec because some keys trigger multiple actions
    /// (e.g., 'd' in visual mode triggers Yank, Delete, and ModeChanged)
    pub fn process_key(&mut self, key: char, ctrl: bool, cursor: Position) -> Vec<VimAction> {
        let mut actions = Vec::new();

        // Handle Escape - always returns to normal
        if key == '\x1b' {
            // ESC
            let prev_mode = self.mode;
            self.reset_to_normal();
            if prev_mode != VimMode::Normal {
                actions.push(VimAction::SelectionChanged(None));
                actions.push(VimAction::ModeChanged(VimMode::Normal));
            }
            actions.push(VimAction::Escape);
            return actions;
        }

        // Handle based on current mode
        match self.mode {
            VimMode::Normal => self.process_normal(key, ctrl, cursor, &mut actions),
            VimMode::Visual | VimMode::VisualBlock => {
                self.process_visual(key, ctrl, cursor, &mut actions)
            }
            VimMode::OperatorPending => {
                self.process_operator_pending(key, ctrl, cursor, &mut actions)
            }
        }

        actions
    }

    // ========================================================================
    // Mode-specific processing
    // ========================================================================

    fn process_normal(
        &mut self,
        key: char,
        ctrl: bool,
        cursor: Position,
        actions: &mut Vec<VimAction>,
    ) {
        match key {
            // Count accumulation
            '1'..='9' if self.count.is_none() => {
                self.count = Some((key as u8 - b'0') as usize);
            }
            '0'..='9' if self.count.is_some() => {
                let current = self.count.unwrap();
                self.count = Some(current * 10 + (key as u8 - b'0') as usize);
            }

            // Visual modes
            'v' if !ctrl => {
                self.mode = VimMode::Visual;
                self.visual_anchor = Some(cursor);
                actions.push(VimAction::ModeChanged(VimMode::Visual));
                actions.push(VimAction::SelectionChanged(self.get_selection(cursor)));
            }
            'v' if ctrl => {
                self.mode = VimMode::VisualBlock;
                self.visual_anchor = Some(cursor);
                actions.push(VimAction::ModeChanged(VimMode::VisualBlock));
                actions.push(VimAction::SelectionChanged(self.get_selection(cursor)));
            }

            // Operators (enter operator-pending mode)
            'd' if !ctrl => {
                self.operator = Some(Operator::Delete);
                self.mode = VimMode::OperatorPending;
                actions.push(VimAction::ModeChanged(VimMode::OperatorPending));
            }
            'y' if !ctrl => {
                self.operator = Some(Operator::Yank);
                self.mode = VimMode::OperatorPending;
                actions.push(VimAction::ModeChanged(VimMode::OperatorPending));
            }
            'c' => {
                self.operator = Some(Operator::Change);
                self.mode = VimMode::OperatorPending;
                actions.push(VimAction::ModeChanged(VimMode::OperatorPending));
            }

            // Paste
            'p' => {
                actions.push(VimAction::Paste);
                self.last_action = Some(RepeatableAction::Paste { before: false });
            }
            'P' => {
                actions.push(VimAction::PasteBefore);
                self.last_action = Some(RepeatableAction::Paste { before: true });
            }

            // Toggle (x in normal mode)
            'x' | '\r' => {
                // '\r' is Enter
                actions.push(VimAction::Toggle);
                self.last_action = Some(RepeatableAction::Toggle);
            }

            // Dot repeat
            '.' => {
                if let Some(ref action) = self.last_action.clone() {
                    match action {
                        RepeatableAction::Toggle => {
                            actions.push(VimAction::Toggle);
                        }
                        RepeatableAction::Paste { before } => {
                            if *before {
                                actions.push(VimAction::PasteBefore);
                            } else {
                                actions.push(VimAction::Paste);
                            }
                        }
                        RepeatableAction::OperatorMotion {
                            operator,
                            motion,
                            count,
                        } => {
                            // Replay the operator+motion
                            self.count = *count;
                            self.operator = Some(*operator);
                            self.mode = VimMode::OperatorPending;
                            // Process the motion
                            self.process_operator_pending(*motion, false, cursor, actions);
                        }
                    }
                }
            }

            // Motions (note: 'e' has guard to avoid conflict with Ctrl+e)
            'h' | 'j' | 'k' | 'l' | 'w' | 'b' => {
                if let Some(new_pos) = self.apply_motion(key, cursor) {
                    actions.push(VimAction::MoveCursor(new_pos));
                }
                self.count = None; // Reset count after motion
            }
            'e' if !ctrl => {
                if let Some(new_pos) = self.apply_motion(key, cursor) {
                    actions.push(VimAction::MoveCursor(new_pos));
                }
                self.count = None; // Reset count after motion
            }

            // Line start/end (zone-aware: goes to current zone start/end)
            '0' if self.count.is_none() => {
                let target_col = self
                    .grid_semantics
                    .as_ref()
                    .and_then(|gs| gs.get_zone_at_col(cursor.col))
                    .map(|z| z.start())
                    .unwrap_or(0);
                actions.push(VimAction::MoveCursor(Position::new(cursor.row, target_col)));
            }
            '$' => {
                let target_col = self
                    .grid_semantics
                    .as_ref()
                    .and_then(|gs| gs.get_zone_at_col(cursor.col))
                    .map(|z| z.end())
                    .unwrap_or_else(|| self.dimensions.cols.saturating_sub(1));
                actions.push(VimAction::MoveCursor(Position::new(cursor.row, target_col)));
            }

            // Top/bottom (jump movements - add to jumplist)
            'g' => {
                // gg - go to top (simplified, real vim waits for second g)
                self.jumplist.push(cursor); // Save current position before jumping
                actions.push(VimAction::MoveCursor(Position::new(0, cursor.col)));
            }
            'G' => {
                self.jumplist.push(cursor); // Save current position before jumping
                let last_row = self.dimensions.rows.saturating_sub(1);
                actions.push(VimAction::MoveCursor(Position::new(last_row, cursor.col)));
            }

            // Jumplist navigation (Ctrl+o / Ctrl+i)
            'o' if ctrl => {
                if let Some(pos) = self.jumplist.go_back(cursor) {
                    actions.push(VimAction::MoveCursor(pos));
                }
            }
            'i' if ctrl => {
                if let Some(pos) = self.jumplist.go_forward() {
                    actions.push(VimAction::MoveCursor(pos));
                }
            }

            // Half-page scroll (Ctrl+d / Ctrl+u)
            'd' if ctrl => {
                let half_page = self.dimensions.rows / 2;
                let new_row = (cursor.row + half_page).min(self.dimensions.rows.saturating_sub(1));
                actions.push(VimAction::MoveCursor(Position::new(new_row, cursor.col)));
            }
            'u' if ctrl => {
                let half_page = self.dimensions.rows / 2;
                let new_row = cursor.row.saturating_sub(half_page);
                actions.push(VimAction::MoveCursor(Position::new(new_row, cursor.col)));
            }

            // Single-line scroll (Ctrl+e / Ctrl+y)
            'e' if ctrl => {
                actions.push(VimAction::ScrollViewport(1));
            }
            'y' if ctrl => {
                actions.push(VimAction::ScrollViewport(-1));
            }

            _ => {}
        }
    }

    fn process_visual(
        &mut self,
        key: char,
        ctrl: bool,
        cursor: Position,
        actions: &mut Vec<VimAction>,
    ) {
        match key {
            // Switch between visual modes
            'v' if !ctrl && self.mode == VimMode::Visual => {
                self.reset_to_normal();
                actions.push(VimAction::SelectionChanged(None));
                actions.push(VimAction::ModeChanged(VimMode::Normal));
            }
            'v' if !ctrl && self.mode == VimMode::VisualBlock => {
                self.mode = VimMode::Visual;
                actions.push(VimAction::ModeChanged(VimMode::Visual));
                actions.push(VimAction::SelectionChanged(self.get_selection(cursor)));
            }
            'v' if ctrl => {
                if self.mode == VimMode::VisualBlock {
                    self.reset_to_normal();
                    actions.push(VimAction::SelectionChanged(None));
                    actions.push(VimAction::ModeChanged(VimMode::Normal));
                } else {
                    self.mode = VimMode::VisualBlock;
                    actions.push(VimAction::ModeChanged(VimMode::VisualBlock));
                    actions.push(VimAction::SelectionChanged(self.get_selection(cursor)));
                }
            }

            // Operators on selection
            'y' if !ctrl => {
                if let Some(range) = self.get_selection(cursor) {
                    actions.push(VimAction::Yank(range));
                }
                self.reset_to_normal();
                actions.push(VimAction::SelectionChanged(None));
                actions.push(VimAction::ModeChanged(VimMode::Normal));
            }
            'd' if !ctrl => {
                if let Some(range) = self.get_selection(cursor) {
                    actions.push(VimAction::Yank(range)); // Yank before delete
                    actions.push(VimAction::Delete(range));
                }
                self.reset_to_normal();
                actions.push(VimAction::SelectionChanged(None));
                actions.push(VimAction::ModeChanged(VimMode::Normal));
            }
            'x' => {
                if let Some(range) = self.get_selection(cursor) {
                    actions.push(VimAction::Yank(range)); // Yank before delete
                    actions.push(VimAction::Delete(range));
                }
                self.reset_to_normal();
                actions.push(VimAction::SelectionChanged(None));
                actions.push(VimAction::ModeChanged(VimMode::Normal));
            }

            // Half-page scroll (Ctrl+d / Ctrl+u) in visual mode
            'd' if ctrl => {
                let half_page = self.dimensions.rows / 2;
                let new_row = (cursor.row + half_page).min(self.dimensions.rows.saturating_sub(1));
                let new_pos = Position::new(new_row, cursor.col);
                actions.push(VimAction::MoveCursor(new_pos));
                actions.push(VimAction::SelectionChanged(self.get_selection(new_pos)));
            }
            'u' if ctrl => {
                let half_page = self.dimensions.rows / 2;
                let new_row = cursor.row.saturating_sub(half_page);
                let new_pos = Position::new(new_row, cursor.col);
                actions.push(VimAction::MoveCursor(new_pos));
                actions.push(VimAction::SelectionChanged(self.get_selection(new_pos)));
            }

            // Single-line scroll (Ctrl+e / Ctrl+y) in visual mode
            'e' if ctrl => {
                actions.push(VimAction::ScrollViewport(1));
                actions.push(VimAction::SelectionChanged(self.get_selection(cursor)));
            }
            'y' if ctrl => {
                actions.push(VimAction::ScrollViewport(-1));
                actions.push(VimAction::SelectionChanged(self.get_selection(cursor)));
            }

            // Motions (extend selection)
            'h' | 'j' | 'k' | 'l' | 'w' | 'b' | 'e' => {
                if let Some(new_pos) = self.apply_motion(key, cursor) {
                    actions.push(VimAction::MoveCursor(new_pos));
                    // Selection will be recalculated with new cursor
                    actions.push(VimAction::SelectionChanged(self.get_selection(new_pos)));
                }
            }

            '0' => {
                let target_col = self
                    .grid_semantics
                    .as_ref()
                    .and_then(|gs| gs.get_zone_at_col(cursor.col))
                    .map(|z| z.start())
                    .unwrap_or(0);
                let new_pos = Position::new(cursor.row, target_col);
                actions.push(VimAction::MoveCursor(new_pos));
                actions.push(VimAction::SelectionChanged(self.get_selection(new_pos)));
            }
            '$' => {
                let target_col = self
                    .grid_semantics
                    .as_ref()
                    .and_then(|gs| gs.get_zone_at_col(cursor.col))
                    .map(|z| z.end())
                    .unwrap_or_else(|| self.dimensions.cols.saturating_sub(1));
                let new_pos = Position::new(cursor.row, target_col);
                actions.push(VimAction::MoveCursor(new_pos));
                actions.push(VimAction::SelectionChanged(self.get_selection(new_pos)));
            }
            'g' => {
                let new_pos = Position::new(0, cursor.col);
                actions.push(VimAction::MoveCursor(new_pos));
                actions.push(VimAction::SelectionChanged(self.get_selection(new_pos)));
            }
            'G' => {
                let last_row = self.dimensions.rows.saturating_sub(1);
                let new_pos = Position::new(last_row, cursor.col);
                actions.push(VimAction::MoveCursor(new_pos));
                actions.push(VimAction::SelectionChanged(self.get_selection(new_pos)));
            }

            _ => {}
        }
    }

    fn process_operator_pending(
        &mut self,
        key: char,
        _ctrl: bool,
        cursor: Position,
        actions: &mut Vec<VimAction>,
    ) {
        let operator = self.operator;

        // Handle count accumulation in operator-pending mode
        match key {
            '1'..='9' if self.count.is_none() => {
                self.count = Some((key as u8 - b'0') as usize);
                return; // Stay in operator-pending mode
            }
            '0'..='9' if self.count.is_some() => {
                let current = self.count.unwrap();
                self.count = Some(current * 10 + (key as u8 - b'0') as usize);
                return; // Stay in operator-pending mode
            }
            _ => {}
        }

        // Handle operator switching (d then y switches to yank)
        let key_operator = match key {
            'd' => Some(Operator::Delete),
            'y' => Some(Operator::Yank),
            'c' => Some(Operator::Change),
            _ => None,
        };

        // Check for line-wise operation (dd, yy, cc) or operator switch
        if let Some(new_op) = key_operator {
            if operator == Some(new_op) {
                // Same operator twice = line-wise operation (dd, yy, cc)
                let count = self.count.unwrap_or(1);
                let end_row = (cursor.row + count - 1).min(self.dimensions.rows.saturating_sub(1));
                let range = Range {
                    start: Position::new(cursor.row, 0),
                    end: Position::new(end_row, self.dimensions.cols.saturating_sub(1)),
                    range_type: RangeType::Line,
                };
                match operator {
                    Some(Operator::Yank) => actions.push(VimAction::Yank(range)),
                    Some(Operator::Delete) | Some(Operator::Change) => {
                        actions.push(VimAction::Yank(range));
                        actions.push(VimAction::Delete(range));
                        // Record for dot repeat (dd, cc)
                        self.last_action = Some(RepeatableAction::OperatorMotion {
                            operator: operator.unwrap(),
                            motion: key,
                            count: Some(count),
                        });
                    }
                    None => {}
                }
                self.reset_to_normal();
                actions.push(VimAction::ModeChanged(VimMode::Normal));
                return;
            } else {
                // Different operator - switch to the new one
                self.operator = Some(new_op);
                return; // Stay in operator-pending mode
            }
        }

        // Motion creates a range from cursor to motion target
        let motion_target = match key {
            'h' | 'j' | 'k' | 'l' | 'w' | 'b' | 'e' => self.apply_motion(key, cursor),
            '0' => Some(Position::new(cursor.row, 0)),
            '$' => Some(Position::new(
                cursor.row,
                self.dimensions.cols.saturating_sub(1),
            )),
            'g' => Some(Position::new(0, cursor.col)),
            'G' => Some(Position::new(
                self.dimensions.rows.saturating_sub(1),
                cursor.col,
            )),
            _ => None,
        };

        if let Some(target) = motion_target {
            // Determine if motion is exclusive or inclusive
            // In vim: l, w, b, h are exclusive (don't include target)
            // e is inclusive (includes target)
            // For exclusive forward motions, we delete up to but not including target
            let is_exclusive_forward = matches!(key, 'l' | 'w');
            let is_exclusive_backward = matches!(key, 'h' | 'b');

            let adjusted_end = if is_exclusive_forward && target.col > cursor.col {
                // For exclusive forward motions, end is one before target
                Position::new(target.row, target.col.saturating_sub(1))
            } else if is_exclusive_backward && target.col < cursor.col {
                // For exclusive backward motions, start from target (not including cursor)
                // The range will be from target to cursor-1
                target
            } else {
                target
            };

            let (range_start, range_end) = if is_exclusive_backward && target.col < cursor.col {
                // For backward exclusive motions, swap and adjust
                (
                    adjusted_end,
                    Position::new(cursor.row, cursor.col.saturating_sub(1)),
                )
            } else {
                (cursor, adjusted_end)
            };

            let range = Range {
                start: range_start,
                end: range_end,
                range_type: if key == 'j' || key == 'k' {
                    RangeType::Line
                } else {
                    RangeType::Char
                },
            };

            match operator {
                Some(Operator::Yank) => actions.push(VimAction::Yank(range)),
                Some(Operator::Delete) | Some(Operator::Change) => {
                    actions.push(VimAction::Yank(range));
                    actions.push(VimAction::Delete(range));
                    // Record for dot repeat (dl, dw, etc.)
                    self.last_action = Some(RepeatableAction::OperatorMotion {
                        operator: operator.unwrap(),
                        motion: key,
                        count: self.count,
                    });
                }
                None => {}
            }
        }

        self.reset_to_normal();
        actions.push(VimAction::ModeChanged(VimMode::Normal));
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    fn apply_motion(&self, key: char, cursor: Position) -> Option<Position> {
        let count = self.count.unwrap_or(1);
        let max_row = self.dimensions.rows.saturating_sub(1);
        let max_col = self.dimensions.cols.saturating_sub(1);

        match key {
            'h' => {
                // Zone-aware left motion: cross zone boundaries
                let mut col = cursor.col;
                for _ in 0..count {
                    if let Some(ref gs) = self.grid_semantics {
                        if let Some(zone) = gs.get_zone_at_col(col) {
                            if col > zone.start() {
                                // Within zone: move left
                                col -= 1;
                            } else if let Some(prev_zone) = gs.get_prev_zone(col) {
                                // At zone boundary: enter previous zone at its end
                                col = prev_zone.end();
                            }
                            // Else: at leftmost zone boundary, stay
                        } else {
                            col = col.saturating_sub(1);
                        }
                    } else {
                        col = col.saturating_sub(1);
                    }
                }
                Some(Position::new(cursor.row, col))
            }
            'l' => {
                // Zone-aware right motion: cross zone boundaries
                let mut col = cursor.col;
                for _ in 0..count {
                    if let Some(ref gs) = self.grid_semantics {
                        if let Some(zone) = gs.get_zone_at_col(col) {
                            if col < zone.end() {
                                // Within zone: move right
                                col += 1;
                            } else if let Some(next_zone) = gs.get_next_zone(col) {
                                // At zone boundary: enter next zone at its start
                                col = next_zone.start();
                            }
                            // Else: at rightmost zone boundary, stay
                        } else {
                            col = (col + 1).min(max_col);
                        }
                    } else {
                        col = (col + 1).min(max_col);
                    }
                }
                Some(Position::new(cursor.row, col))
            }
            'k' => Some(Position::new(cursor.row.saturating_sub(count), cursor.col)),
            'j' => Some(Position::new((cursor.row + count).min(max_row), cursor.col)),
            // Word motions - zone-aware with word intervals
            'w' => {
                let mut new_col = cursor.col;
                for _ in 0..count {
                    if let Some(ref gs) = self.grid_semantics {
                        if let Some(zone) = gs.get_zone_at_col(new_col) {
                            let interval = zone.word_interval.unwrap_or(4);
                            let zone_offset = new_col - zone.start();
                            // Find next word boundary within zone
                            let next_boundary = ((zone_offset / interval) + 1) * interval;
                            let next_col = zone.start() + next_boundary;
                            new_col = next_col.min(zone.end());
                        } else {
                            // Fallback: beat boundaries
                            let next_beat = ((new_col / 4) + 1) * 4;
                            new_col = next_beat.min(max_col);
                        }
                    } else {
                        // No zones: beat boundaries (every 4 steps)
                        let next_beat = ((new_col / 4) + 1) * 4;
                        new_col = next_beat.min(max_col);
                    }
                }
                Some(Position::new(cursor.row, new_col))
            }
            'b' => {
                let mut new_col = cursor.col;
                for _ in 0..count {
                    if new_col == 0 {
                        break;
                    }
                    if let Some(ref gs) = self.grid_semantics {
                        if let Some(zone) = gs.get_zone_at_col(new_col) {
                            let interval = zone.word_interval.unwrap_or(4);
                            let zone_offset = new_col - zone.start();
                            if zone_offset == 0 {
                                // At zone start, stay
                                break;
                            }
                            let current_boundary = zone_offset / interval;
                            if zone_offset.is_multiple_of(interval) && current_boundary > 0 {
                                // At a word boundary, go to previous one
                                new_col = zone.start() + (current_boundary - 1) * interval;
                            } else {
                                // Go to start of current word
                                new_col = zone.start() + current_boundary * interval;
                            }
                        } else {
                            // Fallback
                            let current_beat = new_col / 4;
                            if new_col.is_multiple_of(4) && current_beat > 0 {
                                new_col = (current_beat - 1) * 4;
                            } else {
                                new_col = current_beat * 4;
                            }
                        }
                    } else {
                        // No zones
                        let current_beat = new_col / 4;
                        if new_col.is_multiple_of(4) && current_beat > 0 {
                            new_col = (current_beat - 1) * 4;
                        } else {
                            new_col = current_beat * 4;
                        }
                    }
                }
                Some(Position::new(cursor.row, new_col))
            }
            'e' => {
                let mut new_col = cursor.col;
                for _ in 0..count {
                    if let Some(ref gs) = self.grid_semantics {
                        if let Some(zone) = gs.get_zone_at_col(new_col) {
                            let interval = zone.word_interval.unwrap_or(4);
                            let zone_offset = new_col - zone.start();
                            let current_word = zone_offset / interval;
                            let word_end =
                                zone.start() + ((current_word + 1) * interval).saturating_sub(1);
                            if new_col >= word_end.min(zone.end()) {
                                // Already at end, go to next word end
                                let next_word_end = zone.start()
                                    + ((current_word + 2) * interval).saturating_sub(1);
                                new_col = next_word_end.min(zone.end());
                            } else {
                                new_col = word_end.min(zone.end());
                            }
                        } else {
                            // Fallback
                            let current_beat = new_col / 4;
                            let end_of_beat = ((current_beat + 1) * 4).saturating_sub(1);
                            if new_col >= end_of_beat {
                                new_col = ((current_beat + 2) * 4).saturating_sub(1).min(max_col);
                            } else {
                                new_col = end_of_beat.min(max_col);
                            }
                        }
                    } else {
                        // No zones
                        let current_beat = new_col / 4;
                        let end_of_beat = ((current_beat + 1) * 4).saturating_sub(1);
                        if new_col >= end_of_beat {
                            new_col = ((current_beat + 2) * 4).saturating_sub(1).min(max_col);
                        } else {
                            new_col = end_of_beat.min(max_col);
                        }
                    }
                }
                Some(Position::new(cursor.row, new_col))
            }
            _ => None,
        }
    }

    fn reset_to_normal(&mut self) {
        self.mode = VimMode::Normal;
        self.operator = None;
        self.visual_anchor = None;
        self.count = None;
    }
}

// ============================================================================
// Tests - Comprehensive test suite in vim_tests.rs
// ============================================================================

#[cfg(test)]
#[path = "vim_tests.rs"]
mod tests;
