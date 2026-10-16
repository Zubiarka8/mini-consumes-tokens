//! Tests for `find_duplicate_groups` (issue #207, Stage 7).
//!
//! `mct-index` knows no grammar and has no language crate as a dev-dependency,
//! so everything here runs on TEST DOUBLES over a toy syntax, not on a
//! production lexer:
//!
//! - `Toy` is both a `LanguageParser` and a `TokenSource`. A `.dup` file
//!   (language `dupa`) or `.dpb` file (language `dupb`) holds functions
//!   written as `fn NAME` ... `end` (or `method NAME` ... `end`; a name starting
//!   with `m_` also makes a method); `end` carries
//!   the function's `end_line`. `struct NAME` is a one-line symbol of another
//!   kind. A third language, `nolex` (`.nlx`), has a parser but deliberately no
//!   token source.
//! - Tokens are whitespace-separated words. A fixed list are keywords, another
//!   fixed list punctuation, digits and `"..."` words literals, anything else
//!   an identifier. `#` starts a comment. `<<` ... `>>` is one literal that may
//!   span lines. `{{` and `}}` emit a zero-width, empty-text block start and
//!   end, as a layout-only block (Python's) does. Marker comments drive failure cases: `# !!tokenize-fail`
//!   makes `tokenize` return `Err`; `# !!error` reports its own line in
//!   `error_lines`.
//! - A symbol name prefix makes the parser store a bad range: `noend_` (no end
//!   line), `inverted_` (end before start), `beyond_` (end past the file),
//!   `zero_` (start line 0).
//!
//! Issue case 14 (generated code): this repository has no generated-code
//! marker, so the only mechanism is configured exclusion; it is covered by the
//! `.mctignore` tests, with no heuristic detection to test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::{
    LanguageParser, LanguageRegistry, LineRange, Location, ParseError, ParsedFile, SourceFile,
    SymbolKind, SymbolRecord, Token, TokenKind, TokenSource, TokenSourceRegistry, TokenizedFile,
};
use mct_index::{
    find_duplicate_groups, DuplicateOptions, DuplicateReport, ExcludeSet, Index, SkipReason,
    VerdictCounts, VerdictLevel, VerdictReason, MAX_EXCERPT_LINES, MAX_EXCERPT_LINE_CHARS,
};

// ---------------------------------------------------------------------------
// Test doubles
// ---------------------------------------------------------------------------

struct Toy {
    id: &'static str,
    ext: &'static str,
}

const KEYWORDS: &[&str] = &[
    "fn", "method", "end", "if", "while", "return", "let", "struct",
];
const PUNCTUATION: &[&str] = &["=", "+", "-", "*", "(", ")", ";", ","];

impl LanguageParser for Toy {
    fn language_id(&self) -> &'static str {
        self.id
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        match self.ext {
            "dup" => &["dup"],
            "dpb" => &["dpb"],
            _ => &["nlx"],
        }
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parsed = ParsedFile::default();
        let mut open: Option<(SymbolKind, String, u32)> = None;
        for (index, raw) in file.contents.lines().enumerate() {
            let line_no = u32::try_from(index).unwrap() + 1;
            let code = raw.split('#').next().unwrap_or_default();
            let words: Vec<&str> = code
                .split_whitespace()
                .filter(|word| !matches!(*word, "{{" | "}}"))
                .collect();
            match words.as_slice() {
                ["fn", name, ..] if open.is_none() => {
                    open = Some((SymbolKind::Function, (*name).to_string(), line_no));
                }
                ["method", name, ..] if open.is_none() => {
                    open = Some((SymbolKind::Method, (*name).to_string(), line_no));
                }
                ["end"] => {
                    if let Some((kind, name, start)) = open.take() {
                        push_symbol(&mut parsed, kind, name, start, line_no);
                    }
                }
                ["struct", name, ..] => {
                    let id = u32::try_from(parsed.symbols.len()).unwrap();
                    parsed.symbols.push(SymbolRecord {
                        id,
                        name: (*name).to_string(),
                        kind: SymbolKind::Struct,
                        location: Location {
                            line: line_no,
                            column: 1,
                            byte_len: 1,
                            end_line: Some(line_no),
                        },
                        parent: None,
                        level: None,
                    });
                }
                _ => {}
            }
        }
        Ok(parsed)
    }
}

/// Records a function, bending its stored range when the name asks for it.
fn push_symbol(parsed: &mut ParsedFile, kind: SymbolKind, name: String, start: u32, end: u32) {
    let kind = if name.starts_with("m_") {
        SymbolKind::Method
    } else {
        kind
    };
    let (line, end_line) = if name.starts_with("noend_") {
        (start, None)
    } else if name.starts_with("inverted_") {
        (end, Some(start))
    } else if name.starts_with("beyond_") {
        (start, Some(end + 1000))
    } else if name.starts_with("zero_") {
        (0, Some(end))
    } else {
        (start, Some(end))
    };
    let id = u32::try_from(parsed.symbols.len()).unwrap();
    parsed.symbols.push(SymbolRecord {
        id,
        name,
        kind,
        location: Location {
            line,
            column: 1,
            byte_len: 1,
            end_line,
        },
        parent: None,
        level: None,
    });
}

