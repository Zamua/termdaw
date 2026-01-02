//! Comprehensive vim test suite - matches TypeScript test coverage
//!
//! Test categories:
//! 1. State machine (mode transitions, context)
//! 2. Motions (h/j/k/l/w/b/e/0/$/gg/G)
//! 3. Operators (d/y/c with motions)
//! 4. Visual modes (v, Ctrl+v)
//! 5. Count handling
//! 6. Registers
//! 7. Jumplist
//! 8. Dot repeat
//! 9. Edge cases

use super::*;

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a VimState with default 8x16 grid (like channel rack)
fn create_vim() -> VimState {
    VimState::new(8, 16)
}

/// Process a key with ctrl modifier
fn process_ctrl_key(vim: &mut VimState, key: char, cursor: Position) -> Vec<VimAction> {
    vim.process_key(key, true, cursor)
}

/// Check if actions contain a specific action type
fn has_action(actions: &[VimAction], check: impl Fn(&VimAction) -> bool) -> bool {
    actions.iter().any(check)
}

/// Get the MoveCursor position from actions
fn get_cursor_move(actions: &[VimAction]) -> Option<Position> {
    actions.iter().find_map(|a| {
        if let VimAction::MoveCursor(pos) = a {
            Some(*pos)
        } else {
            None
        }
    })
}

/// Get Yank range from actions
fn get_yank_range(actions: &[VimAction]) -> Option<Range> {
    actions.iter().find_map(|a| {
        if let VimAction::Yank(range) = a {
            Some(*range)
        } else {
            None
        }
    })
}

/// Get Delete range from actions
fn get_delete_range(actions: &[VimAction]) -> Option<Range> {
    actions.iter().find_map(|a| {
        if let VimAction::Delete(range) = a {
            Some(*range)
        } else {
            None
        }
    })
}

// ============================================================================
// 1. INITIAL STATE TESTS
// ============================================================================

#[test]
fn test_initial_state_normal_mode() {
    let vim = create_vim();
    assert_eq!(vim.mode(), VimMode::Normal);
}

#[test]
fn test_initial_state_count_none() {
    let vim = create_vim();
    assert_eq!(vim.count, None);
}

#[test]
fn test_initial_state_operator_none() {
    let vim = create_vim();
    assert_eq!(vim.operator, None);
}

#[test]
fn test_initial_state_visual_anchor_none() {
    let vim = create_vim();
    assert_eq!(vim.visual_anchor, None);
}

// ============================================================================
// 2. MODE TRANSITIONS FROM NORMAL
// ============================================================================

#[test]
fn test_d_transitions_to_operator_pending() {
    let mut vim = create_vim();
    let cursor = Position::new(0, 0);
    vim.process_key('d', false, cursor);
    assert_eq!(vim.mode(), VimMode::OperatorPending);
    assert_eq!(vim.operator, Some(Operator::Delete));
}

#[test]
fn test_y_transitions_to_operator_pending() {
    let mut vim = create_vim();
    let cursor = Position::new(0, 0);
    vim.process_key('y', false, cursor);
    assert_eq!(vim.mode(), VimMode::OperatorPending);
    assert_eq!(vim.operator, Some(Operator::Yank));
}

#[test]
fn test_c_transitions_to_operator_pending() {
    let mut vim = create_vim();
    let cursor = Position::new(0, 0);
    vim.process_key('c', false, cursor);
    assert_eq!(vim.mode(), VimMode::OperatorPending);
    assert_eq!(vim.operator, Some(Operator::Change));
}

#[test]
fn test_v_transitions_to_visual() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('v', false, cursor);

    assert_eq!(vim.mode(), VimMode::Visual);
    assert_eq!(vim.visual_anchor, Some(cursor));
    assert!(has_action(&actions, |a| matches!(
        a,
        VimAction::ModeChanged(VimMode::Visual)
    )));
}

#[test]
fn test_ctrl_v_transitions_to_visual_block() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = process_ctrl_key(&mut vim, 'v', cursor);

    assert_eq!(vim.mode(), VimMode::VisualBlock);
    assert_eq!(vim.visual_anchor, Some(cursor));
    assert!(has_action(&actions, |a| matches!(
        a,
        VimAction::ModeChanged(VimMode::VisualBlock)
    )));
}

#[test]
fn test_motion_stays_in_normal() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('j', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
}

#[test]
fn test_escape_in_normal_stays_normal() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('\x1b', false, cursor); // ESC
    assert_eq!(vim.mode(), VimMode::Normal);
}

// ============================================================================
// 3. MODE TRANSITIONS FROM OPERATOR-PENDING
// ============================================================================

#[test]
fn test_motion_after_operator_returns_to_normal() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('d', false, cursor);
    assert_eq!(vim.mode(), VimMode::OperatorPending);

    vim.process_key('l', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
}

#[test]
fn test_dd_returns_to_normal() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('d', false, cursor);
    vim.process_key('d', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
}

#[test]
fn test_yy_returns_to_normal() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('y', false, cursor);
    vim.process_key('y', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
}

#[test]
fn test_cc_returns_to_normal() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('c', false, cursor);
    vim.process_key('c', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
}

