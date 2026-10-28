//! TTC (Tool Terse Catalog): parses the compact `WHEN`/`ERR`/`TAGS` tool
//! descriptions in `tools.ttc` and expands them into the description
//! strings installed on the `rmcp` tool catalog at server construction time
//! (see `server::MctServer::new`).
//!
//! Rationale for an embedded plain-text data file over Rust string literals
//! scattered across `#[tool(description = "...")]` attributes: a contributor
//! adding a tool edits one small, uniformly-shaped text file — no Rust
//! string-escaping, no hunting through `server.rs` for the right attribute —
//! and the format doubles as living documentation of what each tool is for,
//! similar in spirit to the dogfooding table in this repo's `CLAUDE.md`.
//! `include_str!` embeds it at compile time, so there is no extra runtime
//! I/O or packaging concern versus a Rust literal.
//!
//! The parser never panics: `tools.ttc` ships inside the binary today, but
//! is still free-form text parsed with the same discipline this project
//! applies to any input it doesn't fully control (see the crate-wide
//! no-`unwrap`/no-`panic!` rule in `CLAUDE.md`). A malformed block yields a
//! `TtcParseError` describing the offending line; callers decide how to
//! degrade (`server.rs` falls back to the tool's compiled-in stub
//! description and logs a warning rather than failing startup).

use std::collections::BTreeMap;
use std::fmt;

/// One tool's parsed TTC entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtcEntry {
    pub when: String,
    pub err: String,
    pub tags: String,
    /// Contract (issue #17): what must hold before a call.
    pub pre: String,
    /// Contract: what a successful call returns.
    pub post: String,
    /// Contract: side effects, `<class>; <detail>`. Parsing guarantees the
    /// class is valid, so `effect()` never fails on a parsed entry.
    pub effects: String,
}

/// A tool's side-effect class, the first word of its `EFFECTS` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Touches nothing outside the server's own catalog.
    None,
    /// Reads the index or project files; may fill caches.
    Read,
    /// Changes persistent state (the index).
    Write,
}

impl Effect {
    fn parse(effects: &str) -> Option<Self> {
        match effects.split(';').next().map(str::trim) {
            Some("none") => Some(Self::None),
            Some("read") => Some(Self::Read),
            Some("write") => Some(Self::Write),
            _ => None,
        }
    }
}

impl TtcEntry {
    /// Expands this entry into the compact description string installed as
    /// the tool's live `rmcp` catalog description. The contract stays out:
    /// it is served on demand by `contract()`.
    pub fn expand(&self) -> String {
        format!("{} | NOT: {} | TAGS: {}", self.when, self.err, self.tags)
    }

    /// The contract block `get_tool_schema` appends.
    pub fn contract(&self) -> String {
        format!(
            "PRE: {}\nPOST: {}\nEFFECTS: {}",
            self.pre, self.post, self.effects
        )
    }

    pub fn effect(&self) -> Effect {
        // Unreachable fallback for a hand-built entry: treat it as a write,
        // the class every runtime check refuses.
        Effect::parse(&self.effects).unwrap_or(Effect::Write)
    }
}

/// A malformed TTC block or line. `line` is 1-based, matching editor line
/// numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtcParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for TtcParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TTC parse error at line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for TtcParseError {}

/// Parses TTC source into a `tool name -> entry` map.
///
/// Grammar: blocks are separated by one or more blank lines. Each block is
/// a `TOOL <name>` header line followed by exactly one each of `WHEN `,
/// `ERR `, `TAGS `, `PRE `, `POST ` and `EFFECTS ` lines, in any order. A line anywhere whose trimmed text
/// starts with `#` is a comment and ignored, as is a blank line outside a
/// block. Returns an error — never panics — on a missing/malformed header,
/// an unrecognized line inside a block, a duplicate field within a block,
/// a duplicate `TOOL` name across blocks, a block missing a field, or an
/// `EFFECTS` line whose class isn't `none`, `read` or `write`.
pub fn parse(source: &str) -> Result<BTreeMap<String, TtcEntry>, TtcParseError> {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = BTreeMap::new();
    let mut i = 0usize;

    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        let header_line = i + 1; // 1-based
        let name = trimmed
            .strip_prefix("TOOL ")
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .ok_or_else(|| TtcParseError {
                line: header_line,
                message: format!("expected `TOOL <name>`, got `{trimmed}`"),
            })?
            .to_string();
        i += 1;

        // Field keywords, in the order `TtcEntry`'s fields are filled.
        const FIELDS: [&str; 6] = ["WHEN", "ERR", "TAGS", "PRE", "POST", "EFFECTS"];
        let mut values: [Option<String>; 6] = Default::default();

        while i < lines.len() {
            let raw = lines[i];
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                break;
            }
            if trimmed.starts_with('#') {
                i += 1;
                continue;
            }
            let line_no = i + 1;
            let field = FIELDS.iter().enumerate().find_map(|(slot, key)| {
                trimmed
                    .strip_prefix(key)
                    .and_then(|rest| rest.strip_prefix(' '))
                    .map(|rest| (slot, *key, rest.trim().to_string()))
            });
            let Some((slot, key, value)) = field else {
                return Err(TtcParseError {
                    line: line_no,
                    message: format!("unrecognized line in tool `{name}`: `{trimmed}`"),
                });
            };
            if values[slot].replace(value).is_some() {
                return Err(TtcParseError {
                    line: line_no,
                    message: format!("duplicate {key} in tool `{name}`"),
                });
            }
            i += 1;
        }

        let [when, err, tags, pre, post, effects] = values;
        let missing = |key: &str| TtcParseError {
            line: header_line,
            message: format!("tool `{name}` missing {key}"),
        };
        let entry = TtcEntry {
            when: when.ok_or_else(|| missing("WHEN"))?,
            err: err.ok_or_else(|| missing("ERR"))?,
            tags: tags.ok_or_else(|| missing("TAGS"))?,
            pre: pre.ok_or_else(|| missing("PRE"))?,
            post: post.ok_or_else(|| missing("POST"))?,
            effects: effects.ok_or_else(|| missing("EFFECTS"))?,
        };
        if Effect::parse(&entry.effects).is_none() {
            return Err(TtcParseError {
                line: header_line,
                message: format!("tool `{name}` EFFECTS must start with none, read or write"),
            });
        }

        if out.insert(name.clone(), entry).is_some() {
            return Err(TtcParseError {
                line: header_line,
                message: format!("duplicate TOOL block for `{name}`"),
            });
        }
    }

    Ok(out)
}