impl TokenSource for Toy {
    fn language_id(&self) -> &'static str {
        self.id
    }

    fn tokenize(&self, relative_path: &str, source: &str) -> Result<TokenizedFile, ParseError> {
        if source.contains("!!tokenize-fail") {
            return Err(ParseError::Syntax {
                path: relative_path.to_string(),
                line: 1,
                message: "toy tokenizer failure".to_string(),
            });
        }
        let mut out = TokenizedFile::default();
        let mut line_start = 0usize;
        // Start (line, byte) of a `<< ... >>` literal still open.
        let mut literal: Option<(u32, usize)> = None;
        for (index, raw) in source.split_inclusive('\n').enumerate() {
            let line_no = u32::try_from(index).unwrap() + 1;
            let content = raw.trim_end_matches(['\n', '\r']);
            if content.contains("!!error") {
                out.error_lines.push(LineRange {
                    start: line_no,
                    end: line_no,
                });
            }
            let code = &content[..content.find('#').unwrap_or(content.len())];
            let mut pos = 0usize;
            while pos < code.len() {
                let rest = &code[pos..];
                let trimmed = rest.trim_start();
                pos += rest.len() - trimmed.len();
                if trimmed.is_empty() {
                    break;
                }
                let len = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
                let word = &trimmed[..len];
                let start_byte = line_start + pos;
                let end_byte = start_byte + len;
                pos += len;

                if let Some((start_line, from)) = literal {
                    if word == ">>" {
                        out.tokens.push(Token {
                            kind: TokenKind::Literal,
                            role: Default::default(),
                            primitive_spelling: false,
                            text: source[from..end_byte].to_string(),
                            line: start_line,
                            end_line: line_no,
                            start_byte: from,
                            end_byte,
                        });
                        literal = None;
                    }
                    continue;
                }
                if word == "<<" {
                    literal = Some((line_no, start_byte));
                    continue;
                }
                let marker = match word {
                    "{{" => Some(TokenKind::BlockStart),
                    "}}" => Some(TokenKind::BlockEnd),
                    _ => None,
                };
                if let Some(kind) = marker {
                    out.tokens.push(Token {
                        kind,
                        role: Default::default(),
                        primitive_spelling: false,
                        text: String::new(),
                        line: line_no,
                        end_line: line_no,
                        start_byte,
                        end_byte: start_byte,
                    });
                    continue;
                }
                let kind = if KEYWORDS.contains(&word) {
                    TokenKind::Keyword
                } else if PUNCTUATION.contains(&word) {
                    TokenKind::Punctuation
                } else if word.starts_with('"') || word.chars().all(|c| c.is_ascii_digit()) {
                    TokenKind::Literal
                } else {
                    TokenKind::Identifier
                };
                out.tokens.push(Token {
                    kind,
                    role: Default::default(),
                    primitive_spelling: false,
                    text: word.to_string(),
                    line: line_no,
                    end_line: line_no,
                    start_byte,
                    end_byte,
                });
            }
            line_start += raw.len();
        }
        Ok(out)
    }
}

fn parsers() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    for (id, ext) in [("dupa", "dup"), ("dupb", "dpb"), ("nolex", "nlx")] {
        registry.register(Arc::new(Toy { id, ext }));
    }
    registry
}