#[test]
fn test_different_operator_switches_operator() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('d', false, cursor);
    assert_eq!(vim.operator, Some(Operator::Delete));

    vim.process_key('y', false, cursor);
    // Should switch to yank operator
    assert_eq!(vim.operator, Some(Operator::Yank));
}

#[test]
fn test_escape_cancels_operator() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('d', false, cursor);
    assert_eq!(vim.mode(), VimMode::OperatorPending);

    vim.process_key('\x1b', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
    assert_eq!(vim.operator, None);
}

// ============================================================================
// 4. MODE TRANSITIONS FROM VISUAL
// ============================================================================

#[test]
fn test_v_again_exits_visual() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('v', false, cursor);
    assert_eq!(vim.mode(), VimMode::Visual);

    vim.process_key('v', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
    assert_eq!(vim.visual_anchor, None);
}

#[test]
fn test_ctrl_v_in_visual_switches_to_visual_block() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('v', false, cursor);
    assert_eq!(vim.mode(), VimMode::Visual);

    process_ctrl_key(&mut vim, 'v', cursor);
    assert_eq!(vim.mode(), VimMode::VisualBlock);
}

#[test]
fn test_escape_exits_visual() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('v', false, cursor);
    assert_eq!(vim.mode(), VimMode::Visual);

    vim.process_key('\x1b', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
    assert_eq!(vim.visual_anchor, None);
}

#[test]
fn test_motion_in_visual_stays_visual() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('v', false, cursor);
    vim.process_key('j', false, cursor);
    assert_eq!(vim.mode(), VimMode::Visual);
}

#[test]
fn test_y_in_visual_exits_to_normal() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('v', false, cursor);
    vim.process_key('y', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
}

#[test]
fn test_d_in_visual_exits_to_normal() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('v', false, cursor);
    vim.process_key('d', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
}

// ============================================================================
// 5. MODE TRANSITIONS FROM VISUAL-BLOCK
// ============================================================================

#[test]
fn test_ctrl_v_again_exits_visual_block() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    process_ctrl_key(&mut vim, 'v', cursor);
    assert_eq!(vim.mode(), VimMode::VisualBlock);

    process_ctrl_key(&mut vim, 'v', cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
}

#[test]
fn test_v_in_visual_block_switches_to_visual() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    process_ctrl_key(&mut vim, 'v', cursor);
    assert_eq!(vim.mode(), VimMode::VisualBlock);

    vim.process_key('v', false, cursor);
    assert_eq!(vim.mode(), VimMode::Visual);
}

// ============================================================================
// 6. COUNT ACCUMULATION
// ============================================================================

#[test]
fn test_single_digit_count() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('5', false, cursor);
    assert_eq!(vim.count, Some(5));
}

#[test]
fn test_multiple_digits_accumulate() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('1', false, cursor);
    vim.process_key('2', false, cursor);
    vim.process_key('3', false, cursor);
    assert_eq!(vim.count, Some(123));
}

#[test]
fn test_count_preserved_across_operator() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('5', false, cursor);
    vim.process_key('d', false, cursor);
    assert_eq!(vim.count, Some(5));
    assert_eq!(vim.mode(), VimMode::OperatorPending);
}

#[test]
fn test_count_in_operator_pending() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('d', false, cursor);
    vim.process_key('3', false, cursor);
    assert_eq!(vim.count, Some(3));
}

#[test]
fn test_count_resets_after_motion() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('5', false, cursor);
    vim.process_key('j', false, cursor);
    assert_eq!(vim.count, None);
}

#[test]
fn test_count_resets_after_escape() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('5', false, cursor);
    vim.process_key('\x1b', false, cursor);
    assert_eq!(vim.count, None);
}

#[test]
fn test_zero_at_start_is_motion_not_count() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('0', false, cursor);

    // Should move to column 0, not set count
    assert!(has_action(
        &actions,
        |a| matches!(a, VimAction::MoveCursor(pos) if pos.col == 0)
    ));
    assert_eq!(vim.count, None);
}

#[test]
fn test_zero_after_digit_is_count() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('1', false, cursor);
    vim.process_key('0', false, cursor);
    assert_eq!(vim.count, Some(10));
}

#[test]
fn test_large_count_accumulates() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('9', false, cursor);
    vim.process_key('9', false, cursor);
    vim.process_key('9', false, cursor);
    assert_eq!(vim.count, Some(999));
}

// ============================================================================
// 7. BASIC MOTIONS
// ============================================================================

#[test]
fn test_h_moves_left() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('h', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(2, 4)));
}

#[test]
fn test_l_moves_right() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('l', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(2, 6)));
}

#[test]
fn test_j_moves_down() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('j', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(3, 5)));
}

#[test]
fn test_k_moves_up() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('k', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(1, 5)));
}

#[test]
fn test_h_with_count() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 10);
    vim.process_key('3', false, cursor);
    let actions = vim.process_key('h', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(2, 7)));
}

#[test]
fn test_j_with_count() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('5', false, cursor);
    let actions = vim.process_key('j', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(7, 5)));
}

#[test]
fn test_l_with_count() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('2', false, cursor);
    let actions = vim.process_key('l', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(2, 7)));
}

