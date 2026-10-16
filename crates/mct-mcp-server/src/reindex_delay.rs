//! Parser for the `--reindex-delay` server flag.
//!
//! Accepts the presets `15s`, `30s`, `45s`, `1m`, any custom positive
//! duration written as `<N>s`, `<N>m` or `<N>ms`, and `off`. The result feeds
//! the same debounce `Duration` that `background::spawn_watcher` already takes.

use std::time::Duration;

/// Parsed `--reindex-delay` value; `None` means automatic updates are off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReindexDelay(pub Option<Duration>);

/// clap `value_parser` adapter around [`parse_reindex_delay`].
pub fn parse_reindex_delay_arg(input: &str) -> Result<ReindexDelay, String> {
    parse_reindex_delay(input).map(ReindexDelay)
}

/// Parse a delay such as `30s`, `2m`, `500ms` or `off`.
///
/// Returns `Ok(None)` for `off`, `Ok(Some(d))` for a valid duration, and a
/// human-readable error otherwise. Zero is rejected: use `off` to disable.
pub fn parse_reindex_delay(input: &str) -> Result<Option<Duration>, String> {
    let value = input.trim();
    if value.eq_ignore_ascii_case("off") {
        return Ok(None);
    }
    let hint =
        "expected `off`, a preset (15s, 30s, 45s, 1m) or a positive duration like 90s, 2m, 500ms";
    let split = value
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| format!("missing unit in `{input}`: {hint}"))?;
    let (digits, unit) = value.split_at(split);
    if digits.is_empty() {
        return Err(format!("invalid delay `{input}`: {hint}"));
    }
    let amount: u64 = digits
        .parse()
        .map_err(|_| format!("delay `{input}` is too large: {hint}"))?;
    if amount == 0 {
        return Err(format!(
            "delay `{input}` must be greater than zero; use `off` to disable automatic updates"
        ));
    }
    let overflow = || format!("delay `{input}` is too large: {hint}");
    let duration = match unit.to_ascii_lowercase().as_str() {
        "ms" => Duration::from_millis(amount),
        "s" => Duration::from_secs(amount),
        "m" => Duration::from_secs(amount.checked_mul(60).ok_or_else(overflow)?),
        _ => return Err(format!("unknown unit `{unit}` in `{input}`: {hint}")),
    };
    Ok(Some(duration))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn presets_parse() {
        assert_eq!(
            parse_reindex_delay("15s").unwrap(),
            Some(Duration::from_secs(15))
        );
        assert_eq!(
            parse_reindex_delay("30s").unwrap(),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            parse_reindex_delay("45s").unwrap(),
            Some(Duration::from_secs(45))
        );
        assert_eq!(
            parse_reindex_delay("1m").unwrap(),
            Some(Duration::from_secs(60))
        );
    }

    #[test]
    fn custom_values_parse() {
        assert_eq!(
            parse_reindex_delay("90s").unwrap(),
            Some(Duration::from_secs(90))
        );
        assert_eq!(
            parse_reindex_delay("2m").unwrap(),
            Some(Duration::from_secs(120))
        );
        assert_eq!(
            parse_reindex_delay("500ms").unwrap(),
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn off_disables() {
        assert_eq!(parse_reindex_delay("off").unwrap(), None);
        assert_eq!(parse_reindex_delay("OFF").unwrap(), None);
    }

    #[test]
    fn invalid_inputs_rejected() {
        for bad in [
            "", "0", "0s", "0ms", "0m", "-5s", "abc", "10", "10h", "s", "1.5s", "5 s", "+5s",
        ] {
            assert!(
                parse_reindex_delay(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
        assert!(parse_reindex_delay("0s").unwrap_err().contains("off"));
    }

    #[test]
    fn overflow_rejected() {
        assert!(parse_reindex_delay("99999999999999999999999s").is_err());
        assert!(parse_reindex_delay(&format!("{}m", u64::MAX)).is_err());
    }
}
