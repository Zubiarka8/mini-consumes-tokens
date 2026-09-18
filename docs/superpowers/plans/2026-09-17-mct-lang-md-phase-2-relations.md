# mct-lang-md Phase 2 (WikiLink/Tag Relations) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `crates/mct-lang-md` to index `[[WikiLink]]` and `#tag` occurrences in heading/paragraph text as `RelationKind::References` relations, with zero changes to `mct-core`'s schema.

**Architecture:** `Walker` (the existing recursive AST visitor) gains a `parent_id: Option<SymbolId>` threaded alongside its existing `parent_name`, plus a `relations: Vec<SymbolRelation>` field. Two new hand-rolled text-scanning functions (`scan_wikilinks`, `scan_tags`) run over raw node text — no inline-grammar reparse, no new dependency. Tags are encoded as `to_name = "tag:<name>"` on an ordinary `References` relation; no new `SymbolKind`/`RelationKind` variant, no new `SymbolRecord` field.

**Tech Stack:** Rust, `tree-sitter-md` (block grammar only, already a dependency). No new crates.

**Spec:** `docs/superpowers/specs/2026-09-17-obsidian-docs-vault-and-md-parser-design.md` (Part 2 and Decisions 1–5)

## Global Constraints

- No new `SymbolKind` or `RelationKind` variant (Decision 1, spec Non-goals).
- No new field on `SymbolRecord` (Decision 1).
- No `regex` or other new dependency — hand-rolled scanning only (Decision 4).
- A link/tag with no enclosing heading symbol is silently skipped, never causes a panic (Decision 2).
- WikiLink targets are verbatim except a stripped `|alias` suffix; `#Heading` anchors are NOT split (Decision 3).
- A relation's `Location` is its whole containing heading/paragraph block, not the exact bracket span (Decision 5).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` must stay clean (CI-gating, zero warnings).
- Production code in `crates/mct-lang-md/src/` must never `unwrap()`/`panic!`/`expect()` on repo-input content (project-wide invariant).

---

### Task 1: Thread `parent_id` through the walker (pure refactor)

**Files:**
- Modify: `crates/mct-lang-md/src/lib.rs`

**Interfaces:**
- Produces: `Walker::visit(&mut self, node: Node, parent_name: Option<String>, parent_id: Option<SymbolId>, depth: u32)` and `Walker::visit_children(&mut self, node: Node, parent_name: Option<String>, parent_id: Option<SymbolId>, depth: u32)` — every later task calls these with this 4-argument signature.

- [ ] **Step 1: Confirm the baseline is green**

Run: `cargo test -p mct-lang-md`
Expected: all tests pass (10 in `tests/parse.rs`, 4 in `tests/index_integration.rs`).

- [ ] **Step 2: Edit `visit_children`**

Replace:
```rust
    fn visit_children(&mut self, node: Node, parent_name: Option<String>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, parent_name.clone(), depth + 1);
        }
    }
```
with:
```rust
    fn visit_children(
        &mut self,
        node: Node,
        parent_name: Option<String>,
        parent_id: Option<SymbolId>,
        depth: u32,
    ) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, parent_name.clone(), parent_id, depth + 1);
        }
    }
```

- [ ] **Step 3: Edit `visit`**

Replace:
```rust
    fn visit(&mut self, node: Node, parent_name: Option<String>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "section" => match find_child(node, "atx_heading") {
                Some(heading) => {
                    let name = heading_text(heading, self.source).to_string();
                    self.push_symbol(
                        name.clone(),
                        SymbolKind::Element,
                        location(heading),
                        parent_name,
                    );
                    self.visit_children(node, Some(name), depth + 1);
                }
                None => self.visit_children(node, parent_name, depth + 1),
            },
            _ => self.visit_children(node, parent_name, depth + 1),
        }
    }
```
with:
```rust
    fn visit(
        &mut self,
        node: Node,
        parent_name: Option<String>,
        parent_id: Option<SymbolId>,
        depth: u32,
    ) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "section" => match find_child(node, "atx_heading") {
                Some(heading) => {
                    let name = heading_text(heading, self.source).to_string();
                    let id = self.push_symbol(
                        name.clone(),
                        SymbolKind::Element,
                        location(heading),
                        parent_name,
                    );
                    self.visit_children(node, Some(name), Some(id), depth + 1);
                }
                None => self.visit_children(node, parent_name, parent_id, depth + 1),
            },
            _ => self.visit_children(node, parent_name, parent_id, depth + 1),
        }
    }
```

