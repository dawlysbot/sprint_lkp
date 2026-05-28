pub mod techmino;
#[cfg(feature = "jstris")]
pub mod jstris;

use crate::PieceType;
use crate::config::{ENGINE, GameEngine, MAX_PIECES, ReplayConfig};
use crate::movegen::rebuild_actions_general;
use crate::movegen::Actions;
use crate::search::PathNode;

pub fn gen_replay(seed: String, path: &[PathNode], piece_sequence: &[PieceType]) -> String {
    match ENGINE {
        GameEngine::TECHMINO => techmino::gen_replay(seed, path, piece_sequence, &ReplayConfig::default()),
        #[cfg(feature = "jstris")]
        GameEngine::JSTRIS => jstris::gen_replay(seed, path, piece_sequence, &ReplayConfig::default()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    MoveLeft,
    MoveRight,
    RotateCW,
    RotateCCW,
    Rotate180,
    DasLeft,
    DasRight,
    HardDrop,
    Hold,
}
pub trait ReplayConsumer {
    fn from(das_frame: u32, replay_config: &ReplayConfig) -> Self;
    fn tap(&mut self, action: ActionKind);
    fn das_start(&mut self, dir: ActionKind);
    fn das_release(&mut self, dir: ActionKind);
    fn debug_keys_assertion(&self, _i: usize, _path: &[PathNode], _piece: PieceType, _active_das: Option<ActionKind>, _reused: bool, _hold: bool);
}

fn compile_path<C: ReplayConsumer>(path: &[PathNode], piece_sequence: &[PieceType], consumer: &mut C) {
    debug_assert!(piece_sequence.len() == MAX_PIECES);
    debug_assert!(!path.is_empty() && path.len() <= piece_sequence.len() + 1,
                  "Path length must fit within the generated piece sequence");

    let mut active_das: Option<ActionKind> = None;
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
                        consumer.das_release(das_key);
                        active_das = None;
                    }
                } else {
                    debug_assert!(active_das.is_some(), "Cannot reuse move when inactive DAS");
                    debug_assert!(actions[0] == Actions::DasLeft && active_das == Some(ActionKind::DasLeft)
                            || actions[0] == Actions::DasRight && active_das == Some(ActionKind::DasRight),
                            "Reused move must match active DAS");
                }
            }
            consumer.tap(ActionKind::HardDrop);
            if last_is_hold_only {
                consumer.tap(ActionKind::Hold);
            }
        } else if last_is_hold_only {
            consumer.tap(ActionKind::Hold);
        }
        last_is_hold_only = false;
        is_first = false;
        consumer.debug_keys_assertion(i, path, piece_sequence[i], active_das, reused, hold);
        if hold {
            consumer.tap(ActionKind::Hold);
        }
        for &action in actions[reused as usize..].iter() { // if reused, skip first Das
            match action {
                Actions::RotateCW => consumer.tap(ActionKind::RotateCW),
                // for jstris replay, after every rotation we should apply the das
                Actions::RotateCCW => consumer.tap(ActionKind::RotateCCW),
                Actions::Rotate180 => consumer.tap(ActionKind::Rotate180),
                Actions::MoveLeft => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    consumer.tap(ActionKind::MoveLeft);
                }
                Actions::MoveRight => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    consumer.tap(ActionKind::MoveRight);
                }
                Actions::DasLeft => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    consumer.das_start(ActionKind::DasLeft);
                    // note that the sequence may be [DasLeft, CCW, DasLeftUp], which can be finished in das_wait frames.
                    // but we will use das_wait+tap_wait frames, so the replay may not be optimal.
                    active_das = Some(ActionKind::DasLeft);
                }
                Actions::DasRight => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    consumer.das_start(ActionKind::DasRight);
                    active_das = Some(ActionKind::DasRight);
                }
                Actions::DasLeftUp => {
                    debug_assert!(active_das == Some(ActionKind::DasLeft), "Can only release active DAS");
                    if action == actions.last().copied().unwrap() {
                        // we skip the release if it's the last action
                        continue;
                    }
                    consumer.das_release(ActionKind::DasLeft);
                    active_das = None;
                }
                Actions::DasRightUp => {
                    debug_assert!(active_das == Some(ActionKind::DasRight), "Can only release active DAS");
                    if action == actions.last().copied().unwrap() {
                        continue;
                    }
                    consumer.das_release(ActionKind::DasRight);
                    active_das = None;
                }
            }
        }
    }
    if let Some(das_key) = active_das {
        consumer.das_release(das_key);
    }
    consumer.tap(ActionKind::HardDrop);
}