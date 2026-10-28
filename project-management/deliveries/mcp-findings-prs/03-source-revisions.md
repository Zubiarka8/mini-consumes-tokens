Integrate existing #234 generation/source verification rather than duplicating it, preserve review-aware dead-code handling, and synchronize the inherited fuzz dependency lock. Add stale snippet/signature/cache and restoration regression coverage.

Base: `feature/mcp-dead-code-reviews`. Candidate: `43e77a9`. This is one slice of the dependent findings stack.

Verification: Source regression red 3 passed/3 failed, then 6 passed; generation tests 7 passed; locked fuzz metadata passed. Full gate: 1824 passed, 0 failed, 30 ignored; fmt, CI Clippy and eval passed.

Remote CI must be checked on the published head; local results do not establish remote success. Browser acceptance remains pending the owned-server bind restriction. No main merge, backup PR closure, website changes or phase-H refactor.

🤖 Generated with [Codex](https://openai.com/codex/)
