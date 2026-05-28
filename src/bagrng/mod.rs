pub mod techmino;
#[cfg(feature = "jstris")]
pub mod jstris;
#[cfg(feature = "jstris")]
use rand::Rng;
use crate::{PieceType, config::{ENGINE, GameEngine}};
use rand::{RngCore, SeedableRng, rngs::StdRng};

pub fn generate_piece_sequence(seed: &String, count: usize) -> Vec<PieceType> {
    match ENGINE {
        GameEngine::TECHMINO => techmino::generate_piece_sequence(seed, count),
        #[cfg(feature = "jstris")]
        GameEngine::JSTRIS => jstris::generate_piece_sequence(seed, count),
    }
}

pub fn generate_seed() -> String {
    loop {
        let mut rng = StdRng::from_entropy();
        let seed_val = match ENGINE {
            GameEngine::TECHMINO => rng.next_u32().to_string(),
            #[cfg(feature = "jstris")]
            GameEngine::JSTRIS => (0..6).map(|_| { (match rng.gen_range(0..36) { x if x < 10 => b'0' + x as u8, x=> b'a' + x as u8 - 10}) as char }).collect()
        };
        let piece_seq = generate_piece_sequence(&seed_val, 2);
        if (piece_seq[0] != PieceType::S && piece_seq[0] != PieceType::Z) &&
            !(piece_seq[0] == PieceType::O && (piece_seq[1] == PieceType::S || piece_seq[1] == PieceType::Z)) {
            break seed_val;
        }
    }
}