- [ ] **Step 4: Edit the `parse()` call site**

Replace:
```rust
        let mut walker = Walker::new(&file.contents);
        walker.visit_children(root, None, 0);
        Ok(walker.finish())
```
with:
```rust
        let mut walker = Walker::new(&file.contents);
        walker.visit_children(root, None, None, 0);
        Ok(walker.finish())
```

- [ ] **Step 5: Run tests, verify no regressions**

Run: `cargo test -p mct-lang-md`
Expected: identical pass counts as Step 1 (behavior is unchanged — `parent_id` is threaded but not yet consumed).

- [ ] **Step 6: Commit**

```bash
git add crates/mct-lang-md/src/lib.rs
git commit -m "refactor(mct-lang-md): thread parent_id through the heading walker"
```

---

### Task 2: `[[WikiLink]]` extraction (paragraphs and heading text)

**Files:**
- Modify: `crates/mct-lang-md/src/lib.rs`
- Modify: `crates/mct-lang-md/tests/parse.rs`

**Interfaces:**
- Consumes: `Walker::visit`/`visit_children` with `parent_id` (Task 1).
- Produces: free function `scan_wikilinks(text: &str) -> Vec<String>`; `Walker::push_relations_from_text(&mut self, from: SymbolId, text: &str, loc: Location)` — Task 3 extends this same method to also emit tag relations.

- [ ] **Step 1: Write failing tests**

In `crates/mct-lang-md/tests/parse.rs`, change the import line:

Replace:
```rust
use mct_core::{LanguageParser, SourceFile, SymbolKind};
```
with:
```rust
use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
```

Then add these tests (anywhere after the existing tests, before the closing of the file):

```rust
#[test]
fn wikilink_in_a_paragraph_emits_a_references_relation() {
    let parsed = parse("# A\n\nSee [[Other Note]] for details.\n");
    let a = parsed.symbols.iter().find(|s| s.name == "A").unwrap();
    let rel = parsed
        .relations
        .iter()
        .find(|r| r.to_name == "Other Note")
        .unwrap();
    assert_eq!(rel.kind, RelationKind::References);
    assert_eq!(rel.from, a.id);
}

#[test]
fn wikilink_alias_is_stripped_from_the_target() {
    let parsed = parse("# A\n\nSee [[Other Note|click here]] for details.\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Other Note"));
    assert!(
        !parsed
            .relations
            .iter()
            .any(|r| r.to_name.contains("click here"))
    );
}

#[test]
fn wikilink_anchor_is_indexed_verbatim_not_split() {
    let parsed = parse("# A\n\nSee [[Other Note#Some Heading]] for details.\n");
    assert!(
        parsed
            .relations
            .iter()
            .any(|r| r.to_name == "Other Note#Some Heading")
    );
}

#[test]
fn wikilink_inside_the_heading_text_itself_emits_a_relation() {
    let parsed = parse("# See [[Other Note]]\n");
    let heading = parsed
        .symbols
        .iter()
        .find(|s| s.name == "See [[Other Note]]")
        .unwrap();
    let rel = parsed
        .relations
        .iter()
        .find(|r| r.to_name == "Other Note")
        .unwrap();
    assert_eq!(rel.from, heading.id);
}
```

- [ ] **Step 2: Run the new tests, verify they fail**

Run: `cargo test -p mct-lang-md wikilink_in_a_paragraph_emits_a_references_relation`
Expected: FAIL (panics on `.unwrap()` — no relation exists yet).

- [ ] **Step 3: Add imports and the `relations` field**

