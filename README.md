# Sprint LKP Solver

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**LKP** = **Least number of Key Presses**

A Rust-based solver that finds optimal key press sequences for the *Sprint* mode in **Techmino** (a modern Tetris-like game).  
The solver outputs a valid replay file compatible with Techmino's replay format.

## 🔍 Overview

This project **does not use any code from Techmino**.  
All logic (piece spawning, replay encoding, etc.) was independently implemented by reading and understanding the game's publicly available behavior.  
The solver then generates a replay file that represents the optimal solution in terms of the fewest key presses.

- Written in 100% Rust
- No external game assets or proprietary code included
- Output replay can be directly loaded into Techmino (0.17.14 or later)

## 🚀 Usage

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (edition 2021 or later)
- Techmino (optional – only needed to watch the replay)

### Build & Run

```bash
# Clone the repository
git clone https://github.com/dawlysbot/sprint_lkp.git
cd sprint_lkp

# Run with a specific random seed
cargo run --release -- 12345

# Run with a randomly generated seed (no argument)
cargo run --release