# Duplicate group counts and presentation review

Date: 2026-11-08. Base: `main` at `589d1cc`. Branch:
`fix/duplicate-groups-counts-and-grouping`. No commits, pushes or PR publication.

## Executive summary

The detector's distinct-file count was correct. Its dashboard presentation hid
member counts, silently shortened titles and used fixed plurals. The correction
preserves detector membership, ranking and review identity. Counts now come from
the server in the list, comparison workspace and AI brief. User-facing list
identifiers are English words, with original identifiers retained for lookup.

Risk: LOW for presentation, MEDIUM for the additive dashboard response contract.
No high/critical security regression identified in the changed paths. Remaining
pre-existing limits are listed below; this is not a repository-wide security audit.

## Reproduction and evidence-ranked hypotheses

Started the installed binary with `mct-cli --root . web --port 0 --no-obsidian`,
then queried `GET /api/findings?type=duplicate&size=200` and
`GET /api/finding?key=<stable-key>` against localhost. The initial main snapshot
had 94 groups; after source edits the live index reported 95. Group totals can
change with source input; no detector policy changed. The initial request used
`limit=100`, which is not a findings parameter; the API correctly returned its
default size 25. Subsequent complete requests use `size=200`.

| Group | Members (path, inclusive range, identifier) | memberCount | fileCount | scope |
|---|---|---:|---:|---|
| #1 | `crates/mct-mcp-server/src/server.rs:2343-2370 find_calls`; same file `2374-2401 find_callers` | 2 | 1 | same-file |
| #3 | `crates/mct-index/src/lib.rs:970-985 find_calls_bfs_scoped`; same file `1002-1017 find_callers_bfs_scoped`; same file `1035-1050 find_references_bfs_scoped` | 3 | 1 | same-file |
| #6 | `crates/mct-index/src/duplicates.rs:601-608 as_str`, same file `880-887 as_str`; `crates/mct-index/src/queries.rs:67-74 as_str`; `crates/mct-index/src/reviews/mod.rs:110-117 value`; `crates/mct-lang-rust/tests/corpus/project/errors.rs:129-136 tag` | 5 | 4 | cross-file |

1. Strongest: presentation conflates locations and distinct files; actual #1,
   #3 and #6 member arrays prove the distinction. Corrected labels retain both.
2. Strongest: title `.take(2)` hides names; failing API tests reproduce it,
   including the independently duplicated rule in recent activity.
3. Strongest: fixed plurals print `1 files` / `1 archivos`; failing DOM tests
   reproduce both. The comparison metadata and group state menu had variants.
4. Unsupported as a detector bug: nested overlap. The outermost selection at
   analysis.rs:291 belongs to partial-block streams. Generated real-parser
   examples and the real HTTP inventory found no within-group overlap.
5. Unsupported as a count defect: #6 really has four distinct files.

## Source orientation and consumers

Locations were obtained with explicit-root `mct-cli call --tool batch` queries
using search_symbols_like, get_file_skeleton, build_context_pack and relations.
The connected MCP index described another root; implementation decisions use the
explicit-root CLI, which incrementally refreshes before queries.

- `crates/mct-index/src/duplicates/analysis.rs:40`: orchestration;
  `:291`: outermost partial-block streams; `:365`: file analysis;
  `:406`: candidate fingerprint; `:504`: whole-function grouping;
  `:822`: deterministic path/range ordering.
- `crates/mct-index/src/duplicates/analysis/file.rs:52`: safe file loading.
  The supplied map omitted the `analysis/` directory; corrected skeleton lookup
  succeeded. Other analysis modules are blocks, fingerprint, subgroups,
  evidence and verdict under the same directory.
- `crates/mct-cli/src/web/api/findings.rs:91`: shared bounded raw title;
  `:106`: duplicate row; `:227`: group list; `:291`: detail (line numbers can
  shift with edits; the symbol names are authoritative).
- `crates/mct-cli/src/web/api/code.rs:265`: symbol inspector consumes rows;
  findings list and detail are the other two direct duplicate_row references.
- `crates/mct-cli/src/web/api/overview.rs:123`: recent activity title consumer.
- `crates/mct-cli/src/web/app/page-findings.js:139`: count labels;
  `:152`: readable member names; list/table/review renderers consume counts,
  titles and scope. `core.js:300` formats identifier words without translation.
- `crates/mct-cli/src/web/app/ai.js:158`: complete-group brief;
  `:295`: preview-aware copied list. API title/identifiers remain raw for export.
