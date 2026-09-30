//! Fixture for `tests/dead_code.rs`: which of these functions a dead-code
//! scan should (and should not) report.

mod other;

pub fn actual_use() {}

pub fn entry() {
    let _f = actual_use;
}

pub fn used_in_macro(x: u32) -> u32 {
    x
}

pub fn macro_caller() -> Vec<u32> {
    vec![used_in_macro(1)]
}

pub fn truly_unused() {}

pub fn shadowed() {}

pub fn shadow_user(shadowed: u32) -> u32 {
    shadowed + 1
}

pub fn orphan() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_behavior() {
        entry();
    }

    fn unused_test_helper() {}
}
