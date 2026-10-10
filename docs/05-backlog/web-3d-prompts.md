# Web / 3D portfolio prompts (Claude Opus 5.5)

Prompts for the checklist in [roadmap](roadmap.md#web--3d-portfolio-support-by-priority).
Written against Anthropic's [Opus 5.5 prompting guide](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5-5)
and [general best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices).

## Why a foundation phase first

The seven features share decisions that live in `mct-core`, the SQLite schema, and the MCP
tool contract. These are the cross-cutting areas rules.md says need an issue before code.
Deciding them once, up front, avoids seven sessions each changing the contract their own way.
Facts below were checked with the project's MCP tools on 2026-10-10.

| Fact in the code today | Consequence |
|---|---|
| `SourceFile.contents` is `String`; non-UTF-8 files are dropped before any parser (`a_file_turned_binary_drops_its_symbols`) | `.glb`, images and HDR cannot reach a `LanguageParser` without a trait change |
| `files` rows exist only for parsed files | An asset path has no node to point at, so a relation to it cannot resolve |
| `RelationKind` = Calls, Imports, Extends, Implements, References. `SymbolKind` has no asset, model-node, or dependency kind. Both are stored as TEXT (`symbol_kind_str`/`relation_kind_str` in `mct-index/src/indexer.rs`) | New kinds need no SQL migration, but they are still a core contract change visible to MCP clients |
| `RelationTarget` already carries `language`, `path`, `kind`, `target_module`, `external`; HTML `class` → CSS `Rule` uses it | Cross-language and by-path links (JSX → CSS, code → asset) can reuse existing machinery |
| `package.json` is already parsed by `mct-index/src/manifests.rs` (not a `LanguageParser`) into a `dependencies` table; MCP only shows counts in `get_indexing_status` | P3 is mostly an MCP-surface question, not a new parser |
| `tsx_component_logic_is_indexed_without_structuring_jsx` asserts JSX is **not** structured | P2 deliberately changes a tested contract; that test must be rewritten, not deleted |
| `build_context_pack` exists | P6 extends it; it is not a new tool |
| ADRs: `docs/01-architecture/adrs/` (next: 003). Parser specs: `docs/02-crates/parsers/`. Tool fixtures: `crates/mct-mcp-server/tests/fixtures/`. Eval: `crates/mct-eval/suite.json` + `baseline.json` | Where P0 writes its outputs |

## How to use

- One prompt per fresh session (`/clear` between them). Paste **Shared preamble + one task**.
- `/model opus`, `/effort medium` (the Opus 5.5 default, which matches Opus 5 at `high`).
  P3 can run at `low`. Raise effort only after a check fails twice. Never use `xhigh`/`max`
  without a measured gain.
- Order: **P0 → (maintainer accepts ADR/issue) → P0b → P1 · P2 · P3 · P4 → P5 → P6 · P7**.
  P1–P4 are independent of each other after P0b. P5 needs P2+P4. P6 needs P1–P5. P7 needs P2.
- After P0, every later prompt implements ADR-003 instead of making design decisions on its own.

Guide → prompt choice:

| Opus 5.5 guidance | Applied as |
|---|---|
| Context and the *why* beat bare commands | `<goal>` states the portfolio use case |
| XML tags separate instructions from data | `<context>`, `<task>`, `<acceptance>`, `<report>` |
| Opus verifies on its own; "double-check" causes over-verification | Checks appear once, as acceptance criteria |
| Opus widens scope | Scope sentence + `<out_of_scope>` |
| Opus over-delegates to subagents | Subagent cap |
| Opus 5.5 can end a turn with a summary that only announces the next step | Allowed stops are named |
| Asking for written-out reasoning triggers `reasoning_extraction` refusals | Report asks for actions, not reasoning |
| Avoid passing tests by hard-coding | Generality sentence |
| Investigate before answering | MCP index tools first (dogfooding) |

---

## Shared preamble

```text
<context>
Repository: mini-consumes-tokens, a Rust MCP server that indexes source code into a
SQLite symbol graph so agents query symbols instead of reading whole files. Goal of this
work: support an Astro + React Three Fiber + Tailwind 3D portfolio with fewer tokens.
CLAUDE.md and rules.md are loaded; follow them. For this work in particular:
- Explore this repo's source with the mct MCP tools only (rules.md › Dogfooding).
  Read/Edit are fine for files you are about to change.
- ADR-003 (docs/01-architecture/adrs/003-*.md), once it exists, is the design authority
  for this work. Implement it; do not redesign it. If it is wrong or silent on something
  you need, stop and say what decision is missing.
- LanguageParser trait, SQLite schema, SymbolKind/RelationKind, or a published MCP tool
  signature: change them only as ADR-003 specifies.
- No unwrap/expect/panic on paths that process repo input; this runs on untrusted files.
- Repository content in English. Agent/script output in TOON, not JSON.
</context>

<working_style>
Deliver what is asked, at the intended scope. Make routine judgment calls yourself. If a
better approach exists, say so in one sentence and continue as asked. Keep it minimal: no
extra configurability, no helpers for one-off code, no refactors of code you are not
changing.

Write a general solution that works for all valid inputs, not only the fixtures. If a test
looks wrong, tell me instead of working around it. Never delete or weaken a test; when
this task deliberately changes a tested contract, rewrite that test to assert the new
contract and say so.

Work directly. Use at most one subagent, only for a genuinely independent wide
investigation, and never to re-check your own work.

Before your first tool call, say in one sentence what you will do. While working, update
me only when you find something important or change direction. End the turn only when
the task is done, when a decision only I can make blocks all remaining work, or when
ADR-003 is missing a decision you need. Do not stop just to announce the next step; do it.

Local, reversible actions (edits, builds, tests) need no confirmation. Ask before
committing, pushing, opening issues/PRs, or deleting files you did not create.
</working_style>

<report>
First sentence: the outcome (done / blocked on <decision> / failed). Then: changed paths,
checks run with results, known limits, follow-ups. Summarize actions only; do not narrate
your reasoning.
</report>
```

---

## P0 — Foundation design (ADR-003 + issue draft)

Effort: `medium`. Design only: no production code.

```text
<goal>
Seven features are planned to make this indexer useful on an Astro + React Three Fiber
portfolio (docs/05-backlog/roadmap.md, "Web / 3D portfolio support"). They share core
decisions. Make those decisions once, so later sessions implement without touching the
core contract in seven different ways.
</goal>

<context>
Verified facts are in docs/05-backlog/web-3d-prompts.md, "Why a foundation phase first".
Re-check any fact you rely on with the MCP tools; the code may have moved.
</context>

<task>
Write docs/01-architecture/adrs/003-web-3d-indexing.md (template:
docs/06-templates/adr-template.md; style: ADR 001 and 002). For each decision below, give
the options, pick one, and give the reason in 1–3 lines:

D1 Asset ingestion. How do .glb/.gltf, images (png/jpg/webp/ktx2) and HDR/EXR enter the
   index, given that SourceFile.contents is String and non-UTF-8 files are dropped?
   Compare at least: (a) an mct-index pre-pass like manifests.rs (an assets table, plus
   extracting the .glb JSON chunk to feed a text parser); (b) changing SourceFile to bytes
   for every parser. Specify caps (file size, JSON chunk size, node count).
D2 Vocabulary. List what each of P1–P7 needs to store, then the smallest set of new
   SymbolKind/RelationKind values, if any. Reuse References + RelationTarget wherever the
   semantics hold. Specify kind strings exactly as stored.
D3 Asset resolution. How a string like "/models/robot.glb" or "./tex.png?url" resolves
   to a path: relative to the file; a leading "/" relative to public/; never outside the
   project root (reject "..", symlinks escaping root, absolute paths). Dynamic paths stay
   unresolved.
D4 Embedded languages (.astro now; .vue/.svelte later). Decide between a region splitter
   that reuses mct-lang-html/css/js-ts, with line offsets mapped back to the original
   file, and a dedicated tree-sitter-astro grammar. Say where the shared splitter lives
   (mct-tree-sitter?) and whether a lang crate may depend on another lang crate.
D5 MCP surface. Every published-tool change across P1–P7 in one table: tool, new param
   or output field, backward compatibility. Candidates: exposing indexed dependencies
   (P3), new context-pack roles (P6), the accessibility report (P7). Prefer extending
   existing tools over new ones (rules.md keeps the tool count small).
D6 Shared fixture. One fixture, crates/mct-mcp-server/tests/fixtures/portfolio-3d-app
   (Astro pages + R3F scene + Tailwind + package.json + configs + tiny .gltf/.glb +
   texture). Each phase extends it, and mct-eval suite cases are added per phase.
D7 Migration and reindex. Which changes need a schema migration and which need
   `reindex --force`, and how an older binary behaves on a newer index (see the
   CONNECTION_CLOSED note in rules.md).

Then:
1. Draft a GitHub issue (title + body) for the cross-cutting parts. Do not open it.
2. Update the P0b item in the roadmap checklist so it lists the core changes from
   D1/D2/D5/D7 to implement once the issue is accepted.
3. If any decision changes what a later prompt in docs/05-backlog/web-3d-prompts.md
   asks for, update that prompt so it matches the ADR.
</task>

<out_of_scope>
Implementing any feature or core change.
</out_of_scope>

<acceptance>
- Every D1–D7 has a decision, not "TBD". An open question is allowed only if it names who
  decides and what blocks on it.
- For each of P1–P7, the ADR states which crates it touches and whether it needs core
  changes beyond P0b. The answer should be "no" for all of them.
- `git diff --check` is clean and relative links resolve.
</acceptance>
```

## P0b — Foundation implementation

Effort: `medium`. Run only after the maintainer accepts ADR-003 and the issue.

```text
<goal>
Land the core changes ADR-003 specifies (D1 asset pre-pass, D2 kinds, D3 resolver, D4
mask, the D5 changes marked P0b, the D6 fixture base, D7 with no migration), so P1–P7
become additive work in their own crates.
</goal>

<task>
1. Implement exactly the core changes in ADR-003: the five new kinds and their kind
   strings, the asset pre-pass in mct-index and its caps, mct_core::resolve_reference_path
   (D3), mct_tree_sitter::mask (D4), and the D5 rows marked P0b
   (get_indexing_status.dependency, kind descriptions, no source for asset definitions).
   No schema migration.
2. Create the shared fixture from D6, with only the rows marked P0b, and the second
   mct-eval suite. Later phases extend both.
3. Update every exhaustive match on SymbolKind across crates.
</task>

<out_of_scope>
Feature parsing (P1–P7), including glTF JSON → symbols (P4).
</out_of_scope>

<acceptance>
- Upgrade test: an index written before this change opens without a migration and gains
  asset rows on a normal (not forced) reindex.
- Resolver tests: relative, "/" → <app root>/public/, "?url" suffix, ".." inside root
  (allowed) and escaping root (None), absolute and drive paths, schemes, bare specifiers.
  Index tests: symlink out of root and missing file stay unresolved.
- Ingestion tests: caps enforced; a truncated or oversized binary records an
  syntax_error index_issues row, keeps the asset symbol, and never panics. Add a fuzz
  target for the .glb chunk reader (new crates/mct-index/fuzz).
- scripts/unix/check.sh passes (fmt, clippy with CI flags, tests).
</acceptance>
```

## P1 — `.astro` files

Effort: `medium`. Needs P0b.

```text
<goal>
The portfolio is built with Astro. .astro files are not indexed today, so an agent has to
read them whole. Index them so "which components does this page use, what does it
import, which styles apply" is one query.
</goal>

<task>
Implement .astro support in a new crate, mct-lang-astro, as ADR-003 D4 specifies: parse
masked copies of the file with mct-lang-js-ts (frontmatter and <script>), mct-lang-css
(<style>) and tree-sitter-html (the template, with {…} expressions masked). Emit one
module per file named after the file stem. Record each template component tag as a
`calls` relation to the imported name (ADR-003 D2), with RelationTarget path/language
when the import names an extension. Line numbers must match the original file.
</task>

<out_of_scope>
Config files (P3), .tsx JSX (P2), class-to-CSS links (P7).
</out_of_scope>

<acceptance>
- Follow the "Adding a language" checklist (rules.md, CONTRIBUTING.md): tests, fuzz
  harness, README and internal/checklist.md tables, docs/02-crates/parsers spec.
- Tests: a page using two imported components, a layout with <slot/>, a frontmatter-only
  file, an expression-heavy template, a syntax error that returns ParseError.
- Extend the portfolio-3d-app fixture and add an mct-eval case.
- scripts/unix/check.sh passes.
</acceptance>
```

## P2 — JSX/TSX elements as relations

Effort: `medium`. Needs P0b.

```text
<goal>
The 3D scenes are React Three Fiber components. The index sees the code around JSX but
not the JSX tree, so "where is <Scene> used, and with which props" needs a full file
read. Make JSX usage queryable.
</goal>

<task>
1. In mct-lang-js-ts, record a `calls` relation (ADR-003 D2) from the enclosing
   function/component to each component tag (a capitalized tag or a member expression),
   located at the opening tag. Do not store props. Lowercase intrinsics (<div>, <mesh>)
   are not component references.
2. Rewrite tsx_component_logic_is_indexed_without_structuring_jsx so it asserts the new
   contract; keep its existing Calls assertions.
3. Check that find_references, find_callers, and impact_analysis return these usages
   (they should without tool changes, since the kind is `calls`).
</task>

<out_of_scope>
.astro (P1), asset paths in props (P5), classes (P7).
</out_of_scope>

<acceptance>
- Tests: nested components, fragments, member-expression tags, spread props,
  conditional rendering, a .jsx and a .tsx file, broken JSX without a panic.
- Extend the fixture. Update the mct-eval baseline and explain every diff.
- CHANGELOG says one `reindex --force` is needed (ADR-003 D7).
- scripts/unix/check.sh passes.
</acceptance>
```

## P3 — Web project configuration

Effort: `low`. Needs P0b.

```text
<goal>
Agents waste tokens reading package.json and config files to learn which libraries and
options a repo uses. Answer "is three/drei/tailwind installed, at which version, which
Astro integrations are on" from the index.
</goal>

<task>
1. Dependencies are already parsed (mct-index/src/manifests.rs, dependencies table), and
   P0b added get_indexing_status's `dependency` filter (ADR-003 D5). Test it on the
   fixture; add no MCP change.
2. Check that the JS/TS parser already extracts what an agent needs from
   astro/vite/tailwind config .ts/.mjs files. ADR-003 D2 lists nothing as missing, and
   tsconfig.json stays unindexed. If you find a real gap, stop and report it instead of
   adding it.
3. Confirm .env* files stay excluded and that secret-looking config values are never
   stored.
</task>

<acceptance>
- Tests: a realistic Astro + R3F + Tailwind package.json queried with `dependency`
  (found, with its version; not found), malformed JSON without a panic, a config
  containing a secret-looking value (not stored).
- scripts/unix/check.sh passes.
</acceptance>
```

## P4 — `.glb` / `.gltf` contents

Effort: `medium`. Needs P0b.

```text
<goal>
Agents can't see inside the portfolio's 3D models, so they guess the names in code like
nodes.Head or materials.Metal. Index scenes, nodes, meshes, materials, and animations so
code can be checked against the real names.
</goal>

<task>
In crates/mct-index/src/gltf.rs (ADR-003 D1: a data format next to manifests.rs, not a
LanguageParser), turn glTF JSON (a .gltf file, or the .glb JSON chunk that P0b's reader
returns) into symbols with the kinds from ADR-003 D2: scenes and the node hierarchy as
`model_node` (hierarchy in parent; a scene's parent is the asset name), `material`, and
`animation`. Meshes are not stored. Referenced image URIs become `references` relations
to the image asset, resolved with mct_core::resolve_reference_path. Never decode the
BIN chunk. Enforce the ADR caps: 10,000 symbols per file, depth MAX_TRAVERSAL_DEPTH,
and node cycles terminate.
</task>

<acceptance>
- Tests: a minimal .gltf, a .glb built inside the test, unnamed nodes, a deep hierarchy
  at the cap, a child cycle, a JSON chunk over the cap.
- A fuzz target for the glTF JSON path in crates/mct-index/fuzz. Complete the fixture's
  robot model. CHANGELOG says one `reindex --force` is needed.
- scripts/unix/check.sh passes.
</acceptance>
```

## P5 — Code → 3D asset relations

Effort: `medium`. Needs P2 + P4.

```text
<goal>
"Which component loads robot.glb, and which textures does that scene need?" should be
one query.
</goal>

<task>
1. In mct-lang-js-ts, detect string-literal asset paths in useGLTF, useTexture,
   useEnvironment, useFBX, useLoader(_, url), <Environment files=…>, src-like props on
   component tags, and `import x from './a.glb'` or '?url'. Keep the hook and prop names
   in one constant list, not in config. .astro frontmatter and <script> get this through
   P1's reuse of js-ts. In mct-lang-astro, add src-like attributes in the template.
2. Emit `references` relations to the asset's file name with RelationTarget
   { path: mct_core::resolve_reference_path(..), language: "asset", kind: Asset }
   (ADR-003 D2/D3). Dynamic paths (template strings, variables) record no relation; never
   guess them.
</task>

<acceptance>
- Tests: each loader form above, a dynamic path (no relation), a missing file
  (unresolved), a path escaping root (no relation), and one asset used by two
  components.
- CHANGELOG says one `reindex --force` is needed.
- find_references on the asset lists both components. Extend the fixture and add an
  mct-eval case.
- scripts/unix/check.sh passes.
</acceptance>
```

## P6 — Feature context bundle

Effort: `medium`. Needs P1–P5.

```text
<goal>
Before an agent edits a 3D scene, one short answer should give it the component, its
assets, its styles, its dependencies, and its tests, instead of 10 calls.
</goal>

<task>
1. Run build_context_pack on the fixture's scene component and list what is missing.
2. Add the roles ADR-003 D5 defines: `asset`, `style`, `package`. No new parameters.
   Keep the output budget: each related item once,
   with its location and a one-line signature, and capped counts.
</task>

<acceptance>
- Test: the pack for the fixture scene contains its .glb, its textures, its CSS/Tailwind
  source, and its three/drei dependency, each exactly once.
- Measure the TOON output tokens before and after on that fixture and report both numbers.
- scripts/unix/check.sh passes.
</acceptance>
```

## P7 — Style and accessibility map

Effort: `medium`. Needs P2.

```text
<goal>
Answer from the index: "which components use this class?" and "which scene controls
lack a keyboard, touch, or reduced-motion alternative?".
</goal>

<task>
1. Styles: link className/class literals, including literal arguments of clsx/cn, to CSS
   Rule symbols, the same way HTML class → CSS Rule works today (RelationTarget.language).
   Tailwind utilities with no CSS source are recorded by name only; do not expand
   Tailwind.
2. Accessibility heuristics, evaluated at parse time and stored as `finding` symbols
   (ADR-003 D2: named after the heuristic, parent = enclosing component, located at the
   element), read with list_symbols(path, kind="finding") (D5):
   - `pointer-without-keyboard`: onClick/onPointer* with no onKeyDown/onKeyUp and no
     role/tabIndex;
   - `motion-without-reduced-motion`: OrbitControls, useFrame animation, or autoRotate
     with no prefers-reduced-motion or useReducedMotion check in the same file;
   - `pointer-without-touch`: pointer-only drag or hover with no touch equivalent.
   Add one line naming the findings to list_symbols's description. State each
   heuristic and its limits in the js-ts parser spec. False positives are acceptable;
   silent false negatives on the documented patterns are not.
</task>

<acceptance>
- Tests: a clickable <mesh> without a key handler (flagged) and with one (not flagged),
  an auto-rotating scene with and without a reduced-motion check, and cn("a", x) with a
  dynamic argument.
- CHANGELOG says one `reindex --force` is needed.
- scripts/unix/check.sh passes.
</acceptance>
```
