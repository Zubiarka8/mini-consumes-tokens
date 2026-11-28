//! What a Git diff touches (issue #632): the changed line ranges of each
//! file, mapped to the definitions they fall in, and for each definition
//! its callers, the tests that reach it (with why each is a test) and the
//! relations that may reach it but are not proven to.
//!
//! The diff runs from the merge base of `base` and the head (a commit, or
//! the working tree) to that head, so `--base main` reports this branch's
//! changes only. Added lines are mapped through the current index; removed
//! lines are mapped through the base version of the file, parsed on the
//! spot, so a deleted definition is still named. A file whose indexed bytes
//! are not the head's is reported stale: its line ranges may not match.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use git2::{DiffOptions, Oid, Repository};
use mct_core::{LanguageRegistry, SourceFile};

use crate::queries::{RelationDirection, RelationHit, Resolution, SymbolHit};
use crate::{Index, QueryScope, Result, TestEvidence};

/// One changed file of a [`GitDiff`].
#[derive(Debug, Default)]
pub struct ChangedFile {
    /// Project-relative path on the head side (the old path when deleted).
    pub path: String,
    pub deleted: bool,
    /// Head-side line ranges that were added or rewritten, inclusive.
    pub added: Vec<(u32, u32)>,
    /// Base-side line ranges that were removed or rewritten, inclusive.
    pub removed: Vec<(u32, u32)>,
    /// The head-side line each pure removal follows (git's `new_start`).
    pub removed_at: Vec<u32>,
    /// The base version of the file, when it had one.
    pub base_contents: Option<String>,
    /// The head version's bytes, to tell a stale index row.
    pub head_bytes: Option<Vec<u8>>,
}

/// The files that changed between the merge base and the head.
#[derive(Debug, Default)]
pub struct GitDiff {
    /// The merge base, abbreviated.
    pub base: String,
    /// The head commit, abbreviated; `None` for the working tree.
    pub head: Option<String>,
    pub files: Vec<ChangedFile>,
}

/// Most changed files one diff reports.
const MAX_DIFF_FILES: usize = 2_000;

/// The diff from the merge base of `base` and `head` (the working tree when
/// `None`) to `head`, limited to files under `project_root`.
pub fn git_diff(project_root: &Path, base: &str, head: Option<&str>) -> Result<GitDiff> {
    let canonical = |path: &Path| {
        path.canonicalize().map_err(|source| crate::IndexError::Io {
            path: path.display().to_string(),
            source,
        })
    };
    let root = canonical(project_root)?;
    let repo = Repository::discover(&root)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| git2::Error::from_str("the Git repository has no working tree"))?;
    let workdir = canonical(workdir)?;
    let scope = root
        .strip_prefix(&workdir)
        .unwrap_or(Path::new(""))
        .to_path_buf();
    let base_commit = repo.revparse_single(base)?.peel_to_commit()?;
    let head_commit = match head {
        Some(head) => repo.revparse_single(head)?.peel_to_commit()?,
        None => repo.head()?.peel_to_commit()?,
    };
    let merge_base = repo.find_commit(repo.merge_base(base_commit.id(), head_commit.id())?)?;
    let base_tree = merge_base.tree()?;
    let mut options = DiffOptions::new();
    options.include_typechange(true).context_lines(0);
    if !scope.as_os_str().is_empty() {
        options.pathspec(scope.to_string_lossy().as_ref());
    }
    let diff = match head {
        Some(_) => repo.diff_tree_to_tree(
            Some(&base_tree),
            Some(&head_commit.tree()?),
            Some(&mut options),
        )?,
        None => {
            options.include_untracked(true).recurse_untracked_dirs(true);
            repo.diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut options))?
        }
    };
    let relative = |path: &Path| -> Option<String> {
        let path = path.strip_prefix(&scope).ok()?;
        (!path.as_os_str().is_empty()).then(|| path.to_string_lossy().replace('\\', "/"))
    };
    let mut files: BTreeMap<String, ChangedFile> = BTreeMap::new();
    for (i, delta) in diff.deltas().enumerate().take(MAX_DIFF_FILES) {
        let deleted = delta.status() == git2::Delta::Deleted;
        let side = if deleted {
            delta.old_file()
        } else {
            delta.new_file()
        };
        let Some(path) = side.path().and_then(relative) else {
            continue;
        };
        let blob_text = |id: Oid| -> Option<String> {
            (!id.is_zero())
                .then(|| repo.find_blob(id).ok())
                .flatten()
                .and_then(|b| String::from_utf8(b.content().to_vec()).ok())
        };
        let base_contents = blob_text(delta.old_file().id());
        let head_bytes = if deleted {
            None
        } else if head.is_some() {
            repo.find_blob(delta.new_file().id())
                .ok()
                .map(|b| b.content().to_vec())
        } else {
            std::fs::read(workdir.join(delta.new_file().path().unwrap_or(Path::new("")))).ok()
        };
        let mut file = ChangedFile {
            path: path.clone(),
            deleted,
            base_contents,
            head_bytes,
            ..ChangedFile::default()
        };
        if let Ok(Some(patch)) = git2::Patch::from_diff(&diff, i) {
            for h in 0..patch.num_hunks() {
                let Ok((hunk, _)) = patch.hunk(h) else {
                    continue;
                };
                if hunk.new_lines() > 0 {
                    file.added
                        .push((hunk.new_start(), hunk.new_start() + hunk.new_lines() - 1));
                }
                if hunk.old_lines() > 0 {
                    file.removed
                        .push((hunk.old_start(), hunk.old_start() + hunk.old_lines() - 1));
                    if hunk.new_lines() == 0 {
                        file.removed_at.push(hunk.new_start().max(1));
                    }
                }
            }
        }
        files.insert(path, file);
    }
    Ok(GitDiff {
        base: short(merge_base.id()),
        head: head.map(|_| short(head_commit.id())),
        files: files.into_values().collect(),
    })
}

