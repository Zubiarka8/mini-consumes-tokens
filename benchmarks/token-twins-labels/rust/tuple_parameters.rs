// A direct tuple parameter is a binding pattern, not an external name.
fn scaled_pair((left, right): (i64, i64), scale: i64) -> i64 {
    if scale < 0 {
        return 0;
    }
    if left == 0 {
        return right;
    }
    (left + right) * scale
}

fn scaled_coordinates((x, y): (i64, i64), factor: i64) -> i64 {
    if factor < 0 {
        return 0;
    }
    if x == 0 {
        return y;
    }
    (x + y) * factor
}
