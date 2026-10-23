# Dashboard audit remediation

Status: implementation authorized by the maintainer; acceptance remains evidence-based.
Base: `e707e20f82598e40303f1b2ba4c67b8a8ed36c50` (`feature/code-page-quick-filters`).
Owner: Codex, single sequential writer on `codex/dashboard-audit-remediation`.

## Problem and verified behavior

The 2026-10-23 local dashboard audit reproduces independent same-file comparison
requests cancelling each other, duplicate file filters considering only the firs
member, and mobile symbol details opening below the result list without focus.
Pre-edit source reads confirm these mechanisms in core.js, page-findings.js,
api/findings.rs and page-symbols.js. The connected MCP describes another checkout;
its existing parse diagnostics are not evidence about this candidate. Direc
frontend/server reading was explicitly authorized during the audit; pre-edit reads
are also permitted by rules.md. No external database access is used.

## Capability map and order

| Module | Responsibility | Dependencies |
| --- | --- | --- |
| comparison | Request ownership, pair choice, retry/back | shared API client |
| findings | Complete member-path matching and stable identity | existing snapshot/reviews |
| detail-access | Responsive symbol dialog and focus restoration | shared DOM/dialog runtime |
| consistency | Query synchronization, backlinks, navigation, layout, contrast, locale | critical modules |
| performance | Reproducible graph/search/filter investigation | correctness tests |

Implement critical modules first, then consistency, then measured performance.
No parallel writers, schema changes, MCP contract changes or framework migration.

## Requirements and scenarios

1. P0.1: each comparison side owns a distinct cancellable request; pair switches,
   leaving comparison and teardown cancel obsolete work. Aborts never become an
   error screen. Failures retain Back, pair selectors and retry for the same pair.
2. P0.2: exact-file and component-boundary directory matching considers every
   original group member, including beyond the six-item preview. Keep counts,
   facets, ordering, paging and identity correct. Filter once per request.
3. P0.3: selecting a symbol at narrow/tablet widths opens an accessible native
   modal dialog with an accessible name, focus inside, Escape/Close and focus
   restoration to the originating row. Desktop keeps its inspector. Dismissal
   clears selection; route cleanup closes overlays without mutating a new route.
4. P1 UI-01/02: ordinary content fits 320, 375, 768 and 1440 CSS pixels; global
   search remains readable. Tables and graphs may scroll in their own containers.
5. P1 UI-03: recent activity carries its authoritative key, including findings
   excluded by default filters; missing historical findings are clearly unlinked.
6. P1 UI-04: URL, search input, chips and results agree after clear, presets and
   navigation. Cancel pending debounced input when synchronizing or unmounting.
7. P1 UI-05/06: every outbound/backlink can be enumerated; Show More retains
   keyboard focus. Obsidian active navigation always reflects the current view.
8. P1 UI-07: arbitrary distinct comparison members can be selected, including
   first/fifteenth and same-file pairs; pair indices survive retry in the URL.
9. P1 UI-08: informative normal text >=4.5:1, large text >=3:1 and essential
   non-text controls/focus >=3:1 against actual backgrounds, both themes. Reference:
   https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html
10. P1 UI-09/10: server supplies the canonical review-file path; statistics use
    the current locale for decimal/grouping conventions.
11. P2: use existing disclosures/table/focus/collapse controls where sufficient;
    avoid new preferences. Preserve desktop workflows and keyboard operation.
12. P3: benchmark before optimizing. Reduced motion must not compute all graph
    ticks synchronously. Preserve exact retrieval, ordering and access controls.

## Technical design and affected files

- `web/app/core.js`: explicit request cancellation and ownership cleanup; shared
  locale formatter and responsive detail-dialog lifecycle.
- `web/app/page-findings.js`, `code.js`: side-owned source requests, generation
  guard, persistent controls and URL pair selectors.
- `web/api/findings.rs`: filter original group members, not truncated JSON.
- `web/app/page-symbols.js`: move the existing inspector into a native dialog a
  <=1280px and return it on close/resize; suppress obsolete responses.
