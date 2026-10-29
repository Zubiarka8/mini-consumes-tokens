# React Review Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the open-finding views of the local dashboard (duplicate comparison, dead-code code view, review inspector) with a compact, resizable React workspace, served from the Rust binary with no Node at run time.

**Architecture:** The dashboard stays a Rust-served, same-origin, dependency-free shell of plain ES modules (`crates/mct-cli/src/web/app/`). One React "island" (`crates/mct-cli/web-ui/`, built by Vite into `crates/mct-cli/src/web/review-dist/`) is loaded with a dynamic `import()` only when a finding is opened. The vanilla findings page keeps the list, filters, presets, tabs and URL state, and hands the island a small bridge (`t`, API calls, navigation callbacks). Rust remains the source of truth for findings, numbers and reviews.

**Tech Stack:** Rust (existing `web` module), React 18 + TypeScript, Vite, Tailwind CSS v4 (theme + utilities only, no preflight), shadcn/ui-style Button/Badge (cva + clsx + tailwind-merge, no Radix), lucide-react, motion, react-resizable-panels, sonner, Vitest + Testing Library + jsdom.

**Spec:** the user's request of 2026-10-29 (tasks 1–9), checked against the code of `feature/localhost-ui-improvements` including its uncommitted work.

## Findings that change the request's premises

- The request's file map describes an older branch (`spa/` with Alpine.js, `view_*.rs`, `render.rs`, `store.rs`, `member_reviews.rs`, `build-dashboard-assets.sh`, ADR-018). None exists here. The current dashboard is a no-build ES-module SPA in `src/web/app/`, served by `assets.rs` with `include_str!`; JSON comes from `api.rs` and `api/*.rs`; reviews go through `reviews.rs` over `mct_index::reviews`; ADR-014 is the relevant record.
- Readable numbers (task 4) already exist in the uncommitted work: `src/web/numbers.rs` (`.mct-index/finding-numbers.tsv`, never reused, keyed by review identity) with the test `finding_numbers_are_readable_stable_and_never_reused`.
- Review decisions are per finding: one decision for a whole duplicate group (ADR-015/019). There are no per-file decisions, so the panel shows the selected file as context and does not invent per-file progress.
- The detector already computes aligned differences per group (`GroupEvidence.differences`: kind, per-member value and line), but the detail API does not expose them; the current comparison colours lines with a client-side LCS instead.

## Global Constraints

- No Chrome or Chrome-based automation; browser checks in Safari.
- No remote fonts, icons, scripts, styles or services at run time; CSP stays `script-src 'self'; style-src 'self'` (no injected `<style>` elements).
- Keep localhost Host/Origin/CSRF validation, request limits and file-access limits unchanged.
- Keep existing routes, languages (es/en), themes (light/dark/system), data and reviews.
- `cargo build` must not need Node: the compiled bundle is committed.
- No commits, pushes or PR changes in this session.
- Repository content in English.

## Review Focus

- A finding that disappears or changes after a reindex while its workspace is open: the save must return 409/404 and the island must keep the typed note.
- A failed save (503 read-only, 500, network): the note and decision stay in the form and can be resent.
- Switching files or findings with an unsaved note: the draft is kept per finding and restored.
- Very long lines and many members: columns scroll horizontally; no control is clipped at 360 px wide.
- Reduced motion: no animation delays any action.

---

### Task 1: Expose detector differences in the duplicate detail

**Files:**
- Modify: `crates/mct-cli/src/web/api/findings.rs` (`detail`)
- Test: `crates/mct-cli/src/web/tests.rs`

**Interfaces:**
- Produces: `api/finding` for a duplicate adds `evidence: { identifierPattern: string, distinct: number, differences: [{ kind: string, values: string[], lines: number[] }], warnings: string[] }`, with `values`/`lines` in member order.

- [ ] Test `duplicate_detail_lists_the_detector_differences`: in a project whose two copies differ by one literal, `differences[0].kind == "literal-value"`, `lines.len() == members.len()`, and each line lies inside its member's range.
- [ ] Implement by serialising `group.evidence` (existing `as_str()` names).
- [ ] `cargo test -p mct-cli web::tests::duplicate_detail` passes.

### Task 2: Frontend package, build and serving

