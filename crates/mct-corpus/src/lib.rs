//! Test-only harness for the long, multi-file fixture corpus every
//! `mct-lang-*` crate carries under `tests/corpus/` (issue #74).
//!
//! Layout, identical in every language crate:
//!
//! ```text
//! tests/corpus/
//!   project/               realistic files that reference each other (the
//!                          index root): ≥ 5 of them 300–700 lines long, the
//!                          rest (a small `urls.py`, a config…) at most 700
//!   expected.snap          golden list of every symbol and relation the
//!                          parser extracts from `project/`
//!   malformed/             at least one large file the parser must reject
//!                          with a syntax error
//! ```
//!
//! `CONTRIBUTING.md` ("Language corpus") has the conventions and the review
//! checklist.
//!
//! A crate's `tests/corpus.rs` loads the corpus once with [`Corpus::load`]
//! and runs the shared checks ([`Corpus::assert_size`],
//! [`Corpus::assert_line_ranges`], [`Corpus::assert_snapshot`], [`Corpus::assert_index_round_trip`],
//! [`Corpus::assert_malformed_never_panics`], [`Corpus::assert_malformed_rejected`]) plus its own
//! language-specific assertions through [`Corpus::symbol`] and
//! [`Corpus::relation`].
//!
//! After an intended parser change, regenerate the golden file with
//! `MCT_BLESS=1 cargo test -p <crate> --test corpus` and review its diff.
//! [`Corpus::report`] (printed by the ignored `corpus_report` test that
//! [`standard_tests!`] adds, via `scripts/unix/corpus-report.sh`) digests
//! the corpus and flags likely parser bugs without reading the snapshot.

// Test support code: a failed precondition here must fail the calling test,
// so unwrap()/expect()/panic!() are the correct behavior — this never runs
// over untrusted repo content outside a test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mct_core::{
    LanguageParser, LanguageRegistry, ParseError, ParsedFile, RelationKind, SourceFile, SymbolKind,
    SymbolRecord, SymbolRelation,
};
use mct_index::{ExcludeSet, Index};

/// Defines `fn corpus() -> &'static Corpus` for `$parser` (loaded once per
/// test binary) plus one `#[test]` per shared check. Invoke it at the top
/// level of a crate's `tests/corpus.rs`.
#[macro_export]
macro_rules! standard_tests {
    ($parser:expr) => {
        $crate::standard_tests!(@shared $parser);

        #[test]
        fn malformed_corpus_files_are_syntax_errors() {
            corpus().assert_malformed_rejected();
        }
    };
    // For a grammar that accepts any input (Markdown), the `malformed/`
    // files only have to parse without panicking. The reason is required so
    // the exception stays explained where it is taken.
    ($parser:expr, malformed_may_parse = $reason:literal) => {
        $crate::standard_tests!(@shared $parser);
    };
    (@shared $parser:expr) => {
        fn corpus() -> &'static $crate::Corpus {
            static CORPUS: ::std::sync::OnceLock<$crate::Corpus> = ::std::sync::OnceLock::new();
            CORPUS.get_or_init(|| {
                $crate::Corpus::load(::std::sync::Arc::new($parser), env!("CARGO_MANIFEST_DIR"))
            })
        }

        #[test]
        fn corpus_meets_the_size_target() {
            corpus().assert_size();
        }

        #[test]
        fn corpus_line_ranges_stay_inside_their_files() {
            corpus().assert_line_ranges();
        }

        #[test]
        fn corpus_symbols_and_relations_match_the_snapshot() {
            corpus().assert_snapshot();
        }

        #[test]
        fn corpus_round_trips_through_the_index() {
            corpus().assert_index_round_trip();
        }

        #[test]
        fn malformed_corpus_input_never_panics() {
            corpus().assert_malformed_never_panics();
        }

        /// Prints [`Corpus::report`] (and updates the progress table named
        /// by `MCT_CORPUS_PROGRESS`); run through `scripts/unix/corpus-report.sh`.
        #[test]
        #[ignore = "not a bug: a report, not a check; run via scripts/unix/corpus-report.sh"]
        fn corpus_report() {
            corpus().print_report();
        }
    };
}

/// Issue #74's size target: at least this many files per language…
pub const MIN_FILES: usize = 5;
/// …at least this many lines long. Other files may be shorter, so a
/// project keeps its natural shape (a small `urls.py` next to `models.py`).
pub const MIN_LINES: usize = 300;
/// No corpus file is longer than this. The issue calls 600 a soft ceiling;
/// this hard limit leaves room to go "a bit over" while still catching a dump.
pub const MAX_LINES: usize = 700;

/// Environment variable that rewrites `expected.snap` instead of comparing.
pub const BLESS_ENV: &str = "MCT_BLESS";

/// Environment variable naming a progress table (`internal/corpus-progress.md`)
/// whose row for this crate [`Corpus::print_report`] rewrites.
pub const PROGRESS_ENV: &str = "MCT_CORPUS_PROGRESS";

/// Hits listed per check in [`Corpus::report`]; the rest are counted.
const REPORT_CAP: usize = 12;

/// Lines above a symbol still counted as its own (decorators, attributes).
const ANNOTATION_LINES: u32 = 5;