#[test]
fn test_l_with_count_in_zone() {
    let mut vim = create_channel_rack_vim();
    // At step 3 (vim col 5), 2l should move to step 5 (vim col 7)
    let cursor = Position::new(0, 5);
    vim.process_key('2', false, cursor);
    let actions = vim.process_key('l', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 7)));
}

#[test]
fn test_h_stops_at_column_zero() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 2);
    vim.process_key('1', false, cursor);
    vim.process_key('0', false, cursor); // count = 10
    let actions = vim.process_key('h', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(2, 0)));
}

#[test]
fn test_j_stops_at_last_row() {
    let mut vim = create_vim();
    let cursor = Position::new(5, 5);
    vim.process_key('1', false, cursor);
    vim.process_key('0', false, cursor); // count = 10
    let actions = vim.process_key('j', false, cursor);
    // 8 rows, last row is 7
    assert_eq!(get_cursor_move(&actions), Some(Position::new(7, 5)));
}

#[test]
fn test_zero_goes_to_first_column() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 10);
    let actions = vim.process_key('0', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(2, 0)));
}

#[test]
fn test_dollar_goes_to_last_column() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('$', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(2, 15)));
}

#[test]
fn test_g_goes_to_first_row() {
    let mut vim = create_vim();
    let cursor = Position::new(5, 5);
    let actions = vim.process_key('g', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 5)));
}

#[test]
fn test_shift_g_goes_to_last_row() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('G', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(7, 5)));
}

// ============================================================================
// 8. WORD MOTIONS (w/b/e) - "words" are beat boundaries (every 4 steps)
// ============================================================================

#[test]
fn test_w_moves_to_next_beat() {
    let mut vim = create_vim();
    // Starting at col 0, w should move to col 4 (next beat)
    let cursor = Position::new(0, 0);
    let actions = vim.process_key('w', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 4)));
}

#[test]
fn test_w_from_mid_beat_moves_to_next_beat() {
    let mut vim = create_vim();
    // Starting at col 2, w should move to col 4
    let cursor = Position::new(0, 2);
    let actions = vim.process_key('w', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 4)));
}

#[test]
fn test_b_moves_to_previous_beat() {
    let mut vim = create_vim();
    // Starting at col 8, b should move to col 4
    let cursor = Position::new(0, 8);
    let actions = vim.process_key('b', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 4)));
}

#[test]
fn test_b_from_mid_beat_moves_to_beat_start() {
    let mut vim = create_vim();
    // Starting at col 6, b should move to col 4 (start of current beat)
    let cursor = Position::new(0, 6);
    let actions = vim.process_key('b', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 4)));
}

#[test]
fn test_e_moves_to_end_of_beat() {
    let mut vim = create_vim();
    // Starting at col 0, e should move to col 3 (end of first beat)
    let cursor = Position::new(0, 0);
    let actions = vim.process_key('e', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 3)));
}

#[test]
fn test_e_at_end_of_beat_moves_to_next_end() {
    let mut vim = create_vim();
    // Starting at col 3, e should move to col 7 (end of next beat)
    let cursor = Position::new(0, 3);
    let actions = vim.process_key('e', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 7)));
}

#[test]
fn test_2w_moves_two_beats() {
    let mut vim = create_vim();
    let cursor = Position::new(0, 0);
    vim.process_key('2', false, cursor);
    let actions = vim.process_key('w', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 8)));
}

// ============================================================================
// 9. OPERATORS WITH MOTIONS
// ============================================================================

#[test]
fn test_dl_deletes_char_under_cursor() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('d', false, cursor);
    let actions = vim.process_key('l', false, cursor);

    let range = get_delete_range(&actions).expect("Should have delete action");
    // dl in vim deletes only the char under cursor (exclusive motion)
    assert_eq!(range.start, cursor);
    assert_eq!(range.end.col, 5); // Same as start - only cursor position
}

#[test]
fn test_dh_deletes_char_to_left() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('d', false, cursor);
    let actions = vim.process_key('h', false, cursor);

    let range = get_delete_range(&actions).expect("Should have delete action");
    // dh in vim deletes the char to the left of cursor (exclusive motion)
    let (start, end) = range.normalized();
    assert_eq!(start.col, 4); // One left of cursor
    assert_eq!(end.col, 4); // Same - only one char
}

#[test]
fn test_dd_deletes_line() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('d', false, cursor);
    let actions = vim.process_key('d', false, cursor);

    let range = get_delete_range(&actions).expect("Should have delete action");
    assert_eq!(range.range_type, RangeType::Line);
    assert_eq!(range.start.row, 2);
    assert_eq!(range.end.row, 2);
}

#[test]
fn test_yy_yanks_line() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('y', false, cursor);
    let actions = vim.process_key('y', false, cursor);

    let range = get_yank_range(&actions).expect("Should have yank action");
    assert_eq!(range.range_type, RangeType::Line);
}

#[test]
fn test_yl_yanks_without_delete() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('y', false, cursor);
    let actions = vim.process_key('l', false, cursor);

    assert!(get_yank_range(&actions).is_some());
    assert!(get_delete_range(&actions).is_none());
}