**Files:**
- Create: `crates/mct-cli/web-ui/{package.json,package-lock.json,vite.config.ts,tsconfig.json,src/main.tsx,src/styles.css}`
- Create: `crates/mct-cli/src/web/review-dist/{review.js,review.css}` (generated, committed)
- Create: `scripts/unix/build-dashboard.sh`, `scripts/windows/build-dashboard.ps1`
- Modify: `crates/mct-cli/src/web/assets.rs`, `.gitattributes`
- Test: `crates/mct-cli/src/web/tests.rs`

**Interfaces:**
- Produces: `GET /js/review.js` and `GET /review.css`; `review.js` exports `mountReview(host: HTMLElement, props: ReviewProps): { update(props: ReviewProps): void; unmount(): void }`.

- [ ] Test `review_bundle_is_served_same_origin`: both paths answer 200 with JS/CSS types and the bundle contains no `http://`/`https://` URL loads (`import(`/`src=`) and no `<style` injection.
- [ ] Vite: entry `src/main.tsx`, fixed names `review.js`/`review.css`, one chunk, `outDir ../src/web/review-dist`, production `NODE_ENV`.
- [ ] Tailwind v4: import `theme.css` and `utilities.css` only (no preflight, unlayered so app.css element rules cannot override utilities); colours map to the existing CSS variables, so themes need no new tokens.
- [ ] Scripts run `npm ci`, `npm test`, `npm run build`; the same flags exist in both OS folders.

### Task 3: Bridge from the vanilla findings page

**Files:**
- Modify: `crates/mct-cli/src/web/app/page-findings.js`
- Modify: `crates/mct-cli/tests/dashboard/harness.mjs`, `interactions.test.mjs`
- Create: `crates/mct-cli/tests/dashboard/review-stub.mjs`

**Interfaces:**
- Consumes: `mountReview` (Task 2).
- `ReviewProps = { item, file: number, context: number, lang, t(key, vars), source(params, key), save(key, state, note, revision), openSource(args), confirm(args), onBack(), onSelectFile(i), onContext(n), onSaved(review), drafts: Map<string, Draft> }`.

- [ ] Node tests: opening `n=7` mounts the island once with `item.number == 7`; changing `file` calls `update` without a new `api/finding` request; Back clears `key/n/view/file/context`; closing the page unmounts.
- [ ] Remove the vanilla detail renderers that the island replaces (inspector, comparison, dead-code view) only once these tests pass; keep `findingNumber`, `findingHref`, `ignoreFinding` (used by the list and the workspace page).

### Task 4: Duplicate workspace (files | comparison | review)

**Files:** `crates/mct-cli/web-ui/src/{DuplicateWorkspace,CodeView,Workspace}.tsx` + tests.

- [ ] Vitest: file list marks the active file (`aria-current`), the selected column (`aria-pressed`) and the review header; lines named by `evidence.differences[].lines[i]` carry a text marker and a title listing kind and values; copy/open act on the selected member; columns are in a horizontally scrollable region.
- [ ] Code viewer: the existing tokenizer ported to React (no CodeMirror: read-only fragments of at most 400 lines, all languages already covered, no editing needed).

### Task 5: Dead-code workspace (code | review)

- [ ] Vitest: context select requests `start = max(1, line - n)`; symbol lines marked with a non-colour marker; copy path/code and open source; reasons, references (0 known), imports and notes in collapsible sections; the "zero indexed references is not proof" notice is always visible.

### Task 6: Review panel

- [ ] Vitest: Save is disabled until a decision is chosen; no success toast before the server answers 200; on failure the decision and note remain and a Retry button resends the same payload; 409 shows the server state and keeps the typed note; drafts survive switching files and findings; read-only shows the reason and no form; Ignore keeps working.

### Task 7: Layout, motion, accessibility

- [ ] Panels resize with mouse and keyboard (arrow keys on the handle) with minimum sizes; below 900 px the panes stack vertically with no resize handles.
- [ ] `MotionConfig reducedMotion="user"`; animations ≤ 150 ms and never gate a click.
- [ ] Safari check in light and dark themes, at 1440 px and 390 px.

### Task 8: Documentation

- [ ] New ADR `docs/01-architecture/adrs/025-react-review-workspace.md`; update ADR-014 and `crates/mct-cli/web-ui/README.md` (build, checks, dependencies, limitations, numbering on reindex).
