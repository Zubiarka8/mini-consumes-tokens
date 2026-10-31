//! LuaRocks version and constraint semantics, ported from
//! `luarocks/core/vers.lua` and `luarocks/queries.lua` (3.13.0). The
//! conformance table in `tests/luarocks.rs` was recorded from the real tool.

/// A parsed version: numeric/alphabetic components plus an optional
/// revision (`-2` in `1.1.0-2`).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Version {
    parts: Vec<f64>,
    revision: Option<u64>,
}

fn delta(token: &str) -> Option<f64> {
    Some(match token {
        "dev" => 120_000_000.0,
        "scm" => 110_000_000.0,
        "cvs" => 100_000_000.0,
        "rc" => -1000.0,
        "pre" => -10_000.0,
        "beta" => -100_000.0,
        "alpha" => -1_000_000.0,
        _ => return None,
    })
}

/// `vers.parse_version`. Text LuaRocks cannot parse yields a trailing 0
/// component, as it does there.
pub(super) fn parse_version(text: &str) -> Version {
    let mut rest = text.trim();
    let mut revision = None;
    if let Some(dash) = rest.rfind('-') {
        let tail = &rest[dash + 1..];
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            revision = tail.parse::<u64>().ok();
            rest = &rest[..dash];
        }
    }
    let mut parts: Vec<f64> = Vec::new();
    let mut slot = 0usize;
    while !rest.is_empty() {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let letters = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
        let (consumed, value, numeric) = if digits > 0 {
            (digits, rest[..digits].parse::<f64>().unwrap_or(0.0), true)
        } else if letters > 0 {
            let token = &rest[..letters];
            let first = f64::from(token.as_bytes()[0]);
            (letters, delta(token).unwrap_or(first / 1000.0), false)
        } else {
            if parts.len() <= slot {
                parts.push(0.0);
            } else {
                parts[slot] = 0.0;
            }
            break;
        };
        if numeric {
            if parts.len() > slot {
                parts[slot] += value / 100_000.0;
            } else {
                parts.push(value);
            }
            slot += 1;
        } else if parts.len() > slot {
            parts[slot] = value;
        } else {
            parts.push(value);
        }
        rest = rest[consumed..].trim_start_matches(['.', '-', '_']);
    }
    Version { parts, revision }
}

impl Version {
    fn equal(&self, other: &Version) -> bool {
        if self.parts != other.parts {
            return false;
        }
        match (self.revision, other.revision) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
    }

    fn less(&self, other: &Version) -> bool {
        for i in 0..self.parts.len().max(other.parts.len()) {
            let a = self.parts.get(i).copied().unwrap_or(0.0);
            let b = other.parts.get(i).copied().unwrap_or(0.0);
            if a != b {
                return a < b;
            }
        }
        match (self.revision, other.revision) {
            (Some(a), Some(b)) => a < b,
            _ => false,
        }
    }

    /// `partial_match`: every requested component equals the version's, and
    /// the revision too when the request names one.
    fn partial(&self, requested: &Version) -> bool {
        for (i, wanted) in requested.parts.iter().enumerate() {
            if self.parts.get(i).copied().unwrap_or(0.0) != *wanted {
                return false;
            }
        }
        match requested.revision {
            Some(revision) => self.revision == Some(revision),
            None => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    Compatible,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Constraints(Vec<(Op, Version)>);

fn operator(text: &str) -> Option<Op> {
    Some(match text {
        "==" | "" | "=" => Op::Eq,
        "~=" | "!=" => Op::Ne,
        ">" => Op::Gt,
        "<" => Op::Lt,
        ">=" => Op::Ge,
        "<=" => Op::Le,
        "~>" => Op::Compatible,
        _ => return None,
    })
}

/// `parse_constraints` of `queries.lua`. `None` when LuaRocks would refuse
/// the text: callers must report the constraint as unrecognised.
pub(super) fn parse_constraints(text: &str) -> Option<Constraints> {
    let mut rest = text;
    let mut out = Vec::new();
    while !rest.is_empty() {
        let mut cursor = rest;
        if let Some(stripped) = cursor.strip_prefix('@') {
            cursor = stripped;
        }
        let op_len = cursor
            .bytes()
            .take_while(|b| matches!(b, b'<' | b'>' | b'=' | b'~' | b'!'))
            .count();
        let op = operator(&cursor[..op_len])?;
        cursor = cursor[op_len..].trim_start();
        let version_len = cursor
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            .count();
        if version_len == 0 {
            return None;
        }
        out.push((op, parse_version(&cursor[..version_len])));
        rest = cursor[version_len..].trim_start_matches(|c: char| c.is_whitespace() || c == ',');
    }
    Some(Constraints(out))
}

impl Constraints {
    /// `match_constraints`: whether `installed` meets every constraint.
    pub(super) fn accepts(&self, installed: &Version) -> bool {
        self.0.iter().all(|(op, wanted)| match op {
            Op::Eq => installed.equal(wanted),
            Op::Ne => !installed.equal(wanted),
            Op::Gt => wanted.less(installed),
            Op::Lt => installed.less(wanted),
            Op::Ge => !installed.less(wanted),
            Op::Le => !wanted.less(installed),
            Op::Compatible => installed.partial(wanted),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn accepts(installed: &str, constraint: &str) -> bool {
        parse_constraints(constraint)
            .unwrap()
            .accepts(&parse_version(installed))
    }

    #[test]
    fn matches_the_table_recorded_from_luarocks_3_13_0() {
        // (installed, constraint, LuaRocks' answer)
        let recorded = [
            ("3.1.3-0", "~> 3.1", true),
            ("3.9-1", "~> 3.1", false),
            ("4.0-1", "~> 3.1", false),
            ("3.1.3-0", "~> 3.1.4", false),
            ("3.1.9-1", "~> 3.1.4", false),
            ("3.2.0-1", "~> 3.1.4", false),
            ("3.1.3-0", "3.1.3", true),
            ("3.1.3-0", "== 3.1.3", true),
            ("3.1.3-0", "3.1.3-1", false),
            ("3.1.3-0", ">= 3.1.3-1", false),
            ("scm-1", ">= 1.0", true),
            ("1.0-1", "scm", false),
            ("1.1.0-2", ">= 1.0, < 2", true),
            ("1.0rc1-1", ">= 1.0", false),
            ("1.0-1", "~= 1.0", false),
            ("1.0-1", "!= 1.0", false),
        ];
        for (installed, constraint, expected) in recorded {
            assert_eq!(
                accepts(installed, constraint),
                expected,
                "{installed} {constraint}"
            );
        }
    }

    #[test]
    fn refuses_constraints_luarocks_refuses() {
        assert!(parse_constraints(">>> 1.0").is_none());
        assert!(parse_constraints(">=").is_none());
        assert!(parse_constraints("~~ 1").is_none());
        assert!(parse_constraints("")
            .unwrap()
            .accepts(&parse_version("1.0-1")));
    }
}