#[test]
fn test_dj_deletes_linewise() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('d', false, cursor);
    let actions = vim.process_key('j', false, cursor);

    let range = get_delete_range(&actions).expect("Should have delete action");
    assert_eq!(range.range_type, RangeType::Line);
    // Should span rows 2 and 3
    let (start, end) = range.normalized();
    assert!(start.row <= 2);
    assert!(end.row >= 3);
}

#[test]
fn test_d_dollar_deletes_to_end() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('d', false, cursor);
    let actions = vim.process_key('$', false, cursor);

    let range = get_delete_range(&actions).expect("Should have delete action");
    assert_eq!(range.end.col, 15);
}

#[test]
fn test_d0_deletes_to_start() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('d', false, cursor);
    let actions = vim.process_key('0', false, cursor);

    let range = get_delete_range(&actions).expect("Should have delete action");
    let (start, _) = range.normalized();
    assert_eq!(start.col, 0);
}

#[test]
fn test_3dd_deletes_three_lines() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('3', false, cursor);
    vim.process_key('d', false, cursor);
    let actions = vim.process_key('d', false, cursor);

    let range = get_delete_range(&actions).expect("Should have delete action");
    assert_eq!(range.range_type, RangeType::Line);
    // Should span 3 rows
    let (start, end) = range.normalized();
    assert_eq!(end.row - start.row + 1, 3);
}

// ============================================================================
// 10. VISUAL MODE OPERATIONS
// ============================================================================

#[test]
fn test_visual_mode_has_selection() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('v', false, cursor);

    let selection = vim.get_selection(cursor);
    assert!(selection.is_some());
}

#[test]
fn test_visual_selection_expands_with_motion() {
    let mut vim = create_vim();
    let start_cursor = Position::new(2, 5);
    vim.process_key('v', false, start_cursor);

    let end_cursor = Position::new(2, 10);
    let selection = vim.get_selection(end_cursor);
    assert!(selection.is_some());

    let range = selection.unwrap();
    assert_eq!(range.start, start_cursor);
    assert_eq!(range.end, end_cursor);
}

#[test]
fn test_visual_d_deletes_selection() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('v', false, cursor);

    let end_cursor = Position::new(2, 10);
    let actions = vim.process_key('d', false, end_cursor);

    assert!(get_delete_range(&actions).is_some());
    assert_eq!(vim.mode(), VimMode::Normal);
}

#[test]
fn test_visual_y_yanks_selection() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('v', false, cursor);

    let end_cursor = Position::new(2, 10);
    let actions = vim.process_key('y', false, end_cursor);

    assert!(get_yank_range(&actions).is_some());
    assert!(get_delete_range(&actions).is_none());
    assert_eq!(vim.mode(), VimMode::Normal);
}

#[test]
fn test_visual_block_has_block_type() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    process_ctrl_key(&mut vim, 'v', cursor);

    let end_cursor = Position::new(4, 10);
    let selection = vim.get_selection(end_cursor);

    assert!(selection.is_some());
    assert_eq!(selection.unwrap().range_type, RangeType::Block);
}

#[test]
fn test_visual_has_char_type() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('v', false, cursor);

    let selection = vim.get_selection(cursor);
    assert!(selection.is_some());
    assert_eq!(selection.unwrap().range_type, RangeType::Char);
}

// ============================================================================
// 11. PASTE OPERATIONS
// ============================================================================

#[test]
fn test_p_emits_paste_action() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('p', false, cursor);
    assert!(has_action(&actions, |a| matches!(a, VimAction::Paste)));
}

#[test]
fn test_yank_then_paste() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);

    // Yank something
    vim.process_key('y', false, cursor);
    vim.process_key('l', false, cursor);

    // Should be able to paste
    let actions = vim.process_key('p', false, cursor);
    assert!(has_action(&actions, |a| matches!(a, VimAction::Paste)));
}

// ============================================================================
// 12. REGISTER OPERATIONS
// ============================================================================

#[test]
fn test_yank_stores_in_register_0() {
    let mut vim = create_vim();

    // Simulate a yank by storing data directly
    vim.store_yank(vec![vec![true, false]], RangeType::Char);

    // Register 0 should have the yank
    assert!(vim.get_register_0().is_some());
    let reg = vim.get_register_0().unwrap();
    assert_eq!(reg.data, vec![vec![true, false]]);

    // Default register should also have it
    assert!(vim.get_register().is_some());
}

#[test]
fn test_delete_stores_in_register_1() {
    let mut vim = create_vim();

    // Simulate a delete by storing data directly
    vim.store_delete(vec![vec![true, true, false]], RangeType::Char);

    // Register 1 should have the delete
    assert!(vim.get_register_numbered(1).is_some());
    let reg = vim.get_register_numbered(1).unwrap();
    assert_eq!(reg.data, vec![vec![true, true, false]]);

    // Default register should also have it
    assert!(vim.get_register().is_some());

    // Register 0 should NOT have it (that's for yanks only)
    assert!(vim.get_register_0().is_none());
}