Replace:
```rust
use mct_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, SourceFile, SymbolId, SymbolKind,
    SymbolRecord,
};
```
with:
```rust
use mct_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
```

Replace:
```rust
struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            symbols: Vec::new(),
            next_id: 0,
        }
    }
```
with:
```rust
struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            symbols: Vec::new(),
            relations: Vec::new(),
            next_id: 0,
        }
    }
```

- [ ] **Step 4: Add `push_relations_from_text` and `scan_wikilinks`**

Add this method inside `impl<'a> Walker<'a>`, after `push_symbol`:

```rust
    /// Scans `text` for `[[WikiLink]]` targets and records each as a
    /// `References` relation from `from`. `loc` is the whole containing
    /// heading/paragraph block's location — relations are block-granular,
    /// not exact-span (see the module doc comment).
    fn push_relations_from_text(&mut self, from: SymbolId, text: &str, loc: Location) {
        for to_name in scan_wikilinks(text) {
            self.relations.push(SymbolRelation {
                from,
                kind: RelationKind::References,
                to_name,
                location: loc,
            });
        }
    }
```

Add this free function, after `heading_text`:

```rust
/// Extracts `[[WikiLink]]` targets from raw text. A `|alias` display
/// suffix is stripped (the alias is presentation, not the reference
/// identity); a `#Heading` anchor is kept verbatim as part of the target
/// (anchor-aware resolution is deferred, see the module doc comment). Not
/// grammar-aware — this scans raw node text directly, since
/// `tree-sitter-md`'s inline grammar has no concept of this
/// Obsidian-specific syntax.
fn scan_wikilinks(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut i = 0;
    while let Some(start) = text[i..].find("[[") {
        let open = i + start + 2;
        let Some(rel_end) = text[open..].find("]]") else {
            break;
        };
        let close = open + rel_end;
        let target = text[open..close].split('|').next().unwrap_or("").trim();
        if !target.is_empty() {
            targets.push(target.to_string());
        }
        i = close + 2;
    }
    targets
}
```

- [ ] **Step 5: Wire relation scanning into `visit`, add the `paragraph` arm**

Replace:
```rust
        match node.kind() {
            "section" => match find_child(node, "atx_heading") {
                Some(heading) => {
                    let name = heading_text(heading, self.source).to_string();
                    let id = self.push_symbol(
                        name.clone(),
                        SymbolKind::Element,
                        location(heading),
                        parent_name,
                    );
                    self.visit_children(node, Some(name), Some(id), depth + 1);
                }
                None => self.visit_children(node, parent_name, parent_id, depth + 1),
            },
            _ => self.visit_children(node, parent_name, parent_id, depth + 1),
        }
```
with:
```rust
        match node.kind() {
            "section" => match find_child(node, "atx_heading") {
                Some(heading) => {
                    let name = heading_text(heading, self.source).to_string();
                    let heading_loc = location(heading);
                    let id =
                        self.push_symbol(name.clone(), SymbolKind::Element, heading_loc, parent_name);
                    self.push_relations_from_text(id, &name, heading_loc);
                    self.visit_children(node, Some(name), Some(id), depth + 1);
                }
                None => self.visit_children(node, parent_name, parent_id, depth + 1),
            },
            "paragraph" => {
                if let Some(id) = parent_id {
                    let text = node.utf8_text(self.source.as_bytes()).unwrap_or_default();
                    self.push_relations_from_text(id, text, location(node));
                }
                self.visit_children(node, parent_name, parent_id, depth + 1);
            }
            _ => self.visit_children(node, parent_name, parent_id, depth + 1),
        }
```

- [ ] **Step 6: Update `finish()`**

Replace:
```rust
    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: Vec::new(),
        }
    }
```
with:
```rust
    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
        }
    }
