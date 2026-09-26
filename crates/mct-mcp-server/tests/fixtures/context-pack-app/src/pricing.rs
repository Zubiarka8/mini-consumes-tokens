//! Cart pricing: line totals, subtotals and coupon discounts.

use crate::orders::LineItem;

/// Percentage off for each known coupon code.
const COUPONS: &[(&str, u64)] = &[("WELCOME10", 10), ("VIP25", 25), ("STAFF50", 50)];

/// Largest discount any combination of coupons may give, in percent.
pub const MAX_DISCOUNT_PERCENT: u64 = 50;

/// One line's total: unit price times quantity, saturating on overflow.
pub fn line_total(item: &LineItem) -> u64 {
    item.unit_cents.saturating_mul(u64::from(item.quantity))
}

/// Sum of every line's total.
pub fn subtotal(items: &[LineItem]) -> u64 {
    items.iter().map(line_total).fold(0, u64::saturating_add)
}

/// Looks up a coupon's percentage off; unknown codes give 0.
pub fn coupon_percent(code: &str) -> u64 {
    COUPONS
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(code))
        .map(|(_, percent)| *percent)
        .unwrap_or(0)
}

/// Applies `coupon` (if any) to `cents`, never exceeding
/// [`MAX_DISCOUNT_PERCENT`].
pub fn apply_discount(cents: u64, coupon: Option<&str>) -> u64 {
    let percent = coupon.map(coupon_percent).unwrap_or(0).min(MAX_DISCOUNT_PERCENT);
    cents - cents * percent / 100
}

/// Formats cents as a currency string, e.g. `12.05`.
pub fn format_cents(cents: u64) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}

/// Rounds `cents` to the nearest multiple of `step` (cash rounding).
pub fn round_to(cents: u64, step: u64) -> u64 {
    if step == 0 {
        return cents;
    }
    (cents + step / 2) / step * step
}
