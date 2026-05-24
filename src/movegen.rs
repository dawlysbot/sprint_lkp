use crate::PieceType;
use crate::bitboard::{Board, SearchNode, HOLD_MASK, DAS_MASK, LINES_CLEARED_MASK, SHAPE_RANGES, FINESSE_TABLE};
use core::str;
use std::collections::HashMap;
use once_cell::sync::Lazy;
use arrayvec::ArrayVec;
use crate::search::PathNode;

pub struct ReplayMove {
    hold: bool,
    finesse_idx: usize,
    x: u8,
    reused: bool,
    hold_only: bool,
}
pub trait ReplaySink {
    fn push(&mut self, op: ReplayMove);
}
pub struct NoReplay;
impl ReplaySink for NoReplay {
    #[inline(always)]
    fn push(&mut self, _: ReplayMove) {}
}
impl ReplaySink for ArrayVec<ReplayMove, 68> {
    #[inline(always)]
    fn push(&mut self, op: ReplayMove) {
        self.push(op);
    }
}

pub fn generate_moves<B: Board, S: ReplaySink>(moves: &mut ArrayVec<SearchNode<B>, 68>, replay: &mut S, node: &SearchNode<B>, piece: PieceType, idx: usize) {
    // we use ArrayVec in order to avoid heap allocation
    let hold_piece = (node.meta >> 11) as u8; // hold piece is bit 11-13
    let hold_node = SearchNode {
        state: node.state,
        meta: node.meta & !HOLD_MASK | (piece as u16) << 11, // set hold piece in meta
        keys_pressed: node.keys_pressed + 1, // 1 for hold
        parent_idx: idx,
    };
    for (node, piece) in [node, &hold_node].into_iter().zip([&(piece as u8), &hold_piece]) {
        if *piece == 0 {
            unsafe { moves.push_unchecked(*node); }
            // for performance issue, we don't check the capacity
            // J/L/T has at most 34 choices, hold doubles it to 68, so it should be safe.
            debug_assert!(node == &hold_node, "Only hold move can have piece 0");
            replay.push(ReplayMove { hold: true, finesse_idx: 0, x: 0, reused: false, hold_only: true });
            continue;
        }
        for (i, &finesse) in FINESSE_TABLE.iter().enumerate().take(SHAPE_RANGES[*piece as usize]).skip(SHAPE_RANGES[*piece as usize - 1]) {
            // Generate moves based on the mask and the current node state
            let width = ((finesse >> 30) + 1) as u8;
            let mut finesse_data = finesse & 0x3FFFFFFF; // lower 30 bits for finesse data
            let meta_nodas = node.meta & !DAS_MASK; // das state 11-12
            for x in 0..11-width {
                if let Some(new_state) = node.state.drop_piece(x, i as u8, meta_nodas & LINES_CLEARED_MASK) {
                    let reused = ((finesse_data & 0b100) >> 2) & (node.meta as u32 >> (9 + (x > 4) as u8));
                    // node.meta's bit 9-10 represent the das state, x>4 means right side
                    unsafe { moves.push_unchecked(SearchNode {
                        state: new_state.0,
                        meta: (meta_nodas + new_state.1 as u16) | ((x == 0) as u16) << 9 | ((x == 10 - width) as u16) << 10,
                        keys_pressed: node.keys_pressed + ((finesse_data & 0b11) - reused) as u16 + 1, // 1 for harddrop
                        parent_idx: idx,
                    }); }
                    replay.push(ReplayMove {
                        hold: node == &hold_node,
                        finesse_idx: i - SHAPE_RANGES[*piece as usize - 1] + FINESSE_RANGES[*piece as usize],
                        x,
                        reused: reused != 0,
                        hold_only: false,
                    });
                }
                finesse_data >>= 3;
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Actions {
    MoveLeft,
    MoveRight,
    RotateCW,
    RotateCCW,
    Rotate180,
    DasLeft,
    DasRight,
    DasLeftUp,
    DasRightUp,
}
fn rebuild_actions<B: Board>(node: &SearchNode<B>, piece: PieceType, result: &SearchNode<B>) -> (bool, bool, bool, Vec<Actions>) {
    let mut moves: ArrayVec<SearchNode<B>, 68> = ArrayVec::new();
    let mut replay: ArrayVec<ReplayMove, 68> = ArrayVec::new();
    generate_moves(&mut moves, &mut replay, node, piece, result.parent_idx);
    let rep = &replay[moves.iter().position(|&n| n == *result).unwrap()];
    let actions = if !rep.hold_only {
        let finesse = (if rep.reused { REUSE_TABLE.get(&(rep.finesse_idx, rep.x)) } else { None }).unwrap_or(&FINESSE_OP_TABLE[rep.finesse_idx][rep.x as usize]);
        debug_assert!(!rep.reused || finesse[0] == (if rep.x <= 4 { Actions::MoveLeft } else { Actions::MoveRight }), "Reused move must be a das move in the correct direction");
        finesse.clone()
    } else {
        debug_assert!(rep.hold, "Hold-only move must have hold=true");
        Vec::new()
    };
    (rep.reused, rep.hold, rep.hold_only, actions)
}
pub fn rebuild_actions_general(n1: &PathNode, piece: PieceType, n2: &PathNode) -> (bool, bool, bool, Vec<Actions>) {
    match (n1, n2) {
        (PathNode::Normal(n1), PathNode::Normal(n2)) => {
            rebuild_actions(n1, piece, n2)
        },
        (PathNode::Pc(n1), PathNode::Pc(n2)) => {
            rebuild_actions(n1, piece, n2)
        },
        (PathNode::Normal(n1), PathNode::Pc(n2)) => {
            let n1_bitboard = n1.to_bitboard();
            rebuild_actions(&n1_bitboard, piece, n2)
        },
        _ => unreachable!("cannot have a Pc node followed by a Normal node"),
    }
}

fn parse_actions(str: &str) -> Vec<Actions> {
    str.split(',').map(|s| match s.trim() {
        "L" => Actions::MoveLeft,
        "R" => Actions::MoveRight,
        "CW" => Actions::RotateCW,
        "CCW" => Actions::RotateCCW,
        "180" => Actions::Rotate180,
        "DL" => Actions::DasLeft,
        "DR" => Actions::DasRight,
        "DLU" => Actions::DasLeftUp,
        "DRU" => Actions::DasRightUp,
        _ => panic!("Unknown action: {}", s),
    }).collect()
}
fn parse_actions_batch(strs: &[&str]) -> Vec<Vec<Actions>> {
    strs.iter().map(|&s| parse_actions(s)).collect()
}

// S/Z and L/J/T 's operations are same.
const FINESSE_RANGES: [usize; 8] = [0, 0, 0, 2, 2, 2, 6, 7];
static FINESSE_OP_TABLE: Lazy<[Vec<Vec<Actions>>; 9]> = Lazy::new(|| {[
// for build replay, use Vec in convenience since replay-builder is not performance sensitive.
    // S/Z
    parse_actions_batch(&[
        "DL,DLU",
        "L,L",
        "L",
        "",
        "R",
        "R,R",
        "DR,DRU,L",
        "DR,DRU",
    ]),
    parse_actions_batch(&[
        "DL,CCW,DLU",
        "DL,DLU,CW",
        "L,CCW",
        "CCW",
        "CW",
        "R,CW",
        "R,R,CW",
        "DR,DRU,CCW",
        "DR,CW,DRU",
    ]),
    // L/J/T
    parse_actions_batch(&[
        "DL,DLU",
        "L,L",
        "L",
        "",
        "R",
        "R,R",
        "DR,DRU,L",
        "DR,DRU",
    ]),
    parse_actions_batch(&[
        "DL,CW,DLU",
        "DL,DLU,CW",
        "L,L,CW",
        "L,CW",
        "CW",
        "R,CW",
        "R,R,CW",
        "DR,CW,DRU,L",
        "DR,CW,DRU",
    ]),
    parse_actions_batch(&[
        "DL,180,DLU",
        "L,L,180",
        "L,180",
        "180",
        "R,180",
        "R,R,180",
        "DR,180,DRU,L",
        "DR,180,DRU",
    ]),
    parse_actions_batch(&[
        "DL,CCW,DLU",
        "L,L,CCW",
        "L,CCW",
        "CCW",
        "R,CCW",
        "R,R,CCW",
        "DR,DRU,L,CCW",
        "DR,DRU,CCW",
        "DR,CCW,DRU",
    ]),
    // O
    parse_actions_batch(&[
        "DL,DLU",
        "DL,DLU,R",
        "L,L",
        "L",
        "",
        "R",
        "R,R",
        "DR,DRU,L",
        "DR,DRU",
    ]),
    // I
    parse_actions_batch(&[
        "DL,DLU",
        "L,L",
        "L",
        "",
        "R",
        "R,R",
        "DR,DRU",
    ]),
    parse_actions_batch(&[
        "DL,CCW,DLU",
        "DL,DLU,CCW",
        "DL,DLU,CW",
        "L,CCW",
        "CCW",
        "CW",
        "R,CW",
        "DR,DRU,CCW",
        "DR,DRU,CW",
        "DR,CW,DRU",
    ]),
]});
static REUSE_TABLE: Lazy<HashMap<(usize, u8), Vec<Actions>>> = Lazy::new(|| {
    HashMap::from([
        // (finesse_idx, x) -> operations
        // S/Z on x=1, rot=0
        ((0, 1), parse_actions("DL,DLU,R")),
        // S/Z on x=6, rot=1
        ((1, 6), parse_actions("DR,DRU,L,CCW")),
        // L/J/T on x=1, rot=0
        ((2, 1), parse_actions("DL,DLU,R")),
        // L/J/T on x=1, rot=1
        ((3, 2), parse_actions("DL,DLU,R,CW")),
        // L/J/T on x=1, rot=2
        ((4, 1), parse_actions("DL,DLU,R,180")),
        // L/J/T on x=1, rot=3
        ((5, 1), parse_actions("DL,DLU,R,CCW")),
        // I on x=1, rot=0
        ((7, 1), parse_actions("DL,DLU,R")),
    ])
});