fn short(id: Oid) -> String {
    id.to_string()[..8].to_string()
}

/// A definition a change falls in.
#[derive(Debug)]
pub struct AffectedDefinition {
    pub symbol: SymbolHit,
    /// The changed head-side ranges inside it.
    pub changes: Vec<(u32, u32)>,
    /// Relations resolved to it.
    pub callers: Vec<RelationHit>,
    /// Relations that may reach it: ambiguous among it and others.
    pub uncertain: Vec<RelationHit>,
    /// Tests reaching it within [`TEST_DEPTH`] hops, with why each is one.
    pub tests: Vec<(RelationHit, TestEvidence)>,
}

/// A definition the base had and the head no longer has.
#[derive(Debug)]
pub struct RemovedDefinition {
    pub path: String,
    pub name: String,
    pub kind: String,
    /// Its line in the base version.
    pub line: u32,
    /// Relations that still name it, which may now break.
    pub still_named: Vec<RelationHit>,
}

/// What [`Index::diff_impact`] found.
#[derive(Debug, Default)]
pub struct DiffImpact {
    pub affected: Vec<AffectedDefinition>,
    pub removed: Vec<RemovedDefinition>,
    /// Changed files whose index row is not the head's bytes.
    pub stale: Vec<String>,
    /// Changed files with no indexed definition a change falls in, and no
    /// removed one: unsupported, unparsed, or a change between definitions.
    pub unmapped: Vec<String>,
}

/// How far back the tests of a definition are looked for.
pub const TEST_DEPTH: u32 = 8;

/// Most relations listed per definition and section.
const PER_SECTION: usize = 20;

/// Kinds a change is not attributed to: a file's own module symbol spans
/// the file.
const CONTAINER_KINDS: &[&str] = &["module"];

pub(crate) fn diff_impact(
    index: &Index,
    registry: &LanguageRegistry,
    diff: &GitDiff,
) -> Result<DiffImpact> {
    let mut report = DiffImpact::default();
    let paths: Vec<&str> = diff.files.iter().map(|f| f.path.as_str()).collect();
    let mut symbols: HashMap<String, Vec<SymbolHit>> = HashMap::new();
    for symbol in index.symbols_in_files(&paths)? {
        symbols
            .entry(symbol.relative_path.clone())
            .or_default()
            .push(symbol);
    }
    for file in &diff.files {
        let current = symbols.remove(&file.path).unwrap_or_default();
        if let Some(bytes) = &file.head_bytes {
            if !current.is_empty() && !index.source_matches_index(&file.path, bytes)? {
                report.stale.push(file.path.clone());
            }
        }
        let mut touched: Vec<(SymbolHit, Vec<(u32, u32)>)> = Vec::new();
        // An added range falls in each definition it overlaps; a pure
        // removal only in one holding the lines on both sides of it, so
        // deleting a whole function is not a change to the one before it.
        let ranges = file.added.iter().map(|&range| (range, None)).chain(
            file.removed_at
                .iter()
                .map(|&at| ((at, at), Some((at, at + 1)))),
        );
        for (range, must_hold) in ranges {
            for symbol in innermost(&current, range, must_hold) {
                // The lines of the change inside this definition.
                let last = symbol.end_line.unwrap_or(symbol.line);
                let range = (range.0.max(symbol.line), range.1.min(last));
                match touched.iter_mut().find(|(s, _)| s.id == symbol.id) {
                    Some((_, changes)) => changes.push(range),
                    None => touched.push((symbol.clone(), vec![range])),
                }
            }
        }
        let removed = removed_definitions(index, registry, file, &current)?;
        if touched.is_empty() && removed.is_empty() {
            report.unmapped.push(file.path.clone());
        }
        for (symbol, changes) in touched {
            report.affected.push(affected(index, symbol, changes)?);
        }
        report.removed.extend(removed);
    }
    Ok(report)
}

