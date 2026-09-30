# Agent 05 — note context and wikilinks

Follow the shared rules. Create branch `codex/audit-markdown` from a base that includes 04. Goal: F05/F06, retrieving complete notes and links without confusing their targets.

Reproduce with: a note containing only `[[beta]]`; a Setext title; `[[beta]]` before the first H1; front matter tags/aliases; inline code `[[ghost]]`; two notes with `## Shared` and a link `[[beta#Shared]]`. Today, symbols/links are missing, the overview omits notes, inline code creates references, and `Shared` may resolve to the wrong note.

Scope: `mct-lang-md`, Markdown/integration tests, note selection/rendering in existing tools, and design docs. Agent 04 owns the generic resolution model; use it and coordinate any change before duplicating it.

Design a root note with path identity, ATX/Setext titles and section ranges, `tags`/`aliases` properties, note/anchor links, and exclusion of code nodes. A path, title, and alias are different identities. Reuse existing types when they express the semantics; if a new contract/schema is required, open an issue first as required by `AGENTS.md`. Do not add a public tool if existing tools can provide sufficient context.

Acceptance: notes without headings are visible; links before the H1 are preserved; front matter is queryable; outline and section text are coherent; anchor targets are unambiguous; inline/fenced code does not create links; edits/renames/deletes update references without stale state. Include path/alias and duplicate-title cases. Keep generic layers free of Markdown grammar rules. Deliver small PRs on separate identified branches if phases are needed.
