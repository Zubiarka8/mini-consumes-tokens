// Aligned-evidence corpus for Rust: one pair that differs in a field, a
// method, a type, a callee and a literal category. Parsed, never compiled.

struct Order {
    total: i64,
    label: String,
}

fn order_total(order: &Order, scale: i64) -> i64 {
    let value = order.total.saturating_mul(scale);
    audit(value);
    value + 1
}

fn order_label(order: &Order, scale: String) -> String {
    let value = order.label.repeat(scale);
    trace(value);
    value + "!"
}

fn audit(_: i64) {}

fn trace(_: String) {}

fn main() {}
