//! Calls into `tally.rs` so the Rust half has a cross-file relation.

use crate::tally::settle;

pub fn run_tally(base: i64) -> i64 {
    settle(base)
}
