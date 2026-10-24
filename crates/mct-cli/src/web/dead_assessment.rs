//! Where a dead-code candidate lives and how likely it is to be really unused.
//!
//! The index only knows that nothing references the symbol by name. This adds
//! the cheap evidence that changes what that means: tests, benchmarks, examples
//! and the website are reached by tooling rather than by name, a public item
//! may be called from outside the repository, and some names are entry points
//! or trait methods. It ranks candidates; it never proves them dead.

use std::collections::HashMap;
use std::path::Path;

/// Zone, visibility and confidence of one candidate, as served by the API.
#[derive(Clone, Copy)]
pub(super) struct Assessment {
    pub zone: &'static str,
    pub visibility: &'static str,
    pub confidence: &'static str,
}

const TEST_DIRS: [&str; 8] = [
    "tests",
    "test",
    "__tests__",
    "spec",
    "fixtures",
    "corpus",
    "testdata",
    "snapshots",
];

/// Names that tooling, the language or a trait calls without a visible reference.
const ENTRY_NAMES: [&str; 25] = [
    "main",
    "new",
    "default",
    "fmt",
    "drop",
    "clone",
    "eq",
    "hash",
    "cmp",
    "partial_cmp",
    "from",
    "try_from",
    "into",
    "deref",
    "deref_mut",
    "next",
    "as_ref",
    "as_mut",
    "run",
    "setup",
    "teardown",
    "constructor",
    "render",
    "handler",
    "index",
];

pub(super) fn zone(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    let mut parts: Vec<&str> = lower.split('/').collect();
    let file = parts.pop().unwrap_or("");
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    if parts.iter().any(|part| TEST_DIRS.contains(part))
        || stem.ends_with("_test")
        || stem.ends_with(".test")
        || stem.ends_with(".spec")
        || stem.starts_with("test_")
    {
        "tests"
    } else if parts
        .iter()
        .any(|part| matches!(*part, "benchmarks" | "benchmark" | "bench" | "benches"))
    {
        "benchmarks"
    } else if parts
        .iter()
        .any(|part| matches!(*part, "examples" | "example"))
    {
        "examples"
    } else if parts.contains(&"website") {
        "website"
    } else {
        "source"
    }
}

/// First line of the definition itself, skipping attributes, decorators and comments.
fn definition_line(lines: &[&str], start: u32) -> Option<String> {
    let first = usize::try_from(start).ok()?.checked_sub(1)?;
    lines
        .iter()
        .skip(first)
        .take(8)
        .map(|line| line.trim())
        .find(|line| {
            !line.is_empty()
                && !["#[", "#!", "///", "//", "@", "/*", "*"]
                    .iter()
                    .any(|prefix| line.starts_with(prefix))
        })
        .map(str::to_string)
}

fn visibility(language: &str, name: &str, line: Option<&str>) -> &'static str {
    match language {
        "rust" => match line {
            Some(line) if line.starts_with("pub(") => "internal",
            Some(line) if line.starts_with("pub ") => "public",
            Some(_) => "private",
            None => "unknown",
        },
        "python" => {
            if name.starts_with('_') {
                "private"
            } else {
                "public"
            }
        }
        "javascript_typescript" => match line {
            Some(line) if line.starts_with("export ") => "public",
            Some(line) if line.starts_with("function ") || line.starts_with("const ") => "private",
            _ => "unknown",
        },
        _ => "unknown",
    }
}

fn entry_like(name: &str) -> bool {
    ENTRY_NAMES.contains(&name)
        || name.starts_with("test_")
        || name.starts_with("test")
            && name
                .chars()
                .nth(4)
                .is_some_and(|c| c.is_uppercase() || c == '_')
        || (name.starts_with("__") && name.ends_with("__"))
}

