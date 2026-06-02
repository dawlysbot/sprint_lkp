use crate::config::{PC_END, TARGET_LINES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchNode<B: Board> {
    pub state: B,
    pub keys_pressed: u16,
    pub meta: u16,
    // note that, for search::hash, we will use state (50bit) | meta (14bit).
    // 0-8: lines cleared, 9-10: das state, 11-13: hold piece
    // so only support 511 lines
    // actually, lines_cleared*10=(depth-(hold.is_some))*4-occupied_cells, so we may not store this in meta.
    // in that case, we can support more lines, e.g. 1000 lines.
    pub parent_idx: usize,
}
pub const HOLD_MASK: u16 = 0x3800;
pub const DAS_MASK: u16 = 0x600;
pub const LINES_CLEARED_MASK: u16 = 0x1FF;
impl<B: Board> SearchNode<B> {
    pub fn initial() -> Self {
        SearchNode {
            state: B::default(),
            meta: 0,
            keys_pressed: 0,
            parent_idx: usize::MAX,
        }
    }
}
impl SearchNode<ShapeBoard> {
    pub fn to_bitboard(&self) -> SearchNode<BitBoard> {
        SearchNode {
            state: BitBoard::from_shape_board(self.state),
            meta: self.meta,
            keys_pressed: self.keys_pressed,
            parent_idx: self.parent_idx,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FastMask(u32);
impl FastMask {
    #[inline(always)]
    fn guard_mask(self) -> u32 { self.0 & 0o40404040 }
    #[inline(always)]
    fn added_mask(self) -> u32 { self.0 & 0o07070707 }
    #[inline(always)]
    fn packed_bottoms(self) -> u32 { (self.0 & 0o30303030) >> 3 }
    #[inline(always)]
    fn max_height(self) -> u32 { self.0 >> 24 }
}

pub trait Board: Clone + Copy + PartialEq + Eq + std::hash::Hash + Default {
    fn drop_piece(&self, x: u8, shape_idx: u8, lines_cleared: u16) -> Option<(Self, u8)>;
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShapeBoard {
    pub packed_shape: u64,
}
impl ShapeBoard {
    pub const COL_MASK: u64 = 0x1F;
    pub const LANE_SIZE: u8 = 6;
    const GUARD: u64 = 0o40404040404040404040;

    #[inline(always)]
    pub fn get_height(&self, col: usize) -> u64 {
        (self.packed_shape >> (col as u8 * Self::LANE_SIZE)) & Self::COL_MASK
    }
}
impl Board for ShapeBoard {
    #[inline]
    fn drop_piece(&self, x: u8, shape_idx: u8, lines_cleared: u16) -> Option<(Self, u8)> {
        let mask = &SHAPE_TABLE[shape_idx as usize];
        let guard_mask = mask.guard_mask();
        let lane1_mask = guard_mask >> 5;
        debug_assert!(x as u32 + mask.guard_mask().count_ones() <= 10);
        let shift = x * ShapeBoard::LANE_SIZE;
        let cols = (self.packed_shape >> shift) as u32 & (guard_mask - lane1_mask);
        let diff = (cols | guard_mask) - mask.packed_bottoms();
        #[cfg(feature = "quad_only")]
        {
            if diff != (diff & 0x3F) * lane1_mask {
                return None;
            }
            let peak_height = ((diff & 0x1F) + mask.max_height()) as u16;
            if peak_height > 20 || peak_height > TARGET_LINES - lines_cleared {
                return None;
            }
            let new_shape = self.packed_shape + ((mask.added_mask() as u64) << shift);
            let g = new_shape.wrapping_sub(Self::GUARD >> 5) & Self::GUARD;
            if g == 0 {
                let g = new_shape.wrapping_sub(Self::GUARD >> 3) & Self::GUARD;
                (g == 0).then(|| (Self { packed_shape: new_shape - 4 * const { Self::GUARD >> 5 } }, 4))
            } else {
                Some((Self { packed_shape: new_shape }, 0))
            }
        }
        #[cfg(not(feature = "quad_only"))]
        {
            let base_y_offset = (diff & 0x3F).max((diff >> 6) & 0x3F).max(((diff >> 12) & 0x3F).max((diff >> 18) & 0x3F));
            debug_assert!(base_y_offset >= 32);
            let base_y = base_y_offset & 0x1F;
            let peak_height = (base_y + mask.max_height()) as u16;
            if peak_height > 20 || peak_height > TARGET_LINES - lines_cleared {
                return None;
            }
            let bit_shape = BIT_TABLE[shape_idx as usize] & 0xFFFFFF;
            let board_lane1 = ShapeBoard::GUARD >> 5;
            let mut cleared_mask = 0u32;
            let mut lines_cleared_count = 0;
            for r in 0..mask.max_height() {
                let board_guard = ((self.packed_shape | ShapeBoard::GUARD) - ((base_y + r + 1) as u64) * board_lane1) & ShapeBoard::GUARD;
                // the columns that higher than base_y + r
                let piece_guard = ((((bit_shape >> r) & lane1_mask) << 5) as u64) << shift;
                // the columns that the block cell exists
                if (board_guard | piece_guard) & ShapeBoard::GUARD == ShapeBoard::GUARD {
                    cleared_mask |= 1 << r;
                    lines_cleared_count += 1;
                }
            }
            let cleared_shape = bit_shape & !(cleared_mask * lane1_mask);
            let stacked_guard = (guard_mask - ((base_y_offset * lane1_mask) - diff)) & guard_mask;
            // this will not cause overflow, because diff = col + 32 - bottom, col <= 20, bottom <= 2
            let stacked_full = stacked_guard - (stacked_guard >> 5);
            if (cleared_shape & !stacked_full) != 0 {
                return None;
            }
            let added_board = self.packed_shape + (((mask.added_mask() & stacked_full) as u64) << shift);
            let mut drop_total = 0u64;
            if cleared_mask != 0 {
                for r in 0..mask.max_height() {
                    if cleared_mask & (1 << r) != 0 {
                        drop_total += (((added_board | Self::GUARD) - (base_y + r) as u64 * board_lane1) & Self::GUARD) >> 5;
                    }
                }
            }
            Some((Self { packed_shape: added_board - drop_total }, lines_cleared_count as u8))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BitBoard(u64);
impl BitBoard {
    const BOARD_MASK: u64 = 0x0FFF_FFFF_FFFF_FFFF;
    pub const ROW_MASK: u64 = 0x0041_0410_4104_1041;
    const COL_MASK: u64 = 0x3F;
    #[inline(always)]
    pub fn get_column(&self, col: usize) -> u64 {
        self.0 >> (col * 6) & Self::COL_MASK
    }
    #[inline(always)]
    pub fn raw(&self) -> u64 {
        self.0
    }
    #[inline(always)]
    pub fn occupied_cells(&self) -> u16 {
        self.0.count_ones() as u16
    }
    #[inline(always)]
    fn full_rows(board: u64) -> u64 {
        let mut rows = board;
        rows &= board >> 12;
        // bit 0-48
        rows &= rows >> 6;
        rows &= rows >> 12;
        rows &= rows >> 24;
        debug_assert!(rows < Self::COL_MASK);
        rows
    }
    #[inline]
    pub fn from_shape_board(shape: ShapeBoard) -> Self {
        let mut board = 0;
        for col in 0..10 {
            let height = shape.get_height(col);
            debug_assert!(height <= 4);
            board |= ((1u64 << height) - 1) << (col * 6);
        }
        Self(board)
    }
    fn try_place(&self, x: u8, shape_idx: u8, lines_cleared: u16) -> Option<Self> {
        let shape = BIT_TABLE[shape_idx as usize];
        debug_assert!(TARGET_LINES - lines_cleared <= 4);
        let mut height = (((TARGET_LINES - lines_cleared) as u8) + {2 * !PC_END as u8}).checked_sub((shape >> 24) as u8)?;
        // means the highest row of the piece can be put
        let mask = ((shape as u64) & 0xFFFFFF) << (x * 6);
        // hit check: if I put the piece at height, compute the hitbox
        let mut hitbox = mask << height; // It's guaranteed that this << won't overflow
        hitbox |= (hitbox & (BitBoard::ROW_MASK * 0x1F)) << 1; // 1 line blow-up
        hitbox |= (hitbox & (BitBoard::ROW_MASK * 0xF)) << 2; // 3 lines blow-up
        hitbox |= (hitbox & (BitBoard::ROW_MASK * 0x3)) << 4; // 7 lines blow-up
        if self.0 & hitbox != 0 {
            return None;
        }
        while height > 0 {
            if self.0 & (mask << (height - 1)) != 0 {
                break;
            }
            height -= 1;
        }
        debug_assert!(self.0 & (mask << height) == 0);
        let placed = self.0 | (mask << height);
        debug_assert!(placed < Self::BOARD_MASK);
        Some(Self(placed))
    }
    fn clear_lines(&self) -> (Self, u8) {
        let cleared_lines = Self::full_rows(self.0);
        let mut new_board = self.0;
        if cleared_lines & 32 != 0 {
            new_board &= BitBoard::ROW_MASK * 0x1F;
        }
        if cleared_lines & 16 != 0 {
            new_board = new_board & (BitBoard::ROW_MASK * 0xF) | (new_board & (BitBoard::ROW_MASK * 0x20)) >> 1;
        }
        if cleared_lines & 8 != 0 {
            new_board = new_board & (BitBoard::ROW_MASK * 7) | (new_board & (BitBoard::ROW_MASK * 0x30)) >> 1;
        }
        if cleared_lines & 4 != 0 {
            new_board = new_board & (BitBoard::ROW_MASK * 3) | (new_board & (BitBoard::ROW_MASK * 0x38)) >> 1;
        }
        if cleared_lines & 2 != 0 {
            new_board = new_board & (BitBoard::ROW_MASK) | (new_board & (BitBoard::ROW_MASK * 0x3C)) >> 1;
        }
        if cleared_lines & 1 != 0 {
            new_board = (new_board & (BitBoard::ROW_MASK * 0x3E)) >> 1;
        }
        debug_assert!(new_board < Self::BOARD_MASK);
        (Self(new_board), cleared_lines.count_ones() as u8)
    }
}
impl Board for BitBoard {
    #[inline]
    fn drop_piece(&self, x: u8, shape_idx: u8, lines_cleared: u16) -> Option<(Self, u8)> {
        let placed = self.try_place(x, shape_idx, lines_cleared)?;
        Some(placed.clear_lines())
    }
}

pub const SHAPE_RANGES: [usize; 8] = [
    0,2,4,8,12,16,17,19
];
const SHAPE_PARAMS: [(u8, [u8; 4], u8, u32); 19] = [
    // Z
    (3, [1,0,0,0], 2, 0x121),
    (2, [0,1,0,0], 3, 0x22),
    // S
    (3, [0,0,1,0], 2, 0x121),
    (2, [1,0,0,0], 3, 0x22),
    // J
    (3, [0,0,0,0], 2, 0x211),
    (2, [0,2,0,0], 3, 0x31),
    (3, [1,1,0,0], 2, 0x112),
    (2, [0,0,0,0], 3, 0x13),
    // L
    (3, [0,0,0,0], 2, 0x112),
    (2, [0,0,0,0], 3, 0x31),
    (3, [0,1,1,0], 2, 0x211),
    (2, [2,0,0,0], 3, 0x13),
    // T
    (3, [0,0,0,0], 2, 0x121),
    (2, [0,1,0,0], 3, 0x31),
    (3, [1,0,1,0], 2, 0x121),
    (2, [1,0,0,0], 3, 0x13),
    // O
    (2, [0,0,0,0], 2, 0x22),
    // I
    (4, [0,0,0,0], 1, 0x1111),
    (1, [0,0,0,0], 4, 0x4),
];
const FINESSE_PARAMS: [(u8, [u8; 10], u64, u64, u64, u64); 19] = [
    // Z
    (3, [1,2,1,0,1,2,2,1,0,0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    (2, [2,2,2,1,1,2,3,2,2,0], 0x110000111, 0x101000100, 0x000001101, 0x001000000),
    // S
    (3, [1,2,1,0,1,2,2,1,0,0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    (2, [2,2,2,1,1,2,3,2,2,0], 0x110000111, 0x101000100, 0x000001101, 0x001000000),
    // J
    (3, [1,2,1,0,1,2,2,1,0,0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    (2, [2,2,3,2,1,2,3,3,2,0], 0x111000011, 0x101100010, 0x001001111, 0x000000100),
    (3, [2,3,2,1,2,3,3,2,0,0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    (2, [2,3,2,1,2,3,3,2,2,0], 0x110000111, 0x111000100, 0x010011101, 0x000000100),
    // L
    (3, [1,2,1,0,1,2,2,1,0,0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    (2, [2,2,3,2,1,2,3,3,2,0], 0x111000011, 0x101100010, 0x001001111, 0x000000100),
    (3, [2,3,2,1,2,3,3,2,0,0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    (2, [2,3,2,1,2,3,3,2,2,0], 0x110000111, 0x111000100, 0x010011101, 0x000000100),
    // T
    (3, [1,2,1,0,1,2,2,1,0,0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    (2, [2,2,3,2,1,2,3,3,2,0], 0x111000011, 0x101100010, 0x001001111, 0x000000100),
    (3, [2,3,2,1,2,3,3,2,0,0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    (2, [2,3,2,1,2,3,3,2,2,0], 0x110000111, 0x111000100, 0x010011101, 0x000000100),
    // O
    (2, [1,2,2,1,0,1,2,2,1,0], 0x110000011, 0x101100010, 0x010001101, 0x001000100),
    // I
    (4, [1,2,1,0,1,2,1,0,0,0], 0x1100011, 0x1110010, 0x0100111, 0x0000000),
    (1, [2,2,2,2,1,1,2,2,2,2], 0x1110000111, 0x1001000000, 0x0000001001, 0x0001001000),
];
pub const FINESSE_TABLE: [u64; 19] = {
    let mut table = [0u64; 19];
    let mut i = 0;
    while i < FINESSE_PARAMS.len() {
        let (width, ops, reuse, provide_l, provide_r, special) = FINESSE_PARAMS[i];
        // 3 bits per target column, 30bits in total
        let mut rightmost = 9;
        while rightmost > 0 && ops[rightmost] == 0 {
            rightmost -= 1;
        }
        let mut mask = 0u64;
        let mut j: usize = rightmost;
        let merged = reuse | provide_l << 1 | provide_r << 2 | special << 3;
        loop {
            debug_assert!(ops[j] <= 0b11, "Operation value must be between 0 and 3");
            mask = mask << 6 | ops[j] as u64 | ((merged >> ((rightmost - j) * 4) & 0xF_u64) << 2);
            if j == 0 {
                break;
            }
            j -= 1;
        }
        table[i] = mask | (width as u64) << 60;
        i += 1;
    }
    table
};
const SHAPE_TABLE: [FastMask; 19] = {
    let mut table = [FastMask(0); 19];
    let mut i = 0;
    while i < SHAPE_PARAMS.len() {
        let (width, bottom, max_height, added_mask) = SHAPE_PARAMS[i];
        let parsed_added_mask = {
            let mut mask = 0;
            let mut i = 0;
            let mut cells = 0;
            while i < width {
                let height = (added_mask >> (4 * (width - 1 - i))) & 0xF;
                mask |= height << (ShapeBoard::LANE_SIZE * i);
                assert!(height <= 4);
                assert!(bottom[i as usize] < 4);
                cells += height;
                i += 1;
            }
            assert!(cells == 4, "added_mask must have exactly 4 cells");
            assert!(mask < 1 << 19, "added_mask must be smaller than 2**19");
            mask
        };
        let packed_bottoms = (bottom[0] as u32)
            | (bottom[1] as u32) << 6
            | (bottom[2] as u32) << 12
            | (bottom[3] as u32) << 18;
        let guard_mask = 0o40404040 & ((1u32 << (width * 6)) - 1);
        assert!(guard_mask.count_ones() as u8 == width);
        let packed = FastMask(parsed_added_mask | packed_bottoms << 3 | guard_mask | (max_height as u32) << 24);
        table[i] = packed;
        i += 1;
    }
    table
};
pub const BIT_TABLE: [u32; 19] = {    
    let mut table = [0u32; 19];
    let mut i = 0;
    while i < SHAPE_PARAMS.len() {
        let (width, bottom, max_height, added_mask) = SHAPE_PARAMS[i];
        let bit_mask = {
            let mut mask = 0;
            let mut i = 0;
            while i < width {
                let bottom = bottom[i as usize] as u32;
                let height = (added_mask >> (4 * (width - 1 - i))) & 0xF;
                mask |= ((1u32 << height) - 1) << bottom << (i * 6);
                // The highest shape bit is below bit 24, so bits 24-26 store the max height.
                i += 1;
            }
            assert!(mask.count_ones() == 4, "Each shape must have exactly 4 cells");
            mask
        };
        table[i] = bit_mask | (max_height as u32) << 24;
        i += 1;
    }
    table
};