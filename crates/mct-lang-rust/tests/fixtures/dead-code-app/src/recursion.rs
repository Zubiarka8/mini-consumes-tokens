//! Recursion and same-name delegation: a function's call to itself is not a
//! use, but a call to another symbol that shares its name still is.

/// Only ever called by itself.
pub fn countdown(n: u32) -> u32 {
    if n == 0 {
        0
    } else {
        countdown(n - 1)
    }
}

/// Recursive, and also called from `uses_depth`.
pub fn depth(n: u32) -> u32 {
    if n == 0 {
        0
    } else {
        depth(n - 1) + 1
    }
}

pub fn uses_depth() -> u32 {
    depth(3)
}

mod inner {
    pub fn delegate() -> u32 {
        1
    }
}

/// Calls the other `delegate`, not itself.
pub fn delegate() -> u32 {
    inner::delegate()
}
