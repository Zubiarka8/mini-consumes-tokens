//! `mct-cli duplicates`: on-demand report of candidate structurally
//! duplicated functions and methods (issue #207, Phase 0).
//!
//! Read-only: opens an existing index, runs the analysis from `mct-index`
//! against the current files, and renders the [`DuplicateReport`] as TOON on
//! stdout. It never reindexes and never writes anything. The renderer is local
//! (not `mct_mcp_server::toon::encode_table`) because the report needs nested
//! blocks and a stricter quoting rule: source excerpts keep their leading
//! indentation and contain colons and brackets, which the shared table
//! encoder leaves unquoted.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context as _;
use mct_index::{
    find_duplicate_groups, DuplicateGroup, DuplicateMember, DuplicateOptions, DuplicateReport,
    ExcludeSet, Index, SkipReason, SkippedCandidate,
};
use mct_languages::build_token_sources;

use crate::display_path;

/// First field of every report: groups are leads, not findings.
const NOTICE: &str = "Groups below are CANDIDATE duplicates for human review. \
Identical normalized structure does not prove that the functions are redundant \
or behave the same.";

/// Every skip reason, in the stable order the report lists them.
const SKIP_REASONS: [SkipReason; 4] = [
    SkipReason::ReadError,
    SkipReason::ParseError,
    SkipReason::StaleLocation,
    SkipReason::InvalidRange,
];

/// Clap value parser for `--min-lines` / `--min-tokens`: a positive integer.
pub(crate) fn parse_threshold(raw: &str) -> Result<usize, String> {
    match raw.trim().parse::<usize>() {
        Ok(0) => Err("must be at least 1".to_string()),
        Ok(value) => Ok(value),
        Err(_) => Err(format!("`{raw}` is not a positive integer")),
    }
}

/// Opens the project's existing index (never creating one), analyzes it and
/// prints the TOON report.
pub(crate) fn run(root: &Path, options: DuplicateOptions) -> anyhow::Result<()> {
    if !root.is_dir() {
        anyhow::bail!("project root {} is not a directory", display_path(root));
    }
    let db_path = root.join(".mct-index").join("index.sqlite3");
    if !db_path.is_file() {
        anyhow::bail!(
            "no index found at {}; run `mct-cli --root {} init` first",
            display_path(&db_path),
            display_path(root)
        );
    }
    let exclude = ExcludeSet::new(&mct_index::read_ignore_file(root));
    let index = Index::open(root, &db_path, exclude)
        .with_context(|| format!("cannot open the index at {}", display_path(&db_path)))?;
    let report = find_duplicate_groups(&index, &build_token_sources(), options)
        .context("cannot query the index for duplicate candidates")?;
    print!("{}", render_toon(&report));
    Ok(())
}

/// Renders the report as TOON: scalar header fields, then a `groups` list of
/// objects (each with a `members` table) and a `skipped` table. Follows the
/// report's own order; nothing is re-sorted or hashed.
pub(crate) fn render_toon(report: &DuplicateReport) -> String {
    let mut out = String::new();
    field(&mut out, 0, "notice", NOTICE);
    field(
        &mut out,
        0,
        "status",
        if report.complete {
            "complete"
        } else {
            "partial"
        },
    );
    if !report.complete {
        field(
            &mut out,
            0,
            "partial_reason",
            "some candidates were skipped, so groups involving them may be missing; see skipped",
        );
    }
    out.push_str("options:\n");
    number(&mut out, 2, "min_lines", report.options.min_lines);
    number(&mut out, 2, "min_tokens", report.options.min_tokens);
    inline_array(&mut out, "supported_languages", &report.supported_languages);
    inline_array(
        &mut out,
        "unsupported_languages",
        &report.unsupported_languages,
    );
    field(
        &mut out,
        0,
        "generated_code",
        report.generated_code.description(),
    );
    number(
        &mut out,
        0,
        "candidates_analyzed",
        report.candidates_analyzed,
    );
    number(&mut out, 0, "below_threshold", report.below_threshold);
    number(&mut out, 0, "total_groups", report.groups.len());
    number(&mut out, 0, "total_skipped", report.skipped.len());
    out.push_str("skipped_by_reason:\n");
    for reason in SKIP_REASONS {
        let count = report
            .skipped
            .iter()
            .filter(|skipped| skipped.reason == reason)
            .count();
        number(&mut out, 2, reason.as_str(), count);
    }

    let _ = writeln!(out, "groups[{}]:", report.groups.len());
    for (position, group) in report.groups.iter().enumerate() {
        render_group(&mut out, position + 1, group);
    }

    let _ = writeln!(
        out,
        "skipped[{}]{{path,range,symbol,language,reason,detail}}:",
        report.skipped.len()
    );
    for skipped in &report.skipped {
        render_skipped(&mut out, skipped);
    }
    out
}