#[test]
fn test_delete_shifts_history() {
    let mut vim = create_vim();

    // Do 3 deletes
    vim.store_delete(vec![vec![true]], RangeType::Char); // First delete
    vim.store_delete(vec![vec![false]], RangeType::Char); // Second delete
    vim.store_delete(vec![vec![true, true]], RangeType::Char); // Third delete

    // Register 1 should have the most recent (third) delete
    let reg1 = vim.get_register_numbered(1).unwrap();
    assert_eq!(reg1.data, vec![vec![true, true]]);

    // Register 2 should have the second delete
    let reg2 = vim.get_register_numbered(2).unwrap();
    assert_eq!(reg2.data, vec![vec![false]]);

    // Register 3 should have the first delete
    let reg3 = vim.get_register_numbered(3).unwrap();
    assert_eq!(reg3.data, vec![vec![true]]);

    // Register 4 should be empty
    assert!(vim.get_register_numbered(4).is_none());
}

#[test]
fn test_yank_does_not_affect_delete_history() {
    let mut vim = create_vim();

    // Delete something
    vim.store_delete(vec![vec![true]], RangeType::Char);

    // Yank something
    vim.store_yank(vec![vec![false, false]], RangeType::Char);

    // Register 1 should still have the delete
    let reg1 = vim.get_register_numbered(1).unwrap();
    assert_eq!(reg1.data, vec![vec![true]]);

    // Register 0 should have the yank
    let reg0 = vim.get_register_0().unwrap();
    assert_eq!(reg0.data, vec![vec![false, false]]);

    // Default register should have the yank (most recent operation)
    let unnamed = vim.get_register().unwrap();
    assert_eq!(unnamed.data, vec![vec![false, false]]);
}

#[test]
fn test_shift_p_pastes_before_cursor() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('P', false, cursor);
    assert!(has_action(&actions, |a| matches!(
        a,
        VimAction::PasteBefore
    )));
}

// ============================================================================
// 13. JUMPLIST
// ============================================================================

#[test]
fn test_gg_adds_to_jumplist() {
    let mut vim = create_vim();
    let cursor = Position::new(5, 5);
    let actions = vim.process_key('g', false, cursor);

    // Should move to row 0
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 5)));

    // Now Ctrl+o should jump back to (5, 5)
    let actions = vim.process_key('o', true, Position::new(0, 5));
    assert_eq!(get_cursor_move(&actions), Some(Position::new(5, 5)));
}

#[test]
fn test_ctrl_o_jumps_back() {
    let mut vim = create_vim();

    // Jump from (5, 5) to top
    let cursor = Position::new(5, 5);
    vim.process_key('g', false, cursor);

    // Now at (0, 5), press Ctrl+o to go back
    let actions = vim.process_key('o', true, Position::new(0, 5));
    assert_eq!(get_cursor_move(&actions), Some(Position::new(5, 5)));
}

#[test]
fn test_ctrl_i_jumps_forward() {
    let mut vim = create_vim();

    // Jump from (5, 5) to top
    let cursor = Position::new(5, 5);
    vim.process_key('g', false, cursor);

    // Now at (0, 5), go back with Ctrl+o
    vim.process_key('o', true, Position::new(0, 5));

    // Now at (5, 5), Ctrl+i should go forward to (0, 5)
    let actions = vim.process_key('i', true, Position::new(5, 5));
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 5)));
}

#[test]
fn test_multiple_jumps_and_navigation() {
    let mut vim = create_vim();

    // Jump 1: from (0, 0) to top (already at top, but still records)
    vim.process_key('G', false, Position::new(0, 0)); // Jump to row 7

    // Jump 2: from (7, 0) to top
    vim.process_key('g', false, Position::new(7, 0)); // Jump to row 0

    // Now at (0, 0), go back twice
    let actions = vim.process_key('o', true, Position::new(0, 0));
    assert_eq!(get_cursor_move(&actions), Some(Position::new(7, 0)));

    let actions = vim.process_key('o', true, Position::new(7, 0));
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 0)));
}

// ============================================================================
// 14. DOT REPEAT
// ============================================================================

#[test]
fn test_dot_repeats_delete() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);

    // Delete one character
    vim.process_key('d', false, cursor);
    vim.process_key('l', false, cursor);

    // Dot should repeat the delete
    let actions = vim.process_key('.', false, cursor);
    assert!(get_delete_range(&actions).is_some());
}

#[test]
fn test_dot_repeats_toggle() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);

    // Toggle
    vim.process_key('x', false, cursor);

    // Dot should repeat toggle
    let actions = vim.process_key('.', false, cursor);
    assert!(has_action(&actions, |a| matches!(a, VimAction::Toggle)));
}

#[test]
fn test_dot_with_no_previous_action() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('.', false, cursor);
    // Should do nothing harmful
    assert!(actions.is_empty() || !has_action(&actions, |a| matches!(a, VimAction::Delete(_))));
}

#[test]
fn test_dot_repeats_dd() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);

    // Delete line
    vim.process_key('d', false, cursor);
    vim.process_key('d', false, cursor);

    // Dot should repeat line delete
    let actions = vim.process_key('.', false, cursor);
    let range = get_delete_range(&actions).expect("Should have delete action");
    assert_eq!(range.range_type, RangeType::Line);
}

