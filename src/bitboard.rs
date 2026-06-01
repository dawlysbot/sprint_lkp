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
    pub fn width(self) -> u8 { (self.0 & 0b11) as u8 + 1}
    #[inline(always)]
    fn bottom0(self) -> u8 { ((self.0 >> 2) & 0b11) as u8 }
    #[inline(always)]
    fn bottom1(self) -> u8 { ((self.0 >> 4) & 0b11) as u8 }
    #[inline(always)]
    fn bottom2(self) -> u8 { ((self.0 >> 6) & 0b11) as u8 }
    #[inline(always)]
    fn bottom3(self) -> u8 { ((self.0 >> 8) & 0b11) as u8 }
    #[inline(always)]
    fn max_height(self) -> u8 { ((self.0 >> 10) & 0b11) as u8 + 1 }
    #[inline(always)]
    fn added_mask(self) -> u32 { self.0 >> 12 }
}

const fn encode_finesse_data(operations: [u8; 10], reuse: u64, provide_l: u64, provide_r: u64, special: u64) -> u64 {
    // 3 bits per target column, 30bits in total
    let mut rightmost = 9;
    while rightmost > 0 && operations[rightmost] == 0 {
        rightmost -= 1;
    }
    let mut mask = 0u64;
    let mut i = rightmost;
    let merged = reuse | provide_l << 1 | provide_r << 2 | special << 3;
    loop {
        debug_assert!(operations[i] <= 0b11, "Operation value must be between 0 and 3");
        mask = mask << 6 | operations[i] as u64 | ((merged >> ((rightmost - i) * 4) & 0xF_u64) << 2) as u64;
        if i == 0 {
            break;
        }
        i -= 1;
    }
    mask
}
const fn raw_pack(width: u8, bottom: [u8; 4], max_height: u8, added_mask: u32, operations: [u8; 10], reuse: u64, provide_l: u64, provide_r: u64, special: u64) -> (u32, u64) {
    // added_mask given is like 0x121, we will parse it from 2**4 to 2**6, so it must be less than 2**19
    let finesse_data = encode_finesse_data(operations, reuse, provide_l, provide_r, special);
    let parsed_added_mask = {
        let mut mask = 0;
        let mut i = 0;
        let mut cells = 0;
        while i < width {
            let col_mask = (added_mask >> (4 * (width - 1 - i))) & 0xF;
            mask |= col_mask << (5 * i);
            cells += col_mask;
            i += 1;
        }
        assert!(cells == 4, "added_mask must have exactly 4 cells");
        assert!(mask < 1 << 19, "added_mask must be smaller than 2**19");
        mask
    };
    let packed = ((width - 1) as u32)
        | (bottom[0] as u32) << 2
        | (bottom[1] as u32) << 4
        | (bottom[2] as u32) << 6
        | (bottom[3] as u32) << 8
        | ((max_height - 1) as u32) << 10
        | parsed_added_mask << 12;
    (packed, finesse_data | (width as u64) << 60)
}
pub const SHAPE_RANGES: [usize; 8] = [
    0,2,4,8,12,16,17,19
];
pub const RAW_DATA: [(u32, u64); 19] = [
    // Z
    raw_pack(3, [1, 0, 0, 0], 2, 0x121, [1, 2, 1, 0, 1, 2, 2, 1, 0, 0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    raw_pack(2, [0, 1, 0, 0], 3, 0x22, [2, 2, 2, 1, 1, 2, 3, 2, 2, 0], 0x110000111, 0x101000100, 0x000001101, 0x001000000),
    // S
    raw_pack(3, [0, 0, 1, 0], 2, 0x121, [1, 2, 1, 0, 1, 2, 2, 1, 0, 0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    raw_pack(2, [1, 0, 0, 0], 3, 0x22, [2, 2, 2, 1, 1, 2, 3, 2, 2, 0], 0x110000111, 0x101000100, 0x000001101, 0x001000000),
    // J
    raw_pack(3, [0, 0, 0, 0], 2, 0x211, [1, 2, 1, 0, 1, 2, 2, 1, 0, 0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    raw_pack(2, [0, 2, 0, 0], 3, 0x31, [2, 2, 3, 2, 1, 2, 3, 3, 2, 0], 0x111000011, 0x101100010, 0x001001111, 0x000000100),
    raw_pack(3, [1, 1, 0, 0], 2, 0x112, [2, 3, 2, 1, 2, 3, 3, 2, 0, 0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    raw_pack(2, [0, 0, 0, 0], 3, 0x13, [2, 3, 2, 1, 2, 3, 3, 2, 2, 0], 0x110000111, 0x111000100, 0x010011101, 0x000000100),
    // L
    raw_pack(3, [0, 0, 0, 0], 2, 0x112, [1, 2, 1, 0, 1, 2, 2, 1, 0, 0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    raw_pack(2, [0, 0, 0, 0], 3, 0x31, [2, 2, 3, 2, 1, 2, 3, 3, 2, 0], 0x111000011, 0x101100010, 0x001001111, 0x000000100),
    raw_pack(3, [0, 1, 1, 0], 2, 0x211, [2, 3, 2, 1, 2, 3, 3, 2, 0, 0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    raw_pack(2, [2, 0, 0, 0], 3, 0x13, [2, 3, 2, 1, 2, 3, 3, 2, 2, 0], 0x110000111, 0x111000100, 0x010011101, 0x000000100),
    // T
    raw_pack(3, [0, 0, 0, 0], 2, 0x121, [1, 2, 1, 0, 1, 2, 2, 1, 0, 0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    raw_pack(2, [0, 1, 0, 0], 3, 0x31, [2, 2, 3, 2, 1, 2, 3, 3, 2, 0], 0x111000011, 0x101100010, 0x001001111, 0x000000100),
    raw_pack(3, [1, 0, 1, 0], 2, 0x121, [2, 3, 2, 1, 2, 3, 3, 2, 0, 0], 0x11000011, 0x11100010, 0x01001111, 0x00000100),
    raw_pack(2, [1, 0, 0, 0], 3, 0x13, [2, 3, 2, 1, 2, 3, 3, 2, 2, 0], 0x110000111, 0x111000100, 0x010011101, 0x000000100),
    // O
    raw_pack(2, [0, 0, 0, 0], 2, 0x22, [1, 2, 2, 1, 0, 1, 2, 2, 1, 0], 0x110000011, 0x101100010, 0x010001101, 0x001000100),
    // I
    raw_pack(4, [0, 0, 0, 0], 1, 0x1111, [1, 2, 1, 0, 1, 2, 1, 0, 0, 0], 0x1100011, 0x1110010, 0x0100111, 0x0000000),
    raw_pack(1, [0, 0, 0, 0], 4, 0x4, [2, 2, 2, 2, 1, 1, 2, 2, 2, 2], 0x1110000111, 0x1001000000, 0x0000001001, 0x0001001000),
];
const fn shape_to_bit(arr: &[FastMask; 19]) -> [u32; 19] {
    let mut bit_arr = [0u32; 19];
    let mut i = 0;
    while i < arr.len() {
        let mask = arr[i];
        let mut max_height = 0;
        let mut bit_mask = 0;
        let mut x = 0;
        while x <= (mask.0 & 0b11) {
            let bottom = mask.0 >> (2 + 2 * x) & 0b11;
            let height = mask.0 >> 12 >> (5 * x) & 0xF;
            if max_height < height + bottom {
                max_height = height + bottom;
            }
            bit_mask |= ((1u32 << height) - 1) << bottom << (x * 6);
            // The highest shape bit is below bit 24, so bits 24-26 store the max height.
            x += 1;
        }
        assert!(bit_mask.count_ones() == 4, "Each shape must have exactly 4 cells");
        bit_arr[i] = bit_mask | max_height << 24;
        i += 1;
    }
    bit_arr
}
pub const FINESSE_TABLE: [u64; 19] = {
    let mut table = [0u64; 19];
    let mut i = 0;
    while i < RAW_DATA.len() {
        table[i] = RAW_DATA[i].1;
        i += 1;
    }
    table
};
const SHAPE_TABLE: [FastMask; 19] = {
    let mut table = [FastMask(0); 19];
    let mut i = 0;
    while i < RAW_DATA.len() {
        table[i] = FastMask(RAW_DATA[i].0);
        i += 1;
    }
    table
};
pub const BIT_TABLE: [u32; 19] = shape_to_bit(&SHAPE_TABLE);

pub trait Board: Clone + Copy + PartialEq + Eq + std::hash::Hash + Default {
    const QUAD_ONLY: bool = true;
    fn drop_piece(&self, x: u8, shape_idx: u8, lines_cleared: u16) -> Option<(Self, u8)>;
}
trait BoardInternal: Sized {
    const QUAD_ONLY: bool = true;
    fn try_place(&self, x: u8, shape_idx: u8, lines_cleared: u16) -> Option<Self>;
    fn clear_lines(&self) -> (Self, u8);
}
impl<B> Board for B where B: BoardInternal + Clone + Copy + PartialEq + Eq + std::hash::Hash + Default {
    const QUAD_ONLY: bool = B::QUAD_ONLY;
    #[inline]
    fn drop_piece(&self, x: u8, shape_idx: u8, lines_cleared: u16) -> Option<(Self, u8)> {
        let placed = self.try_place(x, shape_idx, lines_cleared)?;
        let (cleared_board, lines) = placed.clear_lines();
        
        if Self::QUAD_ONLY && lines_cleared <= TARGET_LINES - 4 && lines != 0 && lines != 4 {
            return None;
        }
        
        Some((cleared_board, lines))
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShapeBoard {
    pub packed_shape: u64,
}
impl ShapeBoard {
    pub const COL_MASK: u64 = 0x1F;
    #[inline(always)]
    pub fn get_height(&self, col: usize) -> u64 {
        (self.packed_shape >> (col * 5)) & Self::COL_MASK
    }
}
impl BoardInternal for ShapeBoard {
    #[inline(always)]
    fn try_place(&self, x: u8, shape_idx: u8, lines_cleared: u16) -> Option<Self> {
        let mask = &SHAPE_TABLE[shape_idx as usize];
        debug_assert!(x + mask.width() <= 10);

        let shift = x * 5;
        let cols = self.packed_shape >> shift;
        let h0 = (cols & Self::COL_MASK) as u8;
        let b0 = mask.bottom0();
        if h0 < b0 { return None; }
        let base_y = h0 - b0;
        let width = mask.width();
        if width > 1 {
            let h1 = ((cols >> 5) & Self::COL_MASK) as u8;
            let b1 = mask.bottom1();
            if h1 < b1 || h1 - b1 != base_y { return None; }
        }
        if width > 2 {
            let h2 = ((cols >> 10) & Self::COL_MASK) as u8;
            let b2 = mask.bottom2();
            if h2 < b2 || h2 - b2 != base_y { return None; }
        }
        if width > 3 {
            let h3 = ((cols >> 15) & Self::COL_MASK) as u8;
            let b3 = mask.bottom3();
            if h3 < b3 || h3 - b3 != base_y { return None; }
        }

        if base_y + mask.max_height() > 20 || (base_y + mask.max_height()) as u16 > TARGET_LINES - lines_cleared {
            return None;
        }

        Some(Self{packed_shape: self.packed_shape + ((mask.added_mask() as u64) << shift)})
    }
    #[inline(always)]
    fn clear_lines(&self) -> (Self, u8) {
        let mut min_h = (self.packed_shape & Self::COL_MASK) as u8;
        let mut min_h2 = ((self.packed_shape >> 5) & Self::COL_MASK) as u8;
        min_h = min_h.min(((self.packed_shape >> 10) & Self::COL_MASK) as u8);
        min_h2 = min_h2.min(((self.packed_shape >> 15) & Self::COL_MASK) as u8);
        min_h = min_h.min(((self.packed_shape >> 20) & Self::COL_MASK) as u8);
        min_h2 = min_h2.min(((self.packed_shape >> 25) & Self::COL_MASK) as u8);
        min_h = min_h.min(((self.packed_shape >> 30) & Self::COL_MASK) as u8);
        min_h2 = min_h2.min(((self.packed_shape >> 35) & Self::COL_MASK) as u8);
        min_h = min_h.min(((self.packed_shape >> 40) & Self::COL_MASK) as u8);
        min_h2 = min_h2.min(((self.packed_shape >> 45) & Self::COL_MASK) as u8);

        const MIN_H_MASK: u64 = 1u64 | (1u64 << 5) | (1u64 << 10) | (1u64 << 15)
            | (1u64 << 20) | (1u64 << 25) | (1u64 << 30)
            | (1u64 << 35) | (1u64 << 40) | (1u64 << 45);
        min_h = min_h.min(min_h2);
        (Self{packed_shape: self.packed_shape - min_h as u64 * MIN_H_MASK}, min_h)
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
}
impl BoardInternal for BitBoard {
    const QUAD_ONLY: bool = false;
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
