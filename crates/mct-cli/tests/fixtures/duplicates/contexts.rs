// Context corpus for Rust: the same body at the top level and inside a test
// module, and under different outer attributes. Parsed, never compiled.

fn production_sum(values: &[i64]) -> i64 {
    let total: i64 = values.iter().sum();
    total * 2
}

#[cfg(test)]
mod tests {
    fn helper_sum(values: &[i64]) -> i64 {
        let total: i64 = values.iter().sum();
        total * 2
    }
}

#[inline]
fn fast_max(values: &[i64]) -> i64 {
    let top = values.iter().max();
    top.copied().unwrap_or(0)
}

/// Not inlined.
fn slow_max(values: &[i64]) -> i64 {
    let top = values.iter().max();
    top.copied().unwrap_or(0)
}
