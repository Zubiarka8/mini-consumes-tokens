A 40-member duplicate group at max_tokens 512 could omit members and still report next_offset: none. Retry the incomplete group, explain the larger-budget requirement, and preserve review_store in the metadata fallback.

Base: `feature/mcp-find-duplicate-code`. Candidate: `8d06959`. This is one slice of the dependent findings stack.

Verification: Duplicate regression red then green; duplicate_code 8 passed. Full gate: 1798 passed, 0 failed, 31 ignored; fmt, CI Clippy and eval passed.

Remote CI must be checked on the published head; local results do not establish remote success. Browser acceptance remains pending the owned-server bind restriction. No main merge, backup PR closure, website changes or phase-H refactor.

🤖 Generated with [Codex](https://openai.com/codex/)
