//! A full reindex parses on several threads and writes on one (issue
//! #635): the rows are those of a sequential run, id for id, a broken file
//! is reported in walk order, and a panicking parse rolls the run back.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[path = "common/scratch_dir.rs"]
mod scratch_dir;
use scratch_dir::scratch_dir;

use mct_core::{
    LanguageParser, LanguageRegistry, Location, ParseError, ParsedFile, RelationKind, SourceFile,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use mct_index::{ExcludeSet, Index};

/// `.fake` files: a module, a function `f<n>` per `def <n>` line, a call
/// per `call <name>` line. `broken` fails to parse, `boom` panics. Each
/// parse sleeps a little, longer for earlier files, so threads finish out
/// of order.
struct SlowParser {
    threads: Mutex<HashSet<std::thread::ThreadId>>,
}

impl LanguageParser for SlowParser {
    fn language_id(&self) -> &'static str {
        "fake"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["fake"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        self.threads
            .lock()
            .unwrap()
            .insert(std::thread::current().id());
        let order: u64 = file
            .relative_path
            .trim_start_matches("src/m")
            .trim_end_matches(".fake")
            .parse()
            .unwrap_or(0);
        std::thread::sleep(std::time::Duration::from_micros(
            400u64.saturating_sub(order),
        ));
        if file.contents.contains("boom") {
            panic!("parser bug");
        }
        if file.contents.contains("broken") {
            return Err(ParseError::Syntax {
                path: file.relative_path.clone(),
                line: 1,
                message: "broken".into(),
            });
        }
        let at = |line: usize| Location {
            line: line as u32 + 1,
            column: 1,
            byte_len: 1,
            end_line: None,
        };
        let mut parsed = ParsedFile::default();
        parsed.symbols.push(SymbolRecord {
            id: 0,
            name: file.relative_path.clone(),
            kind: SymbolKind::Module,
            location: at(0),
            parent: None,
            level: None,
        });
        for (number, line) in file.contents.lines().enumerate() {
            if let Some(n) = line.strip_prefix("def ") {
                parsed.symbols.push(SymbolRecord {
                    id: parsed.symbols.len() as u32,
                    name: format!("f{n}"),
                    kind: SymbolKind::Function,
                    location: at(number),
                    parent: None,
                    level: None,
                });
            } else if let Some(name) = line.strip_prefix("call ") {
                parsed.relations.push(SymbolRelation {
                    from: 0,
                    kind: RelationKind::Calls,
                    to_name: name.into(),
                    location: at(number),
                });
            }
        }
        Ok(parsed)
    }
}

const FILES: usize = 150;

fn project(extra: &[(&str, &str)]) -> std::path::PathBuf {
    let root = scratch_dir("mct-parallel-parse");
    std::fs::create_dir_all(root.join("src")).unwrap();
    for n in 0..FILES {
        let next = (n + 1) % FILES;
        std::fs::write(
            root.join(format!("src/m{n}.fake")),
            format!("def {n}\ncall f{next}\ncall f{n}\n"),
        )
        .unwrap();
    }
    for (path, contents) in extra {
        std::fs::write(root.join(path), contents).unwrap();
    }
    root
}

fn index_with(root: &std::path::Path, workers: usize) -> (Index, usize, Vec<String>) {
    let parser = Arc::new(SlowParser {
        threads: Mutex::new(HashSet::new()),
    });
    let mut registry = LanguageRegistry::new();
    registry.register(parser.clone());
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.set_parse_workers(workers);
    let report = index.reindex(&registry, false).unwrap();
    let issues = report
        .issues
        .iter()
        .map(|issue| issue.relative_path.clone())
        .collect();
    let threads = parser.threads.lock().unwrap().len();
    (index, threads, issues)
}

/// Every function and call row, with its id, in id order.
fn rows(index: &Index) -> Vec<String> {
    let mut rows = Vec::new();
    for n in 0..FILES {
        for hit in index.find_symbol(&format!("f{n}")).unwrap() {
            rows.push(format!(
                "symbol {} {} {}:{}",
                hit.id, hit.name, hit.relative_path, hit.line
            ));
        }
        for hit in index.find_callers(&format!("f{n}")).unwrap() {
            rows.push(format!(
                "call {} {} -> {} {}",
                hit.relation_id,
                hit.from_symbol,
                hit.to_name,
                hit.resolution.as_str()
            ));
        }
    }
    rows.sort();
    rows
}

#[test]
fn a_parallel_run_writes_the_rows_of_a_sequential_one() {
    let root = project(&[
        ("src/m40.broken.fake", "broken"),
        ("src/m90.broken.fake", "broken"),
    ]);
    let (sequential, one_thread, issues_one) = index_with(&root, 1);
    let (parallel, threads, issues_many) = index_with(&root, 6);
    assert_eq!(one_thread, 1);
    assert!(threads > 1, "parsed on {threads} thread(s)");
    let expected = rows(&sequential);
    assert_eq!(expected.len(), FILES * 3, "{expected:?}");
    assert_eq!(rows(&parallel), expected);
    // Issues come in walk order either way.
    assert_eq!(issues_many, issues_one);
    assert_eq!(issues_one.len(), 2, "{issues_one:?}");
}

#[test]
fn a_panicking_parse_rolls_the_whole_run_back() {
    let root = project(&[("src/m70.boom.fake", "boom")]);
    let parser = Arc::new(SlowParser {
        threads: Mutex::new(HashSet::new()),
    });
    let mut registry = LanguageRegistry::new();
    registry.register(parser);
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.set_parse_workers(4);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        index.reindex(&registry, false)
    }));
    assert!(outcome.is_err(), "the parser's panic reaches the caller");
    assert!(
        index.find_symbol("f1").unwrap().is_empty(),
        "nothing was committed"
    );
}
