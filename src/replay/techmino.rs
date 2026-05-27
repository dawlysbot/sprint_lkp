use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;
use base64::Engine;
use log::debug;
use crate::PieceType;
use crate::config::{ReplayConfig, TARGET_LINES, TECHMINO_DAS_FRAME};
use crate::search::PathNode;
use crate::replay::{ReplayConsumer, ActionKind, compile_path};
use serde_json::{json, Value};

/*
playerActions={ // key down
    Player.act_moveLeft,  -- 1
    Player.act_moveRight, -- 2
    Player.act_rotRight,  -- 3
    Player.act_rotLeft,   -- 4
    Player.act_rot180,    -- 5
    Player.act_hardDrop,  -- 6
    Player.act_hold,      -- 8
}
key up <- origin + 32
 */
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum ReplayEvent {
    MoveLeft = 1,
    MoveRight = 2,
    RotateCW = 3,
    RotateCCW = 4,
    Rotate180 = 5,
    HardDrop = 6,
    Hold = 8,
}
struct TechminoConsumer {
    operations: Vec<u32>,
    tap_wait: u32,
    das_wait: u32,
    timestamp: u32,
}
impl ReplayConsumer for TechminoConsumer {
    fn from(das_frame: u32, replay_config: &ReplayConfig) -> Self {
        let operations = Vec::new();
        // operations format: (frame, event), (frame, event)...
        let timestamp = replay_config.first_op;
        let das_wait = (das_frame as f64 * replay_config.das_ratio) as u32;
        let tap_wait = (das_frame as f64 * replay_config.short_ratio) as u32;
        Self { operations, timestamp, tap_wait, das_wait }
    }
    fn tap(&mut self, action: ActionKind) {
        let event = match action {
            ActionKind::MoveLeft => ReplayEvent::MoveLeft,
            ActionKind::MoveRight => ReplayEvent::MoveRight,
            ActionKind::RotateCW => ReplayEvent::RotateCW,
            ActionKind::RotateCCW => ReplayEvent::RotateCCW,
            ActionKind::Rotate180 => ReplayEvent::Rotate180,
            ActionKind::HardDrop => ReplayEvent::HardDrop,
            ActionKind::Hold => ReplayEvent::Hold,
            _ => unreachable!()
        };
        self.operations.push(self.timestamp);
        self.operations.push(event as u32);
        self.timestamp += self.tap_wait;
        self.operations.push(self.timestamp);
        self.operations.push(event as u32 + 32);
        self.timestamp += self.tap_wait;
    }
    fn das_start(&mut self, dir: ActionKind) {
        let event = match dir {
            ActionKind::DasLeft => ReplayEvent::MoveLeft,
            ActionKind::DasRight => ReplayEvent::MoveRight,
            _ => unreachable!()
        };
        self.operations.push(self.timestamp);
        self.operations.push(event as u32);
        self.timestamp += self.das_wait;
    }
    fn das_release(&mut self, dir: ActionKind) {
        self.das_start(dir);
        *self.operations.last_mut().unwrap() += 32;
    }
    fn debug_keys_assertion(&self, i: usize, path: &[PathNode], piece: PieceType, active_das: Option<ActionKind>, reused: bool, hold: bool) {
        if self.operations.len() + 2 * (active_das.is_some() as usize) != 4 * path[i].keys_pressed() as usize {
            debug!("Generated operations length {} does not match expected length {} before processing move {}",
                   self.operations.len(), 4 * path[i].keys_pressed() as usize, i);
            debug!("Generated operations: {:?}", self.operations.iter().map(|&x| -> i32 { if !(32..175).contains(&x) { x as i32 } else { -(x as i32 - 32) } }).collect::<Vec<_>>());
            debug!("current piece: {}, reused: {}, hold: {}", piece as u8, reused, hold);
            match path[i] {
                PathNode::Normal(node) => {
                    let heights = (0..10).map(|j| node.state.get_height(j)).collect::<Vec<_>>();
                    debug!("Node meta: {:016b}, keys_pressed: {}, board: {:016x}, heights: {:?}", node.meta, node.keys_pressed, node.state.packed_shape, heights);
                }
                PathNode::Pc(node) => {
                    let heights = (0..10).map(|j| node.state.get_column(j)).collect::<Vec<_>>();
                    debug!("PC Node meta: {:016b}, keys_pressed: {}, board: {:016x}, heights: {:?}", node.meta, node.keys_pressed, node.state.raw(), heights);
                }
            }
            panic!();
        }
    }
}

