use std::sync::{atomic::{AtomicU16, Ordering},Mutex};
use arrayvec::ArrayVec;
use crate::{bitboard::{BitBoard, LINES_CLEARED_MASK, SearchNode}, config::ENDGAME_START};
use crate::config::{ENDGAME_DEPTH, TARGET_LINES};
use crate::movegen::{generate_moves, NoReplay};
use crate::PieceType;

#[derive(Clone, Copy)]
struct TtEntry {
    key: u64,
    keys_pressed: u16,
}
const SHARD_CAPACITY: usize = 1 << 18;
pub struct EndgameShared {
    best_keys: AtomicU16,
    table: Vec<Mutex<Box<[TtEntry]>>>,
}

impl EndgameShared {
    pub fn new() -> Self {
        Self {
            best_keys: AtomicU16::new(u16::MAX),
            table: (0..256).map(|_| Mutex::new(vec![TtEntry{ key: u64::MAX, keys_pressed: u16::MAX }; SHARD_CAPACITY].into_boxed_slice())).collect(),
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
    pub fn best_keys(&self) -> u16 {
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
        let key = node.state.raw();
        let hash = key.wrapping_mul(0x517cc1b727220a95);
        
        let idx1 = (hash as usize) & (SHARD_CAPACITY - 2); 
        let idx2 = idx1 + 1; 

        let shard_idx = Self::shard(piece_pos, node.meta);
        let mut table = self.table[shard_idx].lock().unwrap();

        if table[idx1].key == key {
            debug_assert!((node.meta & LINES_CLEARED_MASK) * 10 == ((piece_pos + ENDGAME_START) as u16 - (node.meta >> 11 != 0) as u16) * 4 - node.state.occupied_cells());
            if table[idx1].keys_pressed <= node.keys_pressed {
                return false;
            }
            table[idx1].keys_pressed = node.keys_pressed;
            return true;
        }

        if table[idx2].key == key {
            debug_assert!((node.meta & LINES_CLEARED_MASK) * 10 == ((piece_pos + ENDGAME_START) as u16 - (node.meta >> 11 != 0) as u16) * 4 - node.state.occupied_cells());
            if table[idx2].keys_pressed <= node.keys_pressed {
                return false;
            }
            table[idx2].keys_pressed = node.keys_pressed;
            return true;
        }

        if table[idx1].keys_pressed > table[idx2].keys_pressed {
            table[idx1].key = key;
            table[idx1].keys_pressed = node.keys_pressed;
        } else {
            table[idx2].key = key;
            table[idx2].keys_pressed = node.keys_pressed;
        }
        
        true
    }
}

impl Default for EndgameShared {
    fn default() -> Self {
        Self::new()
    }
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
        let lines = node.meta & LINES_CLEARED_MASK;
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
        let min_pieces = {
            let remaining_lines = TARGET_LINES - lines;
            if remaining_lines == 0 {
                0
            } else {
                // here we use saturating_sub since occupied cells may exceed remaining_lines * 10
                debug_assert!((remaining_lines * 10).saturating_sub(node.state.occupied_cells()) % 4 == 0);
                (remaining_lines * 10).saturating_sub(node.state.occupied_cells()) / 4
            }
        };
        if min_pieces as usize > pieces_left {
            return;
        }
        if node.keys_pressed + min_pieces > self.shared.best_keys() {
            return;
        }
        debug_assert!(node.state.occupied_cells() as usize + pieces_left * 4 >= (TARGET_LINES - lines) as usize * 10,
                      "the remaining pieces must be able to supply enough cells");

        if !self.shared.visit(piece_idx, &node) {
            return;
        }

        let mut moves: ArrayVec<SearchNode<BitBoard>, 68> = ArrayVec::new();
        generate_moves(&mut moves, &mut NoReplay, &node, self.pieces[piece_idx], path.len());
        moves.sort_unstable_by_key(|node| {
            let cleared = ((node.meta & LINES_CLEARED_MASK) - lines) as u8;
            // no hole first, then clear lines, then fewer keys pressed, then cleared more lines
            (!node.state.raw() & ((node.state.raw() & (BitBoard::ROW_MASK * 0x3E)) >> 1) != 0, cleared == 0, node.keys_pressed, u8::MAX - cleared)
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
        (board.meta & LINES_CLEARED_MASK) >= TARGET_LINES - 4,
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