fn token_sources() -> TokenSourceRegistry {
    let mut registry = TokenSourceRegistry::new();
    for (id, ext) in [("dupa", "dup"), ("dupb", "dpb")] {
        registry.register(Arc::new(Toy { id, ext }));
    }
    registry
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct Fixture {
    root: PathBuf,
    set: ExcludeSet,
    index: Index,
}

impl Fixture {
    /// Writes `files`, then indexes the project (honoring its `.mctignore`).
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-duplicates-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        for (path, contents) in files {
            write(&root, path, contents);
        }
        let set = ExcludeSet::for_project(&root);
        let mut index = Index::open_in_memory(&root, set.clone()).unwrap();
        index.reindex(&parsers(), false).unwrap();
        Self { root, set, index }
    }

    fn run(&self, options: DuplicateOptions) -> DuplicateReport {
        find_duplicate_groups(&self.index, &token_sources(), options).unwrap()
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// 6 lines or more, 20 tokens or more: every `func` below passes.
fn small() -> DuplicateOptions {
    DuplicateOptions {
        min_lines: 6,
        min_tokens: 20,
        ..DuplicateOptions::default()
    }
}

/// A function of 7 token-bearing lines and 30 tokens.
fn func(name: &str, a: &str, b: &str, lit: &str) -> String {
    func_with("fn", name, a, b, lit, 1)
}

/// `kind NAME`, then `repeat` copies of a 5-line / 27-token block, then `end`:
/// `2 + 5 * repeat` lines, `3 + 27 * repeat` tokens.
fn func_with(kind: &str, name: &str, a: &str, b: &str, lit: &str, repeat: usize) -> String {
    let block = format!(
        "let {a} = {lit} + {b} ;\nwhile {a} + {b} ;\nif {a} = {b} ;\n\
         return {a} + {lit} ;\nlet {b} = {a} ;\n"
    );
    format!("{kind} {name}\n{}end\n", block.repeat(repeat))
}

fn borrowed<'a>(files: &'a [(&'a str, String)]) -> Vec<(&'a str, &'a str)> {
    files.iter().map(|(p, c)| (*p, c.as_str())).collect()
}

fn spans(report: &DuplicateReport) -> Vec<Vec<(String, u32, u32)>> {
    report
        .groups
        .iter()
        .map(|group| {
            group
                .members
                .iter()
                .map(|m| (m.path.clone(), m.start_line, m.end_line))
                .collect()
        })
        .collect()
}

fn skipped(report: &DuplicateReport) -> Vec<(String, String, SkipReason)> {
    report
        .skipped
        .iter()
        .map(|s| (s.path.clone(), s.symbol_name.clone(), s.reason))
        .collect()
}

// ---------------------------------------------------------------------------
// Stage 7 cases
// ---------------------------------------------------------------------------

#[test]
fn case_1_same_structure_different_names_form_one_group() {
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        ("b.dup", &func("beta", "left", "right", "1")),
    ]);
    let hashes_before = fixture.index.file_hashes().unwrap();
    let report = fixture.run(small());

    assert_eq!(report.groups.len(), 1, "{report:#?}");
    let group = &report.groups[0];
    assert_eq!(group.language, "dupa");
    assert_eq!((group.line_count, group.token_count), (7, 30));
    let members: Vec<_> = group
        .members
        .iter()
        .map(|m| {
            (
                m.path.as_str(),
                m.symbol_name.as_str(),
                m.symbol_kind.as_str(),
            )
        })
        .collect();
    assert_eq!(
        members,
        [
            ("a.dup", "alpha", "function"),
            ("b.dup", "beta", "function")
        ]
    );
    assert_eq!(
        (group.members[0].start_line, group.members[0].end_line),
        (1, 7)
    );
    assert!(report.complete && report.skipped.is_empty());
    assert_eq!(report.candidates_analyzed, 2);
    assert_eq!(report.below_threshold, 0);
    assert_eq!(report.supported_languages, ["dupa", "dupb"]);
    assert!(report.unsupported_languages.is_empty());
    assert_eq!(report.options, small());
    assert_eq!(
        group.members[0].excerpt,
        ["fn alpha", "let x = 1 + y ;", "while x + y ;"]
    );
    // Read-only: neither the index nor a file changed.
    assert_eq!(fixture.index.file_hashes().unwrap(), hashes_before);
    assert_eq!(
        fs::read_to_string(fixture.path("a.dup")).unwrap(),
        func("alpha", "x", "y", "1")
    );
}

/// A token source without callable spans or body shapes (the toy) can never
/// support a `REAL` verdict, and counts follow the reported page.
#[test]
fn verdicts_need_context_and_body_shape_and_counts_follow_the_page() {
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        ("b.dup", &func("beta", "left", "right", "1")),
        ("c.dup", &func_with("fn", "gamma", "x", "y", "1", 2)),
        ("d.dup", &func_with("fn", "delta", "x", "y", "1", 2)),
    ]);
    let report = fixture.run(small());
    assert_eq!(report.groups.len(), 2, "{report:#?}");
    for group in &report.groups {
        assert_eq!(group.verdict.level, VerdictLevel::Suspicious);
        assert!(
            group.verdict.reasons.ends_with(&[
                VerdictReason::ContextUnavailable,
                VerdictReason::BodyShapeNotEvaluated
            ]),
            "{:?}",
            group.verdict
        );
    }
    // The toy gives no token a role, not even the function names, so every
    // renamed spelling is a name that no member is known to bind.
    assert!(report.groups.iter().all(|group| group
        .verdict
        .reasons
        .contains(&VerdictReason::UndeclaredNamesDiffer)));
    assert_eq!(
        report.verdict_counts,
        VerdictCounts {
            real: 0,
            suspicious: 2,
            false_positive: 0
        }
    );

    let page = fixture.run(DuplicateOptions {
        limit: Some(1),
        offset: 1,
        ..small()
    });
    assert_eq!((page.total_groups, page.groups.len()), (2, 1));
    assert_eq!(page.verdict_counts.suspicious, 1);
    // The verdict never changes membership or order.
    assert_eq!(spans(&page)[0], spans(&report)[1]);
}

#[test]
fn case_2_different_literal_values_still_group() {
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        ("b.dup", &func("beta", "x", "y", "\"text\"")),
    ]);
    let report = fixture.run(small());
    assert_eq!(report.groups.len(), 1, "{report:#?}");
    assert_eq!(report.groups[0].members.len(), 2);
}

