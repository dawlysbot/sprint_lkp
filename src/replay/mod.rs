pub mod techmino;
#[cfg(feature = "jstris")]
pub mod jstris;

use crate::PieceType;
use crate::config::{ENGINE, GameEngine, MAX_PIECES, ReplayConfig};
use crate::movegen::compile_action;
use crate::movegen::Action;
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
    fn burst(&mut self, actions: Vec<ActionKind>);
    fn pipeline(&mut self, dir: ActionKind);
    fn debug_keys_assertion(&self, _i: usize, _path: &[PathNode], _active_das: Option<ActionKind>, _reused: bool);
}

fn compile_replay<C: ReplayConsumer>(path: &[PathNode], piece_sequence: &[PieceType], consumer: &mut C) {
    debug_assert!(piece_sequence.len() == MAX_PIECES);
    debug_assert!(!path.is_empty() && path.len() <= piece_sequence.len() + 1,
                  "Path length must fit within the generated piece sequence");
    
    let steps = compile_action(path, piece_sequence);
    let mut active_das: Option<ActionKind> = None;
    for step in steps {
        let actions = &step.actions;
        if step.hold {
            consumer.tap(ActionKind::Hold);
        }
        // println!("hold = {}, reused = {}, burst = {}, pipeline = {}, actions = {:?}", step.hold, step.reused, step.burst, step.pipeline, actions.iter().map(|&x| x as u8).collect::<Vec<_>>());
        if step.reused {
            debug_assert!(active_das.is_some(), "Cannot reuse move when inactive DAS");
            debug_assert!(matches!((actions.first().unwrap(), active_das),
                         (Action::DasLeft, Some(ActionKind::DasLeft)) | (Action::DasRight, Some(ActionKind::DasRight))
                        ),"Reused move must match active DAS");
        }
        for (idx, &action) in actions.iter().enumerate().skip(step.reused as usize) { // if reused, skip first Das
            let is_last = idx == actions.len() - 1;
            match action {
                Action::RotateCW => consumer.tap(ActionKind::RotateCW),
                // for jstris replay, after every rotation we should apply the das
                Action::RotateCCW => consumer.tap(ActionKind::RotateCCW),
                Action::Rotate180 => consumer.tap(ActionKind::Rotate180),
                Action::MoveLeft => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    if step.pipeline && is_last {
                        consumer.pipeline(ActionKind::DasLeft);
                        active_das = Some(ActionKind::DasLeft);
                        break;
                    }
                    consumer.tap(ActionKind::MoveLeft);
                }
                Action::MoveRight => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    if step.pipeline && is_last {
                        consumer.pipeline(ActionKind::DasRight);
                        active_das = Some(ActionKind::DasRight);
                        break;
                    }
                    consumer.tap(ActionKind::MoveRight);
                }
                Action::DasLeft => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    consumer.das_start(ActionKind::DasLeft);
                    // note that the sequence may be [DasLeft, CCW, DasLeftUp], which can be finished in das_wait frames.
                    // but we will use das_wait+tap_wait frames, so the replay may not be optimal.
                    active_das = Some(ActionKind::DasLeft);
                }
                Action::DasRight => {
                    debug_assert!(active_das.is_none(), "Cannot have press when active DAS");
                    consumer.das_start(ActionKind::DasRight);
                    active_das = Some(ActionKind::DasRight);
                }
                Action::DasLeftUp => {
                    debug_assert!(active_das == Some(ActionKind::DasLeft), "Can only release active DAS");
                    if step.burst {
                        consumer.burst(actions[idx + 1..].iter().map(|act| match act {
                            Action::RotateCW => ActionKind::RotateCW,
                            Action::RotateCCW => ActionKind::RotateCCW,
                            Action::Rotate180 => ActionKind::Rotate180,
                            Action::MoveRight => ActionKind::MoveRight,
                            _ => unreachable!(),
                        }).collect::<Vec<_>>());
                        break;
                    }
                    consumer.das_release(ActionKind::DasLeft);
                    active_das = None;
                }
                Action::DasRightUp => {
                    debug_assert!(active_das == Some(ActionKind::DasRight), "Can only release active DAS");
                    if step.burst {
                        consumer.burst(actions[idx + 1..].iter().map(|act| match act {
                            Action::RotateCW => ActionKind::RotateCW,
                            Action::RotateCCW => ActionKind::RotateCCW,
                            Action::Rotate180 => ActionKind::Rotate180,
                            Action::MoveLeft => ActionKind::MoveLeft,
                            _ => unreachable!(),
                        }).collect::<Vec<_>>());
                        break;
                    }
                    consumer.das_release(ActionKind::DasRight);
                    active_das = None;
                }
            }
        }
        if !step.pipeline && !step.burst {
            consumer.tap(ActionKind::HardDrop);
        }
        consumer.debug_keys_assertion(step.idx + 1, path, active_das, step.burst || step.pipeline); // note that when next_reused, either burst or pipeline
    }
}