use rustc_hash::FxHashMap;
use rayon::prelude::*;
use arrayvec::ArrayVec;
use crate::config::{BEAM_WIDTH, ENDGAME_DEPTH, TARGET_LINES, MAX_PIECES, EVALUATE_DEPTH};
use crate::bitboard::{ShapeBoard, BitBoard, SearchNode, LINES_CLEARED_MASK};
use crate::PieceType;
use crate::movegen::{generate_moves, rebuild_actions_general, Actions, NoReplay};
use crate::evaluator::evaluate;
use crate::endgame::solve_pc;

#[derive(Clone, Copy)]
pub enum PathNode {
    Normal(SearchNode<ShapeBoard>),
    Pc(SearchNode<BitBoard>),
}
impl PathNode {
    pub fn meta(&self) -> u16 {
        match self {
            PathNode::Normal(node) => node.meta,
            PathNode::Pc(node) => node.meta,
        }
    }
    pub fn keys_pressed(&self) -> u16 {
        match self {
            PathNode::Normal(node) => node.keys_pressed,
            PathNode::Pc(node) => node.keys_pressed,
        }
    }
}
struct Path {
    keys_pressed: u16,
    depth: usize,
    node: SearchNode<ShapeBoard>,
    pc_sequence: ArrayVec<SearchNode<BitBoard>, ENDGAME_DEPTH>,
}

pub struct BeamSearch;
impl BeamSearch {
    fn backtrace_path(layers: &[Vec<SearchNode<ShapeBoard>>], depth: usize, node: &SearchNode<ShapeBoard>) -> Vec<SearchNode<ShapeBoard>> {
        let mut path = Vec::new();
        let mut current_node = *node;
        for d in (1..=depth).rev() {
            path.push(current_node);
            let parent_idx = current_node.parent_idx;
            current_node = layers[d - 1][parent_idx];
        }
        path.reverse();
        path
    }
    fn checkmin_path(layers: &[Vec<SearchNode<ShapeBoard>>], piece_sequence: &[PieceType], a: &[PathNode], b: &Path) -> Option<Vec<PathNode>> {
        let a_keys = a.last().map_or(u16::MAX, |node| node.keys_pressed());
        let b_keys = b.pc_sequence.last().unwrap().keys_pressed;
        if a_keys < b_keys {
            return None;
        }
        let b_seq = Self::backtrace_path(layers, b.depth, &b.node).into_iter().map(PathNode::Normal).chain(b.pc_sequence.iter().copied().map(PathNode::Pc)).collect::<Vec<PathNode>>();
        if a.is_empty() || a_keys > b_keys {
            return Some(b_seq);
        }
        // If key_pressed is the same, we compare the number of non-reused DAS moves
        fn count_das_moves(piece_sequence: &[PieceType], path: &[PathNode]) -> u16 {
            let mut das_moves = 0;
            for i in 0..path.len()-1 {
                let piece = piece_sequence[i];
                let (reused, _, _, actions) = rebuild_actions_general(&path[i], piece, &path[i + 1]);
                if !reused && actions.iter().any(|&action| matches!(action, Actions::DasLeftUp | Actions::DasRightUp)) {
                    das_moves += 1;
                }
            }
            das_moves
        }
        if count_das_moves(piece_sequence, a) > count_das_moves(piece_sequence, &b_seq) {
            Some(b_seq)
        } else {
            None
        }
    }
    pub fn run(piece_sequence: &[PieceType]) -> Vec<PathNode> {
    // result format: null state (board=0) at index 0, step i: takes piece_sequence[i], then go to step i+1, so |result| = |piece_sequence| + 1
        let max_depth = piece_sequence.len(); // 101
        let mut layers = vec![Vec::new(); max_depth + 1];
        let mut best_path = Vec::new();
        // keys pressed, depth, searchnode, pc sequence
        
        layers[0].push(SearchNode::<ShapeBoard>::initial());

        for depth in 0..max_depth {
            let current_piece = piece_sequence[depth];
            let future_pieces: ArrayVec<PieceType, EVALUATE_DEPTH> = ArrayVec::try_from(piece_sequence[depth + 1..depth + EVALUATE_DEPTH.min(max_depth - depth)].iter().as_slice()).unwrap();
            let (map, replay) = layers[depth].par_iter().enumerate()
                .fold(|| (FxHashMap::default(), Vec::<Path>::new(), ArrayVec::new(), ArrayVec::from(std::array::from_fn(|_| ArrayVec::new()))), |(mut local_map, mut local_replay, mut move_buf, mut eval_buf), (idx, node)| {
                    if !local_replay.is_empty() && node.keys_pressed + (max_depth - 1 - depth) as u16 > local_replay[0].keys_pressed {
                        return (local_map, local_replay, move_buf, eval_buf);
                    }
                    if depth >= MAX_PIECES - ENDGAME_DEPTH && (node.meta & LINES_CLEARED_MASK) >= TARGET_LINES - 4 {
                        let limit = if local_replay.is_empty() { u8::MAX } else { (local_replay[0].keys_pressed - node.keys_pressed) as u8 };
                        let solution = solve_pc(node.to_bitboard(), &piece_sequence[depth..], limit);
                        if !solution.is_empty() {
                            local_replay.push(Path {
                                keys_pressed: node.keys_pressed,
                                depth,
                                node: *node,
                                pc_sequence: solution,
                            });
                        }
                        return (local_map, local_replay, move_buf, eval_buf);
                    }
                    move_buf.clear();
                    generate_moves(&mut move_buf, &mut NoReplay, node, current_piece, idx);
                    for &next_node in move_buf.iter() {
                        let hash = next_node.state.packed_shape | (next_node.meta as u64) << 50;
                        // 50 bits for board state, 14 bits for hold piece;
                        Self::insert_or_update(&mut local_map, hash, next_node, Self::evaluate_search(&mut eval_buf, &next_node, &future_pieces));
                    }
                    (local_map, local_replay, move_buf, eval_buf)
                }).map(|(a, b, _, _)| (a, b))
                .reduce(|| (FxHashMap::default(), Vec::<Path>::new()), |(mut a, mut b), (c, d)| {
                    for (hash, (score, node)) in c {
                        Self::insert_or_update(&mut a, hash, node, score);
                    }
                    if b.is_empty() || (!d.is_empty() && d[0].keys_pressed < b[0].keys_pressed) {
                        b = d;
                    }
                    else if !d.is_empty() && d[0].keys_pressed == b[0].keys_pressed {
                        b.extend(d);
                    }
                    (a, b)
                });
            best_path = replay.into_iter().fold(best_path, |best, path| {
                if let Some(new_best) = Self::checkmin_path(&layers, piece_sequence, &best, &path) {
                    new_best
                } else {
                    best
                }
            });
            if map.is_empty() {
                break; // No more nodes to explore
            }
            let (scores, next_layer_nodes): (Vec<f64>, Vec<SearchNode<ShapeBoard>>) = map.into_values().unzip();
            let mut indiced_scores: Vec<(usize, f64)> = scores.into_iter().enumerate().collect();
            indiced_scores.par_sort_unstable_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            indiced_scores.truncate(BEAM_WIDTH);
            layers[depth + 1] = indiced_scores.into_iter().map(|(i, _)| next_layer_nodes[i]).collect();
        }
        best_path
    }

