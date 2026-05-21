use std::env;
use tetris_sprint_lkp::{
    search::BeamSearch, config::ReplayConfig, config::Metadata, config::MAX_PIECES
};
use rand::{RngCore, SeedableRng, rngs::StdRng};
use base64::Engine;
use tetris_sprint_lkp::bagrng::generate_piece_sequence;
use tetris_sprint_lkp::replay::export_replay;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    let seed = if args.len() > 1 {
        args[1].clone()
    } else {
        let mut rng = StdRng::from_entropy();
        let seed_val: u32 = rng.next_u32();
        seed_val.to_string()
    };
    
    println!("Seed: {}", seed);
    
    let piece_sequence = generate_piece_sequence(&seed, MAX_PIECES);
    
    let result = BeamSearch::run(&piece_sequence);
    let metadata = Metadata::default();
    let metadata_bin = serde_json::to_vec(&metadata).expect("Failed to serialize metadata");

    let replay_config = ReplayConfig::default();
    let replay_bin = export_replay(&result, &piece_sequence, metadata.setting.das, &replay_config);
    
    let final_data = metadata_bin.into_iter().chain(b"\n".iter().copied()).chain(replay_bin).collect::<Vec<u8>>();
    let encoded = base64::engine::general_purpose::STANDARD.encode(final_data);
    println!("Encoded Replay: {}", encoded);
}