use log::debug;
use crate::PieceType;
use crate::config::{ReplayConfig, MAX_PIECES};
use crate::movegen::rebuild_actions_general;
use crate::movegen::Actions;
use crate::search::PathNode;

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
enum ReplayEvent {
    MoveLeft = 1,
    MoveRight = 2,
    RotateCW = 3,
    RotateCCW = 4,
    Rotate180 = 5,
    HardDrop = 6,
    Hold = 8,
}
fn compile_path(path: &[PathNode], piece_sequence: &[PieceType], das_frame: u32, replay_config: &ReplayConfig) -> Vec<u32> {
    // unimplemented!()
    debug_assert!(piece_sequence.len() == MAX_PIECES);
    debug_assert!(!path.is_empty() && path.len() <= piece_sequence.len() + 1,
                  "Path length must fit within the generated piece sequence");
    let mut operations = Vec::new();
    // operations format: (frame, event), (frame, event)...
    let mut timestamp = replay_config.first_op;
    let das_wait = (das_frame as f64 * replay_config.das_ratio) as u32;
    let tap_wait = (das_frame as f64 * replay_config.short_ratio) as u32;
    
    let push_down = |ops: &mut Vec<u32>, ts: u32, ev: ReplayEvent| {
        ops.push(ts);
        ops.push(ev as u32);
    };
    let push_up = |ops: &mut Vec<u32>, ts: u32, ev: ReplayEvent| {
        ops.push(ts);
        ops.push(ev as u32 + 32);
    };
    let tap = |ops: &mut Vec<u32>, ts: &mut u32, ev: ReplayEvent| {
        push_down(ops, *ts, ev);
        *ts += tap_wait;
        push_up(ops, *ts, ev);
        *ts += tap_wait;
        if ev == ReplayEvent::HardDrop {
            *ts += 1;
        }
    };

    let mut active_das: Option<ReplayEvent> = None;
    let mut is_first = true;
    let mut last_is_hold_only = false;
    for i in 0..path.len()-1 {
        let (reused, hold, hold_only, actions) = rebuild_actions_general(&path[i], piece_sequence[i], &path[i + 1]);
        if hold_only {
            debug_assert!(actions.is_empty(), "Hold-only move should have no actions");
            // hold_only means this step only has one hold action but not drop
            // theoretically the game do not allow double-hold, but it's possible for our replay, such as hold_only -> hold -> harddrop
            // but this sequence is not optimal (we can directly harddrop -> hold_only), so we allow it for simplicity.
            last_is_hold_only = true;
            continue;
        }
        if !is_first {
            if !hold_only {
                if !reused {
                    if let Some(das_key) = active_das {
                        push_up(&mut operations, timestamp, das_key);
                        active_das = None;
                    }
                } else {
                    debug_assert!(active_das.is_some(), "Cannot reuse move when inactive DAS");
                    debug_assert!(actions[0] == Actions::DasLeft && active_das == Some(ReplayEvent::MoveLeft)
                            || actions[0] == Actions::DasRight && active_das == Some(ReplayEvent::MoveRight),
                            "Reused move must match active DAS");
                }
            }
            tap(&mut operations, &mut timestamp, ReplayEvent::HardDrop);
            if last_is_hold_only {
                tap(&mut operations, &mut timestamp, ReplayEvent::Hold);
            }
        } else if last_is_hold_only {
            tap(&mut operations, &mut timestamp, ReplayEvent::Hold);
        }
        last_is_hold_only = false;
        is_first = false;
        if operations.len() + 2 * (active_das.is_some() as usize) != 4 * path[i].keys_pressed() as usize {
            debug!("Generated operations length {} does not match expected length {} before processing move {}",
                   operations.len(), 4 * path[i].keys_pressed() as usize, i);
            debug!("Generated operations: {:?}", operations.iter().map(|&x| -> i32 { if x < 32 || x >= 175 { x as i32 } else { -(x as i32 - 32) } }).collect::<Vec<_>>());
            debug!("current piece: {}, reused: {}, hold: {}", piece_sequence[i] as u8, reused, hold);
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
        if hold {
            tap(&mut operations, &mut timestamp, ReplayEvent::Hold);
        }
        for &action in actions[reused as usize..].iter() { // if reused, skip first Das
            match action {
                Actions::RotateCW => tap(&mut operations, &mut timestamp, ReplayEvent::RotateCW),
                Actions::RotateCCW => tap(&mut operations, &mut timestamp, ReplayEvent::RotateCCW),
                Actions::Rotate180 => tap(&mut operations, &mut timestamp, ReplayEvent::Rotate180),
                Actions::MoveLeft => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    tap(&mut operations, &mut timestamp, ReplayEvent::MoveLeft);
                }
                Actions::MoveRight => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    tap(&mut operations, &mut timestamp, ReplayEvent::MoveRight);
                }
                Actions::DasLeft => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    push_down(&mut operations, timestamp, ReplayEvent::MoveLeft);
                    timestamp += das_wait;
                    // note that the sequence may be [DasLeft, CCW, DasLeftUp], which can be finished in das_wait frames.
                    // but we will use das_wait+tap_wait frames, so the replay may not be optimal.
                    active_das = Some(ReplayEvent::MoveLeft);
                }
                Actions::DasRight => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    push_down(&mut operations, timestamp, ReplayEvent::MoveRight);
                    timestamp += das_wait;
                    active_das = Some(ReplayEvent::MoveRight);
                }
                Actions::DasLeftUp => {
                    debug_assert!(active_das == Some(ReplayEvent::MoveLeft), "Can only release active DAS");
                    if action == actions.last().copied().unwrap() {
                        // we skip the release if it's the last action
                        continue;
                    }
                    push_up(&mut operations, timestamp, ReplayEvent::MoveLeft);
                    active_das = None;
                }
                Actions::DasRightUp => {
                    debug_assert!(active_das == Some(ReplayEvent::MoveRight), "Can only release active DAS");
                    if action == actions.last().copied().unwrap() {
                        continue;
                    }
                    push_up(&mut operations, timestamp, ReplayEvent::MoveRight);
                    active_das = None;
                }
            }
        }
    }
    if let Some(das_key) = active_das {
        push_up(&mut operations, timestamp, das_key);
    }
    tap(&mut operations, &mut timestamp, ReplayEvent::HardDrop);
    debug_assert!(operations.len() == path.last().unwrap().keys_pressed() as usize * 4, "Operations length must be four times the number of keys pressed");
    operations
}

pub fn export_replay(path: &[PathNode], piece_sequence: &[PieceType], das_frame: u32, replay_config: &ReplayConfig) -> Vec<u8> {
    let path_data = compile_path(path, piece_sequence, das_frame, replay_config);
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
        for i in 1..tmp.len() {
            tmp[i] += 128;
        }
        bytes.extend(tmp.into_iter().rev());
    }
    bytes
}