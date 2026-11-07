//! Text helpers for syntax-tree attribute values.

/// Strip exactly one pair of matching ASCII quotes, leaving unquoted text intact.
pub fn unquote_attribute_value(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' || first == b'\'') && first == last {
            return &raw[1..raw.len() - 1];
        }
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::unquote_attribute_value;

    #[test]
    fn strips_only_one_matching_pair_and_preserves_unicode() {
        for (raw, expected) in [
            ("", ""),
            ("'", "'"),
            ("plain", "plain"),
            ("\"é🦀\"", "é🦀"),
            ("'value'", "value"),
            ("\"\"", ""),
            ("\"value'", "\"value'"),
            ("''value''", "'value'"),
        ] {
            assert_eq!(unquote_attribute_value(raw), expected, "{raw}");
        }
    }
}