/// The embedded TTC source for this server's tool catalog.
pub const CATALOG_SOURCE: &str = include_str!("tools.ttc");

/// Every tool name `CATALOG_SOURCE` is expected to cover — kept in sync
/// with the `#[tool(...)]` methods in `server.rs` by
/// `catalog_source_parses_and_covers_every_known_tool` below. Exposed so
/// `server.rs` can assert full coverage without duplicating the list.
pub const KNOWN_TOOL_NAMES: &[&str] = &[
    "list_symbols",
    "find_symbol",
    "search_symbols_like",
    "hybrid_search_like",
    "search_symbols",
    "hybrid_search",
    "semantic_search",
    "find_references",
    "find_calls",
    "find_callers",
    "impact_analysis",
    "build_context_pack",
    "reindex",
    "get_indexing_status",
    "get_file_skeleton",
    "get_project_overview",
    "find_dead_code",
    "find_duplicate_code",
    "get_dependency_graph",
    "get_file_tree",
    "discover_tool_categories",
    "get_tool_schema",
    "batch",
    "select_tools",
];

#[cfg(test)]
mod tests {
    // Test code: an unwrap()/expect() here means a broken test precondition,
    // and panicking is the correct behavior — this is not production code
    // parsing untrusted repo content (see the module doc comment above for
    // that policy).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn parses_a_single_well_formed_block() {
        let src = "TOOL foo\nWHEN do the thing.\nERR not for the other thing.\nTAGS a, b\nPRE p\nPOST q\nEFFECTS read; r\n";
        let parsed = parse(src).expect("valid TTC should parse");
        assert_eq!(parsed.len(), 1);
        let entry = &parsed["foo"];
        assert_eq!(entry.when, "do the thing.");
        assert_eq!(entry.err, "not for the other thing.");
        assert_eq!(entry.tags, "a, b");
        assert_eq!(entry.pre, "p");
        assert_eq!(entry.post, "q");
        assert_eq!(entry.effects, "read; r");
        assert_eq!(entry.effect(), Effect::Read);
        assert_eq!(entry.contract(), "PRE: p\nPOST: q\nEFFECTS: read; r");
    }

    #[test]
    fn missing_contract_field_is_an_error() {
        let src = "TOOL foo\nWHEN a\nERR b\nTAGS c\nPRE p\nEFFECTS none\n";
        let err = parse(src).expect_err("missing POST should error");
        assert!(err.message.contains("missing POST"), "{}", err.message);
    }

    #[test]
    fn unknown_effect_class_is_an_error() {
        let src = "TOOL foo\nWHEN a\nERR b\nTAGS c\nPRE p\nPOST q\nEFFECTS mutate; x\n";
        let err = parse(src).expect_err("unknown EFFECTS class should error");
        assert!(
            err.message.contains("none, read or write"),
            "{}",
            err.message
        );
    }

    #[test]
    fn field_order_within_a_block_is_flexible() {
        let src = "TOOL foo\nEFFECTS none\nTAGS a\nPOST q\nERR nope\nPRE p\nWHEN yep\n";
        let parsed = parse(src).expect("any field order should parse");
        assert_eq!(parsed["foo"].when, "yep");
        assert_eq!(parsed["foo"].err, "nope");
        assert_eq!(parsed["foo"].tags, "a");
    }

    #[test]
    fn comment_and_blank_lines_are_ignored() {
        let src = "# header comment\n\nTOOL foo\n# mid-block comment\nWHEN yep\nERR nope\nTAGS a\nPRE p\nPOST q\nEFFECTS none\n\n# trailing\n";
        let parsed = parse(src).expect("comments/blank lines should be ignored");
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn multiple_blocks_parse_independently() {
        let src = "TOOL foo\nWHEN a\nERR b\nTAGS c\nPRE p\nPOST q\nEFFECTS read\n\nTOOL bar\nWHEN d\nERR e\nTAGS f\nPRE p\nPOST q\nEFFECTS write\n";
        let parsed = parse(src).expect("multiple blocks should parse");
        assert_eq!(parsed.len(), 2);
        assert!(parsed.contains_key("foo"));
        assert!(parsed.contains_key("bar"));
    }

    #[test]
    fn missing_field_is_an_error_not_a_panic() {
        let src = "TOOL foo\nWHEN a\nTAGS c\n";
        let err = parse(src).expect_err("missing ERR should error");
        assert!(err.message.contains("missing ERR"));
    }

    #[test]
    fn duplicate_field_in_one_block_is_an_error() {
        let src = "TOOL foo\nWHEN a\nWHEN a again\nERR b\nTAGS c\n";
        let err = parse(src).expect_err("duplicate WHEN should error");
        assert!(err.message.contains("duplicate WHEN"));
    }

    #[test]
    fn duplicate_tool_name_across_blocks_is_an_error() {
        let src = "TOOL foo\nWHEN a\nERR b\nTAGS c\nPRE p\nPOST q\nEFFECTS none\n\nTOOL foo\nWHEN a\nERR b\nTAGS c\nPRE p\nPOST q\nEFFECTS none\n";
        let err = parse(src).expect_err("duplicate TOOL block should error");
        assert!(err.message.contains("duplicate TOOL block"));
    }

    #[test]
    fn unrecognized_line_is_an_error_not_a_panic() {
        let src = "TOOL foo\nWHEN a\nERR b\nTAGS c\nBOGUS x\n";
        let err = parse(src).expect_err("unrecognized line should error");
        assert!(err.message.contains("unrecognized line"));
    }

    #[test]
    fn missing_tool_header_is_an_error_not_a_panic() {
        let src = "WHEN a\nERR b\nTAGS c\n";
        let err = parse(src).expect_err("missing TOOL header should error");
        assert!(err.message.contains("expected `TOOL <name>`"));
    }

    #[test]
    fn empty_source_parses_to_an_empty_map() {
        let parsed = parse("").expect("empty source is valid, just empty");
        assert!(parsed.is_empty());
    }

    #[test]
    fn only_comments_and_blank_lines_parses_to_an_empty_map() {
        let parsed = parse("# just a comment\n\n# another\n").expect("comments-only is valid");
        assert!(parsed.is_empty());
    }

    #[test]
    fn expand_formats_when_err_tags_compactly() {
        let entry = TtcEntry {
            when: "do X".to_string(),
            err: "not Y".to_string(),
            tags: "x, y".to_string(),
            pre: "p".to_string(),
            post: "q".to_string(),
            effects: "none".to_string(),
        };
        assert_eq!(entry.expand(), "do X | NOT: not Y | TAGS: x, y");
    }

    /// The embedded catalog itself must parse cleanly and cover every tool
    /// `server.rs` actually registers — this is the guard that keeps
    /// `tools.ttc` and the `#[tool(...)]` methods from drifting apart.
    #[test]
    fn catalog_source_parses_and_covers_every_known_tool() {
        let parsed = parse(CATALOG_SOURCE).expect("tools.ttc must parse");
        for name in KNOWN_TOOL_NAMES {
            assert!(
                parsed.contains_key(*name),
                "tools.ttc is missing a TOOL block for `{name}`"
            );
        }
        assert_eq!(
            parsed.len(),
            KNOWN_TOOL_NAMES.len(),
            "tools.ttc has an entry not listed in KNOWN_TOOL_NAMES (or vice versa)"
        );
    }

    /// Every field in the shipped catalog is non-empty — an empty WHEN/ERR/
    /// TAGS would parse successfully (it's valid TTC) but produce a useless
    /// description; this test is the substantive floor `expand` output
    /// needs to actually be worth calling a description.
    #[test]
    fn catalog_source_entries_have_no_blank_fields() {
        let parsed = parse(CATALOG_SOURCE).expect("tools.ttc must parse");
        for (name, entry) in &parsed {
            assert!(!entry.when.is_empty(), "`{name}` has an empty WHEN");
            assert!(!entry.err.is_empty(), "`{name}` has an empty ERR");
            assert!(!entry.tags.is_empty(), "`{name}` has an empty TAGS");
            assert!(!entry.pre.is_empty(), "`{name}` has an empty PRE");
            assert!(!entry.post.is_empty(), "`{name}` has an empty POST");
        }
    }
}