/// One parsed corpus file.
pub struct CorpusFile {
    /// Path relative to `tests/corpus/project`, forward slashes — the same
    /// `relative_path` the index stores.
    pub path: String,
    pub contents: String,
    pub parsed: ParsedFile,
}

impl CorpusFile {
    pub fn line_count(&self) -> usize {
        self.contents.lines().count()
    }

    fn symbol_by_id(&self, id: u32) -> Option<&SymbolRecord> {
        self.parsed.symbols.iter().find(|s| s.id == id)
    }
}

/// A language crate's whole corpus, parsed once.
pub struct Corpus {
    parser: Arc<dyn LanguageParser>,
    dir: PathBuf,
    pub files: Vec<CorpusFile>,
}

/// A relation resolved to readable names, for language-specific assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rel<'a> {
    pub path: &'a str,
    pub from: &'a str,
    pub from_parent: Option<&'a str>,
    pub kind: RelationKind,
    pub to: &'a str,
    pub line: u32,
}

impl Corpus {
    /// Parses every file under `<crate_dir>/tests/corpus/project`, in path
    /// order. Every corpus file must be valid for the grammar: a
    /// `ParseError` here fails the test with the file and line.
    pub fn load(parser: Arc<dyn LanguageParser>, crate_dir: &str) -> Corpus {
        let dir = Path::new(crate_dir).join("tests/corpus");
        let project = dir.join("project");
        let mut paths = Vec::new();
        collect_files(&project, &mut paths);
        paths.sort();
        assert!(
            !paths.is_empty(),
            "no corpus files under {}",
            project.display()
        );
        let files = paths
            .into_iter()
            .map(|path| {
                let rel = relative(&project, &path);
                let contents = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
                let parsed = parser
                    .parse(&SourceFile {
                        relative_path: rel.clone(),
                        contents: contents.clone(),
                    })
                    .unwrap_or_else(|e| panic!("corpus file {rel} must parse cleanly: {e}"));
                CorpusFile {
                    path: rel,
                    contents,
                    parsed,
                }
            })
            .collect();
        Corpus { parser, dir, files }
    }

    fn project_dir(&self) -> PathBuf {
        self.dir.join("project")
    }