// ============================================================================
// 15. TOGGLE ACTION (x in normal mode)
// ============================================================================

#[test]
fn test_x_in_normal_emits_toggle() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('x', false, cursor);
    assert!(has_action(&actions, |a| matches!(a, VimAction::Toggle)));
}

#[test]
fn test_enter_in_normal_emits_toggle() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('\r', false, cursor);
    assert!(has_action(&actions, |a| matches!(a, VimAction::Toggle)));
}

// ============================================================================
// 16. EDGE CASES
// ============================================================================

#[test]
fn test_motion_at_boundary_no_movement() {
    let mut vim = create_vim();
    let cursor = Position::new(0, 0);
    let actions = vim.process_key('h', false, cursor);
    // At col 0, h should not move or move to same position
    let new_pos = get_cursor_move(&actions).unwrap_or(cursor);
    assert_eq!(new_pos.col, 0);
}

#[test]
fn test_motion_at_bottom_boundary() {
    let mut vim = create_vim();
    let cursor = Position::new(7, 5); // Last row
    let actions = vim.process_key('j', false, cursor);
    let new_pos = get_cursor_move(&actions).unwrap_or(cursor);
    assert_eq!(new_pos.row, 7);
}

#[test]
fn test_rapid_mode_switches() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);

    vim.process_key('v', false, cursor);
    assert_eq!(vim.mode(), VimMode::Visual);

    process_ctrl_key(&mut vim, 'v', cursor);
    assert_eq!(vim.mode(), VimMode::VisualBlock);

    vim.process_key('v', false, cursor);
    assert_eq!(vim.mode(), VimMode::Visual);

    vim.process_key('\x1b', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
}

#[test]
fn test_complex_sequence_5dw() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);

    vim.process_key('5', false, cursor);
    assert_eq!(vim.count, Some(5));

    vim.process_key('d', false, cursor);
    assert_eq!(vim.mode(), VimMode::OperatorPending);
    assert_eq!(vim.count, Some(5));

    // w motion not implemented yet, use l instead
    let actions = vim.process_key('l', false, cursor);
    assert_eq!(vim.mode(), VimMode::Normal);
    assert!(get_delete_range(&actions).is_some());
}

#[test]
fn test_escape_during_complex_sequence() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);

    vim.process_key('5', false, cursor);
    vim.process_key('d', false, cursor);
    vim.process_key('3', false, cursor);

    // Escape should cancel everything
    vim.process_key('\x1b', false, cursor);

    assert_eq!(vim.mode(), VimMode::Normal);
    assert_eq!(vim.count, None);
    assert_eq!(vim.operator, None);
}

// ============================================================================
// 17. RANGE CONTAINMENT
// ============================================================================

#[test]
fn test_block_range_contains() {
    let range = Range {
        start: Position::new(1, 2),
        end: Position::new(3, 5),
        range_type: RangeType::Block,
    };

    assert!(range.contains(Position::new(2, 3)));
    assert!(range.contains(Position::new(1, 2)));
    assert!(range.contains(Position::new(3, 5)));
    assert!(!range.contains(Position::new(0, 3)));
    assert!(!range.contains(Position::new(2, 6)));
}

#[test]
fn test_line_range_contains() {
    let range = Range {
        start: Position::new(1, 5),
        end: Position::new(3, 2),
        range_type: RangeType::Line,
    };

    // Line ranges include all columns in the row range
    assert!(range.contains(Position::new(1, 0)));
    assert!(range.contains(Position::new(2, 100)));
    assert!(range.contains(Position::new(3, 50)));
    assert!(!range.contains(Position::new(0, 5)));
    assert!(!range.contains(Position::new(4, 5)));
}

#[test]
fn test_char_range_contains() {
    let range = Range {
        start: Position::new(1, 5),
        end: Position::new(3, 2),
        range_type: RangeType::Char,
    };

    // Row 1: from col 5 onwards
    assert!(range.contains(Position::new(1, 5)));
    assert!(range.contains(Position::new(1, 10)));
    assert!(!range.contains(Position::new(1, 4)));

    // Row 2: all columns
    assert!(range.contains(Position::new(2, 0)));
    assert!(range.contains(Position::new(2, 100)));

    // Row 3: up to col 2
    assert!(range.contains(Position::new(3, 0)));
    assert!(range.contains(Position::new(3, 2)));
    assert!(!range.contains(Position::new(3, 3)));
}

// ============================================================================
// 18. ACTION TYPES
// ============================================================================

#[test]
fn test_mode_changed_action_on_visual_entry() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('v', false, cursor);

    assert!(has_action(&actions, |a| matches!(
        a,
        VimAction::ModeChanged(VimMode::Visual)
    )));
}

#[test]
fn test_selection_changed_action_on_visual_entry() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('v', false, cursor);

    assert!(has_action(&actions, |a| matches!(
        a,
        VimAction::SelectionChanged(Some(_))
    )));
}

#[test]
fn test_selection_cleared_on_visual_exit() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    vim.process_key('v', false, cursor);
    let actions = vim.process_key('\x1b', false, cursor);

    assert!(has_action(&actions, |a| matches!(
        a,
        VimAction::SelectionChanged(None)
    )));
}