- `web/app/components.js`: explicit search synchronization/disposal and link paging.
- `web/app/page-notes.js`: enumerable links, navigation redraw, graph disposal.
- `web/api/overview.rs`, `page-dashboard.js`, `page-settings.js`: authoritative
  identity/configuration. `app.css`, `page-stats.js`, locale dictionaries: P1.
- `web/app/graph.js`: retain SVG and graph semantics while yielding pairwise
  force calculations to bounded animation-frame work; cap visible labels at 25.
All paths above are relative to `crates/mct-cli/src/`.

## Risks and compatibility

Retain URL keys and additive HTTP API fields. Pair indices are snapshot-local;
validate bounds and distinctness. Native dialog prevents background interaction;
resize and route teardown must restore DOM ownership without stealing focus.
Review storage/CSRF/capability checks and heuristic detection remain unchanged.
Do not operate on the running user's review data. Use disposable test fixtures.
No source-file cache without an invalidation design and measured need.

## Validation and Definition of Done

Rust wire-level regressions: cross-file groups beyond six members, directory
boundaries, totals/facets/pages and recent keys/meta paths. JavaScript interaction
regressions: same-file comparison, stale request cancellation/retry, mobile focus,
Escape/Close/background isolation, query clearing/debounce, pair selection,
backlinks/navigation, locale and rendered contrast. Run in a non-Chrome engine
(WebKit or Firefox) as available; no screen-reader certification is implied.
Run `cargo fmt --all -- --check`, workspace tests and strict workspace Clippy;
frontend syntax checks and embedding production build. Record failures honestly.
Benchmark deterministic graphs at 100,600,1347,5000 nodes, both motion modes;
record environment, tasks, frame time and limitations. Do not invent network or
heap measurements. Completion requires all P0 regressions passing, each P1 either
verified or explicitly outstanding, documentation synchronized, reviewed scoped
diff and no unrelated changes. No commit/push/merge/PR without authorization.

## Documentation inventory

Existing documents, not proposed replacements:

| Path | Purpose / audience | Update trigger / relationship |
| --- | --- | --- |
| rules.md, AGENTS.md, PROJECT_MANAGER.md | Contributor rules and coordination | Workflow changes; AGENTS points to rules |
| README.md, COMMANDS.md, CONTRIBUTING.md | User entry, CLI, contribution | Public behavior or contributor commands |
| .github/workflows/ci.yml | CI maintainers | Required checks change; agent directory excluded from reads |
| docs/01-architecture/web-dashboard.md | Dashboard maintainers | Architecture, acceptance and test commands |
| docs/01-architecture/adrs/019-shared-review-decisions.md | Architecture reviewers | Review storage contract changes |
| docs/superpowers/specs/2026-10-22-shared-review-decisions-design.md | Implementers | Review API/store changes; no redesign here |
| crates/mct-cli/src/web/tests.rs | Maintainers; executable acceptance | Wire/API/security behavior changes |
| crates/mct-cli/examples/ | Benchmark/tool examples | No dashboard-specific Markdown found |
| benchmarks/review-decisions.md | Performance/review maintainers | Reproduced review measurements |
| website/package.json, website/tests/ | Public Next.js site maintainers | Separate product; reuse installed test tooling only |
| .agents/skills/spec-driven-development/SKILL.md | Local agent SDD workflow | Skill changes; no global skills used |
| project-management/ | Shared historical coordination | Canonical personal state remains outside tracked docs |

This specification is new. A single companion remediation report will hold actual
results and outstanding findings. Existing API tests cover shape/bounds/security
and review persistence, but do not execute frontend focus/cancellation/layout.
The dashboard has no frontend bundler, linter or TypeScript build; ES modules are
embedded by Rust `include_str!`. The separate website uses Next.js/TypeScript and
Playwright; it is not the dashboard. Rust workspace contains mct-core, mct-index,
mct-cli, mct-mcp-server, mct-languages, mct-obsidian and language/parser/eval crates.
