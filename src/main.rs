use tetris_sprint_lkp::PieceType;
use std::env;
use tetris_sprint_lkp::{
    search::BeamSearch, config::MAX_PIECES
};
use tetris_sprint_lkp::bagrng::{generate_piece_sequence, generate_seed};
use tetris_sprint_lkp::replay::gen_replay;

#[hotpath::main]
fn main() {
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    
    let seed: String = if args.len() > 1 {
        args[1].clone()
    } else {
        generate_seed()
    };
    let piece_sequence = generate_piece_sequence(&seed, MAX_PIECES);
    assert!(piece_sequence[0] != PieceType::S && piece_sequence[0] != PieceType::Z, "The first piece cannot be S or Z");
    assert!(piece_sequence[0] != PieceType::O || piece_sequence[1] != PieceType::S && piece_sequence[1] != PieceType::Z, "When the first piece is O, the second piece cannot be S or Z");

    let result = BeamSearch::run(&piece_sequence);

    println!("Seed: {}", seed);
    println!("Find solution! result length = {}, pressed keys = {}", result.len(), result.last().unwrap().keys_pressed());

    println!("Replay code: {}", gen_replay(seed, &result, &piece_sequence));
}