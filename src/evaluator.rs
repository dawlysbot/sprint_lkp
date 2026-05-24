use crate::PieceType;
use crate::config::{EvaluateConfig, TARGET_LINES};
use crate::bitboard::{Board, ShapeBoard, SearchNode};
use crate::bitboard::{HOLD_MASK, LINES_CLEARED_MASK};

pub fn evaluate(node: &SearchNode<ShapeBoard>, i_left: usize) -> f64 {
// the smaller the score, the better the node
    const { assert!(ShapeBoard::QUAD_ONLY); } // this evaluation function is designed for quad-only search
    let config = EvaluateConfig::default();
    let mut score = node.keys_pressed as f64;
    // first hold increases depth by 1 but with no effort, we should add a penalty to align them, weight_hold is about the average keys pressed
    if node.meta & HOLD_MASK != 0 {
        score += config.weight_hold;
    }
    fn get_well(board: ShapeBoard) -> usize {
        if board.get_height(6) == 0 {
            return 6;
        }
        if board.get_height(9) == 0 {
            return 9;
        }
        if board.get_height(3) == 0 {
            return 3;
        }
        let mut well = 0;
        while well < 10 && board.get_height(well) != 0 {
            well += 1;
        }
        well
    }
    let well = get_well(node.state) as i8;
    let mut packed = node.state.packed_shape;
    let mut prev = (packed & ShapeBoard::COL_MASK) as i16;
    let mut max_h = prev;
    for i in 1..10i8 {
        packed >>= 5;
        let h = (packed & ShapeBoard::COL_MASK) as i16;
        max_h = max_h.max(h);
        // we don't punish bump in the well, column 2 and 6 are das range gap, so we punish less
        let diff_weight = match i {
            x if x == well || x == well + 1 => 0.0,
            2 | 6 => config.weight_bump_light,
            _ => config.weight_bump,
        };
        score += (h - prev).abs() as f64 * diff_weight;
        prev = h;
    }
    if max_h >= 18 {
        score += 3.0 * config.weight_height_warning + ((max_h - 17) * (max_h - 17)) as f64 * config.weight_height_danger;
    } else if max_h > 14 {
        score += (max_h - 14) as f64 * config.weight_height_warning;
    }
    // 0.75=0.25*3, place a I in column 6 needs 3 ops, we want to cancel this penalty
    let lines_cleared = node.meta & LINES_CLEARED_MASK;
    score -= lines_cleared.min(TARGET_LINES - 4) as f64 * 0.75;

    if lines_cleared < TARGET_LINES - 4 {
        let normal_lines_left = TARGET_LINES - 4 - lines_cleared;
        let required_i = normal_lines_left.div_ceil(4) as usize; // allow the case that (TARGET_LINES - 4) % 4 != 0
        let available_i = i_left + (node.meta >> 11 & 0x7 == PieceType::I as u16) as usize;
        if available_i < required_i {
            return f64::INFINITY;
        }
        score += match available_i - required_i {
            0 => config.weight_i_tight,
            1 => config.weight_i_tight_light,
            _ => 0.0,
        };
    }

    score
}