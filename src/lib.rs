pub mod bitboard;
pub mod config;
pub mod movegen;
pub mod search;
pub mod evaluator;
pub mod endgame;
pub mod bagrng;
pub mod replay;

pub use bitboard::{Board, SearchNode};
pub use config::Config;
pub use evaluator::evaluate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PieceType {
    Empty, Z, S, J, L, T, O, I,
}