#[test]
fn case_3_a_function_below_min_lines_is_not_grouped_or_skipped() {
    // 5 token-bearing lines, 5 + 6 + 5 + 5 + 1 tokens: plenty of tokens.
    let short = |name: &str| {
        format!("fn {name}\nlet a = 1 + b ;\nwhile a + b ;\nif a = b ;\nreturn a + 1 ;\nend\n")
    };
    let fixture = Fixture::new(&[("a.dup", &short("one")), ("b.dup", &short("two"))]);
    let report = fixture.run(DuplicateOptions {
        min_lines: 7,
        min_tokens: 1,
        ..DuplicateOptions::default()
    });
    assert!(report.groups.is_empty());
    assert_eq!(report.candidates_analyzed, 2);
    assert_eq!(report.below_threshold, 2);
    assert!(report.skipped.is_empty() && report.complete);

    // The same functions pass when the threshold is their size.
    let report = fixture.run(DuplicateOptions {
        min_lines: 6,
        min_tokens: 1,
        ..DuplicateOptions::default()
    });
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.below_threshold, 0);
}

#[test]
fn case_4_a_function_below_min_tokens_is_not_grouped_or_skipped() {
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        ("b.dup", &func("beta", "x", "y", "1")),
    ]);
    let report = fixture.run(DuplicateOptions {
        min_lines: 1,
        min_tokens: 31,
        ..DuplicateOptions::default()
    });
    assert!(report.groups.is_empty());
    assert_eq!(report.below_threshold, 2);
    assert!(report.skipped.is_empty() && report.complete);

    // Exactly the token count (30) is enough: the bound is inclusive.
    let report = fixture.run(DuplicateOptions {
        min_lines: 1,
        min_tokens: 30,
        ..DuplicateOptions::default()
    });
    assert_eq!(report.groups.len(), 1);
}

#[test]
fn the_default_thresholds_are_six_lines_and_forty_tokens() {
    let big = |name: &str| func_with("fn", name, "x", "y", "1", 2); // 12 lines, 57 tokens
    let fixture = Fixture::new(&[
        ("big_a.dup", &big("one")),
        ("big_b.dup", &big("two")),
        // 7 lines but only 30 tokens: under the default token threshold.
        ("small_a.dup", &func("three", "x", "y", "1")),
        ("small_b.dup", &func("four", "x", "y", "1")),
    ]);
    let report = fixture.run(DuplicateOptions::default());
    assert_eq!(report.options.min_lines, 6);
    assert_eq!(report.options.min_tokens, 40);
    assert_eq!(report.groups.len(), 1, "{report:#?}");
    assert_eq!(
        (report.groups[0].line_count, report.groups[0].token_count),
        (12, 57)
    );
    assert_eq!(report.candidates_analyzed, 4);
    assert_eq!(report.below_threshold, 2);
}

#[test]
fn case_5_comments_and_whitespace_do_not_change_the_group() {
    let plain = func("alpha", "x", "y", "1");
    let noisy = "# leading comment\n\nfn beta   # trailing comment\n\n   let a   =  1\t+ b ;   # c\n\
                 # a comment-only line\nwhile a + b ;\n\n\nif a = b ;\n    return a + 1 ;\nlet b = a ;\n\n# done\nend\n";
    let fixture = Fixture::new(&[("a.dup", &plain), ("b.dup", noisy)]);
    let report = fixture.run(small());
    assert_eq!(report.groups.len(), 1, "{report:#?}");
    let group = &report.groups[0];
    assert_eq!((group.line_count, group.token_count), (7, 30));
    // Comment and blank lines are inside the range but not counted.
    let noisy_member = &group.members[1];
    assert_eq!((noisy_member.start_line, noisy_member.end_line), (3, 15));
    assert_eq!(noisy_member.line_count, 7);
}

/// 9 physical lines; `{{`/`}}` hold no source tokens, so 7 token-bearing
/// lines and 30 tokens. `close_after` names the statement the block ends after.
fn blocked(name: &str, a: &str, b: &str, close_after: &str) -> String {
    let statements = [
        ("let", format!("let {a} = 1 + {b} ;\n")),
        ("while", format!("while {a} + {b} ;\n")),
        ("if", format!("if {a} = {b} ;\n")),
        ("return", format!("return {a} + 1 ;\n")),
        ("last", format!("let {b} = {a} ;\n")),
    ];
    let mut body = String::from("{{\n");
    for (key, statement) in &statements {
        body.push_str(statement);
        if *key == close_after {
            body.push_str("}}\n");
        }
    }
    format!("fn {name}\n{body}end\n")
}

#[test]
fn block_markers_separate_other_nesting_without_adding_tokens_or_lines() {
    let files = [
        ("a.dup", blocked("a", "x", "y", "while")),
        ("b.dup", blocked("b", "p", "q", "while")),
        // Same source tokens as `a`, but `if` moved into the block.
        ("c.dup", blocked("c", "x", "y", "if")),
    ];
    let fixture = Fixture::new(&borrowed(&files));

    let report = fixture.run(small());
    assert_eq!(
        spans(&report),
        [[("a.dup".to_string(), 1, 9), ("b.dup".to_string(), 1, 9)]]
    );
    let member = &report.groups[0].members[0];
    assert_eq!((member.line_count, member.token_count), (7, 30));
    assert_eq!(report.below_threshold, 0);

    // Counting the marker-only lines would make these 9 lines long.
    let report = fixture.run(DuplicateOptions {
        min_lines: 8,
        min_tokens: 20,
        ..DuplicateOptions::default()
    });
    assert!(report.groups.is_empty());
    assert_eq!(report.below_threshold, 3);
}

