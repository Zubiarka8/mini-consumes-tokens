pub fn duplicate_alpha(value: i64) -> i64 {
    let mut total = value;
    total += 1;
    total *= 2;
    total -= 4;
    if total > 12 { total -= 3; }
    total += 8;
    total
}
pub fn duplicate_beta(value: i64) -> i64 {
    let mut total = value;
    total += 1;
    total *= 2;
    total -= 4;
    if total > 12 { total -= 3; }
    total += 8;
    total
}
pub fn distinct_control() -> bool { true }
