/*
 * SPDX-License-Identifier: MIT
 * Copyright (c) 2026 Dawlysbot
 *
 * This solver is an independent implementation inspired by the logic of
 * Techmino (LGPL-3.0). No code from the original game is used.
 */

use crate::PieceType;
pub struct XorShift64Star {
    state: u64,
}

impl XorShift64Star {
    pub fn new(seed: u64) -> Self {
        let mut key = if seed == 0 {
            0x0139408D_CBBF7A44u64
        } else {
            seed
        };
        key = (!key).wrapping_add(key.wrapping_shl(21)); // key = (key << 21) - key - 1;
        key ^= key.wrapping_shr(24);
        key = key.wrapping_mul(265);
        key ^= key.wrapping_shr(14);
        key = key.wrapping_mul(21);
        key ^= key.wrapping_shr(28);
        key = key.wrapping_add(key.wrapping_shl(31));
        XorShift64Star { state: key }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state.wrapping_shr(12);
        self.state ^= self.state.wrapping_shl(25);
        self.state ^= self.state.wrapping_shr(27);
        self.state.wrapping_mul(2685821657736338717u64)
    }

    pub fn next_f64(&mut self) -> f64 {
        let r = self.next_u64();
        let magic = 0x3FFu64 << 52;
        let u = magic | (r >> 12);
        f64::from_bits(u) - 1.0
    }
}

pub struct ShuffleSequence {
    rng: XorShift64Star,
    bag: Vec<i32>,
    rev_seq: [i32; 7],
}

impl ShuffleSequence {
    pub fn new(seed: u64) -> Self {
        let rev_seq: [i32; 7] = [7, 6, 5, 4, 3, 2, 1];
        ShuffleSequence {
            rng: XorShift64Star::new(seed),
            bag: Vec::new(),
            rev_seq,
        }
    }

    fn refill_bag(&mut self) {
        self.bag.clear();
        self.bag.extend_from_slice(&self.rev_seq);
    }
}

impl Iterator for ShuffleSequence {
    type Item = i32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bag.is_empty() {
            self.refill_bag();
        }
        let idx = (self.rng.next_f64() * self.bag.len() as f64) as usize;
        Some(self.bag.remove(idx))
    }
}

pub fn generate_piece_sequence(seed: &str, count: usize) -> Vec<PieceType> {
    let seed_val = seed.parse::<u32>().unwrap() as u64;
    let mut rng = ShuffleSequence::new(seed_val);
    let mut seq = Vec::with_capacity(count);
    for _ in 0..count {
        let idx = rng.next().unwrap_or(0);
        seq.push(match idx {
            1 => PieceType::Z,
            2 => PieceType::S,
            3 => PieceType::J,
            4 => PieceType::L,
            5 => PieceType::T,
            6 => PieceType::O,
            7 => PieceType::I,
            _ => PieceType::Empty,
        });
    }
    seq
}