#[test]
fn markers_of_an_enclosing_block_on_a_boundary_line_are_ignored() {
    let plain = blocked("a", "x", "y", "while");
    // An enclosing block that starts on the function's first line and ends on
    // its last one (a class whose only method this is).
    let wrapped = format!("{{{{ {}", blocked("b", "x", "y", "while")).replace("end\n", "end }}\n");
    // An enclosing block opened above the function and closed on its last line
    // (the last method of a class).
    let closed = format!("{{{{\n{}", blocked("c", "x", "y", "while")).replace("end\n", "end }}\n");
    let files = [("a.dup", plain), ("b.dup", wrapped), ("c.dup", closed)];
    let fixture = Fixture::new(&borrowed(&files));

    let report = fixture.run(small());
    assert_eq!(
        spans(&report),
        [[
            ("a.dup".to_string(), 1, 9),
            ("b.dup".to_string(), 1, 9),
            ("c.dup".to_string(), 2, 10)
        ]]
    );
}

#[test]
fn case_6_different_operators_order_or_control_flow_do_not_group() {
    let base = func("base", "x", "y", "1");
    let variants = [
        ("operator", base.replacen("while x + y", "while x - y", 1)),
        ("control flow", base.replacen("while", "if", 1)),
        (
            "order",
            "fn base\nwhile x + y ;\nlet x = 1 + y ;\nif x = y ;\nreturn x + 1 ;\nlet y = x ;\nend\n"
                .to_string(),
        ),
        ("extra token", base.replacen("let y = x ;", "let y = x , x ;", 1)),
        ("keyword vs identifier", base.replacen("return", "ret", 1)),
    ];
    for (label, variant) in variants {
        let fixture = Fixture::new(&[("a.dup", &base), ("b.dup", &variant)]);
        let report = fixture.run(small());
        assert!(report.groups.is_empty(), "{label}: {report:#?}");
        assert_eq!(report.candidates_analyzed, 2, "{label}");
        assert!(report.complete, "{label}");
    }
}

#[test]
fn case_7_symbols_under_an_excluded_directory_do_not_appear() {
    let fixture = Fixture::new(&[
        (".mctignore", "gen/\n"),
        ("gen/a.dup", &func("alpha", "x", "y", "1")),
        ("src/b.dup", &func("beta", "x", "y", "1")),
    ]);
    let report = fixture.run(small());
    assert!(report.groups.is_empty(), "{report:#?}");
    assert_eq!(report.candidates_analyzed, 1);
    assert!(report.skipped.is_empty() && report.complete);
}

#[test]
fn case_7_rows_of_a_directory_excluded_after_indexing_are_dropped_silently() {
    let fixture = Fixture::new(&[
        ("gen/a.dup", &func("alpha", "x", "y", "1")),
        ("src/b.dup", &func("beta", "x", "y", "1")),
    ]);
    assert_eq!(fixture.run(small()).groups.len(), 1);

    // The rules change but the index is not refreshed: its rows for `gen/`
    // remain, yet the analysis must not report them.
    write(&fixture.root, ".mctignore", "gen/\n");
    assert!(fixture.set.reload());
    let report = fixture.run(small());
    assert!(report.groups.is_empty(), "{report:#?}");
    assert_eq!(report.candidates_analyzed, 1);
    assert!(report.skipped.is_empty() && report.complete);
}

#[test]
fn case_8_an_unreadable_file_is_skipped_as_a_read_error() {
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        ("b.dup", &func("beta", "x", "y", "1")),
        ("c.dup", &func("gamma", "x", "y", "1")),
    ]);
    // A directory in place of the file: not a regular file, not "not found".
    fs::remove_file(fixture.path("b.dup")).unwrap();
    fs::create_dir(fixture.path("b.dup")).unwrap();

    let report = fixture.run(small());
    assert_eq!(
        skipped(&report),
        [(
            "b.dup".to_string(),
            "beta".to_string(),
            SkipReason::ReadError
        )]
    );
    assert!(!report.complete);
    assert!(!report.skipped[0].detail.is_empty());
    // The healthy files still group.
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].members.len(), 2);
}

#[test]
fn case_8_a_failing_token_source_skips_every_candidate_of_the_file() {
    let broken = format!(
        "{}{}# !!tokenize-fail\n",
        func("one", "x", "y", "1"),
        func("two", "x", "y", "1")
    );
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        ("b.dup", &func("beta", "x", "y", "1")),
        ("broken.dup", &broken),
    ]);
    let report = fixture.run(small());
    assert_eq!(
        skipped(&report),
        [
            (
                "broken.dup".to_string(),
                "one".to_string(),
                SkipReason::ParseError
            ),
            (
                "broken.dup".to_string(),
                "two".to_string(),
                SkipReason::ParseError
            ),
        ]
    );
    assert!(!report.complete);
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.candidates_analyzed, 2);
}

