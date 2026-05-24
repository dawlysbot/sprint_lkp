use std::sync::{
    atomic::{AtomicU16, Ordering},
    Mutex,
};

use arrayvec::ArrayVec;
use rustc_hash::FxHashMap;

use crate::bitboard::{BitBoard, SearchNode, LINES_CLEARED_MASK};
use crate::config::{ENDGAME_DEPTH, TARGET_LINES};
use crate::movegen::{generate_moves, NoReplay};
use crate::PieceType;

pub struct EndgameShared {
    best_keys: AtomicU16,
    table: Vec<Mutex<FxHashMap<u64, u16>>>,
}

impl EndgameShared {
    pub fn new() -> Self {
        Self {
            best_keys: AtomicU16::new(u16::MAX),
            table: (0..256).map(|_| Mutex::new(FxHashMap::default())).collect(),
            // 256 = 8 (piece pos) * 4 (das state) * 8 (hold piece)
        }
    }

    #[inline]
    pub fn set_best(&self, keys: u16) {
        let mut current = self.best_keys.load(Ordering::Relaxed);
        while keys < current {
            match self.best_keys.compare_exchange_weak(current, keys, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    #[inline(always)]
    fn best_keys(&self) -> u16 {
        self.best_keys.load(Ordering::Relaxed)
    }

    #[inline(always)]
    fn shard(piece_pos: usize, meta: u16) -> usize {
        debug_assert!(piece_pos < ENDGAME_DEPTH);
        ((meta & !LINES_CLEARED_MASK) >> 6) as usize | piece_pos
        // piece_pos is bit 0-2, das state is bit 9-10, hold piece is bit 11-13, so after shifting 6, we get a value in 0..256
    }

    #[inline]
    fn visit(&self, piece_pos: usize, node: &SearchNode<BitBoard>) -> bool {
        let mut table = self.table[Self::shard(piece_pos, node.meta)].lock().unwrap();
        let key = node.state.raw();
        if table.get(&key).is_some_and(|&best| best <= node.keys_pressed) {
            return false;
        }
        table.insert(key, node.keys_pressed);
        true
    }
}

impl Default for EndgameShared {
    fn default() -> Self {
        Self::new()
    }
}

#[inline(always)]
fn lines_cleared<B: crate::bitboard::Board>(node: &SearchNode<B>) -> u16 {
    node.meta & LINES_CLEARED_MASK
}

#[inline]
fn min_remaining_pieces(board: BitBoard, lines: u16) -> u16 {
    let remaining_lines = TARGET_LINES - lines;
    if remaining_lines == 0 {
        return 0;
    }
    // here we use saturating_sub since occupied cells may exceed remaining_lines * 10
    (remaining_lines * 10).saturating_sub(board.occupied_cells()).div_ceil(4)
}

#[inline]
fn cannot_supply_enough_cells(board: BitBoard, lines: u16, pieces_left: usize) -> bool {
    let remaining_lines = TARGET_LINES - lines;
    let needed_cells = remaining_lines as usize * 10;
    board.occupied_cells() as usize + pieces_left * 4 < needed_cells
}

struct Dfs<'a> {
    pieces: &'a [PieceType],
    shared: &'a EndgameShared,
    best_path: ArrayVec<SearchNode<BitBoard>, ENDGAME_DEPTH>,
}

impl<'a> Dfs<'a> {
    fn search(
        &mut self,
        node: SearchNode<BitBoard>,
        piece_idx: usize,
        path: &mut ArrayVec<SearchNode<BitBoard>, ENDGAME_DEPTH>,
    ) {
        let lines = lines_cleared(&node);
        if lines >= TARGET_LINES {
            let final_keys = path.last().map_or(node.keys_pressed, |n| n.keys_pressed);
            if self.best_path.is_empty()
                || final_keys < self.best_path.last().unwrap().keys_pressed
                || final_keys == self.best_path.last().unwrap().keys_pressed && path.len() < self.best_path.len()
            {
                self.best_path = path.clone();
            }
            self.shared.set_best(final_keys);
            return;
        }

        if piece_idx >= self.pieces.len() || path.len() >= ENDGAME_DEPTH {
            return;
        }

        let pieces_left = self.pieces.len() - piece_idx;
        let min_pieces = min_remaining_pieces(node.state, lines);
        if min_pieces as usize > pieces_left {
            return;
        }
        if node.keys_pressed + min_pieces > self.shared.best_keys() {
            return;
        }
        debug_assert!(!cannot_supply_enough_cells(node.state, lines, pieces_left), "the remaining pieces must be able to supply enough cells");

        if !self.shared.visit(piece_idx, &node) {
            return;
        }

        let mut moves: ArrayVec<SearchNode<BitBoard>, 68> = ArrayVec::new();
        generate_moves(&mut moves, &mut NoReplay, &node, self.pieces[piece_idx], path.len());
        moves.sort_unstable_by_key(|node| {
            let cleared = (lines_cleared(node) - lines) as u8;
            (cleared == 0, node.keys_pressed, u8::MAX - cleared)
        });

        for next in moves {
            if next.keys_pressed > self.shared.best_keys() {
                continue;
            }
            path.push(next);
            self.search(next, piece_idx + 1, path);
            path.pop();
        }
    }
}

pub fn solve_pc(
    board: SearchNode<BitBoard>,
    pieces: &[PieceType],
    piece_idx: usize,
    shared: &EndgameShared,
) -> ArrayVec<SearchNode<BitBoard>, ENDGAME_DEPTH> {
    debug_assert!(
        lines_cleared(&board) >= TARGET_LINES - 4,
        "Endgame solver only works when at most 4 lines remain"
    );
    let mut dfs = Dfs {
        pieces,
        shared,
        best_path: ArrayVec::new(),
    };
    let mut path = ArrayVec::new();
    dfs.search(board, piece_idx, &mut path);
    dfs.best_path
}
