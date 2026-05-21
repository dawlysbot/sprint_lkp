use crate::config::TARGET_LINES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchNode<B: Board> {
    pub state: B,
    pub keys_pressed: u16,
    pub meta: u16,
    // note that, for search::hash, we will use state (50bit) | meta (14bit).
    // 0-8: lines cleared, 9-10: das state, 11-13: hold piece
    // so only support 511 lines
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

const fn encode_finesse_data(operations: [u8; 10], reuse: u32) -> u32 {
    // 3 bits per target column, 30bits in total
    let mut rightmost = 9;
    while rightmost > 0 && operations[rightmost] == 0 {
        rightmost -= 1;
    }
    let mut mask = 0u32;
    let mut i = 0;
    while i <= rightmost {
        debug_assert!(operations[i] <= 0b11, "Operation value must be between 0 and 3");
        mask = mask << 3 | (operations[i] as u32) | (reuse >> (rightmost - i) as u32 & 1u32) << 2;
        i += 1;
    }
    mask
}
const fn raw_pack(width: u8, bottom: [u8; 4], max_height: u8, added_mask: u32, operations: [u8; 10], reuse: u32) -> (u32, u32) {
    // added_mask given is like 0x121, we will parse it from 2**4 to 2**6, so it must be less than 2**19
    let finesse_data = encode_finesse_data(operations, reuse);
    let parsed_added_mask = {
        let mut mask = 0;
        let mut i = 0;
        while i < width {
            let col_mask = (added_mask >> (4 * (width - 1 - i))) & 0xF;
            mask |= col_mask << (6 * i);
            i += 1;
        }
        mask
    };
    let packed = ((width - 1) as u32)
        | (bottom[0] as u32) << 2
        | (bottom[1] as u32) << 4
        | (bottom[2] as u32) << 6
        | (bottom[3] as u32) << 8
        | ((max_height - 1) as u32) << 10
        | parsed_added_mask << 12;
    (packed, finesse_data | ((width - 1) as u32) << 30)
}
pub const SHAPE_RANGES: [usize; 8] = [
    0,2,4,8,12,16,17,19
];
pub const RAW_DATA: [(u32, u32); 19] = [
    // Z
    raw_pack(3, [1, 0, 0, 0], 2, 0x121, [1, 2, 1, 0, 1, 2, 2, 1, 0, 0], 0b11000011),
    raw_pack(2, [0, 1, 0, 0], 3, 0x22, [2, 2, 2, 1, 1, 2, 3, 2, 2, 0], 0b110000111),
    // S
    raw_pack(3, [0, 0, 1, 0], 2, 0x121, [1, 2, 1, 0, 1, 2, 2, 1, 0, 0], 0b11000011),
    raw_pack(2, [1, 0, 0, 0], 3, 0x22, [2, 2, 2, 1, 1, 2, 3, 2, 2, 0], 0b110000111),
    // J
    raw_pack(3, [0, 0, 0, 0], 2, 0x211, [1, 2, 1, 0, 1, 2, 2, 1, 0, 0], 0b11000011),
    raw_pack(2, [0, 2, 0, 0], 3, 0x31, [2, 2, 3, 2, 1, 2, 3, 3, 2, 0], 0b111000011),
    raw_pack(3, [1, 1, 0, 0], 2, 0x112, [2, 3, 2, 1, 2, 3, 3, 2, 0, 0], 0b11000011),
    raw_pack(2, [0, 0, 0, 0], 3, 0x13, [2, 3, 2, 1, 2, 3, 3, 2, 2, 0], 0b110000111),
    // L
    raw_pack(3, [0, 0, 0, 0], 2, 0x112, [1, 2, 1, 0, 1, 2, 2, 1, 0, 0], 0b11000011),
    raw_pack(2, [0, 0, 0, 0], 3, 0x31, [2, 2, 3, 2, 1, 2, 3, 3, 2, 0], 0b111000011),
    raw_pack(3, [0, 1, 1, 0], 2, 0x211, [2, 3, 2, 1, 2, 3, 3, 2, 0, 0], 0b11000011),
    raw_pack(2, [2, 0, 0, 0], 3, 0x13, [2, 3, 2, 1, 2, 3, 3, 2, 2, 0], 0b110000111),
    // T
    raw_pack(3, [0, 0, 0, 0], 2, 0x121, [1, 2, 1, 0, 1, 2, 2, 1, 0, 0], 0b11000011),
    raw_pack(2, [0, 1, 0, 0], 3, 0x31, [2, 2, 3, 2, 1, 2, 3, 3, 2, 0], 0b111000011),
    raw_pack(3, [1, 0, 1, 0], 2, 0x121, [2, 3, 2, 1, 2, 3, 3, 2, 0, 0], 0b11000011),
    raw_pack(2, [1, 0, 0, 0], 3, 0x13, [2, 3, 2, 1, 2, 3, 3, 2, 2, 0], 0b110000111),
    // O
    raw_pack(2, [0, 0, 0, 0], 2, 0x22, [1, 2, 2, 1, 0, 1, 2, 2, 1, 0], 0b110000011),
    // I
    raw_pack(4, [0, 0, 0, 0], 1, 0x1111, [1, 2, 1, 0, 1, 2, 1, 0, 0, 0], 0b1100011),
    raw_pack(1, [0, 0, 0, 0], 4, 0x4, [2, 2, 2, 2, 1, 1, 2, 2, 2, 2], 0b1110000111),
];
const fn shape_to_bit(arr: &[FastMask; 19]) -> [u16; 19] {
    let mut bit_arr = [0u16; 19];
    let mut i = 0;
    while i < arr.len() {
        let mask = arr[i];
        let mut max_height = 0;
        let mut bit_mask = 0;
        let mut x = 0;
        while x <= (mask.0 & 0b11) {
            let bottom = mask.0 >> (2 + 2 * x) & 0b11;
            let height = mask.0 >> 12 >> (6 * x) & 0xF;
            if max_height < height + bottom {
                max_height = height + bottom;
            }
            bit_mask |= ((1u16 << height) - 1) << bottom << (x * 4);
            // highest bit is 12 (I0)
            // so we use bit 13-15 to store the max height
            x += 1;
        }
        bit_arr[i] = bit_mask | (max_height as u16) << 13;
        i += 1;
    }
    bit_arr
}
pub const FINESSE_TABLE: [u32; 19] = {
    let mut table = [0u32; 19];
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
pub const BIT_TABLE: [u16; 19] = shape_to_bit(&SHAPE_TABLE);

pub trait Board: Clone + Copy + PartialEq + Eq + std::hash::Hash + Default {
    fn drop_piece(&self, x: u8, piece_idx: u8, lines_cleared: u16) -> Option<(Self, u8)>;
}
trait BoardInternal: Sized {
    const QUAD_ONLY: bool = true;
    fn try_place(&self, x: u8, piece_idx: u8, lines_cleared: u16) -> Option<Self>;
    fn clear_lines(&self) -> (Self, u64);
}
impl<B> Board for B where B: BoardInternal + Clone + Copy + PartialEq + Eq + std::hash::Hash + Default {
    #[inline]
    fn drop_piece(&self, x: u8, piece_idx: u8, lines_cleared: u16) -> Option<(Self, u8)> {
        let placed = self.try_place(x, piece_idx, lines_cleared)?;
        let (cleared_board, lines) = placed.clear_lines();
        
        if Self::QUAD_ONLY && lines_cleared <= TARGET_LINES - 4 && lines != 0 && lines != 4 {
            return None;
        }
        
        Some((cleared_board, lines as u8))
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShapeBoard {
    pub packed_shape: u64,
}
impl ShapeBoard {
    const COL_MASK: u64 = 0x1F;
    #[inline(always)]
    pub fn get_height(&self, col: usize) -> u64 {
        (self.packed_shape >> (col * 5)) & Self::COL_MASK
    }
}
impl BoardInternal for ShapeBoard {
    #[inline(always)]
    fn try_place(&self, x: u8, piece_idx: u8, lines_cleared: u16) -> Option<Self> {
        let mask = &SHAPE_TABLE[piece_idx as usize];
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

        if base_y + mask.max_height() > 20 || base_y + mask.max_height() > (TARGET_LINES - lines_cleared) as u8 {
            return None;
        }

        Some(Self{packed_shape: self.packed_shape + ((mask.added_mask() as u64) << shift)})
    }
    #[inline(always)]
    fn clear_lines(&self) -> (Self, u64) {
        let mut min_h = self.packed_shape & Self::COL_MASK;
        let mut min_h2 = (self.packed_shape >> 5) & Self::COL_MASK;
        min_h = min_h.min((self.packed_shape >> 10) & Self::COL_MASK);
        min_h2 = min_h2.min((self.packed_shape >> 15) & Self::COL_MASK);
        min_h = min_h.min((self.packed_shape >> 20) & Self::COL_MASK);
        min_h2 = min_h2.min((self.packed_shape >> 25) & Self::COL_MASK);
        min_h = min_h.min((self.packed_shape >> 30) & Self::COL_MASK);
        min_h2 = min_h2.min((self.packed_shape >> 35) & Self::COL_MASK);
        min_h = min_h.min((self.packed_shape >> 40) & Self::COL_MASK);
        min_h2 = min_h2.min((self.packed_shape >> 45) & Self::COL_MASK);

        const MIN_H_MASK: u64 = 1u64 | (1u64 << 5) | (1u64 << 10) | (1u64 << 15)
            | (1u64 << 20) | (1u64 << 25) | (1u64 << 30)
            | (1u64 << 35) | (1u64 << 40) | (1u64 << 45);
        min_h = min_h.min(min_h2);
        (Self{packed_shape: self.packed_shape - min_h * MIN_H_MASK}, min_h)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BitBoard(u64);
impl BitBoard {
    const ROW_MASK: u64 = 0x0011_1111_1111;
    const COL_MASK: u64 = 0xF;
    #[inline(always)]
    pub fn get_column(&self, col: usize) -> u64 {
        self.0 >> (col * 4) & Self::COL_MASK
    }
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
    #[inline]
    pub fn from_shape_board(shape: ShapeBoard) -> Self {
        let mut board = 0;
        for col in 0..10 {
            let height = shape.get_height(col);
            debug_assert!(height <= 4);
            board |= ((1u64 << height) - 1) << (col * 4);
        }
        Self(board)
    }
}
impl BoardInternal for BitBoard {
    const QUAD_ONLY: bool = false;
    fn try_place(&self, x: u8, piece_idx: u8, lines_cleared: u16) -> Option<Self> {
        let mut height = ((TARGET_LINES - lines_cleared) as u8).checked_sub((BIT_TABLE[piece_idx as usize] >> 13) as u8)?;
        // means the highest row of the piece can be put
        let mask = ((BIT_TABLE[piece_idx as usize] & 0x1FFF) as u64) << (x * 4);
        // hit check: if I put the piece at height, compute the hitbox
        let mut hitbox = mask << height; // It's guaranteed that this << won't overflow
        hitbox |= (hitbox & (BitBoard::ROW_MASK * 7)) << 1; // one line blow-up
        hitbox |= (hitbox & (BitBoard::ROW_MASK * 3)) << 2; // two lines blow-up
        if self.0 & hitbox != 0 {
            return None;
        }
        while height > 0 {
            if self.0 & (mask << (height - 1)) != 0 {
                break;
            }
            height -= 1;
        }
        Some(Self(self.0 | (mask << height)))
    }
    fn clear_lines(&self) -> (Self, u64) {
        let mut cleared_lines = self.0;
        cleared_lines &= cleared_lines >> 8;
        // 40bits -> 32bits
        cleared_lines &= cleared_lines >> 4;
        cleared_lines &= cleared_lines >> 8;
        cleared_lines &= cleared_lines >> 16;
        debug_assert!(cleared_lines <= 0xF);
        let mut new_board = self.0;
        if cleared_lines & 8 != 0 {
            new_board &= BitBoard::ROW_MASK * 7;
        }
        if cleared_lines & 4 != 0 {
            new_board = new_board & (BitBoard::ROW_MASK * 3) | (new_board & (BitBoard::ROW_MASK * 8)) >> 1;
        }
        if cleared_lines & 2 != 0 {
            new_board = new_board & (BitBoard::ROW_MASK) | (new_board & (BitBoard::ROW_MASK * 12)) >> 1;
        }
        if cleared_lines & 1 != 0 {
            new_board = (new_board & (BitBoard::ROW_MASK * 15)) >> 1;
        }
        (Self(new_board), cleared_lines.count_ones() as u64)
    }
}