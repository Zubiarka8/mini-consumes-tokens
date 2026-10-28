Apply shared review decisions before dead-code paging. Add optional include_reviewed and max_tokens, identities, review labels, counts and fail-open degraded-store reporting while preserving existing arguments and text/TOON formats. Notes, history and source bodies are not returned.

Base: `fix/mcp-duplicate-pagination`. Candidate: `4b0d012`. This is one slice of the dependent findings stack.

Verification: Six review regressions and text/TOON compatibility passed. Full gate: 1804 passed, 0 failed, 31 ignored; fmt, CI Clippy and eval passed. Baselines: catalog 12033 and explicit dead-code metadata 845 response tokens.

Remote CI must be checked on the published head; local results do not establish remote success. Browser acceptance remains pending the owned-server bind restriction. No main merge, backup PR closure, website changes or phase-H refactor.

🤖 Generated with [Codex](https://openai.com/codex/)
