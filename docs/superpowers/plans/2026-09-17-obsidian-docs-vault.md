# Obsidian Docs Vault Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a token-optimized, Obsidian-compatible documentation vault under `docs/` — atomic, cross-linked, tagged notes covering this project's system/architecture/crates/performance/security/backlog.

**Architecture:** Plain Markdown files under `docs/`, one concept per file, cross-linked via `[[WikiLink]]` and tagged via `#tag`. No tooling required to read/write — Obsidian is optional, the files are just Markdown. `.obsidian/` (Obsidian's local workspace-state folder) is gitignored since it's per-user editor state, not vault content.

**Tech Stack:** Markdown only. No build step.

**Spec:** `docs/superpowers/specs/2026-09-17-obsidian-docs-vault-and-md-parser-design.md` (Part 1 and Decision 6)

## Global Constraints

- Every note is atomic (one concept), bullet-structured, and concise — templates in `docs/06-templates/` must stay under ~100 words each.
- Every note cross-links related notes via `[[WikiLink]]` (using the target's filename without `.md`) and tags itself with at least one `#tag` where it aids retrieval.
- `docs/02-crates/parsers/` holds exactly two files: `overview.md` (the mandatory contract) and `ccm-lang-php.md` (the one language with a documented security exception). No other per-language file.
- This plan does not touch any file under `crates/` — it is pure documentation content.

---

### Task 1: `.gitignore` + system glossary

**Files:**
- Modify: `.gitignore`
- Create: `docs/00-system/glossary.md`

**Interfaces:** None (first task, nothing to consume; later tasks link `[[glossary]]`).

- [ ] **Step 1: Add `.obsidian/` to `.gitignore`**

Open `.gitignore` and append a new line:

```
.obsidian/
```

- [ ] **Step 2: Verify the entry was added**

Run: `grep -c "^.obsidian/$" .gitignore`
Expected: `1`

- [ ] **Step 3: Create `docs/00-system/glossary.md`**

```markdown
# Glossary

- **MOC** — Map of Content; a note that links out to a topic's other notes instead of holding content itself. See [[00-index]].
- **Symbol** — a definable code entity (`SymbolRecord`): function, class, struct, heading, etc.
- **Relation** — a directed link from one symbol to another (`SymbolRelation`): call, import, extends/implements, reference.
- **LanguageParser** — the trait every `ccm-lang-*` crate implements to turn source text into symbols/relations. See [[overview]].
- **Blast radius** — everything that could break if a symbol changes: its callers, references, and likely-affected tests. Computed by `impact_analysis`.
- **WikiLink** — `[[Note Name]]` syntax; indexed as a `References` relation by `ccm-lang-md`.
- **Tag** — `#tag` syntax; indexed as a `tag:`-prefixed `References` relation by `ccm-lang-md`.

#glossary
```

- [ ] **Step 4: Verify the file exists and is non-empty**

Run: `test -s docs/00-system/glossary.md && echo OK`
Expected: `OK`

- [ ] **Step 5: Commit**

```bash
git add .gitignore docs/00-system/glossary.md
git commit -m "docs: add .obsidian/ to gitignore and vault glossary"
```

---

### Task 2: Architecture notes

**Files:**
- Create: `docs/01-architecture/mcp-protocol-spec.md`
- Create: `docs/01-architecture/adrs/001-sqlite-storage.md`

**Interfaces:** Links `[[glossary]]` (Task 1), `[[ccm-mcp-server]]` and `[[overview]]` (Task 3, forward references — valid in Obsidian/WikiLinks regardless of creation order).

- [ ] **Step 1: Create `docs/01-architecture/mcp-protocol-spec.md`**

```markdown
# MCP Protocol Spec

`ccm-mcp-server` speaks [MCP](https://modelcontextprotocol.io) over stdio via `rmcp`.

## Tools

| Tool | Type | Purpose |
|---|---|---|
| `list_symbols` | discovery | List symbols under a file or directory/crate prefix |
| `find_symbol` | atomic | Exact-name symbol lookup |
| `find_references` | atomic | Every reference to a symbol |
| `find_calls` | atomic | What a function calls |
| `find_callers` | atomic | What calls a function |
| `impact_analysis` | composite | Full blast radius in one call |
| `get_file_skeleton` | discovery | Top-level shape of one file, bodies collapsed |
| `reindex` | maintenance | Force a re-scan |
| `get_indexing_status` | maintenance | Index health report |

`find_references`/`find_calls`/`find_callers`/`impact_analysis` accept `depth` (multi-hop, default 1), `limit`, `offset`.

See [[ccm-mcp-server]] for the crate that implements this, [[glossary]] for term definitions.

#architecture #mcp
```

- [ ] **Step 2: Create `docs/01-architecture/adrs/001-sqlite-storage.md`**

```markdown
# ADR-001: SQLite as the index storage engine

## Status
Accepted

## Context
The index must store symbols/relations for any supported language, be queryable with joins (parent/child, cross-file relations), and work identically on Linux/macOS/Windows with zero external services.

## Decision
Use SQLite via `rusqlite` (WAL mode, FTS5), one schema shared by every language — a `language` column on `files`, not per-language tables (see [[overview]]). Tables: `files`, `symbols`, `relations`, `symbols_fts`, `index_issues`, `index_meta`, `dependencies`.

## Consequences
- No server process, no network calls, single-file portability.
- A single shared schema means adding a language never touches storage — only a new `LanguageParser` impl.
- Schema changes are cross-cutting (every language crate + the MCP tool contract) — require an issue first, not a drive-by PR.

#adr #architecture
```

- [ ] **Step 3: Verify both files exist**

Run: `test -s docs/01-architecture/mcp-protocol-spec.md && test -s docs/01-architecture/adrs/001-sqlite-storage.md && echo OK`
Expected: `OK`

- [ ] **Step 4: Commit**

```bash
git add docs/01-architecture
git commit -m "docs: add MCP protocol spec and ADR-001 (SQLite storage)"
```

---

### Task 3: Crate notes

**Files:**
- Create: `docs/02-crates/core/ccm-mcp-server.md`
- Create: `docs/02-crates/parsers/overview.md`
- Create: `docs/02-crates/parsers/ccm-lang-php.md`

**Interfaces:** `overview.md` and `ccm-lang-php.md` link `[[sec-001-php-stack-overflow]]` and `[[limits-spec]]` (Task 4, forward references).

- [ ] **Step 1: Create `docs/02-crates/core/ccm-mcp-server.md`**

```markdown
# ccm-mcp-server

MCP tools over stdio (`rmcp`), exposing the `ccm-index` SQLite index to any MCP-capable agent.

## Depends on
`ccm-core`, `ccm-index`, every `ccm-lang-*` production crate (registers each via `registry.rs::build_registry`).

## Responsibilities
- Auto-reindexes incrementally at startup.
- Implements the 9 tools listed in [[mcp-protocol-spec]].
- Knows no language's grammar — routes everything through `ccm_core::LanguageRegistry`.

## Related
[[overview]] · [[001-sqlite-storage]]

#crate #mcp
```

- [ ] **Step 2: Create `docs/02-crates/parsers/overview.md`**

```markdown
# Parser contract (`ccm-lang-*`)

Every `ccm-lang-*` crate implements `ccm_core::LanguageParser`:

- `language_id() -> &'static str`
- `file_extensions() -> &'static [&'static str]`
- `parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError>`

## Rules
- **Pure AST walk.** Never executes or evaluates input — parsers run over arbitrary, potentially adversarial repo content.
- **No panics on repo input.** A syntax error returns `ParseError::Syntax`, never `unwrap()`/`panic!`/`expect()` on a path that processes file content.
- **Bounded recursion.** Every recursive walker function threads a `depth: u32` counter and stops at `ccm_core::MAX_TRAVERSAL_DEPTH` (256) — see [[sec-001-php-stack-overflow]] for why this exists and [[limits-spec]] for the exact value.
- **Shared model, no new variants without a cross-cutting review.** Emit `SymbolRecord`/`SymbolRelation` using the existing `SymbolKind`/`RelationKind` enums; a new variant touches every crate's `match` arms plus `ccm-index`'s SQL mapping — needs an issue first.
- **Registration is the only integration point.** Add one line each to `ccm-mcp-server/src/registry.rs::build_registry` and `ccm-cli/src/main.rs::build_registry`. Nothing else in `ccm-core`/`ccm-index`/`ccm-mcp-server` changes.

Exceptions with real security/complexity implications get their own note (e.g. [[ccm-lang-php]]); everything else is just a row in [[00-index]]'s extension→crate table.

#architecture #contract
```

- [ ] **Step 3: Create `docs/02-crates/parsers/ccm-lang-php.md`**

```markdown
# ccm-lang-php

The crate CI's fuzzer first caught the unbounded-recursion class of bug in — see [[sec-001-php-stack-overflow]].

## Special behaviors
- `self::`/`parent::`/`static::`/`Class::foo()` scoped calls: the scope is ignored, only `name` is registered as `Calls`.
- Constructor property promotion (`property_promotion_parameter`, PHP 8.0+) indexed as `Field`.
- `use TraitName;` (trait composition) mapped to `RelationKind::Implements` — `ccm-core` has no "mixin" relation kind, documented as a deliberate reuse rather than a new variant.
- `SymbolKind::Trait` reused from Rust with different semantics (documented limitation).

## Related
[[overview]] · [[sec-001-php-stack-overflow]]

#security #crate
```

- [ ] **Step 4: Verify `docs/02-crates/parsers/` has exactly these two files**

Run: `ls docs/02-crates/parsers/ | sort`
Expected:
```
ccm-lang-php.md
overview.md
```

- [ ] **Step 5: Commit**

```bash
git add docs/02-crates
git commit -m "docs: add ccm-mcp-server, parser contract, and ccm-lang-php notes"
```

---

### Task 4: Performance and security notes

**Files:**
- Create: `docs/03-performance/limits-spec.md`
- Create: `docs/04-security/advisories/sec-001-php-stack-overflow.md`

**Interfaces:** None new. Both reference each other and Task 3's `[[overview]]`/`[[ccm-lang-php]]`.

- [ ] **Step 1: Create `docs/03-performance/limits-spec.md`**

```markdown
# Performance & limits

- **`MAX_TRAVERSAL_DEPTH = 256`** — every language crate's AST walker stops recursing past this depth. See [[sec-001-php-stack-overflow]].
- **Fuzzing**: `cargo-fuzz`, 30s smoke run per language crate, CI matrix `fuzz-smoke` — Linux/macOS only (ASan DLL/MSVC-sancov issues on Windows).
- **Token benchmark**: 3 canonical queries (`find_symbol`, `find_callers`, `find_references`) vs. Grep+Read baseline, most languages in the high-80s–90s% character reduction; ceiling 97.8% (Go); floor 72.0% (XML `find_symbol`). Full methodology: `benchmarks/token-benchmark.md`.

#performance
```

- [ ] **Step 2: Create `docs/04-security/advisories/sec-001-php-stack-overflow.md`**

```markdown
# SEC-001: AST-walker stack overflow (unbounded recursion)

- **Status**: Fixed (commit `44a3a25`)
- **Component**: originally caught on `ccm-lang-php`; root cause shared by all 17 crates' `Walker::visit`/`visit_children`
- **Severity**: crash on adversarial/pathological input (ASan stack-overflow), not memory-unsafe — a DoS class, not RCE

## Description
CI's `fuzz-smoke` job crashed `ccm-lang-php` on deeply nested input (AddressSanitizer stack-overflow). Every crate shared the same unbounded mutual recursion pattern; PHP was just the one the fuzzer reached first.

## Fix
Added `ccm_core::MAX_TRAVERSAL_DEPTH` (256) and threaded a depth counter through every crate's walker and its recursive helpers — past the ceiling, `visit()` returns instead of recursing further. Regression test reproduces the original crash shape (5,000 nested parens) and asserts `parse()` still returns `Ok`.

## Related
[[overview]] · [[limits-spec]] · [[ccm-lang-php]]

#security #advisory
```

- [ ] **Step 3: Verify both files exist**

Run: `test -s docs/03-performance/limits-spec.md && test -s docs/04-security/advisories/sec-001-php-stack-overflow.md && echo OK`
Expected: `OK`

- [ ] **Step 4: Commit**

```bash
git add docs/03-performance docs/04-security
git commit -m "docs: add performance/limits spec and SEC-001 advisory"
```

---

### Task 5: Backlog and templates

**Files:**
- Create: `docs/05-backlog/roadmap.md`
- Create: `docs/06-templates/adr-template.md`
- Create: `docs/06-templates/crate-spec-template.md`
- Create: `docs/06-templates/lang-spec-template.md`

**Interfaces:** `lang-spec-template.md` links `[[00-index]]` and `[[overview]]` (Task 3/6, forward reference to Task 6 is fine).

- [ ] **Step 1: Create `docs/05-backlog/roadmap.md`**

```markdown
# Backlog

## Deferred from `ccm-lang-md` Phase 2
See spec `docs/superpowers/specs/2026-09-17-obsidian-docs-vault-and-md-parser-design.md`.
- Anchor-aware resolution of `[[Page#Heading]]` against the target file's real heading symbol.
- Tags/links inside list items, blockquotes, tables.
- Code-span exclusion for `#tag`/`[[...]]` (needs an inline-grammar reparse).
- Exact-span relation locations instead of block-granular.

## Open project decisions
- HTML template-engine handling (Django/Jinja2 embedded in `.html`) — undecided, see `internal/checklist.md`.
- PHP-specific secret patterns (`wp-config.php`, `config/database.php`) — named, not implemented.

#backlog
```

- [ ] **Step 2: Create `docs/06-templates/adr-template.md`**

```markdown
# ADR-NNN: <decision title>

## Status
Proposed | Accepted | Superseded by [[ADR-NNN]]

## Context
<the problem, constraints, forces at play — 2-4 bullets>

## Decision
<what was chosen, one paragraph or bullets>

## Consequences
<what becomes easier/harder as a result>

#adr
```

- [ ] **Step 3: Create `docs/06-templates/crate-spec-template.md`**

```markdown
# <crate-name>

<one-line purpose>

## Depends on
<crates, in one line>

## Responsibilities
- <bullet>
- <bullet>

## Related
[[note]] · [[note]]

#crate
```

- [ ] **Step 4: Create `docs/06-templates/lang-spec-template.md`**

```markdown
# ccm-lang-<name>

> **Use this template only if the language has special behaviors, security-relevant edge cases, or macro-like syntax worth documenting.** Otherwise, do not create a note — just add a row to [[00-index]]'s extension→crate table. Most languages need no note at all.

## Special behaviors
- <bullet>

## Related
[[overview]] · [[00-index]]

#crate
```

- [ ] **Step 5: Verify the template files stay under ~100 words each**

Run: `for f in docs/06-templates/*.md; do echo "$f: $(wc -w < "$f") words"; done`
Expected: each file under 100 words.

- [ ] **Step 6: Commit**

```bash
git add docs/05-backlog docs/06-templates
git commit -m "docs: add backlog roadmap and note templates"
```

---

### Task 6: Central index (MOC) and final verification

**Files:**
- Create: `docs/00-system/00-index.md`

**Interfaces:** Links every note created in Tasks 1–5 by filename.

- [ ] **Step 1: Create `docs/00-system/00-index.md`**

```markdown
# Index

Central map of this vault. Start here.

## Sections

- [[glossary]] — terms used across this vault
- [[mcp-protocol-spec]] — MCP tool contract
- [[001-sqlite-storage]] — ADR: why SQLite
- [[ccm-mcp-server]] — MCP server crate
- [[overview]] — mandatory parser contract every `ccm-lang-*` crate follows
- [[ccm-lang-php]] — PHP parser exception (security-relevant)
- [[limits-spec]] — recursion/fuzzing/benchmark limits
- [[sec-001-php-stack-overflow]] — security advisory
- [[roadmap]] — backlog

## Extension → Crate map

| Extension | Language | Crate |
|---|---|---|
| .rs | Rust | `ccm-lang-rust` |
| .py | Python | `ccm-lang-python` |
| .js/.jsx/.mjs/.cjs/.ts/.mts/.cts/.tsx | JS/TS | `ccm-lang-js-ts` |
| .java | Java | `ccm-lang-java` |
| .cs | C# | `ccm-lang-csharp` |
| .kt | Kotlin | `ccm-lang-kotlin` |
| .cpp/.cc/.cxx/.hpp/.hh/.h | C++ | `ccm-lang-cpp` |
| .go | Go | `ccm-lang-go` |
| .html | HTML | `ccm-lang-html` |
| .css | CSS | `ccm-lang-css` |
| .xml | XML | `ccm-lang-xml` |
| .xaml | XAML | `ccm-lang-xaml` |
| .sh | Bash | `ccm-lang-bash` |
| .ps1/.psm1 | PowerShell | `ccm-lang-powershell` |
| .php | PHP | `ccm-lang-php` |
| .md | Markdown | `ccm-lang-md` |
| .lua | Lua (acceptance-test only) | `ccm-lang-lua` |

Adding a language does not need a new note here beyond a row in this table — see [[lang-spec-template]] for the one exception.

#moc
```

- [ ] **Step 2: Verify every WikiLink target in `00-index.md` resolves to a real file in the vault**

Run:
```bash
for link in glossary mcp-protocol-spec 001-sqlite-storage ccm-mcp-server overview ccm-lang-php limits-spec sec-001-php-stack-overflow roadmap lang-spec-template; do
  found=$(find docs -name "${link}.md" | wc -l)
  echo "$link: $found"
done
```
Expected: every line ends in `: 1`.

- [ ] **Step 3: Verify the full vault structure**

Run: `find docs/00-system docs/01-architecture docs/02-crates docs/03-performance docs/04-security docs/05-backlog docs/06-templates -type f | sort`

Expected (13 files):
```
docs/00-system/00-index.md
docs/00-system/glossary.md
docs/01-architecture/adrs/001-sqlite-storage.md
docs/01-architecture/mcp-protocol-spec.md
docs/02-crates/core/ccm-mcp-server.md
docs/02-crates/parsers/ccm-lang-php.md
docs/02-crates/parsers/overview.md
docs/03-performance/limits-spec.md
docs/04-security/advisories/sec-001-php-stack-overflow.md
docs/05-backlog/roadmap.md
docs/06-templates/adr-template.md
docs/06-templates/crate-spec-template.md
docs/06-templates/lang-spec-template.md
```

- [ ] **Step 4: Commit**

```bash
git add docs/00-system/00-index.md
git commit -m "docs: add central MOC linking the full vault"
```