fn render_group(out: &mut String, index: usize, group: &DuplicateGroup) {
    let ranges: Vec<String> = group.members.iter().map(member_location).collect();
    let evidence = format!(
        "members share an identical normalized token sequence (identifiers -> $ID, \
         literals -> $LIT); inspect {}",
        ranges.join(", ")
    );
    out.push_str("  - ");
    number_inline(out, "index", index);
    field(out, 4, "language", &group.language);
    number(out, 4, "member_count", group.members.len());
    number(out, 4, "line_count", group.line_count);
    number(out, 4, "token_count", group.token_count);
    field(out, 4, "evidence", &evidence);
    let _ = writeln!(
        out,
        "    members[{}]{{path,range,symbol,kind,language,lines,tokens,excerpt}}:",
        group.members.len()
    );
    for member in &group.members {
        let row = [
            quote_if_needed(&member.path),
            format!("{}-{}", member.start_line, member.end_line),
            quote_if_needed(&member.symbol_name),
            quote_if_needed(&member.symbol_kind),
            quote_if_needed(&member.language),
            member.line_count.to_string(),
            member.token_count.to_string(),
            quote_if_needed(&member.excerpt.join("\n")),
        ];
        let _ = writeln!(out, "      {}", row.join(","));
    }
}

fn render_skipped(out: &mut String, skipped: &SkippedCandidate) {
    let range = match skipped.end_line {
        Some(end) => format!("{}-{end}", skipped.start_line),
        None => format!("{}-?", skipped.start_line),
    };
    let row = [
        quote_if_needed(&skipped.path),
        range,
        quote_if_needed(&skipped.symbol_name),
        quote_if_needed(&skipped.language),
        quote_if_needed(skipped.reason.as_str()),
        quote_if_needed(&skipped.detail),
    ];
    let _ = writeln!(out, "  {}", row.join(","));
}

/// `path:start-end`, the form the evidence line lists for inspection.
fn member_location(member: &DuplicateMember) -> String {
    format!("{}:{}-{}", member.path, member.start_line, member.end_line)
}

fn field(out: &mut String, indent: usize, key: &str, value: &str) {
    let _ = writeln!(out, "{:indent$}{key}: {}", "", quote_if_needed(value));
}

fn number(out: &mut String, indent: usize, key: &str, value: usize) {
    let _ = writeln!(out, "{:indent$}{key}: {value}", "");
}

/// A number on the same line as a list item's hyphen.
fn number_inline(out: &mut String, key: &str, value: usize) {
    let _ = writeln!(out, "{key}: {value}");
}

/// `key[N]: a,b,c`, or `key[0]:` when empty.
fn inline_array(out: &mut String, key: &str, values: &[String]) {
    let _ = write!(out, "{key}[{}]:", values.len());
    if !values.is_empty() {
        let cells: Vec<String> = values.iter().map(|value| quote_if_needed(value)).collect();
        let _ = write!(out, " {}", cells.join(","));
    }
    out.push('\n');
}

