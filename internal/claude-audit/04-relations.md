# Agent 04 — reliable relation context

Follow the shared rules. Create branch `codex/audit-relations` from a base that already includes 01 and 03. Goal: F02, preventing homonyms from being shown as resolved dependencies.

Reproduce with `build_context_pack` on `query_relations`, path `crates/mct-index`: C# `Ok`, PHP `get`, and C++ `map` appear. Review `pick_definition`, `RelatedSet`, relation queries, BFS, and Rust `call_target`. Having a path/language on the initial definition does not guarantee the identity of its targets.

Compatible phase: remove arbitrary target selection; when evidence is insufficient, preserve ambiguity or an explicit external target. Do not make legitimate cross-language relations impossible; require evidence to resolve them. Keep useful candidate information and do not filter results merely to pass a test.

Cross-cutting phase: design qualified identity, caller scope, candidates, and `resolved`/`ambiguous`/`external` states. Preserve the distinction between syntactic reference and semantic resolution. Before changing the schema, `LanguageParser`, or MCP signatures, prepare the issue required by `AGENTS.md` and wait for it to be published. A design PR or compatible mitigation may be delivered first.

Scope: `mct-index` queries/traversal, context-pack composition/formats, focused tests, and an ADR. Avoid registry, exclusion, and another agent's Markdown changes. Do not try to solve every type system in one PR.

Acceptance: cases for `A::run`/`B::run`, standard/third-party libraries, same-name symbols in one file, and multiple languages; no incorrect targets presented as resolved; BFS does not claim precision it cannot prove. Update relevant evaluation cases without lowering thresholds. Deliver a small prepared PR and a design for remaining work, if any.