/// The definitions `range` falls in, innermost only: a change inside a
/// method is the method's, not its class's too.
fn innermost(
    symbols: &[SymbolHit],
    (start, end): (u32, u32),
    must_hold: Option<(u32, u32)>,
) -> Vec<&SymbolHit> {
    let span = |s: &SymbolHit| (s.line, s.end_line.unwrap_or(s.line));
    let overlapping: Vec<&SymbolHit> = symbols
        .iter()
        .filter(|s| !CONTAINER_KINDS.contains(&s.kind.as_str()))
        .filter(|s| {
            let (first, last) = span(s);
            let holds = must_hold.is_none_or(|(a, b)| first <= a && b <= last);
            first <= end && last >= start && holds
        })
        .collect();
    overlapping
        .iter()
        .copied()
        .filter(|outer| {
            let (first, last) = span(outer);
            !overlapping.iter().any(|inner| {
                let (a, b) = span(inner);
                inner.id != outer.id && first <= a && b <= last && (a, b) != (first, last)
            })
        })
        .collect()
}

fn affected(
    index: &Index,
    symbol: SymbolHit,
    changes: Vec<(u32, u32)>,
) -> Result<AffectedDefinition> {
    let direct = crate::traversal::find_referrers_of(index, &[symbol.id], false, 1, usize::MAX)?;
    let (callers, uncertain): (Vec<RelationHit>, Vec<RelationHit>) = direct
        .into_iter()
        .filter(|hit| hit.from_symbol_id != symbol.id)
        .partition(|hit| hit.resolution == Resolution::Resolved);
    let reaching =
        crate::traversal::find_referrers_of(index, &[symbol.id], false, TEST_DEPTH, 2_000)?;
    let ids: Vec<i64> = reaching.iter().map(|hit| hit.from_symbol_id).collect();
    let declared = index.declared_tests(&ids)?;
    // A test file's module owns its imports but is no test to run, as in
    // `affected_test_page`.
    let modules: HashSet<i64> = index
        .symbols_by_ids(&ids)?
        .into_iter()
        .filter(|row| row.kind == "module")
        .map(|row| row.id)
        .collect();
    let mut seen = HashSet::new();
    let tests = reaching
        .into_iter()
        .filter(|hit| !modules.contains(&hit.from_symbol_id))
        .filter_map(|hit| {
            let evidence = crate::test_evidence(
                declared.get(&hit.from_symbol_id).map(String::as_str),
                &hit.from_symbol,
                &hit.relative_path,
            )?;
            seen.insert(hit.from_symbol_id).then_some((hit, evidence))
        })
        .take(PER_SECTION)
        .collect();
    Ok(AffectedDefinition {
        symbol,
        changes,
        callers: callers.into_iter().take(PER_SECTION).collect(),
        uncertain: uncertain.into_iter().take(PER_SECTION).collect(),
        tests,
    })
}

/// Definitions of the base version that removed lines fall in and the
/// head no longer has (same name and kind), with the relations still
/// naming each.
fn removed_definitions(
    index: &Index,
    registry: &LanguageRegistry,
    file: &ChangedFile,
    current: &[SymbolHit],
) -> Result<Vec<RemovedDefinition>> {
    let Some(contents) = &file.base_contents else {
        return Ok(Vec::new());
    };
    if file.removed.is_empty() {
        return Ok(Vec::new());
    }
    let source = SourceFile {
        relative_path: file.path.clone(),
        contents: contents.clone(),
    };
    let Ok(parsed) = registry.parse(&source) else {
        return Ok(Vec::new());
    };
    let still_there: HashSet<(&str, &str)> = current
        .iter()
        .map(|s| (s.name.as_str(), s.kind.as_str()))
        .collect();
    let mut removed = Vec::new();
    for symbol in &parsed.symbols {
        let kind = crate::indexer::symbol_kind_str(symbol.kind);
        if CONTAINER_KINDS.contains(&kind) {
            continue;
        }
        let first = symbol.location.line;
        let last = symbol.location.end_line.unwrap_or(first);
        // Removed only when its own lines are among the removed ones.
        let gone = file
            .removed
            .iter()
            .any(|&(start, end)| start <= first && last <= end);
        if !gone || still_there.contains(&(symbol.name.as_str(), kind)) {
            continue;
        }
        let still_named = index
            .relation_page(
                &symbol.name,
                RelationDirection::References,
                1,
                (PER_SECTION, 0),
                QueryScope::default(),
            )?
            .hits;
        removed.push(RemovedDefinition {
            path: file.path.clone(),
            name: symbol.name.clone(),
            kind: kind.to_string(),
            line: first,
            still_named,
        });
    }
    Ok(removed)
}
