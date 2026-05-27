pub const TARGET_LINES: u16 = 40;
pub const ALLOW_HOLD: bool = true;
pub const PC_END: bool = false;
pub const MAX_PIECES: usize = TARGET_LINES as usize * 5 / 2 + (ALLOW_HOLD as usize) + (!PC_END as usize);

pub const BEAM_WIDTH: usize = 10000;
pub const DIVERSITY_BUCKET_SIZE: usize = 200;
pub const ENDGAME_DEPTH: usize = 7;
pub const ENDGAME_START: usize = MAX_PIECES - ENDGAME_DEPTH;

pub enum GameEngine {
    TECHMINO,
    JSTRIS
}
pub const ENGINE: GameEngine = GameEngine::TECHMINO;

pub const EVALUATE_DEPTH: usize = 5;
pub struct EvaluateConfig {
    pub weight_hold: f64,
    pub weight_i_tight: f64,
    pub weight_i_tight_light: f64,
    pub weight_parity: f64,
    pub weight_bump: f64,
    pub weight_bump_light: f64,
    pub weight_das_preserve: f64,
    pub weight_height_warning: f64,
    pub weight_height_danger: f64,
}
impl Default for EvaluateConfig {
    fn default() -> Self {
        EvaluateConfig {
            weight_hold: 2.0,
            weight_i_tight: 3.0,
            weight_i_tight_light: 3.0 * 0.25,
            weight_parity: 0.6,
            weight_bump: 0.6,
            weight_bump_light: 0.6 * 0.35,
            weight_das_preserve: 0.25,
            weight_height_warning: 0.7,
            weight_height_danger: 6.0,
        }
    }
}

pub struct ReplayConfig {
    pub first_op: u32,
    pub das_ratio: f64,
    pub short_ratio: f64,
}
impl Default for ReplayConfig {
    fn default() -> Self {
        ReplayConfig {
            first_op: 180,
            das_ratio: 1.2,
            short_ratio: 0.5,
        }
    }
}

pub const TECHMINO_DAS_FRAME: u32 = 6;
pub const JSTRIS_DAS_MS: u32 = 100;