- `crates/mct-cli/web-ui/src/types.ts:56`: typed comparison contract;
  `DuplicateWorkspace.tsx:239`: detail workspace and status bar.
- `crates/mct-cli/src/duplicates.rs:355`: report cache key; `:446`:
  CLI report renderer. CLI consumes DuplicateGroup, not dashboard JSON fields.
- `crates/mct-obsidian/src/lib.rs`: atlas generated from index navigation;
  no duplicate-row JSON consumer was found. `page-stats.js:27` exports index
  and latency measurements, not preview-derived duplicate counts.

Object-key string access is not exhaustively represented by find_references:
queries for fileCount/memberCount returned no graph references. Targeted reads
of the already located implementation and consumer types completed that check.

## API and implementation decisions

The API design is documented in `docs/01-architecture/web-dashboard.md`.
`membersOmitted` is additive; members stays a six-entry list preview and the
existing detail still returns all members. No MCP signature or SQLite schema
changed. The user-level mct-backend-batch skill was read from
`~/.claude/skills/mct-backend-batch/SKILL.md`; its group-integrity and bounded-work
principles apply, but it does not require a new MCP batch handler for this
HTTP-only correction. api-and-interface-design remains the primary contract skill.

Root files now share the empty directory component instead of each filename
being mistaken for a module. Existing enum values are preserved; UI labels use
Same file, Several files and Several folders. Counts and exact directory
semantics are documented. Title expansion intentionally affects explicit title
sorting, while default rank and stable keys remain unchanged.

## Original defect triage and variants

| Item | Status | Evidence / decision |
|---|---|---|
| 1. File count vs occurrences | FIXED presentation; count defect REJECTED | #1 2/1, #3 3/1, #6 5/4; both counts shown |
| 2. Singular/plural | FIXED | ES/EN failing then passing DOM tests; React singular test |
| 3. Two-name title cutoff | FIXED | six names plus explicit +N; full members in detail |
| 4. Scope invisible / misleading module name | FIXED | visible simple labels, exact directory semantics documented |
| 5. Six-member preview loses members | REJECTED as data loss | omission notice existed; additive explicit count; full-detail copy test with nine members |
| 6. Wrappers incorrectly treated as real | REJECTED for reported groups | all three are SUSPICIOUS already; #1 reasons include undeclared names, type names, literals and decorators; keep policy |
| 7. Nested overlap / double counting | NOT REPRODUCED | 21 generated cases; 95 real groups / 239 members / zero overlapping line ranges |
| 8. IDs/order/reviews/cache | no new defect confirmed | key/number/state survive reindex in regression test; existing ordering/paging/cache tests pass; cache key hashes current bytes/options/generation |
| New: recent activity cutoff | FIXED | regression test fails with two-name activity title, then passes using shared formatter |
| New: root files called different modules | FIXED | root-file/cross-directory HTTP regression |
| New: state menu labels members as files | FIXED | failing then passing menu test; location wording |
| New: comparator reconstructs counts/fixed file plural | FIXED | server counts required in internal type; four failing React assertions then green |
| New: readability of identifiers | FIXED | English word labels, acronyms preserved, duplicate labels consolidated, originals in tooltips |
| Existing: unbounded detail metadata/repeated bodies | OPEN | detail returns every member; no total byte/member cap |
| Existing: malformed numbering file warning | OBSERVED, not repaired | initial web startup: `line 3 is not a valid row`; no manual rewrite of local finding numbers |

Variant sweep inspected dead-code row and review displays, dependency table
counts, grouped search totals and statistics exports. A search group retains
`total` when `.take(limit)` truncates it; dependency counts refer to file nodes;
statistics exports are complete measurement rows. This was a focused sweep,
not an exhaustive proof that all translation keys in the repository pluralize
correctly. No unrelated cleanup was made.

## Security and differential review

Repository size: LARGE; surgical review of changed producer/consumer paths.
Base history: row/scope logic from `91f27e62`; count/title set-based presentation
from `c3179f44`; whole-group fileCount repair from `3071f3d8` is preserved.
Direct blast radius: duplicate_row has three indexed consumers; duplicate_title
has two; symbolLabel has five production call sites plus its test.

Trust boundary: indexed repository paths/names/source → localhost API → DOM or
copied evidence. Existing canonical-root containment, content hashes, bounded
repository reads, CSP, Host/Origin checks and CSRF remain. h() creates text nodes;
no HTML or command execution is introduced. FileFailure and HTTP errors retain
their existing generic diagnostics. Neither auth nor destructive paths changed.

