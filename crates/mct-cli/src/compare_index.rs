//! `mct-cli compare-index`: what changed between two index databases of
//! the same project (issue #637). Both are opened read-only; neither is
//! migrated, reindexed or written.

use std::path::Path;

use anyhow::Context;
use mct_index::{compare_indexes, ExcludeSet, Index, IndexComparison, RowChanges};

pub(crate) fn run(
    root: &Path,
    before: &Path,
    after: Option<&Path>,
    examples: usize,
) -> anyhow::Result<()> {
    let default_after = root.join(".mct-index").join("index.sqlite3");
    let after = after.unwrap_or(&default_after);
    let open = |path: &Path| {
        Index::open_existing_read_only(root, path, ExcludeSet::default())
            .with_context(|| format!("open index {} read-only", path.display()))
    };
    let (old, new) = (open(before)?, open(after)?);
    let comparison = compare_indexes(&old, &new, examples)?;
    print!("{}", render(before, after, &comparison));
    Ok(())
}

fn render(before: &Path, after: &Path, c: &IndexComparison) -> String {
    let mut out = format!(
        "Index comparison: {} -> {}\n",
        before.display(),
        after.display()
    );
    let line = |label: &str, rows: &RowChanges| {
        format!(
            "{label}: +{} -{} ({} unchanged)\n",
            rows.added, rows.removed, rows.unchanged
        )
    };
    out.push_str(&line("Files", &c.files));
    if !c.files_changed.is_empty() {
        out.push_str(&format!(
            "  {} file(s) on both sides with new content\n",
            c.files_changed.len()
        ));
    }
    out.push_str(&line("Symbols", &c.symbols));
    out.push_str(&format!(
        "  {} symbol(s) on both sides with a new signature\n",
        c.signature_changes
    ));
    out.push_str(&line("Relations", &c.relations));
    let changed: usize = c.transitions.values().sum();
    out.push_str(&format!("Relations decided differently: {changed}\n"));
    for ((was, now), n) in &c.transitions {
        out.push_str(&format!("  {was} -> {now}: {n}\n"));
    }
    let examples = |out: &mut String, title: &str, rows: &[String]| {
        if !rows.is_empty() {
            out.push_str(&format!("{title}:\n"));
            for row in rows {
                out.push_str(&format!("  {row}\n"));
            }
        }
    };
    examples(&mut out, "Files added", &c.files.added_examples);
    examples(&mut out, "Files removed", &c.files.removed_examples);
    examples(&mut out, "Symbols added", &c.symbols.added_examples);
    examples(&mut out, "Symbols removed", &c.symbols.removed_examples);
    if !c.signature_examples.is_empty() {
        out.push_str("Signatures changed:\n");
        for change in &c.signature_examples {
            out.push_str(&format!(
                "  {}\n    before: {}\n    after:  {}\n",
                change.key, change.before, change.after
            ));
        }
    }
    examples(&mut out, "Relations added", &c.relations.added_examples);
    examples(&mut out, "Relations removed", &c.relations.removed_examples);
    if !c.outcome_examples.is_empty() {
        out.push_str("Relations decided differently, examples:\n");
        for change in &c.outcome_examples {
            out.push_str(&format!(
                "  {}\n    before: {}\n    after:  {}\n",
                change.key, change.before, change.after
            ));
        }
    }
    out
}
