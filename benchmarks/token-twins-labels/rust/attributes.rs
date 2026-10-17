//! Members whose bodies match but whose attributes or enclosing context differ.
//!
//! Labeled evaluation corpus for the duplicate report (see ../labels.toon).
//! Parsed by mct-cli, never compiled.

// Case rs-ctx-inline: an inlining hint on one of two identical clamps.
#[inline]
pub fn clamp_volume(level: i32, low: i32, high: i32) -> i32 {
    if level < low {
        return low;
    }
    if level > high {
        return high;
    }
    level
}

pub fn clamp_brightness(value: i32, min: i32, max: i32) -> i32 {
    if value < min {
        return min;
    }
    if value > max {
        return max;
    }
    value
}

// Case rs-ctx-platform: per-platform alternatives that differ in one variable name.
#[cfg(target_os = "windows")]
fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    let mut path = PathBuf::from(base);
    path.push("mct");
    path.push("config");
    Some(path)
}

#[cfg(not(target_os = "windows"))]
fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("HOME")?;
    let mut path = PathBuf::from(base);
    path.push(".config");
    path.push("mct");
    Some(path)
}

pub fn escape_field(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len() + 2);
    for ch in raw.chars() {
        match ch {
            '"' => escaped.push_str("\"\""),
            '\n' => escaped.push(' '),
            other => escaped.push(other),
        }
    }
    escaped
}

// Case rs-ctx-test-oracle: a test module re-implements the production escaper.
#[cfg(test)]
mod tests {
    fn expected_escape(input: &str) -> String {
        let mut out = String::with_capacity(input.len() + 2);
        for c in input.chars() {
            match c {
                '"' => out.push_str("\"\""),
                '\n' => out.push(' '),
                other => out.push(other),
            }
        }
        out
    }
}