#[test]
fn test_escape_action_emitted() {
    let mut vim = create_vim();
    let cursor = Position::new(2, 5);
    let actions = vim.process_key('\x1b', false, cursor);

    assert!(has_action(&actions, |a| matches!(a, VimAction::Escape)));
}

// ============================================================================
// 19. ZONE-AWARE NAVIGATION
// ============================================================================

/// Create channel rack zones matching the app implementation:
/// - Metadata zone: cols 0-1 (vim space), sample name + mute/solo indicator
/// - Steps zone: cols 2-17 (vim space), the main step sequencer grid
fn create_channel_rack_zones() -> GridSemantics {
    GridSemantics::with_zones(vec![
        Zone::new(0, 1),
        Zone::new(2, 17).main().with_word_interval(4),
    ])
}

/// Create a vim state configured for channel rack
fn create_channel_rack_vim() -> VimState {
    let semantics = create_channel_rack_zones();
    VimState::with_grid_semantics(8, 18, semantics)
}

#[test]
fn test_zone_contains_col() {
    let zone = Zone::new(5, 10);
    assert!(!zone.contains_col(4));
    assert!(zone.contains_col(5));
    assert!(zone.contains_col(7));
    assert!(zone.contains_col(10));
    assert!(!zone.contains_col(11));
}

#[test]
fn test_zone_start_end() {
    let zone = Zone::new(5, 10);
    assert_eq!(zone.start(), 5);
    assert_eq!(zone.end(), 10);
}

#[test]
fn test_zone_main_flag() {
    let zone = Zone::new(0, 5);
    assert!(!zone.is_main);

    let main_zone = zone.main();
    assert!(main_zone.is_main);
}

#[test]
fn test_grid_semantics_get_zone_at_col() {
    let gs = create_channel_rack_zones();

    // Metadata zone at cols 0-1
    let zone = gs.get_zone_at_col(0).unwrap();
    assert_eq!(zone.col_range, (0, 1));
    let zone = gs.get_zone_at_col(1).unwrap();
    assert_eq!(zone.col_range, (0, 1));

    // Steps zone at cols 2-17
    let zone = gs.get_zone_at_col(2).unwrap();
    assert_eq!(zone.col_range, (2, 17));
    let zone = gs.get_zone_at_col(10).unwrap();
    assert_eq!(zone.col_range, (2, 17));
    let zone = gs.get_zone_at_col(17).unwrap();
    assert_eq!(zone.col_range, (2, 17));

    // Out of range
    assert!(gs.get_zone_at_col(18).is_none());
}

#[test]
fn test_grid_semantics_get_prev_zone() {
    let gs = create_channel_rack_zones();

    // At steps zone (col 2), prev should be metadata (col 0-1)
    let prev = gs.get_prev_zone(2).unwrap();
    assert_eq!(prev.col_range, (0, 1));

    // At metadata zone (col 0), no prev zone
    assert!(gs.get_prev_zone(0).is_none());
    assert!(gs.get_prev_zone(1).is_none());
}

#[test]
fn test_grid_semantics_get_next_zone() {
    let gs = create_channel_rack_zones();

    // At metadata zone (col 1), next should be steps (col 2)
    let next = gs.get_next_zone(1).unwrap();
    assert_eq!(next.col_range, (2, 17));

    // At steps zone (col 17), no next zone
    assert!(gs.get_next_zone(17).is_none());
}

#[test]
fn test_h_crosses_zone_boundary() {
    let mut vim = create_channel_rack_vim();

    // At steps zone col 2 (first step), h should enter metadata zone at col 1
    let cursor = Position::new(0, 2);
    let actions = vim.process_key('h', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 1)));
}

#[test]
fn test_h_within_zone() {
    let mut vim = create_channel_rack_vim();

    // At steps zone col 5, h should move to col 4
    let cursor = Position::new(0, 5);
    let actions = vim.process_key('h', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 4)));
}

#[test]
fn test_h_at_leftmost_zone_boundary() {
    let mut vim = create_channel_rack_vim();

    // At metadata zone col 0, h should stay at col 0
    let cursor = Position::new(0, 0);
    let actions = vim.process_key('h', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 0)));
}

#[test]
fn test_l_crosses_zone_boundary() {
    let mut vim = create_channel_rack_vim();

    // At metadata zone col 1, l should enter steps zone at col 2
    let cursor = Position::new(0, 1);
    let actions = vim.process_key('l', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 2)));
}

#[test]
fn test_l_within_zone() {
    let mut vim = create_channel_rack_vim();

    // At steps zone col 5, l should move to col 6
    let cursor = Position::new(0, 5);
    let actions = vim.process_key('l', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 6)));
}

#[test]
fn test_l_at_rightmost_zone_boundary() {
    let mut vim = create_channel_rack_vim();

    // At steps zone col 17 (last step), l should stay at col 17
    let cursor = Position::new(0, 17);
    let actions = vim.process_key('l', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 17)));
}