#[test]
fn case_8_only_candidates_overlapping_a_syntax_error_are_skipped() {
    let damaged = format!(
        "{}{}",
        func("good", "x", "y", "1"),
        func("bad", "x", "y", "1").replacen("while x + y ;", "while x + y ; # !!error", 1)
    );
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        ("damaged.dup", &damaged),
    ]);
    let report = fixture.run(small());
    assert_eq!(
        skipped(&report),
        [(
            "damaged.dup".to_string(),
            "bad".to_string(),
            SkipReason::ParseError
        )]
    );
    assert!(report.skipped[0].detail.contains("syntax error"));
    // The function in the same file that does not overlap the error is used.
    assert_eq!(report.groups.len(), 1, "{report:#?}");
    assert_eq!(report.groups[0].members.len(), 2);
}

#[test]
fn case_8_a_deleted_file_is_a_stale_location() {
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        ("b.dup", &func("beta", "x", "y", "1")),
    ]);
    fs::remove_file(fixture.path("b.dup")).unwrap();
    let report = fixture.run(small());
    assert_eq!(
        skipped(&report),
        [(
            "b.dup".to_string(),
            "beta".to_string(),
            SkipReason::StaleLocation
        )]
    );
    assert!(report.groups.is_empty() && !report.complete);
}

#[test]
fn case_8_invalid_indexed_ranges_are_skipped_as_invalid_range() {
    let source: String = ["noend_f", "inverted_f", "beyond_f", "zero_f"]
        .iter()
        .map(|name| func(name, "x", "y", "1"))
        .collect();
    let fixture = Fixture::new(&[("a.dup", &source), ("ok.dup", &func("fine", "x", "y", "1"))]);
    let report = fixture.run(small());

    let mut names: Vec<_> = report
        .skipped
        .iter()
        .map(|s| (s.symbol_name.as_str(), s.reason))
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            ("beyond_f", SkipReason::InvalidRange),
            ("inverted_f", SkipReason::InvalidRange),
            ("noend_f", SkipReason::InvalidRange),
            ("zero_f", SkipReason::InvalidRange),
        ]
    );
    let noend = report
        .skipped
        .iter()
        .find(|s| s.symbol_name == "noend_f")
        .unwrap();
    assert_eq!(noend.end_line, None);
    assert!(!report.complete);
    assert!(report.groups.is_empty());
    assert_eq!(report.candidates_analyzed, 1);
}

#[test]
fn case_9_a_file_changed_after_indexing_never_forms_a_group() {
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        ("b.dup", &func("beta", "x", "y", "1")),
    ]);
    // Same structure, but every line moved down: the indexed range now points
    // at the wrong lines, so the file must not be trusted at all.
    write(
        &fixture.root,
        "b.dup",
        &format!("# added\n\n{}", func("beta", "x", "y", "1")),
    );
    let report = fixture.run(small());
    assert!(report.groups.is_empty(), "{report:#?}");
    assert_eq!(
        skipped(&report),
        [(
            "b.dup".to_string(),
            "beta".to_string(),
            SkipReason::StaleLocation
        )]
    );
    assert!(!report.complete);

    // A rewrite to a different function is just as stale.
    write(&fixture.root, "b.dup", "fn other\nreturn 1 ;\nend\n");
    let report = fixture.run(small());
    assert!(report.groups.is_empty());
    assert_eq!(report.skipped[0].reason, SkipReason::StaleLocation);

    // Re-indexing makes the file current again.
    let mut fixture = fixture;
    write(&fixture.root, "b.dup", &func("beta", "x", "y", "1"));
    fixture.index.reindex(&parsers(), false).unwrap();
    let report = fixture.run(small());
    assert_eq!(report.groups.len(), 1);
    assert!(report.complete);
}

#[test]
fn case_9_content_that_stopped_being_utf8_is_stale() {
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        ("b.dup", &func("beta", "x", "y", "1")),
    ]);
    fs::write(fixture.path("b.dup"), [0xff, 0xfe, 0x00, 0x80]).unwrap();
    let report = fixture.run(small());
    // The content hash is compared first, so a changed file is stale whatever
    // its encoding (the indexer never indexes non-UTF-8 content).
    assert_eq!(
        skipped(&report),
        [(
            "b.dup".to_string(),
            "beta".to_string(),
            SkipReason::StaleLocation
        )]
    );
}

#[cfg(unix)]
#[test]
fn a_file_replaced_by_a_symlink_out_of_the_root_is_never_read() {
    let outside = Fixture::new(&[("secret.dup", &func("secret", "x", "y", "1"))]);
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        ("b.dup", &func("beta", "x", "y", "1")),
    ]);
    fs::remove_file(fixture.path("b.dup")).unwrap();
    std::os::unix::fs::symlink(outside.path("secret.dup"), fixture.path("b.dup")).unwrap();

    let report = fixture.run(small());
    assert_eq!(
        skipped(&report),
        [(
            "b.dup".to_string(),
            "beta".to_string(),
            SkipReason::ReadError
        )]
    );
    assert!(report.groups.is_empty());
}