No security validation was removed. The path-order invariant used to count
adjacent distinct paths is documented by DuplicateGroup and build_group; 256
path partitions check it against an independent set oracle. Complete detail
metadata remains unbounded (pre-existing availability risk); a separate bounded
member-retrieval contract would be needed to fix it without dropping AI evidence.

Tests cover title omissions, exact counts, same-file groups, root directories,
review persistence, full copy recovery, generated nesting and identifier labels.
The generated minified React artifact was rebuilt by the existing TypeScript /
Vite pipeline and reviewed with its source; it is not hand edited. No dependency
or lockfile changed. `npm ci --ignore-scripts` installed the already locked
React test/build dependencies; its audit reported zero vulnerabilities.

## Performance ledger

Command: `cargo test -p mct-cli --bin mct-cli measure_large_duplicate_rows -- --ignored --nocapture`.
Debug profile, 100 groups × 1,000 members, 11 HTTP list samples; timings include
row building, filtering, facets and response serialization, not isolated
CPU time of duplicate_row.

| Step | Median | Min–max | Decision |
|---|---:|---|---|
| Main implementation | 24,273 µs | 24,181–27,616 | baseline |
| Presentation fix, one path set | 19,148 µs | 19,089–19,947 | correctness change |
| Adjacent path counting | 13,865 µs | 13,821–14,679 | keep; no set allocations per row |
| Final confirmation | 13,241 µs | 13,107–15,754 | confirms lower cost in this synthetic case |

No production/RUM latency claim; no extra cache or dependency introduced.

## Verification

| Command | Result |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo clippy -p mct-cli -p mct-index --all-targets -- -D warnings` | PASS |
| `cargo test -p mct-cli -p mct-index` | PASS; 702 reported passes, zero failures, 10 ignored measurement/platform cases |
| `cargo test -p mct-cli --bin mct-cli web::tests::` after final CSS | PASS; 46 passed, one manual timing test ignored |
| `npm test` in `crates/mct-cli/tests/dashboard` | PASS; 52 passed, zero skipped |
| `npm test` in `crates/mct-cli/web-ui` | PASS; 36 passed; jsdom prints its existing scrollTo not-implemented diagnostic |
| `npm run build` in `crates/mct-cli/web-ui` | PASS; TypeScript and Vite |
| `cargo build -p mct-cli` | PASS; dashboard relaunched with target/debug/mct-cli |
| Manual timing test | PASS; explicitly executed despite default ignore marker |
| `git diff --check` | PASS |

Desktop visual verification used Safari/WebKit at requested width 1440 (effective
content height reported by the tool was 819). #1, #3 and #6 show readable English
labels, exact counts, visible scope, review menu and AI action. #3 opens all three
members and returns to the same filtered list; no review was saved on the real
project. Focus exposes the locations panel; Escape closes it without moving focus.
The restart briefly produced two connection/fetch console errors while the server was down; stable-page console verification is recorded separately below. The final browser remains on the desktop list at
`http://localhost:63583/#/duplicados?state=all`.

Earlier exploratory checks included narrower widths. The user subsequently
excluded mobile work; final acceptance and screenshots focus on desktop. No
Chrome tool or Chromium automation was used. Exact 320px rendering was not proven
because WebKit reported a minimum effective innerWidth of 336 during that check.

Final screenshot during the session: `/tmp/mct-duplicate-groups-desktop-final.png`.

## PR draft

Title: `fix(dashboard): distinguish duplicate locations and display readable group names`

### Summary

```text
duplicate group
  → server counts: locations + distinct files
  → bounded title / explicit preview omissions
  → English word labels + locations panel
  → full detail → comparison / AI template
```

Show every previewed identifier, collapse repeated visual labels, correct
singular files and describe directory scope plainly. Preserve detector groups,
review keys, saved decisions and exact identifiers in exported evidence.

### Evidence

- **Before:** failing API tests show `alpha ≈ beta` for three members; ES/EN
  DOM tests show `1 archivos` / `1 files`; recent activity has its own cutoff.
- **After:** requested Rust/JS gates pass, 36 React tests pass, real desktop
  groups #1/#3/#6 show 2/1, 3/1 and 5/4 location/file counts. Generated tests
  cover 21 nested layouts and all 256 path partitions for eight locations.

### Merge Danger

**Door:** two-way. No schema or review-file migration.

**Blast Radius:** dashboard.

The existing detail response still scales with group size. Explicit title sorts
can change because titles now describe more members. The internal React type
requires server counts already present in the API. No PR has been opened.


Stable-page follow-up after the server was ready and the page reloaded: Safari console warn/error query returned `[]`. Transient restart errors did not recur.