#[test]
fn test_0_goes_to_current_zone_start() {
    let mut vim = create_channel_rack_vim();

    // From steps zone, 0 should go to start of steps zone (col 2)
    let cursor = Position::new(0, 10);
    let actions = vim.process_key('0', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 2)));

    // From sample zone (col 0), 0 should stay at start of metadata zone (col 0)
    let cursor = Position::new(0, 0);
    let actions = vim.process_key('0', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 0)));

    // From mute zone (col 1), 0 should go to start of metadata zone (col 0)
    let cursor = Position::new(0, 1);
    let actions = vim.process_key('0', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 0)));
}

#[test]
fn test_dollar_goes_to_current_zone_end() {
    let mut vim = create_channel_rack_vim();

    // From steps zone, $ should go to end of steps zone (col 17)
    let cursor = Position::new(0, 5);
    let actions = vim.process_key('$', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 17)));

    // From sample zone (col 0), $ should go to end of metadata zone (col 1)
    let cursor = Position::new(0, 0);
    let actions = vim.process_key('$', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 1)));

    // From mute zone (col 1), $ should stay at end of metadata zone (col 1)
    let cursor = Position::new(0, 1);
    let actions = vim.process_key('$', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 1)));
}

#[test]
fn test_h_traverses_all_zones() {
    let mut vim = create_channel_rack_vim();

    // Start at steps zone col 2, press h to enter metadata zone
    let mut cursor = Position::new(0, 2);

    // h at col 2 -> col 1 (metadata zone)
    let actions = vim.process_key('h', false, cursor);
    cursor = get_cursor_move(&actions).unwrap();
    assert_eq!(cursor.col, 1);

    // h at col 1 -> col 0 (still metadata zone)
    let actions = vim.process_key('h', false, cursor);
    cursor = get_cursor_move(&actions).unwrap();
    assert_eq!(cursor.col, 0);

    // h at col 0 -> stays at 0 (leftmost boundary)
    let actions = vim.process_key('h', false, cursor);
    cursor = get_cursor_move(&actions).unwrap();
    assert_eq!(cursor.col, 0);
}

#[test]
fn test_l_traverses_all_zones() {
    let mut vim = create_channel_rack_vim();

    // Start at metadata zone col 0, press l to traverse
    let mut cursor = Position::new(0, 0);

    // l at col 0 -> col 1 (still metadata zone)
    let actions = vim.process_key('l', false, cursor);
    cursor = get_cursor_move(&actions).unwrap();
    assert_eq!(cursor.col, 1);

    // l at col 1 -> col 2 (enters steps zone)
    let actions = vim.process_key('l', false, cursor);
    cursor = get_cursor_move(&actions).unwrap();
    assert_eq!(cursor.col, 2);

    // l at col 2 -> col 3 (still in steps zone)
    let actions = vim.process_key('l', false, cursor);
    cursor = get_cursor_move(&actions).unwrap();
    assert_eq!(cursor.col, 3);
}

#[test]
fn test_w_respects_zone_word_interval() {
    let mut vim = create_channel_rack_vim();

    // Steps zone has word_interval of 4 (beats)
    // At col 2 (step 0), w should move to col 6 (step 4, next beat)
    let cursor = Position::new(0, 2);
    let actions = vim.process_key('w', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 6)));
}

#[test]
fn test_b_respects_zone_word_interval() {
    let mut vim = create_channel_rack_vim();

    // Steps zone has word_interval of 4
    // At col 6 (step 4), b should move to col 2 (step 0, previous beat start)
    let cursor = Position::new(0, 6);
    let actions = vim.process_key('b', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 2)));
}

#[test]
fn test_zone_aware_visual_mode() {
    let mut vim = create_channel_rack_vim();

    // Enter visual mode at col 2
    let cursor = Position::new(0, 2);
    vim.process_key('v', false, cursor);

    // Move to col 5 with l
    let cursor = Position::new(0, 5);
    let actions = vim.process_key('l', false, cursor);

    // Selection should be updated
    assert!(has_action(&actions, |a| matches!(
        a,
        VimAction::SelectionChanged(Some(_))
    )));
}

#[test]
fn test_set_grid_semantics_at_runtime() {
    let mut vim = create_vim(); // No zones initially

    // Without zones, 0 goes to col 0
    let cursor = Position::new(0, 10);
    let actions = vim.process_key('0', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 0)));

    // Set zones at runtime
    vim.set_grid_semantics(create_channel_rack_zones());

    // Now 0 goes to current zone start (col 10 is in steps zone, which starts at col 2)
    let cursor = Position::new(0, 10);
    let actions = vim.process_key('0', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 2)));
}

#[test]
fn test_clear_grid_semantics() {
    let mut vim = create_channel_rack_vim();

    // With zones, 0 goes to current zone start (col 10 is in steps zone -> col 2)
    let cursor = Position::new(0, 10);
    let actions = vim.process_key('0', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 2)));

    // Clear zones
    vim.clear_grid_semantics();

    // Without zones, 0 goes to col 0
    let cursor = Position::new(0, 10);
    let actions = vim.process_key('0', false, cursor);
    assert_eq!(get_cursor_move(&actions), Some(Position::new(0, 0)));
}
