//! Rust side of the omni fixture.

pub struct Tally {
    pub cents: i64,
}

pub fn add_cents(base: i64, delta: i64) -> i64 {
    base + delta
}

pub fn settle(base: i64) -> i64 {
    add_cents(base, 100)
}
