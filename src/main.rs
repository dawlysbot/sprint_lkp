use env_logger;
use tetris_sprint_lkp::PieceType;
use std::env;
use tetris_sprint_lkp::{
    search::BeamSearch, config::ReplayConfig, config::Metadata, config::MAX_PIECES
};
use rand::{RngCore, SeedableRng, rngs::StdRng};
use base64::Engine;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;
use tetris_sprint_lkp::bagrng::generate_piece_sequence;
use tetris_sprint_lkp::replay::export_replay;

fn main() {
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    
    let seed = if args.len() > 1 {
        args[1].clone().parse::<u32>().unwrap()
    } else {
        loop {
            let mut rng = StdRng::from_entropy();
            let seed_val = rng.next_u32();
            let piece_seq = generate_piece_sequence(&seed_val, 2);
            if (piece_seq[0] != PieceType::S && piece_seq[0] != PieceType::Z) &&
               !(piece_seq[0] == PieceType::O && (piece_seq[1] == PieceType::S || piece_seq[1] == PieceType::Z)) {
                break seed_val;
            }
        }
    };
    let piece_sequence = generate_piece_sequence(&seed, MAX_PIECES);
    assert!(piece_sequence[0] != PieceType::S && piece_sequence[0] != PieceType::Z, "The first piece cannot be S or Z");
    assert!(piece_sequence[0] != PieceType::O || piece_sequence[1] != PieceType::S && piece_sequence[1] != PieceType::Z, "When the first piece is O, the second piece cannot be S or Z");
    println!("Seed: {}", seed);

    let result = BeamSearch::run(&piece_sequence);
    println!("Find solution! result length = {}, pressed keys = {}", result.len(), result.last().unwrap().keys_pressed());

    let mut metadata = Metadata::default();
    metadata.seed = seed;
    let metadata_bin = serde_json::to_vec(&metadata).expect("Failed to serialize metadata");

    let replay_config = ReplayConfig::default();
    let replay_bin = export_replay(&result, &piece_sequence, metadata.setting.das, &replay_config);
    
    let final_data = metadata_bin.into_iter().chain(b"\n".iter().copied()).chain(replay_bin).collect::<Vec<u8>>();
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&final_data).expect("Failed to write data to encoder");
    let compressed_data = encoder.finish().expect("Failed to finish compression");
    let base64_encoded = base64::engine::general_purpose::STANDARD.encode(compressed_data);
    println!("Encoded Replay: {}", base64_encoded);
}