#[test]
fn case_10_no_duplicates_gives_zero_groups_and_a_complete_report() {
    let fixture = Fixture::new(&[
        ("a.dup", &func("alpha", "x", "y", "1")),
        (
            "b.dup",
            "fn beta\nlet a = 1 ;\nlet b = 2 ;\nlet c = 3 ;\nlet d = 4 ;\nlet e = 5 ;\nlet f = 6 ;\nend\n",
        ),
    ]);
    let report = fixture.run(small());
    assert!(report.groups.is_empty());
    assert!(report.skipped.is_empty() && report.complete);
    assert_eq!(report.candidates_analyzed, 2);

    let empty = Fixture::new(&[]);
    let report = empty.run(DuplicateOptions::default());
    assert!(report.groups.is_empty() && report.complete);
    assert_eq!(report.candidates_analyzed, 0);
    assert_eq!(report.supported_languages, ["dupa", "dupb"]);
}

#[test]
fn case_11_equally_weighted_groups_are_ordered_by_first_member_path() {
    let files = [
        ("z1.dup", func("z_one", "x", "y", "1")),
        ("z2.dup", func("z_two", "x", "y", "1")),
        (
            "m1.dup",
            func("m_one", "x", "y", "1").replacen("while", "if", 1),
        ),
        (
            "m2.dup",
            func("m_two", "x", "y", "1").replacen("while", "if", 1),
        ),
        (
            "a1.dup",
            func("a_one", "x", "y", "1").replacen("return", "while", 1),
        ),
        (
            "a2.dup",
            func("a_two", "x", "y", "1").replacen("return", "while", 1),
        ),
    ];
    let borrowed = borrowed(&files);
    let fixture = Fixture::new(&borrowed);
    let report = fixture.run(small());

    let firsts: Vec<&str> = report
        .groups
        .iter()
        .map(|g| g.members[0].path.as_str())
        .collect();
    assert_eq!(firsts, ["a1.dup", "m1.dup", "z1.dup"]);
    // Deterministic: the same tree and index give an identical report, also
    // from a freshly built index over the same tree.
    assert_eq!(report, fixture.run(small()));
    let mut reversed = borrowed.clone();
    reversed.reverse();
    assert_eq!(report.groups, Fixture::new(&reversed).run(small()).groups);
}

#[test]
fn groups_are_ordered_by_lines_times_members_before_path() {
    let big = |name: &str| func_with("fn", name, "x", "y", "1", 2); // 12 lines
    let files = [
        // Weight 7 * 3 = 21 ...
        ("a1.dup", func("p1", "x", "y", "1")),
        ("a2.dup", func("p2", "x", "y", "1")),
        ("a3.dup", func("p3", "x", "y", "1")),
        // ... against 12 * 2 = 24, which sorts first despite its later path.
        ("z1.dup", big("q1")),
        ("z2.dup", big("q2")),
    ];
    let fixture = Fixture::new(&borrowed(&files));
    let report = fixture.run(small());
    let sizes: Vec<(usize, usize)> = report
        .groups
        .iter()
        .map(|g| (g.line_count, g.members.len()))
        .collect();
    assert_eq!(sizes, [(12, 2), (7, 3)]);
}

#[test]
fn case_12_more_than_two_copies_form_one_group_ordered_by_path_and_line() {
    let two_in_one_file = format!(
        "{}\n{}",
        func("second_in_file", "p", "q", "9"),
        func("third_in_file", "r", "s", "8")
    );
    let fixture = Fixture::new(&[
        ("c.dup", &func("copy_c", "x", "y", "1")),
        ("a.dup", &two_in_one_file),
        ("b.dup", &func("copy_b", "x", "y", "1")),
    ]);
    let report = fixture.run(small());
    assert_eq!(report.groups.len(), 1, "{report:#?}");
    assert_eq!(
        spans(&report)[0],
        [
            ("a.dup".to_string(), 1, 7),
            ("a.dup".to_string(), 9, 15),
            ("b.dup".to_string(), 1, 7),
            ("c.dup".to_string(), 1, 7),
        ]
    );
    assert_eq!(report.groups[0].members.len(), 4);
}

#[test]
fn case_13_identical_sequences_in_different_languages_are_not_grouped() {
    let text = func("alpha", "x", "y", "1");
    let fixture = Fixture::new(&[("a.dup", &text), ("b.dpb", &text)]);
    let report = fixture.run(small());
    assert!(report.groups.is_empty(), "{report:#?}");
    assert_eq!(report.candidates_analyzed, 2);
    assert!(report.complete);

    // With a pair in each language, two groups appear, each of one language.
    let fixture = Fixture::new(&[
        ("a.dup", &text),
        ("a2.dup", &text),
        ("b.dpb", &text),
        ("b2.dpb", &text),
    ]);
    let report = fixture.run(small());
    let mut languages: Vec<_> = report
        .groups
        .iter()
        .map(|g| {
            assert!(g.members.iter().all(|m| m.language == g.language));
            assert_eq!(g.members.len(), 2);
            g.language.as_str()
        })
        .collect();
    languages.sort();
    assert_eq!(languages, ["dupa", "dupb"]);
}