/// Quotes `value` when TOON would otherwise misread it: empty, leading or
/// trailing whitespace, the comma delimiter, a colon, quotes, backslash,
/// brackets or braces, a control character, a leading hyphen, or text that
/// reads as a number, boolean or null. Escapes `\\`, `\"`, `\n`, `\r`, `\t`;
/// any other control character becomes U+FFFD.
fn quote_if_needed(value: &str) -> String {
    if !needs_quotes(value) {
        return value.to_string();
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for c in value.chars() {
        match c {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            c if c.is_control() => quoted.push('\u{FFFD}'),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

fn needs_quotes(value: &str) -> bool {
    value.is_empty()
        || value.trim() != value
        || matches!(value, "true" | "false" | "null")
        || value.starts_with('-')
        || value.parse::<f64>().is_ok()
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, ',' | ':' | '"' | '\\' | '[' | ']' | '{' | '}'))
}

#[cfg(test)]
mod tests {
    // Test code: hand-built reports and string assertions, no repo input.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use mct_index::GeneratedCodeHandling;

    use super::*;

    fn member(path: &str, start: u32, end: u32, name: &str, excerpt: &[&str]) -> DuplicateMember {
        DuplicateMember {
            path: path.to_string(),
            start_line: start,
            end_line: end,
            symbol_name: name.to_string(),
            symbol_kind: "function".to_string(),
            language: "rust".to_string(),
            line_count: 8,
            token_count: 52,
            excerpt: excerpt.iter().map(|line| (*line).to_string()).collect(),
        }
    }

    fn empty_report() -> DuplicateReport {
        DuplicateReport {
            groups: Vec::new(),
            skipped: Vec::new(),
            complete: true,
            options: DuplicateOptions::default(),
            candidates_analyzed: 0,
            below_threshold: 0,
            supported_languages: Vec::new(),
            unsupported_languages: Vec::new(),
            generated_code: GeneratedCodeHandling::ConfiguredExclusionsOnly,
        }
    }

    fn group_report() -> DuplicateReport {
        let mut report = empty_report();
        report.supported_languages = vec!["python".to_string(), "rust".to_string()];
        report.unsupported_languages = vec!["go".to_string()];
        report.candidates_analyzed = 5;
        report.below_threshold = 2;
        report.groups = vec![DuplicateGroup {
            members: vec![
                member(
                    "src/a.rs",
                    10,
                    20,
                    "load",
                    &[
                        "    fn load(name: &str, flag: bool) -> Result<u8, String> {",
                        "        let x = \"a,b\";",
                        "    }",
                    ],
                ),
                member("src/b.rs", 3, 13, "read", &["fn read() {"]),
            ],
            language: "rust".to_string(),
            line_count: 8,
            token_count: 52,
        }];
        report
    }

    fn partial_report() -> DuplicateReport {
        let mut report = empty_report();
        report.complete = false;
        report.supported_languages = vec!["rust".to_string()];
        report.skipped = vec![
            SkippedCandidate {
                path: "a.rs".to_string(),
                start_line: 4,
                end_line: Some(30),
                symbol_name: "f".to_string(),
                language: "rust".to_string(),
                reason: SkipReason::StaleLocation,
                detail: "file changed since indexing".to_string(),
            },
            SkippedCandidate {
                path: "b.rs".to_string(),
                start_line: 9,
                end_line: None,
                symbol_name: "g".to_string(),
                language: "rust".to_string(),
                reason: SkipReason::InvalidRange,
                detail: "missing end line".to_string(),
            },
            SkippedCandidate {
                path: "c.rs".to_string(),
                start_line: 1,
                end_line: Some(9),
                symbol_name: "h".to_string(),
                language: "rust".to_string(),
                reason: SkipReason::ParseError,
                detail: "syntax error at line 5: unexpected \"}\", expected `;`".to_string(),
            },
            SkippedCandidate {
                path: "d.rs".to_string(),
                start_line: 2,
                end_line: Some(12),
                symbol_name: "i".to_string(),
                language: "rust".to_string(),
                reason: SkipReason::StaleLocation,
                detail: "file changed since indexing".to_string(),
            },
        ];
        report
    }

    #[test]
    fn empty_report_is_valid_complete_output_with_zero_groups() {
        let expected = "\
notice: Groups below are CANDIDATE duplicates for human review. Identical normalized structure does not prove that the functions are redundant or behave the same.
status: complete
options:
  min_lines: 6
  min_tokens: 40
supported_languages[0]:
unsupported_languages[0]:
generated_code: generated code is excluded only by the project's configured exclusion rules; no generated-code marker is detected
candidates_analyzed: 0
below_threshold: 0
total_groups: 0
total_skipped: 0
skipped_by_reason:
  read-error: 0
  parse-error: 0
  stale-location: 0
  invalid-range: 0
groups[0]:
skipped[0]{path,range,symbol,language,reason,detail}:
";
        assert_eq!(render_toon(&empty_report()), expected);
    }

    #[test]
    fn groups_render_header_evidence_and_member_rows_in_report_order() {
        let out = render_toon(&group_report());
        assert!(
            out.contains("supported_languages[2]: python,rust\n"),
            "{out}"
        );
        assert!(out.contains("unsupported_languages[1]: go\n"), "{out}");
        assert!(out.contains("candidates_analyzed: 5\n"), "{out}");
        assert!(out.contains("below_threshold: 2\n"), "{out}");
        assert!(out.contains("total_groups: 1\n"), "{out}");
        assert!(out.contains("total_skipped: 0\n"), "{out}");
        let expected = "\
groups[1]:
  - index: 1
    language: rust
    member_count: 2
    line_count: 8
    token_count: 52
    evidence: \"members share an identical normalized token sequence (identifiers -> $ID, literals -> $LIT); inspect src/a.rs:10-20, src/b.rs:3-13\"
    members[2]{path,range,symbol,kind,language,lines,tokens,excerpt}:
      src/a.rs,10-20,load,function,rust,8,52,\"    fn load(name: &str, flag: bool) -> Result<u8, String> {\\n        let x = \\\"a,b\\\";\\n    }\"
      src/b.rs,3-13,read,function,rust,8,52,\"fn read() {\"
skipped[0]{path,range,symbol,language,reason,detail}:
";
        assert!(out.ends_with(expected), "{out}");
        assert!(out.contains("status: complete\n"), "{out}");
        assert!(!out.contains("partial_reason"), "{out}");
    }

    #[test]
    fn partial_report_lists_every_reason_count_and_each_skipped_row() {
        let out = render_toon(&partial_report());
        assert!(out.contains("status: partial\n"), "{out}");
        assert!(out.contains("partial_reason: "), "{out}");
        assert!(out.contains("total_groups: 0\n"), "{out}");
        assert!(out.contains("total_skipped: 4\n"), "{out}");
        let counts = "\
skipped_by_reason:
  read-error: 0
  parse-error: 1
  stale-location: 2
  invalid-range: 1
";
        assert!(out.contains(counts), "{out}");
        let rows = "\
skipped[4]{path,range,symbol,language,reason,detail}:
  a.rs,4-30,f,rust,stale-location,file changed since indexing
  b.rs,9-?,g,rust,invalid-range,missing end line
  c.rs,1-9,h,rust,parse-error,\"syntax error at line 5: unexpected \\\"}\\\", expected `;`\"
  d.rs,2-12,i,rust,stale-location,file changed since indexing
";
        assert!(out.ends_with(rows), "{out}");
    }

    #[test]
    fn rendering_is_deterministic_and_keeps_the_reports_order() {
        let report = group_report();
        assert_eq!(render_toon(&report), render_toon(&report.clone()));

        let mut reordered = report.clone();
        reordered.groups[0].members.reverse();
        let out = render_toon(&reordered);
        let b = out.find("src/b.rs,3-13").unwrap();
        let a = out.find("src/a.rs,10-20").unwrap();
        assert!(b < a, "members must follow the report's order: {out}");
    }

    #[test]
    fn quoting_follows_the_toon_rules() {
        for plain in ["abc", "src/a.rs", "a b", "rust", "$ID", "a-b", "x.y"] {
            assert_eq!(quote_if_needed(plain), plain);
        }
        for (raw, quoted) in [
            ("", "\"\""),
            ("a,b", "\"a,b\""),
            ("a: b", "\"a: b\""),
            ("say \"hi\"", "\"say \\\"hi\\\"\""),
            ("back\\slash", "\"back\\\\slash\""),
            (" lead", "\" lead\""),
            ("trail ", "\"trail \""),
            ("[x]", "\"[x]\""),
            ("{x}", "\"{x}\""),
            ("true", "\"true\""),
            ("false", "\"false\""),
            ("null", "\"null\""),
            ("42", "\"42\""),
            ("-7", "\"-7\""),
            ("3.14", "\"3.14\""),
            ("05", "\"05\""),
            ("-x", "\"-x\""),
            ("a\nb", "\"a\\nb\""),
            ("a\r\nb\tc", "\"a\\r\\nb\\tc\""),
            ("a\u{1}b", "\"a\u{FFFD}b\""),
        ] {
            assert_eq!(quote_if_needed(raw), quoted, "value {raw:?}");
        }
    }

    #[test]
    fn excerpt_with_commas_quotes_and_colons_stays_on_one_row() {
        let out = render_toon(&group_report());
        let row = out
            .lines()
            .find(|line| line.trim_start().starts_with("src/a.rs,10-20"))
            .unwrap();
        // The three excerpt lines are escaped into one quoted cell.
        assert!(
            row.contains("\\n        let x = \\\"a,b\\\";\\n    }\""),
            "{row}"
        );
        assert_eq!(
            out.lines().filter(|l| l.contains("src/a.rs,10-20")).count(),
            1
        );
    }

    #[test]
    fn empty_excerpt_is_an_explicit_empty_string() {
        let mut report = group_report();
        report.groups[0].members[1].excerpt.clear();
        let out = render_toon(&report);
        assert!(
            out.contains("src/b.rs,3-13,read,function,rust,8,52,\"\"\n"),
            "{out}"
        );
    }

    #[test]
    fn language_lists_with_special_characters_are_quoted() {
        let mut report = empty_report();
        report.supported_languages = vec!["c++".to_string(), "a,b".to_string()];
        let out = render_toon(&report);
        assert!(
            out.contains("supported_languages[2]: c++,\"a,b\"\n"),
            "{out}"
        );
    }

    #[test]
    fn threshold_parser_accepts_positive_integers_only() {
        assert_eq!(parse_threshold("6"), Ok(6));
        assert_eq!(parse_threshold(" 12 "), Ok(12));
        assert!(parse_threshold("0").is_err());
        assert!(parse_threshold("-3").is_err());
        assert!(parse_threshold("abc").is_err());
        assert!(parse_threshold("").is_err());
        assert!(parse_threshold("2.5").is_err());
    }

    #[test]
    fn skip_reason_order_covers_every_reason_once() {
        let names: Vec<&str> = SKIP_REASONS.iter().map(|r| r.as_str()).collect();
        assert_eq!(
            names,
            [
                "read-error",
                "parse-error",
                "stale-location",
                "invalid-range"
            ]
        );
        // Exhaustive match: a new variant breaks the build here, reminding the
        // author to extend SKIP_REASONS.
        for reason in SKIP_REASONS {
            match reason {
                SkipReason::ReadError
                | SkipReason::ParseError
                | SkipReason::StaleLocation
                | SkipReason::InvalidRange => {}
            }
        }
    }
}