    fn evaluate_search(eval_buf: &mut ArrayVec<ArrayVec<SearchNode<ShapeBoard>, 68>, {EVALUATE_DEPTH+1}>, node: &SearchNode<ShapeBoard>, piece_sequence: &ArrayVec<PieceType, EVALUATE_DEPTH>) -> f64 {
        // we use a brute-force dfs to search for EVALUATE_DEPTH pieces and return the best score
        let mut best_score = f64::INFINITY;
        debug_assert!(eval_buf.len() == EVALUATE_DEPTH + 1, "Eval buffer length should be equal to EVALUATE_DEPTH + 1");
        debug_assert!(eval_buf[0].is_empty(), "Eval buffer at depth 0 should be empty");
        eval_buf[0].push(*node);
        let mut depth = 0;
        loop {
            if let Some(node) = eval_buf[depth].pop() {
                if depth < piece_sequence.len() {
                    depth += 1;
                    debug_assert!(eval_buf[depth].is_empty(), "Eval buffer at depth {} should be empty", depth);
                    generate_moves(&mut eval_buf[depth], &mut NoReplay, &node, piece_sequence[depth - 1], 0);
                } else {
                    best_score = best_score.min(evaluate(&node));
                }
            } else if depth == 0 {
                break;
            } else {
                depth -= 1;
            }
        }
        best_score
    }
    
    #[inline(always)]
    fn insert_or_update(map: &mut FxHashMap<u64, (f64, SearchNode<ShapeBoard>)>, hash: u64, new_node: SearchNode<ShapeBoard>, new_score: f64) {
        map.entry(hash)
           .and_modify(|(existing_score, existing_node)| {
                if new_score < *existing_score {
                    *existing_score = new_score;
                    *existing_node = new_node;
                }
           })
           .or_insert((new_score, new_node));
    }
}