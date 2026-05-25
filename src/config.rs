pub const TARGET_LINES: u16 = 40;
pub const MAX_PIECES: usize = TARGET_LINES as usize * 5 / 2 + 2;

pub const BEAM_WIDTH: usize = 5000;
pub const ENDGAME_DEPTH: usize = 6;
pub const ENDGAME_START: usize = MAX_PIECES - ENDGAME_DEPTH;

pub const EVALUATE_DEPTH: usize = 4;
pub struct EvaluateConfig {
    pub weight_hold: f64,
    pub weight_i_tight: f64,
    pub weight_i_tight_light: f64,
    pub weight_bump: f64,
    pub weight_bump_light: f64,
    pub weight_height_warning: f64,
    pub weight_height_danger: f64,
}
impl Default for EvaluateConfig {
    fn default() -> Self {
        EvaluateConfig {
            weight_hold: 2.4,
            weight_i_tight: 3.0,
            weight_i_tight_light: 3.0 * 0.25,
            weight_bump: 0.6,
            weight_bump_light: 0.6 * 0.3,
            weight_height_warning: 1.5,
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
            das_ratio: 1.5,
            short_ratio: 0.5,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Setting {
    pub das: u32,
    pub arr: u32,
    pub sddas: u32,
    pub sdarr: u32,
    pub dascut: u32,
    pub ghost: f64,
    pub rs: String,
    pub face: [u8; 29],
    pub grid: f64,
    pub warn: bool,
    pub score: bool,
    pub bag_line: bool,
    pub lock_fx: u32,
    pub drop_fx: u32,
    pub move_fx: u32,
    pub clear_fx: u32,
    pub block: bool,
    pub shake_fx: u32,
    pub atk_fx: u32,
    pub text: bool,
    pub splash_fx: u32,
    pub high_cam: bool,
    pub next_pos: bool,
    pub irs: bool,
    pub smooth: bool,
    pub center: u8,
    pub ims: bool,
    pub skin: [u8; 29],
    pub dropcut: u32,
    pub ihs: bool,
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Metadata {
    pub player: String,
    pub version: String,
    pub setting: Setting,
    pub date: String,
    pub tas_used: bool,
    pub seed: u32,
    #[serde(rename = "mod")]
    pub mod_list: Vec<String>,
    pub mode: String,
}
impl Default for Metadata {
    fn default() -> Self {
        const {
            debug_assert!(TARGET_LINES == 10 || TARGET_LINES == 20 || TARGET_LINES == 40 || TARGET_LINES == 100 || TARGET_LINES == 200 || TARGET_LINES == 400,
                      "Metadata::default is only valid 10/20/40/100/200/400 sprint");
        }
        Metadata {
            player: "LKPbot".to_string(),
            version: "V0.17.14".to_string(),
            setting: Setting {
                das: 5,
                arr: 0,
                sddas: 0,
                sdarr: 0,
                dascut: 0,
                ghost: 0.3,
                rs: "TRS".to_string(),
                face: [0; 29],
                grid: 0.16,
                warn: true,
                score: true,
                bag_line: false,
                lock_fx: 2,
                drop_fx: 2,
                move_fx: 2,
                clear_fx: 2,
                block: true,
                shake_fx: 2,
                atk_fx: 2,
                text: true,
                splash_fx: 2,
                high_cam: true,
                next_pos: true,
                irs: true,
                smooth: true,
                center: 1,
                ims: true,
                skin: [1,7,11,3,14,4,9,1,7,2,6,10,2,13,5,9,15,4,11,3,12,2,16,8,4,10,13,2,8],
                dropcut: 0,
                ihs: true
            },
            date: "2026/06/26 06:26:26".to_string(),
            tas_used: false,
            seed: 1866555892u32,
            mod_list: Vec::new(),
            mode: format!("sprint_{}l", TARGET_LINES),
        }
    }
}