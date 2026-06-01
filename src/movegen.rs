use crate::PieceType;
use crate::bitboard::{Board, SearchNode, HOLD_MASK, DAS_MASK, LINES_CLEARED_MASK, SHAPE_RANGES, FINESSE_TABLE};
use crate::config::NO_HOLD;
use core::str;
use std::collections::HashMap;
use once_cell::sync::Lazy;
use arrayvec::ArrayVec;
use crate::search::PathNode;

#[derive(Clone)]
pub struct ReplayMove {
    hold: bool,
    finesse_idx: usize,
    x: u8,
    reused: bool,
    hold_only: bool,
    special: bool,
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
            moves.push(*node);
            // for performance issue, we don't check the capacity
            // J/L/T has at most 34 choices, hold doubles it to 68, so it should be safe.
            debug_assert!(node == &hold_node, "Only hold move can have piece 0");
            replay.push(ReplayMove { hold: true, finesse_idx: 0, x: 0, reused: false, hold_only: true, special: false });
            continue;
        }
        for (i, &finesse) in FINESSE_TABLE.iter().enumerate().take(SHAPE_RANGES[*piece as usize]).skip(SHAPE_RANGES[*piece as usize - 1]) {
            // Generate moves based on the mask and the current node state
            let width = (finesse >> 60) as u8;
            let mut finesse_data = finesse & ((1u64 << 60) - 1); // lower 50 bits for finesse data
            let meta_nodas = node.meta & !DAS_MASK; // das state 11-12
            for x in 0..11-width {
                if let Some(new_state) = node.state.drop_piece(x, i as u8, meta_nodas & LINES_CLEARED_MASK) {
                    let support = node.meta as u64 >> (9 + (x > 4) as u8);
                    let reused = ((finesse_data & 0b100) >> 2) & support;
                    let special = ((finesse_data & 0x20) >> 5) & support;
                    // node.meta's bit 9-10 represent the das state, x>4 means right side
                    debug_assert!(((meta_nodas & LINES_CLEARED_MASK) + new_state.1 as u16) < (1 << 9), "Lines cleared should be less than 512");
                    let meta_das = if cfg!(feature = "advanced_das") {
                        ((finesse_data & 0x18) as u16) << 6 | (special as u16) << (9 + (x <= 4) as u8)
                    } else { (((x == 0) as u16) << 9) | ((x == 10 - width) as u16) << 10 };
                    // finesse_data's bit 3-4 represent the provided DAS state, and 9-3=6 is the offset
                    moves.push(SearchNode {
                        state: new_state.0,
                        meta: (meta_nodas + new_state.1 as u16) | meta_das,
                        keys_pressed: node.keys_pressed + ((finesse_data & 0b11) - reused) as u16 + 1, // 1 for harddrop
                        parent_idx: idx,
                    });
                    replay.push(ReplayMove {
                        hold: node == &hold_node,
                        finesse_idx: i - SHAPE_RANGES[*piece as usize - 1] + FINESSE_RANGES[*piece as usize],
                        x,
                        reused: reused != 0,
                        hold_only: false,
                        special: special != 0,
                    });
                }
                finesse_data >>= 6;
            }
        }
        if NO_HOLD {
            break; // if hold is not allowed, we only generate moves for the original piece
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
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
fn rebuild_actions<B: Board>(node: &SearchNode<B>, piece: PieceType, result: &SearchNode<B>) -> ReplayMove {
    let mut moves: ArrayVec<SearchNode<B>, 68> = ArrayVec::new();
    let mut replay: ArrayVec<ReplayMove, 68> = ArrayVec::new();
    generate_moves(&mut moves, &mut replay, node, piece, result.parent_idx);
    replay[moves.iter().position(|&n| n == *result).unwrap()].clone()
}
fn rebuild_actions_general(n1: &PathNode, piece: PieceType, n2: &PathNode) -> ReplayMove {
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
pub struct ActionStep {
    pub idx: usize,
    pub reused: bool,
    pub hold: bool,
    pub burst: bool,
    pub pipeline: bool,
    pub actions: Vec<Action>,
}
pub fn compile_action(path: &[PathNode], piece_sequence: &[PieceType]) -> Vec<ActionStep> {
    let mut raw_moves = Vec::new();
    for i in 0..path.len()-1 {
        raw_moves.push(rebuild_actions_general(&path[i], piece_sequence[i], &path[i+1]));
    }
    let mut moves = Vec::new();
    let mut pending_hold = false;
    for (i, mov) in raw_moves.into_iter().enumerate() {
        if mov.hold_only {
            // hold_only means this step only has one hold action but not drop
            // theoretically the game do not allow double-hold, but it's possible for our replay, such as hold_only -> hold -> harddrop
            // but this sequence is not optimal (we can directly harddrop -> hold_only), so we allow it for simplicity.
            pending_hold = true;
            continue;
        }
        debug_assert!(!(mov.hold && pending_hold), "Double hold would occur when merging hold_only");
        moves.push((i, ReplayMove { hold: mov.hold || pending_hold, ..mov }));
        pending_hold = false;
    }
    let mut next_dir = vec![None; moves.len()];
    for i in (0..moves.len()-1).rev() {
        next_dir[i] = moves[i + 1].1.reused.then_some(if moves[i + 1].1.x <= 4 {Action::DasLeft} else {Action::DasRight});
        moves[i].1.reused |= moves[i].1.special && next_dir[i].is_some() && next_dir[i] != Some(if moves[i].1.x <= 4 {Action::DasLeft} else {Action::DasRight});
    }
    let mut steps = Vec::new();
    for (i, (idx, mov)) in moves.into_iter().enumerate() {
        let try_get_actions = |seq: Vec<Action>| -> Option<(Vec<Action>, bool, bool)> {
            if mov.reused && !matches!(seq.first().unwrap(), Action::DasLeft | Action::DasRight) {
                return None;
            }
            if let Some(dir) = next_dir[i] {
                if seq.first() == Some(&dir) {
                    let mut seq = seq;
                    let pos = seq.iter().position(|x| matches!(x, Action::DasLeftUp | Action::DasRightUp)).unwrap();
                    if !seq.get(pos + 1).is_none_or(|a| matches!(a, Action::MoveLeft | Action::MoveRight)) {
                        assert!(matches!(seq.last().unwrap(), Action::MoveLeft | Action::MoveRight));
                        let last_move = seq.pop().unwrap();
                        seq.insert(pos + 1, last_move);
                    }
                    return Some((seq, true, false))
                }
                if seq.last() == Some(&match dir { Action::DasLeft => Action::MoveLeft, Action::DasRight => Action::MoveRight, _ => unreachable!()}) {
                    return Some((seq, false, true))
                }
                None
            } else {
                Some((seq, false, false))
            }
        };
        let (actions, burst, pipeline) = Some(FINESSE_OP_TABLE[mov.finesse_idx][mov.x as usize].clone())
            .and_then(&try_get_actions)
            .or_else(|| REUSE_TABLE.get(&(mov.finesse_idx, mov.x)).cloned().and_then(&try_get_actions))
            .or_else(|| SPECIAL_TABLE.get(&(mov.finesse_idx, mov.x)).cloned().and_then(&try_get_actions))
            .unwrap();
        steps.push(ActionStep { idx, reused: mov.reused, hold: mov.hold, burst, pipeline, actions });
    }
    steps
}

fn parse_actions(str: &str) -> Vec<Action> {
    if str.is_empty() {
        return Vec::new();
    }
    str.split(',').map(|s| match s.trim() {
        "L" => Action::MoveLeft,
        "R" => Action::MoveRight,
        "CW" => Action::RotateCW,
        "CCW" => Action::RotateCCW,
        "180" => Action::Rotate180,
        "DL" => Action::DasLeft,
        "DR" => Action::DasRight,
        "DLU" => Action::DasLeftUp,
        "DRU" => Action::DasRightUp,
        _ => panic!("Unknown action: {}", s),
    }).collect()
}
fn parse_actions_batch(strs: &[&str]) -> Vec<Vec<Action>> {
    strs.iter().map(|&s| parse_actions(s)).collect()
}

// S/Z and L/J/T 's operations are same.
const FINESSE_RANGES: [usize; 8] = [0, 0, 0, 2, 2, 2, 6, 7];
static FINESSE_OP_TABLE: Lazy<[Vec<Vec<Action>>; 9]> = Lazy::new(|| {[
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
        "CCW,L",
        "CCW",
        "CW",
        "CW,R",
        "CW,R,R",
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
        "CW,L,L",
        "CW,L",
        "CW",
        "CW,R",
        "CW,R,R",
        "DR,CW,DRU,L",
        "DR,CW,DRU",
    ]),
    parse_actions_batch(&[
        "DL,180,DLU",
        "180,L,L",
        "180,L",
        "180",
        "180,R",
        "180,R,R",
        "DR,180,DRU,L",
        "DR,180,DRU",
    ]),
    parse_actions_batch(&[
        "DL,CCW,DLU",
        "CCW,L,L",
        "CCW,L",
        "CCW",
        "CCW,R",
        "CCW,R,R",
        "DR,DRU,CCW,L",
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
        "CCW,L",
        "CCW",
        "CW",
        "CW,R",
        "DR,DRU,CCW",
        "DR,DRU,CW",
        "DR,CW,DRU",
    ]),
]});
static REUSE_TABLE: Lazy<HashMap<(usize, u8), Vec<Action>>> = Lazy::new(|| {
    HashMap::from([
        // (finesse_idx, x) -> operations
        // S/Z on x=1, rot=0
        ((0, 1), parse_actions("DL,DLU,R")),
        // S/Z on x=6, rot=1
        ((1, 6), parse_actions("DR,DRU,CCW,L")),
        // L/J/T on x=1, rot=0
        ((2, 1), parse_actions("DL,DLU,R")),
        // L/J/T on x=2, rot=1
        ((3, 2), parse_actions("DL,DLU,CW,R")),
        // L/J/T on x=1, rot=2
        ((4, 1), parse_actions("DL,DLU,180,R")),
        // L/J/T on x=1, rot=3
        ((5, 1), parse_actions("DL,DLU,CCW,R")),
        // I on x=1, rot=0
        ((7, 1), parse_actions("DL,DLU,R")),
        // I on x=5, rot=0
        ((7, 5), parse_actions("DR,DRU,L")),
    ])
});
static SPECIAL_TABLE: Lazy<HashMap<(usize, u8), Vec<Action>>> = Lazy::new(|| {
    HashMap::from([
        // S/Z on x=5, rot=0
        ((0, 5), parse_actions("DR,DRU,L,L")),
        // S/Z on x=2, rot=1
        ((1, 2), parse_actions("DL,DLU,CW,R")),
        // L/J/T on x=5, rot=0
        ((2, 5), parse_actions("DR,DRU,L,L")),
        // L/J/T on x=6, rot=1
        ((3, 6), parse_actions("DR,DRU,CW,L,L")),
        // L/J/T on x=5, rot=2
        ((4, 5), parse_actions("DR,DRU,180,L,L")),
        // L/J/T on x=5, rot=3
        ((5, 5), parse_actions("DR,DRU,CCW,L,L")),
        // O on x=2,rot=0
        ((6, 2), parse_actions("DL,DLU,R,R")),
        // O on x=6,rot=0
        ((6, 6), parse_actions("DR,DRU,L,L")),
        // I on x=3, rot=1
        ((8, 3), parse_actions("DL,DLU,CW,R")),
        // I on x=6, rot=1
        ((8, 6), parse_actions("DR,DRU,CCW,L")),
    ])
});