    /// At least [`MIN_FILES`] files of [`MIN_LINES`]..=[`MAX_LINES`] lines,
    /// and none longer than [`MAX_LINES`]. Every file under `project/` is
    /// parsed, so it must be one the parser claims.
    pub fn assert_size(&self) {
        let sizes: Vec<_> = self
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.line_count()))
            .collect();
        let problems = size_problems(&sizes);
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    /// Every symbol range and relation site lies inside its file:
    /// `1 <= start <= end <= line count`. Catches off-by-N ends such as a
    /// file-level module ending on the line after the last one.
    pub fn assert_line_ranges(&self) {
        for f in &self.files {
            let lines = f.line_count() as u32;
            for s in &f.parsed.symbols {
                let end = s.location.end_line.unwrap_or(s.location.line);
                assert!(
                    s.location.line >= 1 && s.location.line <= end && end <= lines,
                    "{}: {:?} {} spans {}-{end}, file has {lines} lines",
                    f.path,
                    s.kind,
                    s.name,
                    s.location.line
                );
            }
            for r in &f.parsed.relations {
                assert!(
                    (1..=lines).contains(&r.location.line),
                    "{}: relation to {} at line {} outside 1..={lines}",
                    f.path,
                    r.to_name,
                    r.location.line
                );
            }
        }
    }

    /// Renders every file's symbols and relations and compares them with
    /// `tests/corpus/expected.snap` (or rewrites it under `MCT_BLESS=1`).
    pub fn assert_snapshot(&self) {
        let actual = self.render();
        let snap = self.dir.join("expected.snap");
        if std::env::var_os(BLESS_ENV).is_some() {
            std::fs::write(&snap, &actual).unwrap();
            return;
        }
        let expected = std::fs::read_to_string(&snap)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if expected != actual {
            panic!(
                "corpus snapshot mismatch for {}\n{}\nIf the change is intended, rerun with {BLESS_ENV}=1 and review the diff of expected.snap.",
                snap.display(),
                line_diff(&expected, &actual)
            );
        }
    }

    /// The golden text: one `== path` section per file, then a `S` line per
    /// symbol (`start-end kind parent::name`) and an `R` line per relation
    /// (`line:col kind from -> to`), with `(name-matched: path)` appended
    /// when a symbol with the target's *name* is defined in a different
    /// corpus file — a name coincidence, not a resolved reference.
    pub fn render(&self) -> String {
        let defined_in = self.definition_files();
        let mut out = String::new();
        let (syms, rels, xfile) = self.totals(&defined_in);
        writeln!(
            out,
            "# language={} files={} symbols={syms} relations={rels} name_matched_across_files={xfile}",
            self.parser.language_id(),
            self.files.len()
        )
        .unwrap();
        for f in &self.files {
            writeln!(out, "\n== {} ({} lines)", f.path, f.line_count()).unwrap();
            for s in &f.parsed.symbols {
                let end = s
                    .location
                    .end_line
                    .map_or_else(|| "?".to_string(), |e| e.to_string());
                write!(
                    out,
                    "S {}-{} {} {}",
                    s.location.line,
                    end,
                    kind_str(s.kind),
                    qualified(s.parent.as_deref(), &s.name)
                )
                .unwrap();
                if let Some(level) = s.level {
                    write!(out, " L{level}").unwrap();
                }
                out.push('\n');
            }
            for r in &f.parsed.relations {
                let from = f.symbol_by_id(r.from).map_or_else(
                    || format!("<#{}>", r.from),
                    |s| qualified(s.parent.as_deref(), &s.name),
                );
                write!(
                    out,
                    "R {}:{} {} {} -> {}",
                    r.location.line,
                    r.location.column,
                    relation_str(r.kind),
                    from,
                    r.to_name
                )
                .unwrap();
                if let Some(other) = name_matched_file(&defined_in, &f.path, r) {
                    write!(out, " (name-matched: {other})").unwrap();
                }
                out.push('\n');
            }
        }
        out
    }

    fn totals(&self, defined_in: &HashMap<&str, Vec<&str>>) -> (usize, usize, usize) {
        let syms = self.files.iter().map(|f| f.parsed.symbols.len()).sum();
        let rels = self.files.iter().map(|f| f.parsed.relations.len()).sum();
        let xfile = self
            .files
            .iter()
            .flat_map(|f| {
                f.parsed
                    .relations
                    .iter()
                    .filter(|r| name_matched_file(defined_in, &f.path, r).is_some())
            })
            .count();
        (syms, rels, xfile)
    }

    fn definition_files(&self) -> HashMap<&str, Vec<&str>> {
        let mut map: HashMap<&str, Vec<&str>> = HashMap::new();
        for f in &self.files {
            for s in &f.parsed.symbols {
                let files = map.entry(s.name.as_str()).or_default();
                if !files.contains(&f.path.as_str()) {
                    files.push(f.path.as_str());
                }
            }
        }
        map
    }

    /// Every relation in the corpus, resolved to names.
    pub fn relations(&self) -> Vec<Rel<'_>> {
        self.files
            .iter()
            .flat_map(|f| {
                f.parsed.relations.iter().map(move |r| {
                    let from = f.symbol_by_id(r.from);
                    Rel {
                        path: &f.path,
                        from: from.map_or("", |s| s.name.as_str()),
                        from_parent: from.and_then(|s| s.parent.as_deref()),
                        kind: r.kind,
                        to: &r.to_name,
                        line: r.line(),
                    }
                })
            })
            .collect()
    }

    /// The single relation `from --kind--> to` in `path` (a path suffix is
    /// enough). Panics listing the candidates when there is none.
    pub fn relation(&self, path: &str, from: &str, kind: RelationKind, to: &str) -> Rel<'_> {
        let all = self.relations();
        if let Some(r) = all
            .iter()
            .find(|r| r.path.ends_with(path) && r.from == from && r.kind == kind && r.to == to)
        {
            return r.clone();
        }
        let near: Vec<_> = all
            .iter()
            .filter(|r| r.path.ends_with(path) && (r.from == from || r.to == to))
            .collect();
        panic!("no {kind:?} relation {from} -> {to} in {path}; related: {near:#?}");
    }

    /// Whether any relation `from --kind--> to` exists anywhere.
    pub fn has_relation(&self, from: &str, kind: RelationKind, to: &str) -> bool {
        self.relations()
            .iter()
            .any(|r| r.from == from && r.kind == kind && r.to == to)
    }

    /// The single symbol `name` of `kind` in `path` (a path suffix is
    /// enough). Panics listing same-named symbols when there is none.
    pub fn symbol(&self, path: &str, name: &str, kind: SymbolKind) -> &SymbolRecord {
        let matches: Vec<_> = self
            .files
            .iter()
            .filter(|f| f.path.ends_with(path))
            .flat_map(|f| f.parsed.symbols.iter())
            .filter(|s| s.name == name && s.kind == kind)
            .collect();
        match matches.as_slice() {
            [one] => one,
            [] => {
                let near: Vec<_> = self
                    .files
                    .iter()
                    .flat_map(|f| f.parsed.symbols.iter().map(move |s| (&f.path, s)))
                    .filter(|(_, s)| s.name == name)
                    .collect();
                panic!("no {kind:?} `{name}` in {path}; same name elsewhere: {near:#?}")
            }
            many => panic!("{} {kind:?}s named `{name}` in {path}", many.len()),
        }
    }

    /// Every symbol in the corpus named `name`, with its file.
    pub fn symbols_named(&self, name: &str) -> Vec<(&str, &SymbolRecord)> {
        self.files
            .iter()
            .flat_map(|f| f.parsed.symbols.iter().map(move |s| (f.path.as_str(), s)))
            .filter(|(_, s)| s.name == name)
            .collect()
    }

    /// Relations whose target *name* is also defined in another corpus
    /// file. A floor on how interconnected the project is, not a count of
    /// resolved cross-file references: assert those one by one with
    /// [`Corpus::relation`].
    pub fn name_matched_relation_count(&self) -> usize {
        self.totals(&self.definition_files()).2
    }

    /// A fresh in-memory index of `tests/corpus/project`, for
    /// language-specific query assertions.
    pub fn index(&self) -> Index {
        let mut registry = LanguageRegistry::new();
        registry.register(self.parser.clone());
        let mut index = Index::open_in_memory(&self.project_dir(), ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        index
    }

    /// The crate this corpus belongs to (`mct-lang-…`), from its directory.
    fn crate_name(&self) -> String {
        self.dir
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Prints [`Corpus::report`]; with `MCT_CORPUS_PROGRESS=<file>` also
    /// rewrites this crate's counts in that progress table.
    pub fn print_report(&self) {
        println!("{}", self.report());
        if let Some(path) = std::env::var_os(PROGRESS_ENV) {
            let path = PathBuf::from(path);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
            match self.update_progress(&text) {
                Some(updated) => {
                    std::fs::write(&path, updated).unwrap();
                    println!("updated {} row in {}", self.crate_name(), path.display());
                }
                None => println!(
                    "no `{}` row in {}; progress not updated",
                    self.crate_name(),
                    path.display()
                ),
            }
        }
    }

    /// A short digest of the corpus for reviewing a parser change without
    /// reading `expected.snap`: totals, per-kind counts, the progress-table
    /// cells, and heuristic checks listing what usually turns out to be a
    /// parser bug (each one still needs a look — not every hit is wrong).
    pub fn report(&self) -> String {
        let defined_in = self.definition_files();
        let (syms, rels, xfile) = self.totals(&defined_in);
        let lines: usize = self.files.iter().map(CorpusFile::line_count).sum();
        let mut out = String::new();
        writeln!(
            out,
            "corpus report: {} ({})",
            self.crate_name(),
            self.parser.language_id()
        )
        .unwrap();
        let per_file: Vec<_> = self
            .files
            .iter()
            .map(|f| format!("{} {}", f.path, f.line_count()))
            .collect();
        writeln!(
            out,
            "files: {} ({} lines): {}",
            self.files.len(),
            thousands(lines),
            per_file.join(", ")
        )
        .unwrap();
        writeln!(
            out,
            "symbols: {syms}  relations: {rels}  name-matched across files: {xfile} (target name defined in another file, not resolved)"
        )
        .unwrap();
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        let mut rel_kinds: BTreeMap<&str, usize> = BTreeMap::new();
        for f in &self.files {
            for s in &f.parsed.symbols {
                *kinds.entry(kind_str(s.kind)).or_default() += 1;
            }
            for r in &f.parsed.relations {
                *rel_kinds.entry(relation_str(r.kind)).or_default() += 1;
            }
        }
        let join = |m: &BTreeMap<&str, usize>| {
            m.iter()
                .map(|(k, n)| format!("{k} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        writeln!(out, "symbol kinds: {}", join(&kinds)).unwrap();
        writeln!(out, "relation kinds: {}", join(&rel_kinds)).unwrap();
        let (files_cell, counts_cell) = self.progress_cells();
        writeln!(out, "progress cells: | {files_cell} | {counts_cell} |").unwrap();

        out.push_str("\nchecks (heuristics: review each hit, not all are bugs)\n");
        for (name, hits) in self.checks(&defined_in) {
            if hits.is_empty() {
                writeln!(out, "  ok   {name}").unwrap();
                continue;
            }
            writeln!(out, "  {:<4} {name}", hits.len()).unwrap();
            for h in hits.iter().take(REPORT_CAP) {
                writeln!(out, "         {h}").unwrap();
            }
            if hits.len() > REPORT_CAP {
                writeln!(out, "         … {} more", hits.len() - REPORT_CAP).unwrap();
            }
        }
        out
    }

    /// `(files / lines, symbols / relations (name-matched across files))`, as in
    /// `internal/corpus-progress.md`.
    fn progress_cells(&self) -> (String, String) {
        let (syms, rels, xfile) = self.totals(&self.definition_files());
        let lines: usize = self.files.iter().map(CorpusFile::line_count).sum();
        (
            format!("{} / {}", self.files.len(), thousands(lines)),
            format!("{syms} / {rels} ({xfile})"),
        )
    }

    /// Rewrites this crate's files/lines and symbols/relations cells in a
    /// progress table (columns 5 and 6 of the row whose second column is
    /// the crate name) and recounts the `**Resumen: …**` line from the
    /// state column. `None` when the table has no row for this crate.
    pub fn update_progress(&self, table: &str) -> Option<String> {
        let (files_cell, counts_cell) = self.progress_cells();
        update_progress_table(table, &self.crate_name(), &files_cell, &counts_cell)
    }

    /// Each heuristic's name and its hits, one readable line per hit.
    fn checks(&self, defined_in: &HashMap<&str, Vec<&str>>) -> Vec<(&'static str, Vec<String>)> {
        let mut module_end = Vec::new();
        let mut orphan_members = Vec::new();
        let mut inside_bodies = Vec::new();
        let mut odd_names = Vec::new();
        let mut owner_mismatch = Vec::new();
        let mut odd_targets = Vec::new();
        let mut external: BTreeMap<&str, usize> = BTreeMap::new();
        let mut identities: BTreeMap<&str, Vec<(SymbolKind, Option<&str>)>> = BTreeMap::new();

        for f in &self.files {
            let lines = f.line_count() as u32;
            let bodies: Vec<&SymbolRecord> = f
                .parsed
                .symbols
                .iter()
                .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
                .collect();
            for s in &f.parsed.symbols {
                let end = s.location.end_line.unwrap_or(s.location.line);
                let at = format!("{}:{}", f.path, s.location.line);
                if s.kind == SymbolKind::Module
                    && s.parent.is_none()
                    && s.location.line == 1
                    && end != lines
                {
                    module_end.push(format!("{at} {} ends at {end}, file has {lines}", s.name));
                }
                if matches!(s.kind, SymbolKind::Method | SymbolKind::Field) && s.parent.is_none() {
                    orphan_members.push(format!("{at} {} {}", kind_str(s.kind), s.name));
                }
                // A member of a class declared in a body is reported through
                // its class, not once per member.
                let in_type = s.parent.as_deref().is_some_and(|p| {
                    f.parsed.symbols.iter().any(|t| {
                        t.name == p && !matches!(t.kind, SymbolKind::Function | SymbolKind::Method)
                    })
                });
                if let Some(body) = bodies.iter().filter(|_| !in_type).find(|b| {
                    let b_end = b.location.end_line.unwrap_or(b.location.line);
                    !std::ptr::eq(**b, s) && b.location.line < s.location.line && end <= b_end
                }) {
                    inside_bodies.push(format!(
                        "{at} {} {} inside {} {}",
                        kind_str(s.kind),
                        qualified(s.parent.as_deref(), &s.name),
                        kind_str(body.kind),
                        qualified(body.parent.as_deref(), &body.name)
                    ));
                }
                if odd_name(&s.name) {
                    odd_names.push(format!("{at} {} {:?}", kind_str(s.kind), s.name));
                }
                let identity = (s.kind, s.parent.as_deref());
                let seen = identities.entry(s.name.as_str()).or_default();
                if !seen.contains(&identity) {
                    seen.push(identity);
                }
            }
            for r in &f.parsed.relations {
                let at = format!("{}:{}", f.path, r.location.line);
                match f.symbol_by_id(r.from) {
                    None => owner_mismatch.push(format!("{at} from unknown symbol #{}", r.from)),
                    Some(from) if from.kind != SymbolKind::Module => {
                        let end = from.location.end_line.unwrap_or(from.location.line);
                        // Decorators, attributes and annotations sit on the
                        // lines just above the symbol they belong to.
                        let first = from.location.line.saturating_sub(ANNOTATION_LINES);
                        if !(first..=end).contains(&r.location.line) {
                            owner_mismatch.push(format!(
                                "{at} {} -> {} but {} spans {}-{end}",
                                relation_str(r.kind),
                                r.to_name,
                                from.name,
                                from.location.line
                            ));
                        }
                    }
                    Some(_) => {}
                }
                if odd_name(&r.to_name) {
                    odd_targets.push(format!("{at} {} -> {:?}", relation_str(r.kind), r.to_name));
                }
                if r.kind == RelationKind::Calls && !defined_in.contains_key(r.to_name.as_str()) {
                    *external.entry(r.to_name.as_str()).or_default() += 1;
                }
            }
        }

        let split_identities = identities
            .into_iter()
            // Same-named methods of different classes are normal; a name
            // that is both a method and a function, or both owned and
            // top-level, usually means one definition lost its scope.
            .filter(|(_, ids)| {
                ids.iter().any(|(k, _)| *k != ids[0].0)
                    || ids.iter().any(|(_, p)| p.is_some() != ids[0].1.is_some())
            })
            .map(|(name, ids)| {
                let ids: Vec<_> = ids
                    .iter()
                    .map(|(k, p)| format!("{} {}", kind_str(*k), qualified(*p, name)))
                    .collect();
                format!("{name}: {}", ids.join(" | "))
            })
            .collect();
        let mut external: Vec<_> = external.into_iter().collect();
        external.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let external = external
            .into_iter()
            .map(|(name, n)| format!("{n:>3}× {name}"))
            .collect();

        vec![
            ("file-level module ends on the last line", module_end),
            ("method/field without a parent", orphan_members),
            (
                "symbol inside a function/method body (local? nested?)",
                inside_bodies,
            ),
            ("empty or odd symbol name", odd_names),
            ("relation outside its `from` symbol's range", owner_mismatch),
            ("empty or odd relation target", odd_targets),
            (
                "name with mixed kinds or owned/top-level definitions",
                split_identities,
            ),
            (
                "calls to names not defined in the corpus (most frequent first)",
                external,
            ),
        ]
    }

    /// Reindexes `tests/corpus/project` through the real `mct-index`
    /// pipeline and checks that every parsed symbol comes back from
    /// `find_symbol`, every call from `find_callers` and `find_calls`, and
    /// every relation from `find_references`, at the same file and line.
    pub fn assert_index_round_trip(&self) {
        let mut registry = LanguageRegistry::new();
        registry.register(self.parser.clone());
        let mut index = Index::open_in_memory(&self.project_dir(), ExcludeSet::default()).unwrap();
        let report = index.reindex(&registry, false).unwrap();
        assert_eq!(report.files_parsed, self.files.len(), "files indexed");
        assert!(report.issues.is_empty(), "issues: {:?}", report.issues);

        let status = index.status().unwrap();
        let lang = status
            .languages
            .iter()
            .find(|l| l.language == self.parser.language_id())
            .expect("the corpus language shows up in status");
        assert_eq!(lang.file_count, self.files.len());
        let total_symbols: usize = self.files.iter().map(|f| f.parsed.symbols.len()).sum();
        assert_eq!(lang.symbol_count, total_symbols);
        assert!(status.syntax_errors.is_empty());

        let mut find_symbol = HashMap::new();
        for f in &self.files {
            for s in &f.parsed.symbols {
                let hits = find_symbol
                    .entry(s.name.clone())
                    .or_insert_with(|| index.find_symbol(&s.name).unwrap());
                assert!(
                    hits.iter().any(|h| h.relative_path == f.path
                        && h.line == s.location.line
                        && h.end_line == s.location.end_line
                        && h.kind == kind_str(s.kind)
                        && h.parent == s.parent),
                    "find_symbol({}) misses {}:{} {:?}; got {hits:#?}",
                    s.name,
                    f.path,
                    s.location.line,
                    s.kind
                );
            }
        }

        let mut callers = HashMap::new();
        let mut calls = HashMap::new();
        let mut refs = HashMap::new();
        for f in &self.files {
            for r in &f.parsed.relations {
                let Some(from) = f.symbol_by_id(r.from) else {
                    continue;
                };
                let line = r.line();
                let same_site = |h: &mct_index::RelationHit| {
                    h.relative_path == f.path && h.line == line && h.from_symbol == from.name
                };
                let hits = refs
                    .entry(r.to_name.clone())
                    .or_insert_with(|| index.find_references(&r.to_name).unwrap());
                assert!(
                    hits.iter()
                        .any(|h| same_site(h) && h.kind == relation_str(r.kind)),
                    "find_references({}) misses {:?} from {} at {}:{line}",
                    r.to_name,
                    r.kind,
                    from.name,
                    f.path
                );
                if r.kind != RelationKind::Calls {
                    continue;
                }
                let hits = callers
                    .entry(r.to_name.clone())
                    .or_insert_with(|| index.find_callers(&r.to_name).unwrap());
                assert!(
                    hits.iter().any(same_site),
                    "find_callers({}) misses {} at {}:{line}",
                    r.to_name,
                    from.name,
                    f.path
                );
                let hits = calls
                    .entry(from.name.clone())
                    .or_insert_with(|| index.find_calls(&from.name).unwrap());
                assert!(
                    hits.iter().any(|h| same_site(h) && h.to_name == r.to_name),
                    "find_calls({}) misses {} at {}:{line}",
                    from.name,
                    r.to_name,
                    f.path
                );
            }
        }
    }

    /// The checked-in `tests/corpus/malformed/` files (at least one, each at
    /// least [`MIN_LINES`] long), as `(relative path, contents)`.
    fn malformed_files(&self) -> Vec<(String, String)> {
        let dir = self.dir.join("malformed");
        let mut paths = Vec::new();
        collect_files(&dir, &mut paths);
        paths.sort();
        assert!(!paths.is_empty(), "no files under {}", dir.display());
        paths
            .into_iter()
            .map(|p| {
                let contents = std::fs::read_to_string(&p).unwrap();
                let rel = relative(&dir, &p);
                assert!(
                    contents.lines().count() >= MIN_LINES,
                    "malformed/{rel} should be a large file"
                );
                (rel, contents)
            })
            .collect()
    }

    /// Every checked-in `malformed/` file ends in `ParseError::Syntax`: a
    /// fixture that parses cleanly proves nothing about error handling.
    /// [`standard_tests!`]'s `malformed_may_parse = "…"` form skips this
    /// for a grammar that accepts any input.
    pub fn assert_malformed_rejected(&self) {
        let problems: Vec<_> = self
            .malformed_files()
            .into_iter()
            .filter_map(|(rel, contents)| {
                let result = self.parser.parse(&SourceFile {
                    relative_path: rel.clone(),
                    contents,
                });
                malformed_verdict(&rel, &result)
            })
            .collect();
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    /// Parses the checked-in `malformed/` files plus mechanically corrupted
    /// variants of every corpus file. None may panic; each must end in a
    /// `ParseError::Syntax` or a partial result whose line ranges stay
    /// inside the input.
    pub fn assert_malformed_never_panics(&self) {
        let mut inputs = self.malformed_files();
        for f in &self.files {
            for (label, text) in corruptions(&f.contents) {
                inputs.push((format!("{} [{label}]", f.path), text));
            }
        }
        for (label, contents) in inputs {
            // Keep the real path (and so the extension) for the parser.
            let path = label.split(" [").next().unwrap_or(&label).to_string();
            let lines = contents.lines().count().max(1) as u32;
            let parser = &self.parser;
            let result = catch_unwind(AssertUnwindSafe(|| {
                parser.parse(&SourceFile {
                    relative_path: path.clone(),
                    contents: contents.clone(),
                })
            }));
            match result {
                Err(_) => panic!("parser panicked on malformed input {label}"),
                Ok(Err(ParseError::Syntax { .. })) => {}
                Ok(Err(e)) => panic!("{label}: expected a syntax error, got {e}"),
                Ok(Ok(parsed)) => {
                    for s in &parsed.symbols {
                        let end = s.location.end_line.unwrap_or(s.location.line);
                        assert!(
                            s.location.line >= 1 && s.location.line <= end && end <= lines,
                            "{label}: {} has range {}-{end} outside 1..={lines}",
                            s.name,
                            s.location.line
                        );
                    }
                }
            }
        }
    }
}

trait RelationLine {
    fn line(&self) -> u32;
}

impl RelationLine for SymbolRelation {
    fn line(&self) -> u32 {
        self.location.line
    }
}

/// Mechanically broken variants of a valid file.
fn corruptions(src: &str) -> Vec<(&'static str, String)> {
    let lines: Vec<&str> = src.lines().collect();
    let mut half = src.len() / 2;
    while !src.is_char_boundary(half) {
        half -= 1;
    }
    let drop_every_fifth = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 5 != 4)
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join("\n");
    let garbage = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if i % 40 == 20 {
                format!("{l}\n)]}}>{{[(<\"'@@@ ;; \\ `")
            } else {
                (*l).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let no_closers: String = src
        .chars()
        .filter(|c| !matches!(c, '}' | ')' | ']' | '>'))
        .collect();
    let deep = format!("{}{src}", "({[<".repeat(2_000));
    vec![
        ("truncated", src[..half].to_string()),
        ("every 5th line dropped", drop_every_fifth),
        ("garbage lines", garbage),
        ("closers removed", no_closers),
        ("deep nesting prefix", deep),
    ]
}

/// The other corpus file defining a symbol named like `r`'s target, if any.
/// By name only, as the snapshot and report say: a call to a standard
/// library `get` matches any `get` the corpus defines.
fn name_matched_file<'a>(
    defined_in: &HashMap<&str, Vec<&'a str>>,
    path: &str,
    r: &SymbolRelation,
) -> Option<&'a str> {
    let files = defined_in.get(r.to_name.as_str())?;
    if files.contains(&path) {
        return None;
    }
    files.first().copied()
}

/// See [`Corpus::update_progress`].
fn update_progress_table(
    table: &str,
    crate_name: &str,
    files_cell: &str,
    counts_cell: &str,
) -> Option<String> {
    let crate_cell = format!("`{crate_name}`");
    let mut found = false;
    let mut rows: Vec<String> = table
        .lines()
        .map(|line| {
            let mut cells: Vec<String> = line.split('|').map(str::to_string).collect();
            // `| a | b | … |` splits into an empty first and last cell.
            if cells.len() >= 8 && cells[2].trim() == crate_cell {
                found = true;
                cells[5] = format!(" {files_cell} ");
                cells[6] = format!(" {counts_cell} ");
                return cells.join("|");
            }
            line.to_string()
        })
        .collect();
    if !found {
        return None;
    }
    let (mut done, mut in_pr, mut pending, mut total) = (0, 0, 0, 0);
    for line in &rows {
        let cells: Vec<&str> = line.split('|').collect();
        if cells.len() < 8 || !cells[2].trim().starts_with("`mct-lang-") {
            continue;
        }
        total += 1;
        match cells[3].trim().trim_matches('*') {
            "Done" => done += 1,
            "In PR" => in_pr += 1,
            _ => pending += 1,
        }
    }
    for line in &mut rows {
        if line.starts_with("**Summary:") {
            *line = format!(
                "**Summary: {done} done, {in_pr} in PR, {pending} pending ({total} total).**"
            );
        }
    }
    let mut out = rows.join("\n");
    if table.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

/// What breaks the size target in `(path, lines)`, one line per problem.
fn size_problems(files: &[(&str, usize)]) -> Vec<String> {
    let mut problems: Vec<String> = files
        .iter()
        .filter(|(_, n)| *n > MAX_LINES)
        .map(|(path, n)| format!("{path} has {n} lines, more than {MAX_LINES}"))
        .collect();
    let full = files
        .iter()
        .filter(|(_, n)| (MIN_LINES..=MAX_LINES).contains(n))
        .count();
    if full < MIN_FILES {
        problems.push(format!(
            "{full} file(s) have {MIN_LINES}..={MAX_LINES} lines, need at least {MIN_FILES}"
        ));
    }
    problems
}

/// Why a checked-in `malformed/` file's parse result does not prove the
/// parser rejects it, or `None` when it ended in a syntax error.
fn malformed_verdict(rel: &str, result: &Result<ParsedFile, ParseError>) -> Option<String> {
    match result {
        Err(ParseError::Syntax { .. }) => None,
        Err(e) => Some(format!("malformed/{rel}: expected a syntax error, got {e}")),
        Ok(_) => Some(format!(
            "malformed/{rel} parsed without a syntax error: break it for this grammar \
             (scripts/unix/parse-probe.sh shows the first error), or use \
             standard_tests!(…, malformed_may_parse = \"why\") if the grammar accepts any input"
        )),
    }
}

/// `1568` → `1,568`, the thousands style of the progress table.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A name no parser should produce: empty, multi-line, padded, or
/// carrying punctuation that means a whole expression was taken as a name.
fn odd_name(name: &str) -> bool {
    name.is_empty()
        || name.len() > 60
        || name.trim() != name
        || name.contains(['\n', ';', '{', '}'])
        || name.contains("  ")
}

fn qualified(parent: Option<&str>, name: &str) -> String {
    match parent {
        Some(p) => format!("{p}::{name}"),
        None => name.to_string(),
    }
}

/// Same strings `mct-index` stores in `symbols.kind`.
pub fn kind_str(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Class => "class",
        SymbolKind::Struct => "struct",
        SymbolKind::Interface => "interface",
        SymbolKind::Enum => "enum",
        SymbolKind::Trait => "trait",
        SymbolKind::TypeAlias => "type_alias",
        SymbolKind::Module => "module",
        SymbolKind::Variable => "variable",
        SymbolKind::Constant => "constant",
        SymbolKind::Field => "field",
        SymbolKind::Element => "element",
        SymbolKind::Rule => "rule",
        SymbolKind::Asset => "asset",
        SymbolKind::ModelNode => "model_node",
        SymbolKind::Material => "material",
        SymbolKind::Animation => "animation",
        SymbolKind::Finding => "finding",
    }
}

/// Same strings `mct-index` stores in `relations.kind`.
pub fn relation_str(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::Calls => "calls",
        RelationKind::Imports => "imports",
        RelationKind::Extends => "extends",
        RelationKind::Implements => "implements",
        RelationKind::References => "references",
    }
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap()
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Lines missing from / added to the golden file, grouped by section,
/// capped so a wholesale change doesn't flood the test output.
fn line_diff(expected: &str, actual: &str) -> String {
    fn counts(text: &str) -> BTreeMap<&str, isize> {
        let mut m = BTreeMap::new();
        for l in text.lines() {
            *m.entry(l).or_insert(0) += 1;
        }
        m
    }
    let e = counts(expected);
    let a = counts(actual);
    let mut out = String::new();
    let mut shown = 0;
    for (line, n) in &e {
        let d = n - a.get(line).copied().unwrap_or(0);
        for _ in 0..d.max(0) {
            if shown < 60 {
                writeln!(out, "- {line}").unwrap();
            }
            shown += 1;
        }
    }
    for (line, n) in &a {
        let d = n - e.get(line).copied().unwrap_or(0);
        for _ in 0..d.max(0) {
            if shown < 60 {
                writeln!(out, "+ {line}").unwrap();
            }
            shown += 1;
        }
    }
    if shown > 60 {
        writeln!(out, "… {} more differing lines", shown - 60).unwrap();
    }
    if shown == 0 {
        out.push_str("(same lines, different order)\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_uses_commas() {
        assert_eq!(thousands(7), "7");
        assert_eq!(thousands(1568), "1,568");
        assert_eq!(thousands(1234567), "1,234,567");
    }

    #[test]
    fn five_full_files_meet_the_size_target_with_smaller_ones_beside_them() {
        let five = [("a", 300), ("b", 450), ("c", 700), ("d", 320), ("e", 301)];
        assert!(size_problems(&five).is_empty());
        let mut with_small = five.to_vec();
        with_small.push(("urls.py", 40));
        assert!(size_problems(&with_small).is_empty());
    }

    #[test]
    fn too_few_full_files_or_an_oversized_one_breaks_the_size_target() {
        let four = [("a", 300), ("b", 450), ("c", 700), ("d", 320), ("e", 299)];
        assert_eq!(
            size_problems(&four),
            ["4 file(s) have 300..=700 lines, need at least 5"]
        );
        let dump = [("a", 300), ("b", 450), ("c", 701), ("d", 320), ("e", 301)];
        assert_eq!(size_problems(&dump)[0], "c has 701 lines, more than 700");
    }

    #[test]
    fn a_malformed_file_must_end_in_a_syntax_error() {
        let syntax = Err(ParseError::Syntax {
            path: "x".into(),
            line: 3,
            message: "m".into(),
        });
        assert_eq!(malformed_verdict("x", &syntax), None);
        let parsed = malformed_verdict("broken.go", &Ok(ParsedFile::default())).unwrap();
        assert!(parsed.starts_with("malformed/broken.go parsed without a syntax error"));
        let unsupported = Err(ParseError::UnsupportedExtension {
            extension: "zz".into(),
        });
        assert!(malformed_verdict("x.zz", &unsupported).is_some());
    }

    #[test]
    fn odd_names_are_flagged() {
        assert!(odd_name(""));
        assert!(odd_name("a;b"));
        assert!(odd_name(" padded"));
        assert!(!odd_name("operator()"));
        assert!(!odd_name("inv::service"));
    }

    const TABLE: &str = "\
| Language | Crate | Status | PR | Files / lines | Symbols / relations (name-matched across files) | Bugs |
|---|---|---|---|---|---|---|
| Rust | `mct-lang-rust` | **Done** | #77 | 6 / 2,179 | see | x |
| Go | `mct-lang-go` | **In PR** | #90 | | | |
| Lua | `mct-lang-lua` | Pending | | | | |

**Summary: 0 done, 0 in PR, 0 pending (0 total).**
";

    #[test]
    fn progress_row_and_summary_are_rewritten() {
        let out = update_progress_table(TABLE, "mct-lang-go", "5 / 1,600", "10 / 20 (3)").unwrap();
        assert!(
            out.contains("| Go | `mct-lang-go` | **In PR** | #90 | 5 / 1,600 | 10 / 20 (3) | |")
        );
        assert!(out.contains("| Rust | `mct-lang-rust` | **Done** | #77 | 6 / 2,179 | see | x |"));
        assert!(out.contains("**Summary: 1 done, 1 in PR, 1 pending (3 total).**"));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn missing_row_leaves_the_table_alone() {
        assert!(update_progress_table(TABLE, "mct-lang-xml", "a", "b").is_none());
    }
}