```

- [ ] **Step 7: Run tests, verify they pass**

Run: `cargo test -p mct-lang-md`
Expected: all tests pass, including the 4 new ones. (`full_abcdef_hierarchy_matches_the_spec_example` and the old phase-1 no-relations test must still pass — their inputs contain no `[[`.)

- [ ] **Step 8: Commit**

```bash
git add crates/mct-lang-md/src/lib.rs crates/mct-lang-md/tests/parse.rs
git commit -m "feat(mct-lang-md): extract [[WikiLinks]] as References relations"
```

---

### Task 3: `#tag` extraction (paragraphs and heading text)

**Files:**
- Modify: `crates/mct-lang-md/src/lib.rs`
- Modify: `crates/mct-lang-md/tests/parse.rs`

**Interfaces:**
- Consumes: `Walker::push_relations_from_text` (Task 2) — this task adds a second loop to it.
- Produces: free function `scan_tags(text: &str) -> Vec<String>`.

- [ ] **Step 1: Write failing tests**

Add to `crates/mct-lang-md/tests/parse.rs`:

```rust
#[test]
fn hashtag_in_a_paragraph_emits_a_tag_prefixed_relation() {
    let parsed = parse("# A\n\nThis note is about #productivity today.\n");
    let a = parsed.symbols.iter().find(|s| s.name == "A").unwrap();
    let rel = parsed
        .relations
        .iter()
        .find(|r| r.to_name == "tag:productivity")
        .unwrap();
    assert_eq!(rel.kind, RelationKind::References);
    assert_eq!(rel.from, a.id);
}

#[test]
fn a_url_fragment_is_not_mistaken_for_a_tag() {
    let parsed = parse("# A\n\nSee http://example.com/page#section for more.\n");
    assert!(!parsed.relations.iter().any(|r| r.to_name == "tag:section"));
}

#[test]
fn hashtag_inside_the_heading_text_itself_emits_a_relation() {
    let parsed = parse("# Ideas #brainstorm\n");
    let heading = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Ideas #brainstorm")
        .unwrap();
    let rel = parsed
        .relations
        .iter()
        .find(|r| r.to_name == "tag:brainstorm")
        .unwrap();
    assert_eq!(rel.from, heading.id);
}
```

- [ ] **Step 2: Run the new tests, verify they fail**

Run: `cargo test -p mct-lang-md hashtag_in_a_paragraph_emits_a_tag_prefixed_relation`
Expected: FAIL.

- [ ] **Step 3: Add `scan_tags` and wire it into `push_relations_from_text`**

Replace:
```rust
    fn push_relations_from_text(&mut self, from: SymbolId, text: &str, loc: Location) {
        for to_name in scan_wikilinks(text) {
            self.relations.push(SymbolRelation {
                from,
                kind: RelationKind::References,
                to_name,
                location: loc,
            });
        }
    }
```
with:
```rust
    fn push_relations_from_text(&mut self, from: SymbolId, text: &str, loc: Location) {
        for to_name in scan_wikilinks(text) {
            self.relations.push(SymbolRelation {
                from,
                kind: RelationKind::References,
                to_name,
                location: loc,
            });
        }
        for tag in scan_tags(text) {
            self.relations.push(SymbolRelation {
                from,
                kind: RelationKind::References,
                to_name: format!("tag:{tag}"),
                location: loc,
            });
        }
    }
```

Add this free function, after `scan_wikilinks`:

```rust
/// Extracts `#tag` occurrences from raw text, each returned as its bare
/// name (the `tag:` prefix is applied by the caller). A `#` only starts a
/// tag when preceded by start-of-text or whitespace — this is what keeps a
/// URL fragment like `.../page#section` from being mistaken for a tag (the
/// character before its `#` is `/`, never whitespace). A tag-shaped token
/// inside inline code (`` `#notatag` ``) is still matched — excluding code
/// spans needs an inline-grammar reparse, deferred (see the module doc
/// comment).
fn scan_tags(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tags = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let at_boundary = i == 0 || chars[i - 1].is_whitespace();
        if chars[i] == '#' && at_boundary {
            let start = i + 1;
            let mut end = start;
            while end < chars.len()
                && (chars[end].is_alphanumeric() || matches!(chars[end], '_' | '/' | '-'))
            {
                end += 1;
            }
            if end > start {
                tags.push(chars[start..end].iter().collect());
                i = end;
                continue;
            }
        }
        i += 1;
    }
    tags
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p mct-lang-md`
Expected: all tests pass, including the 3 new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/mct-lang-md/src/lib.rs crates/mct-lang-md/tests/parse.rs
git commit -m "feat(mct-lang-md): extract #tags as tag:-prefixed References relations"
```

---

### Task 4: Lock in orphan-link skip; retire the "Phase 1" framing

**Files:**
- Modify: `crates/mct-lang-md/src/lib.rs` (module doc comment only)
- Modify: `crates/mct-lang-md/tests/parse.rs`

**Interfaces:** None new — this task adds regression tests for already-correct behavior (Decision 2 falls out of Task 1's `parent_id: None` for pre-heading content) and fixes stale naming/comments, no production logic changes.

- [ ] **Step 1: Add the orphan-link regression test**

Add to `crates/mct-lang-md/tests/parse.rs`:

```rust
#[test]
fn a_wikilink_before_any_heading_is_not_indexed() {
    let parsed = parse("See [[Orphan Note]] before any heading.\n\n# A\n");
    assert!(parsed.relations.is_empty(), "{:?}", parsed.relations);
}
```

Run: `cargo test -p mct-lang-md a_wikilink_before_any_heading_is_not_indexed`
Expected: PASS immediately — no implementation change needed, this locks in the guarantee that Task 1's `parent_id: None` (before any heading exists) already provides.

- [ ] **Step 2: Rename the stale "Phase 1" test and fix its assertion message**

Replace:
```rust
#[test]
fn no_relations_are_ever_emitted_in_phase_1() {
    let parsed = parse("# A\n\nSome text with a [link](other.md) in it.\n\n## B\n");
    assert!(parsed.relations.is_empty(), "{:?}", parsed.relations);
}
```
with:
```rust
#[test]
fn a_standard_markdown_link_is_not_mistaken_for_a_wikilink() {
    let parsed = parse("# A\n\nSome text with a [link](other.md) in it.\n\n## B\n");
    assert!(
        parsed.relations.is_empty(),
        "single-bracket links are not [[WikiLinks]]: {:?}",
        parsed.relations
    );
}
```

- [ ] **Step 3: Fix the stale assertion message in `full_abcdef_hierarchy_matches_the_spec_example`**

Replace:
```rust
    assert!(
        parsed.relations.is_empty(),
        "Phase 1 emits no relations: {:?}",
        parsed.relations
    );
```
with:
```rust
    assert!(
        parsed.relations.is_empty(),
        "no [[WikiLinks]] or #tags appear in this input: {:?}",
        parsed.relations
    );
```

- [ ] **Step 4: Update the module doc comment**

Replace the whole doc comment block at the top of `crates/mct-lang-md/src/lib.rs`:
```rust
//! `LanguageParser` implementation for Markdown headings, via `tree-sitter-md`.
//!
//! Phase 1 scope only: ATX headings (`#`..`######`) become `Element` symbols,
//! nested by the block grammar's own `section` structure — no manual
//! level-counting stack is needed, `tree-sitter-md` already wraps each
//! heading and its lower-level content in a `section` node nested by level
//! (verified against a real parse before writing this walker: `# A / ## B /
//! ## C / ### D / # E / ## F` nests D under C's section, not B's, and E
//! starts a sibling section of A's, not a child of it). No relations are
//! emitted yet — internal links, `RelationKind::Imports`, anchors, setext
//! headings, lists, tables, and code blocks are all deliberately out of
//! scope for this phase.
//!
//! Unlike `mct-lang-xml`, this crate does not emit a synthetic root `Module`
//! symbol for the file: a heading-less document must produce zero symbols.
```
with:
```rust
//! `LanguageParser` implementation for Markdown, via `tree-sitter-md`.
//!
//! Phase 1: ATX headings (`#`..`######`) become `Element` symbols, nested by
//! the block grammar's own `section` structure — no manual level-counting
//! stack is needed, `tree-sitter-md` already wraps each heading and its
//! lower-level content in a `section` node nested by level (verified against
//! a real parse before writing this walker: `# A / ## B / ## C / ### D / # E
//! / ## F` nests D under C's section, not B's, and E starts a sibling
//! section of A's, not a child of it).
//!
//! Phase 2: `[[WikiLink]]` and `#tag` occurrences in a heading's own text or
//! in a `paragraph` block are indexed as `RelationKind::References`
//! relations from the enclosing heading symbol (a `#tag` target is
//! `tag:<name>`, since `mct-core::SymbolRecord` has no attribute field — see
//! `docs/superpowers/specs/2026-09-17-obsidian-docs-vault-and-md-parser-design.md`).
//! A `|alias` suffix on a WikiLink is stripped; a `#Heading` anchor is kept
//! verbatim as part of the target. A link/tag with no enclosing heading (text
//! before the first heading, or in a heading-less file) is silently skipped.
//! Scanning is hand-rolled text scanning, not a second parse with the inline
//! grammar — `tree-sitter-md`'s inline grammar is CommonMark and has no
//! concept of this Obsidian-specific syntax. Deliberately out of scope:
//! links/tags inside list items, blockquotes, tables, or code spans;
//! anchor-aware target resolution; exact-span relation locations (a
//! relation's `Location` is its whole containing block, not the bracket
//! span). Standard Markdown links (`[text](url)`) are not `[[WikiLinks]]`
//! and are never indexed. Setext headings, lists, tables, and code blocks
//! remain out of scope for symbol extraction, same as Phase 1.
//!
//! Unlike `mct-lang-xml`, this crate does not emit a synthetic root `Module`
//! symbol for the file: a heading-less document must produce zero symbols.
```

- [ ] **Step 5: Run tests, verify all pass**

Run: `cargo test -p mct-lang-md`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/mct-lang-md/src/lib.rs crates/mct-lang-md/tests/parse.rs
git commit -m "test(mct-lang-md): lock in orphan-link skip; update module doc comment for Phase 2"
```

---

### Task 5: Integration fixture through the real SQLite index

**Files:**
- Modify: `crates/mct-lang-md/tests/fixtures/docs-project/architecture.md`
- Modify: `crates/mct-lang-md/tests/index_integration.rs`

**Interfaces:**
- Consumes: `Index::find_references(&self, symbol: &str) -> Result<Vec<RelationHit>>` (existing `mct-index` API; `RelationHit` has `kind: String`, `from_symbol: String`, `to_name: String`).

- [ ] **Step 1: Add a WikiLink and a tag to the fixture**

Replace the full contents of `crates/mct-lang-md/tests/fixtures/docs-project/architecture.md`:
```markdown
# Architecture

## Overview

A short description of the system.

## Components

### Indexer

Walks the file tree and writes to SQLite.

### Parser

Wraps tree-sitter for one language.

# Testing

## Unit Tests

Per-crate `tests/parse.rs`.
```
with:
```markdown
# Architecture

## Overview

A short description of the system.

## Components

### Indexer

Walks the file tree and writes to SQLite. See [[Parser]] for the tree-sitter wrapper. #core

### Parser

Wraps tree-sitter for one language.

# Testing

## Unit Tests

Per-crate `tests/parse.rs`.
```

- [ ] **Step 2: Write failing tests**

Add to `crates/mct-lang-md/tests/index_integration.rs`:

```rust
#[test]
fn a_wikilink_in_the_fixture_resolves_as_a_references_relation() {
    let index = open_indexed();
    let hits = index.find_references("Parser").unwrap();
    assert!(
        hits.iter().any(|h| h.from_symbol == "Indexer" && h.kind == "references"),
        "{hits:?}"
    );
}

#[test]
fn a_tag_in_the_fixture_resolves_as_a_tag_prefixed_reference() {
    let index = open_indexed();
    let hits = index.find_references("tag:core").unwrap();
    assert!(hits.iter().any(|h| h.from_symbol == "Indexer"), "{hits:?}");
}
```

- [ ] **Step 3: Rename the now-misleading status test**

Replace:
```rust
#[test]
fn status_reports_full_markdown_coverage_with_no_relations() {
```
with:
```rust
#[test]
fn status_reports_full_markdown_coverage() {
```
(leave the test body unchanged — it only ever asserted symbol counts, which are unaffected by the fixture edit).

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p mct-lang-md`
Expected: all pass, including the 2 new ones. `report.files_parsed == 1` and `md.symbol_count == 7` still hold (the fixture edit adds a link/tag inside existing text, no new heading).

- [ ] **Step 5: Commit**

```bash
git add crates/mct-lang-md/tests/fixtures/docs-project/architecture.md crates/mct-lang-md/tests/index_integration.rs
git commit -m "test(mct-lang-md): verify WikiLink/tag relations resolve through the real SQLite index"
```

---

### Task 6: README polish and full workspace validation

**Files:**
- Modify: `README.md`

**Interfaces:** None — documentation and validation only.

- [ ] **Step 1: Run the full test suite and note the new total**

Run: `cargo test --workspace 2>&1 | tail -30`
Expected: all tests pass. Note the final aggregate test count reported across all crates (this replaces the "261" figure below — use the real number from this run, not an assumed one).

- [ ] **Step 2: Update the "Status" paragraph**

In `README.md`, replace:
```
Plain XML is deliberately structural-only, and Markdown (Phase 1) is headings-only with no relations yet — see the coverage table below.
```
with:
```
Plain XML is deliberately structural-only, and Markdown (Phase 2) indexes headings plus `[[WikiLink]]`/`#tag` relations found in heading and paragraph text — see the coverage table below.
```

- [ ] **Step 3: Update the "Supported languages" table row**

Replace:
```
| Markdown | Implemented (Phase 1 — ATX headings only, no relations yet: no internal links, anchors, setext headings, lists, tables, or code blocks) | `crates/mct-lang-md` |
```
with:
```
| Markdown | Implemented (Phase 2 — ATX headings as `Element` symbols; `[[WikiLinks]]`/`#tags` in heading/paragraph text as `References` relations, tags prefixed `tag:`; anchors indexed verbatim, no anchor-aware resolution; lists, tables, blockquotes, and code spans not scanned) | `crates/mct-lang-md` |
```

- [ ] **Step 4: Update the "Workspace structure" block**

Replace:
```
  mct-lang-md        LanguageParser impl for Markdown (tree-sitter-md) — ATX headings as nested Element symbols via the grammar's own section nesting, Phase 1, no relations yet
```
with:
```
  mct-lang-md        LanguageParser impl for Markdown (tree-sitter-md) — ATX headings as nested Element symbols via the grammar's own section nesting; Phase 2 adds `[[WikiLink]]`/`#tag` extraction from heading/paragraph text as `References` relations
```

- [ ] **Step 5: Update the "Running tests" paragraph**

Replace the clause `or Markdown's nested-heading \`section\` hierarchy — each through a real multi-file, multi-language fixture, not just both parsers running side by side)` with `or Markdown's nested-heading \`section\` hierarchy plus its \`[[WikiLink]]\`/\`#tag\` relation extraction — each through a real multi-file, multi-language fixture, not just both parsers running side by side)`.

Then replace the leading test-count figure (currently `261 tests across the workspace`) with the real number captured in Step 1 (e.g. `271 tests across the workspace` if that's what the run reported — use the actual observed number).

- [ ] **Step 6: Run full validation**

Run:
```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```
Expected: all three succeed with no errors and no clippy warnings.

- [ ] **Step 7: Verify `docs/02-crates/parsers/` (from the companion docs-vault plan) still holds only the two intended files, if that plan has already run**

Run: `ls docs/02-crates/parsers/ 2>/dev/null | sort || echo "docs vault not yet created"`
Expected: either `mct-lang-php.md` / `overview.md` (nothing else), or the "not yet created" message if the docs-vault plan hasn't run yet — both are fine, this task doesn't depend on it.

- [ ] **Step 8: Commit**

```bash
git add README.md
git commit -m "docs(mct-lang-md): update README for Phase 2 WikiLink/tag relation support"
```