// ---------------------------------------------------------------------------
// Beyond the numbered cases
// ---------------------------------------------------------------------------

#[test]
fn a_language_without_a_token_source_is_reported_unsupported_not_skipped() {
    let text = func("alpha", "x", "y", "1");
    let fixture = Fixture::new(&[
        ("a.nlx", &text),
        ("b.nlx", &text),
        ("c.dup", &text),
        // A language with a parser but no token source whose symbols are not
        // functions is not listed.
        ("only_structs.nlx", "struct Thing\n"),
    ]);
    let report = fixture.run(small());
    assert_eq!(report.unsupported_languages, ["nolex"]);
    assert!(report.skipped.is_empty() && report.complete);
    assert!(report.groups.is_empty());
    assert_eq!(report.candidates_analyzed, 1);

    let fixture = Fixture::new(&[("only_structs.nlx", "struct Thing\n")]);
    assert!(fixture.run(small()).unsupported_languages.is_empty());
}

#[test]
fn only_functions_and_methods_are_compared_and_both_can_group() {
    let fixture = Fixture::new(&[
        (
            "a.dup",
            &format!("struct Thing\n{}", func("m_run", "x", "y", "1")),
        ),
        ("b.dup", &func("go", "x", "y", "1")),
    ]);
    let report = fixture.run(small());
    assert_eq!(report.groups.len(), 1, "{report:#?}");
    let kinds: Vec<&str> = report.groups[0]
        .members
        .iter()
        .map(|m| m.symbol_kind.as_str())
        .collect();
    assert_eq!(kinds, ["method", "function"]);
    assert_eq!(report.candidates_analyzed, 2);
}

#[test]
fn a_multi_line_literal_counts_every_line_it_spans() {
    let text = |name: &str| {
        format!("fn {name}\nlet a = <<\nfiller words\nmore filler\n>> ;\nreturn a ;\nend\n")
    };
    let fixture = Fixture::new(&[("a.dup", &text("one")), ("b.dup", &text("two"))]);
    let report = fixture.run(DuplicateOptions {
        min_lines: 7,
        min_tokens: 1,
        ..DuplicateOptions::default()
    });
    assert_eq!(report.groups.len(), 1, "{report:#?}");
    // Lines 3 and 4 hold only literal text but still contain token bytes.
    assert_eq!(
        (report.groups[0].line_count, report.groups[0].token_count),
        (7, 11)
    );
    let report = fixture.run(DuplicateOptions {
        min_lines: 8,
        min_tokens: 1,
        ..DuplicateOptions::default()
    });
    assert!(report.groups.is_empty());
    assert_eq!(report.below_threshold, 2);
}

#[test]
fn excerpts_are_bounded_in_lines_and_characters() {
    let long_name = "é".repeat(MAX_EXCERPT_LINE_CHARS * 2);
    let text = |name: &str| func(name, "x", "y", "1");
    let fixture = Fixture::new(&[("a.dup", &text(&long_name)), ("b.dup", &text("short"))]);
    let report = fixture.run(small());
    let excerpt = &report.groups[0].members[0].excerpt;
    assert_eq!(excerpt.len(), MAX_EXCERPT_LINES);
    assert_eq!(
        excerpt[0],
        format!("fn {}", "é".repeat(MAX_EXCERPT_LINE_CHARS - 3))
    );
    assert_eq!(excerpt[0].chars().count(), MAX_EXCERPT_LINE_CHARS);
    assert_eq!(excerpt[1], "let x = 1 + y ;");
}

#[test]
fn token_ranking_does_not_promote_a_long_docstring_over_more_repeated_code() {
    let documented = |name: &str| {
        format!(
            "fn {name}\nlet a = <<\n{}>> ;\nreturn a ;\nend\n",
            "documentation words\n".repeat(30)
        )
    };
    let fixture = Fixture::new(&[
        ("docs_a.dup", &documented("docs_a")),
        ("docs_b.dup", &documented("docs_b")),
        ("code_a.dup", &func("code_a", "x", "y", "1")),
        ("code_b.dup", &func("code_b", "x", "y", "2")),
    ]);
    let options = DuplicateOptions {
        min_lines: 1,
        min_tokens: 1,
        excerpts: false,
        ..DuplicateOptions::default()
    };
    let by_lines = fixture.run(options.clone());
    assert_eq!(by_lines.groups[0].members[0].symbol_name, "docs_a");
    let by_tokens = fixture.run(DuplicateOptions {
        ranking: mct_index::DuplicateRanking::Tokens,
        ..options
    });
    assert_eq!(by_tokens.groups[0].members[0].symbol_name, "code_a");
    assert!(by_tokens
        .groups
        .iter()
        .flat_map(|group| &group.members)
        .all(|member| member.excerpt.is_empty()));
}
