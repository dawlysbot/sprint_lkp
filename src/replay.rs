use crate::PieceType;
use crate::config::{ReplayConfig, MAX_PIECES};
use crate::bitboard::HOLD_MASK;
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
    debug_assert!(path.len() == piece_sequence.len() + 1 || path.len() == piece_sequence.len() && path.last().unwrap().meta() & HOLD_MASK == 0,
                  "Path length must be one more than piece sequence length, or equal when last node has no hold");
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
        if ev == ReplayEvent::HardDrop {
            *ts += 1;
        }
    };

    let mut active_das: Option<ReplayEvent> = None;
    let mut is_first = true;
    for i in 0..path.len()-1 {
        let (reused, hold, hold_only, actions) = rebuild_actions_general(&path[i], piece_sequence[i], &path[i + 1]);
        if hold_only {
            debug_assert!(actions.is_empty(), "Hold-only move should have no actions");
            // hold_only means this step only has one hold action but not drop
            // theoretically the game do not allow double-hold, but it's possible for our replay, such as hold_only -> hold -> harddrop
            // but this sequence is not optimal (we can directly harddrop -> hold_only), so we allow it for simplicity.
            tap(&mut operations, &mut timestamp, ReplayEvent::Hold);
            continue;
        }
        if !is_first {
            if !reused {
                if let Some(das_key) = active_das {
                    push_up(&mut operations, timestamp, das_key);
                    active_das = None;
                }
            } else {
                debug_assert!(active_das.is_some(), "Cannot reuse move when inactive DAS");
                debug_assert!(actions[0] == Actions::MoveLeft && active_das == Some(ReplayEvent::MoveLeft)
                           || actions[0] == Actions::MoveRight && active_das == Some(ReplayEvent::MoveRight),
                           "Reused move must match active DAS");
            }
            tap(&mut operations, &mut timestamp, ReplayEvent::HardDrop);
        }
        is_first = false;
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
    debug_assert!(operations.len() == path.last().unwrap().keys_pressed() as usize * 2, "Operations length must be twice the number of keys pressed");
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
        while x >= 128 {
            bytes.push(((x & 0x7F) | 0x80) as u8);
            x /= 128;
        }
        bytes.push(x as u8);
    }
    bytes
}