fn export_replay(path: &[PathNode], piece_sequence: &[PieceType], replay_config: &ReplayConfig) -> Vec<u8> {
    let mut consumer = <TechminoConsumer as ReplayConsumer>::from(TECHMINO_DAS_FRAME, replay_config);
    compile_path(path, piece_sequence, &mut consumer);
    debug_assert!(consumer.operations.len() == path.last().unwrap().keys_pressed() as usize * 4, "Operations length must be four times the number of keys pressed");
    let path_data = &consumer.operations;
    debug_assert!(consumer.operations.len() == path.last().unwrap().keys_pressed() as usize * 4, "Operations length must be four times the number of keys pressed");
    let mut bytes = Vec::new();
    for (i, &item) in path_data.iter().enumerate() {
        let mut x = item;
        if i > 0 && i % 2 == 0 {
            x -= path_data[i - 2];
        }
        let mut tmp = Vec::new();
        while x >= 128 {
            tmp.push((x & 0x7F) as u8);
            x /= 128;
        }
        tmp.push(x as u8);
        for v in tmp.iter_mut().skip(1) {
            *v += 128;
        }
        bytes.extend(tmp.into_iter().rev());
    }
    bytes
}

pub fn gen_replay(seed: String, path: &[PathNode], piece_sequence: &[PieceType], replay_config: &ReplayConfig) -> String {
    let metadata = default_config(seed);
    let metadata_bin = serde_json::to_vec(&metadata).expect("Failed to serialize metadata");
    let replay_bin = export_replay(path, piece_sequence, &replay_config);
    
    let final_data = metadata_bin.into_iter().chain(b"\n".iter().copied()).chain(replay_bin).collect::<Vec<u8>>();
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&final_data).expect("Failed to write data to encoder");
    let compressed_data = encoder.finish().expect("Failed to finish compression");
    base64::engine::general_purpose::STANDARD.encode(compressed_data)
}

fn default_config(seed: String) -> Value {
    const {
        debug_assert!(TARGET_LINES == 10 || TARGET_LINES == 20 || TARGET_LINES == 40 || TARGET_LINES == 100 || TARGET_LINES == 400,
                    "Metadata::default is only valid 10/20/40/100/400 sprint");
    }
    let mode_str = format!("sprint_{}l", TARGET_LINES);
    let setting = json!({
        "das": TECHMINO_DAS_FRAME,
        "arr": 0,
        "sddas": 0,
        "sdarr": 0,
        "dascut": 0,
        "ghost": 0.3,
        "rs": "TRS",
        "face": [
        0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
        ],
        "grid": 0.16,
        "warn": true,
        "score": true,
        "bag_line": false,
        "lock_fx": 2,
        "drop_fx": 2,
        "move_fx": 2,
        "clear_fx": 2,
        "block": true,
        "shake_fx": 2,
        "atk_fx": 2,
        "text": true,
        "splash_fx": 2,
        "high_cam": true,
        "next_pos": true,
        "irs": true,
        "smooth": true,
        "center": 1,
        "ims": true,
        "skin": [
        1,7,11,3,14,4,9,1,7,2,6,10,2,13,5,9,15,4,11,3,12,2,16,8,4,10,13,2,8
        ],
        "dropcut": 0,
        "ihs": true
    });
    json!({
        "player": "LKPbot",
        "version": "V0.17.14",
        "setting": setting,
        "date": "2026/06/26 06:26:26",
        "tas_used": false,
        "seed": seed,
        "mod": [],
        "mode": mode_str,
    })
}