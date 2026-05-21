use arrayvec::ArrayVec;
use crate::config::{TARGET_LINES, ENDGAME_DEPTH};
use crate::bitboard::{BitBoard, SearchNode, LINES_CLEARED_MASK};
use crate::PieceType;

pub fn solve_pc(board: SearchNode<BitBoard>, pieces: &[PieceType], limit: u8) -> ArrayVec<SearchNode<BitBoard>, ENDGAME_DEPTH> {
    debug_assert!(pieces.len() as u8 - 1 <= limit);
    debug_assert!(board.meta & LINES_CLEARED_MASK >= TARGET_LINES - 4, "Endgame solver only works when 4 lines left");
    unimplemented!()
}