/// Whole-word occurrences of `name` in `text`.
fn mentions(text: &str, name: &str) -> usize {
    if name.is_empty() {
        return 0;
    }
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    text.match_indices(name)
        .filter(|(start, _)| {
            let before = text[..*start].chars().next_back();
            let after = text[start + name.len()..].chars().next();
            !before.is_some_and(word) && !after.is_some_and(word)
        })
        .count()
}

/// Reads each candidate file once and assesses every candidate.
pub(super) struct Assessor<'a> {
    root: &'a Path,
    files: HashMap<String, Option<String>>,
}

impl<'a> Assessor<'a> {
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            files: HashMap::new(),
        }
    }

    pub fn assess(
        &mut self,
        path: &str,
        language: &str,
        name: &str,
        kind: &str,
        line: u32,
    ) -> Assessment {
        let root = self.root;
        let text = self
            .files
            .entry(path.to_string())
            .or_insert_with(|| {
                mct_index::read_repository_file(&root.join(path))
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
            })
            .as_deref();
        let lines: Vec<&str> = text.map(|text| text.lines().collect()).unwrap_or_default();
        let definition = definition_line(&lines, line);
        let visibility = visibility(language, name, definition.as_deref());
        let zone = zone(path);
        // A function named again elsewhere in its own file is most likely passed
        // around as a value (`run: handler`), which the index does not count as a reference.
        let used_as_value = matches!(kind, "function" | "method")
            && text.is_some_and(|text| mentions(text, name) > 1);
        let confidence = if zone != "source" || entry_like(name) || used_as_value {
            "low"
        } else if matches!(visibility, "public" | "unknown") || kind != "function" {
            "medium"
        } else {
            "high"
        };
        Assessment {
            zone,
            visibility,
            confidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_follow_the_path() {
        for (path, expected) in [
            ("crates/mct-index/src/lib.rs", "source"),
            (
                "crates/mct-lang-js-ts/tests/corpus/project/src/order.ts",
                "tests",
            ),
            ("crates/x/src/foo_test.rs", "tests"),
            ("benchmarks/token-twins-labels/python/a.py", "benchmarks"),
            ("crates/mct-mcp-server/examples/demo.rs", "examples"),
            ("website/components/landing/hero.tsx", "website"),
        ] {
            assert_eq!(zone(path), expected, "{path}");
        }
    }

    #[test]
    fn confidence_ranks_private_source_functions_first() {
        let root = std::env::temp_dir();
        let mut assessor = Assessor::new(&root);
        assessor.files.insert(
            "src/a.rs".into(),
            Some("fn helper() {}\npub fn api() {}\n".into()),
        );
        assessor.files.insert(
            "src/b.rs".into(),
            Some("#[inline]\n/// doc\npub(crate) fn inner() {}\n".into()),
        );
        assessor.files.insert(
            "src/c.js".into(),
            Some("const cycle = () => {};\nconst cmds = [{ run: cycle }];\n".into()),
        );
        let value = assessor.assess("src/c.js", "javascript_typescript", "cycle", "function", 1);
        assert_eq!(
            value.confidence, "low",
            "a function used as a value is not high"
        );
        let high = assessor.assess("src/a.rs", "rust", "helper", "function", 1);
        assert_eq!((high.visibility, high.confidence), ("private", "high"));
        let public = assessor.assess("src/a.rs", "rust", "api", "function", 2);
        assert_eq!((public.visibility, public.confidence), ("public", "medium"));
        let internal = assessor.assess("src/b.rs", "rust", "inner", "function", 1);
        assert_eq!(
            (internal.visibility, internal.confidence),
            ("internal", "high")
        );
        assert_eq!(
            assessor
                .assess("src/a.rs", "rust", "main", "function", 1)
                .confidence,
            "low"
        );
        assert_eq!(
            assessor
                .assess("src/a.rs", "rust", "Thing", "struct", 1)
                .confidence,
            "medium"
        );
        assert_eq!(
            assessor
                .assess("benchmarks/x.py", "python", "score", "function", 1)
                .confidence,
            "low"
        );
    }
}
