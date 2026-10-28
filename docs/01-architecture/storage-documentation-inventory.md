# Storage architecture documentation inventory

Dated inventory: 2026-10-25, dirty base b61cc7ce971fdc46a61259af95af7fac823ae352.
Every path below was enumerated and its Markdown title/links inspected. Purpose,
audience and update conditions are editorial classifications, not proof that each
historical claim was revalidated. Detailed current implementation evidence lives
in [the audit](storage-reliability-audit.md); future requirements live in
[the SDD](storage-reliability-spec.md). ADR-001 explains SQLite; ADR-019 owns
review semantics; ADR-020/021 discuss the proposed reliability boundaries.

Scope: nonignored Markdown returned by rg, excluding generated targets, vendor,
parser/test fixture prose and the untracked visual prototype. Hidden agent/skill
folders are instruction packages, not architecture authority, and were not read.
The denied .github/agents directory was not inspected. Fixture Markdown tests the
parser rather than describing production architecture. Website content may repeat
user-facing guides; update it only with an accepted feature, without copying
operational history or private paths. This inventory itself changes when documents
are added/removed or their authority changes. It does not replace the existing
vault navigation or project board.

| Document | Purpose | Audience | Update condition | Relationships (first local links) |
| --- | --- | --- | --- | --- |
| [This inventory](storage-documentation-inventory.md) | Documentation authority and navigation map | Maintainers/contributors | Documents or authority change | Audit, SDD, verification |
| [Verification](storage-reliability-verification.md) | Actual implementation/check outcomes and limitations | Reviewers/operators | Code, checks or acceptance evidence change | SDD and task checkpoint |
| [AGENTS.md](../../AGENTS.md) | Repository instructions: AGENTS | Contributors/agents | Repository workflow or policy changes | `rules.md`, `rules.md#table-of-contents`, `PROJECT_MANAGER.md` |
| [ARCHITECTURE.md](../../ARCHITECTURE.md) | Reference/guide: How it works | Users/maintainers | Behavior, commands, architecture or setup changes | `README.md`, `CONTRIBUTING.md` |
| [CHANGELOG.md](../../CHANGELOG.md) | Reference/guide: Changelog | Users/maintainers | Behavior, commands, architecture or setup changes | `COMMANDS.md#duplicates--duplicates---min-lines-n---min-tokens-n`, `docs/01-architecture/adrs/011-duplicate-subgroups.md`, `docs/01-architecture/adrs/012-repeated-partial-blocks.md` |
| [CLAUDE.md](../../CLAUDE.md) | Repository instructions: CLAUDE | Contributors/agents | Repository workflow or policy changes | `rules.md`, `rules.md#table-of-contents`, `PROJECT_MANAGER.md` |
| [COMMANDS.md](../../COMMANDS.md) | Reference/guide: Command reference | Users/maintainers | Behavior, commands, architecture or setup changes | `README.md`, `docs/01-architecture/adrs/013-obsidian-vault-location.md`, `docs/01-architecture/adrs/014-local-project-web.md` |
| [CONTRIBUTING.md](../../CONTRIBUTING.md) | Repository instructions: Contributing | Contributors/agents | Repository workflow or policy changes | `docs/06-templates/lang-crate-base.md`, `docs/01-architecture/adrs/004-grammar-version-requirements.md`, `docs/06-templates/library-module-template.md` |
| [Claude-Plan.md](../../Claude-Plan.md) | Reference/guide: Remediation plan | Users/maintainers | Behavior, commands, architecture or setup changes | `rules.md`, `internal/claude-audit/00-rules.md`, `internal/claude-audit/01-exclusions.md` |
| [GPT-Report.md](../../GPT-Report.md) | Reference/guide: GPT Report — mini-consumes-tokens audit | Users/maintainers | Behavior, commands, architecture or setup changes | `docs/01-architecture/adrs/002-qualified-relation-resolution.md`, `docs/superpowers/specs/2026-10-01-qualified-relation-identity-design.md`, `docs/superpowers/specs/2026-10-01-markdown-note-graph-design.md` |
| [ISSUES_PENDING.md](../../ISSUES_PENDING.md) | Reference/guide: Pending issues | Users/maintainers | Behavior, commands, architecture or setup changes | Wiki links / containing section |
| [LOCAL_INSTRUCTIONS.md](../../LOCAL_INSTRUCTIONS.md) | Reference/guide: Local Repository Instructions | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [PROJECT_MANAGER.md](../../PROJECT_MANAGER.md) | Repository instructions: Project Manager — Multiagent Coordination | Contributors/agents | Repository workflow or policy changes | `rules.md`, `project-management/PROJECT_STATE.md`, `project-management/TASKS.md` |
| [README.md](../../README.md) | Reference/guide: mini-consumes-tokens (mct) | Users/maintainers | Behavior, commands, architecture or setup changes | `benchmarks/intent-selection.md#tool-attention-issue-11`, `benchmarks/context-budget.md`, `benchmarks/real-repos.md` |
| [RELEASING.md](../../RELEASING.md) | Reference/guide: Releasing | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [ROADMAP.md](../../ROADMAP.md) | Reference/guide: ROADMAP — mini-consumes-tokens (post-0.1.0) | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) | Reference/guide: Third-Party Notices | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [benchmarks/adaptive-detail.md](../../benchmarks/adaptive-detail.md) | Measurement or experiment: Adaptive detail selection: extended evaluation and offline simulation | Performance reviewers | Reproducible benchmark/code/baseline changes | Containing crate/section |
| [benchmarks/agent-benchmark.md](../../benchmarks/agent-benchmark.md) | Measurement or experiment: Agent benchmark: Claude Code with and without the MCP server | Performance reviewers | Reproducible benchmark/code/baseline changes | Containing crate/section |
| [benchmarks/context-budget.md](../../benchmarks/context-budget.md) | Measurement or experiment: Context budget benchmark | Performance reviewers | Reproducible benchmark/code/baseline changes | Containing crate/section |
| [benchmarks/intent-selection.md](../../benchmarks/intent-selection.md) | Measurement or experiment: Intent extraction and tool preselection (issue #19) | Performance reviewers | Reproducible benchmark/code/baseline changes | Containing crate/section |
| [benchmarks/quality-eval.md](../../benchmarks/quality-eval.md) | Measurement or experiment: Quality evaluation (`mct-eval`) | Performance reviewers | Reproducible benchmark/code/baseline changes | Containing crate/section |
| [benchmarks/real-repos.md](../../benchmarks/real-repos.md) | Measurement or experiment: Context budget on real repositories | Performance reviewers | Reproducible benchmark/code/baseline changes | Wiki links / containing section |
| [benchmarks/semantic-runtime.md](../../benchmarks/semantic-runtime.md) | Measurement or experiment: Semantic runtime audit (R03) | Performance reviewers | Reproducible benchmark/code/baseline changes | Containing crate/section |
| [benchmarks/token-benchmark.md](../../benchmarks/token-benchmark.md) | Measurement or experiment: Token benchmark: MCP tools vs. Read/Grep/Glob | Performance reviewers | Reproducible benchmark/code/baseline changes | Containing crate/section |
| [benchmarks/token-twins-presentation.md](../../benchmarks/token-twins-presentation.md) | Measurement or experiment: Presenting duplicate code to people and to agents | Performance reviewers | Reproducible benchmark/code/baseline changes | Containing crate/section |
| [benchmarks/token-twins.md](../../benchmarks/token-twins.md) | Measurement or experiment: Token twins validation | Performance reviewers | Reproducible benchmark/code/baseline changes | `../docs/01-architecture/adrs/008-token-twins-comparison-policies.md`, `duplicate_analysis.py`, `token-twins-results.toon` |
| [docs/00-system/00-index.md](../00-system/00-index.md) | Reference/guide: Index | Users/maintainers | Behavior, commands, architecture or setup changes | Wiki links / containing section |
| [docs/00-system/glossary.md](../00-system/glossary.md) | Reference/guide: Glossary | Users/maintainers | Behavior, commands, architecture or setup changes | Wiki links / containing section |
| [docs/01-architecture/adrs/001-sqlite-storage.md](adrs/001-sqlite-storage.md) | Decision record: ADR-001: SQLite as the index storage engine | Maintainers/reviewers | Decision accepted, revised or superseded | Wiki links / containing section |
| [docs/01-architecture/adrs/002-qualified-relation-resolution.md](adrs/002-qualified-relation-resolution.md) | Decision record: ADR-002: Qualified relation resolution | Maintainers/reviewers | Decision accepted, revised or superseded | Wiki links / containing section |
| [docs/01-architecture/adrs/003-web-3d-indexing.md](adrs/003-web-3d-indexing.md) | Decision record: ADR-003: Web / 3D portfolio indexing foundation | Maintainers/reviewers | Decision accepted, revised or superseded | `../../05-backlog/roadmap.md#web--3d-portfolio-support-by-priority`, `../../05-backlog/web-3d-prompts.md`, `002-qualified-relation-resolution.md` |
| [docs/01-architecture/adrs/004-grammar-version-requirements.md](adrs/004-grammar-version-requirements.md) | Decision record: ADR-004: Grammar version requirements | Maintainers/reviewers | Decision accepted, revised or superseded | Containing crate/section |
| [docs/01-architecture/adrs/005-markdown-alias-storage.md](adrs/005-markdown-alias-storage.md) | Decision record: ADR-005: Markdown frontmatter alias storage | Maintainers/reviewers | Decision accepted, revised or superseded | `003-web-3d-indexing.md`, `002-qualified-relation-resolution.md` |
| [docs/01-architecture/adrs/006-dependency-graph-tool.md](adrs/006-dependency-graph-tool.md) | Decision record: ADR-006: Module dependency graph tool | Maintainers/reviewers | Decision accepted, revised or superseded | `002-qualified-relation-resolution.md` |
| [docs/01-architecture/adrs/007-duplicate-function-report.md](adrs/007-duplicate-function-report.md) | Decision record: ADR-007: On-demand duplicate function report | Maintainers/reviewers | Decision accepted, revised or superseded | `008-token-twins-comparison-policies.md` |
| [docs/01-architecture/adrs/008-token-twins-comparison-policies.md](adrs/008-token-twins-comparison-policies.md) | Decision record: ADR-008: Precise token twins with explicit comparison policies | Maintainers/reviewers | Decision accepted, revised or superseded | `007-duplicate-function-report.md`, `../../../benchmarks/token-twins.md` |
| [docs/01-architecture/adrs/009-duplicate-group-aligned-evidence.md](adrs/009-duplicate-group-aligned-evidence.md) | Decision record: ADR-009: Aligned evidence for duplicate groups | Maintainers/reviewers | Decision accepted, revised or superseded | `008-token-twins-comparison-policies.md`, `../../../benchmarks/token-twins.md#aligned-evidence-cost` |
| [docs/01-architecture/adrs/010-duplicate-group-verdicts.md](adrs/010-duplicate-group-verdicts.md) | Decision record: ADR-010: Explainable verdicts for duplicate groups | Maintainers/reviewers | Decision accepted, revised or superseded | `009-duplicate-group-aligned-evidence.md`, `../../../benchmarks/token-twins.md#explainable-verdicts` |
| [docs/01-architecture/adrs/011-duplicate-subgroups.md](adrs/011-duplicate-subgroups.md) | Decision record: ADR-011: Evidence-based subgroups of duplicate groups | Maintainers/reviewers | Decision accepted, revised or superseded | `009-duplicate-group-aligned-evidence.md`, `../../../benchmarks/token-twins.md#evidence-subgroups-2026-10-15` |
| [docs/01-architecture/adrs/012-repeated-partial-blocks.md](adrs/012-repeated-partial-blocks.md) | Decision record: ADR-012: Repeated partial blocks | Maintainers/reviewers | Decision accepted, revised or superseded | `008-token-twins-comparison-policies.md`, `009-duplicate-group-aligned-evidence.md`, `../../../benchmarks/token-twins.md#repeated-partial-blocks` |
| [docs/01-architecture/adrs/013-obsidian-vault-location.md](adrs/013-obsidian-vault-location.md) | Decision record: ADR-013: Obsidian vault location and generation | Maintainers/reviewers | Decision accepted, revised or superseded | `014-local-project-web.md` |
| [docs/01-architecture/adrs/014-local-project-web.md](adrs/014-local-project-web.md) | Decision record: ADR-014: One local website for atlas and duplicate review | Maintainers/reviewers | Decision accepted, revised or superseded | `015-duplicate-review-decisions.md` |
| [docs/01-architecture/adrs/015-duplicate-review-decisions.md](adrs/015-duplicate-review-decisions.md) | Decision record: ADR-015: Reversible user review decisions for duplicate groups | Maintainers/reviewers | Decision accepted, revised or superseded | Containing crate/section |
| [docs/01-architecture/adrs/016-context-budget.md](adrs/016-context-budget.md) | Decision record: ADR-016: Estimated token budget and progressive detail | Maintainers/reviewers | Decision accepted, revised or superseded | Containing crate/section |
| [docs/01-architecture/adrs/017-markdown-link-folding-and-alias-backlinks.md](adrs/017-markdown-link-folding-and-alias-backlinks.md) | Decision record: ADR-017: Case-folded Markdown link resolution and alias-aware references | Maintainers/reviewers | Decision accepted, revised or superseded | `005-markdown-alias-storage.md`, `Note.md` |
| [docs/01-architecture/adrs/018-web-dashboard.md](adrs/018-web-dashboard.md) | Decision record: ADR-018: Local web dashboard with unified finding reviews | Maintainers/reviewers | Decision accepted, revised or superseded | Containing crate/section |
| [docs/01-architecture/adrs/019-dashboard-sqlite-store.md](adrs/019-dashboard-sqlite-store.md) | Decision record: ADR-019: Dashboard state in a separate SQLite database, JSON API and embedded SPA | Maintainers/reviewers | Decision accepted, revised or superseded | Containing crate/section |
| [docs/01-architecture/adrs/020-user-state-backup.md](adrs/020-user-state-backup.md) | Decision record: ADR-020: Consistent user-state snapshots and isolated recovery verification | Maintainers/reviewers | Decision accepted, revised or superseded | `../storage-reliability-spec.md` |
| [docs/01-architecture/adrs/021-index-writer-and-generations.md](adrs/021-index-writer-and-generations.md) | Decision record: ADR-021: Project writer ownership and explicit index completion | Maintainers/reviewers | Decision accepted, revised or superseded | Storage SDD Phase 4; verification |
| [docs/01-architecture/adrs/022-mcp-duplicate-findings.md](adrs/022-mcp-duplicate-findings.md) | Proposed decision: review-aware duplicate findings over MCP; implementation waits for a tracked issue | Maintainers/reviewers | Issue opened or decision accepted/rejected | `mcp-protocol-spec.md` |
| [docs/01-architecture/library-support.md](library-support.md) | Reference/guide: Library and framework support | Users/maintainers | Behavior, commands, architecture or setup changes | `../../crates/mct-index/src/manifests.rs`, `../../crates/mct-index/src/manifests`, `../../crates/mct-index/src/manifests/javascript.rs` |
| [docs/01-architecture/mcp-protocol-spec.md](mcp-protocol-spec.md) | Specification: MCP Protocol Spec and Phase 8 retrieval contract | Implementers/reviewers | MCP contract, review semantics, benchmarks or acceptance evidence change | ADR-006, ADR-016, proposed ADR-022 |
| [docs/01-architecture/storage-reliability-audit.md](storage-reliability-audit.md) | Reference/guide: Storage reliability audit | Users/maintainers | Behavior, commands, architecture or setup changes | `storage-reliability-spec.md`, `storage-documentation-inventory.md`, `web-database-lifecycle.md` |
| [docs/01-architecture/storage-reliability-spec.md](storage-reliability-spec.md) | Specification: Reliable indexing, user-state backup and recovery | Implementers/reviewers | Requirement or acceptance change | `storage-reliability-audit.md`, `adrs/019-dashboard-sqlite-store.md`, `web-database-lifecycle.md` |
| [docs/01-architecture/web-database-lifecycle.md](web-database-lifecycle.md) | Reference/guide: Web dashboard database and review lifecycle | Users/maintainers | Behavior, commands, architecture or setup changes | `adrs/019-dashboard-sqlite-store.md`, `../../tasks/plan.md`, `../../rules.md` |
| [docs/02-crates/core/mct-mcp-server.md](../02-crates/core/mct-mcp-server.md) | Reference/guide: mct-mcp-server | Users/maintainers | Behavior, commands, architecture or setup changes | Wiki links / containing section |
| [docs/02-crates/parsers/mct-lang-php.md](../02-crates/parsers/mct-lang-php.md) | Reference/guide: mct-lang-php | Users/maintainers | Behavior, commands, architecture or setup changes | Wiki links / containing section |
| [docs/02-crates/parsers/overview.md](../02-crates/parsers/overview.md) | Reference/guide: Parser contract (`mct-lang-*`) | Users/maintainers | Behavior, commands, architecture or setup changes | Wiki links / containing section |
| [docs/02-crates/parsers/symbol-coverage.md](../02-crates/parsers/symbol-coverage.md) | Reference/guide: Symbol coverage by language | Users/maintainers | Behavior, commands, architecture or setup changes | `../../06-templates/lang-crate-base.md`, `../../../internal/checklist.md`, `../../../internal/corpus-progress.md` |
| [docs/03-performance/limits-spec.md](../03-performance/limits-spec.md) | Specification: Performance & limits | Implementers/reviewers | Requirement or acceptance change | Wiki links / containing section |
| [docs/03-performance/query-cache.md](../03-performance/query-cache.md) | Reference/guide: Query cache (issue #16) | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [docs/04-security/advisories/sec-001-php-stack-overflow.md](../04-security/advisories/sec-001-php-stack-overflow.md) | Reference/guide: SEC-001: AST-walker stack overflow (unbounded recursion) | Users/maintainers | Behavior, commands, architecture or setup changes | Wiki links / containing section |
| [docs/05-backlog/roadmap.md](../05-backlog/roadmap.md) | Plan/engineering record: Backlog | Maintainers/implementers | Milestone, evidence or implementation changes | `web-3d-prompts.md`, `../01-architecture/adrs/003-web-3d-indexing.md` |
| [docs/05-backlog/web-3d-prompts.md](../05-backlog/web-3d-prompts.md) | Plan/engineering record: Web / 3D portfolio prompts (Claude Opus 5.5) | Maintainers/implementers | Milestone, evidence or implementation changes | `roadmap.md#web--3d-portfolio-support-by-priority` |
| [docs/06-templates/adr-template.md](../06-templates/adr-template.md) | Template: ADR-NNN: <decision title> | Documentation authors | Documentation convention changes | Wiki links / containing section |
| [docs/06-templates/crate-spec-template.md](../06-templates/crate-spec-template.md) | Template: <crate-name> | Documentation authors | Documentation convention changes | Wiki links / containing section |
| [docs/06-templates/lang-crate-base.md](../06-templates/lang-crate-base.md) | Template: Base template: new `mct-lang-*` crate | Documentation authors | Documentation convention changes | `../../CONTRIBUTING.md#adding-a-new-language`, `../02-crates/parsers/symbol-coverage.md` |
| [docs/06-templates/lang-spec-template.md](../06-templates/lang-spec-template.md) | Template: mct-lang-<name> | Documentation authors | Documentation convention changes | Wiki links / containing section |
| [docs/06-templates/library-module-template.md](../06-templates/library-module-template.md) | Template: Library/framework module template | Documentation authors | Documentation convention changes | `../01-architecture/library-support.md`, `../../CONTRIBUTING.md#frameworklibrary-coverage-beyond-the-language-table`, `../../CONTRIBUTING.md#changing-the-languageparser-trait-the-sqlite-schema-or-an-already-published-mcp-tool-signature` |
| [docs/superpowers/plans/2026-09-17-mct-lang-md-phase-2-relations.md](../superpowers/plans/2026-09-17-mct-lang-md-phase-2-relations.md) | Plan/engineering record: mct-lang-md Phase 2 (WikiLink/Tag Relations) Implementation Plan | Maintainers/implementers | Milestone, evidence or implementation changes | `other.md`, `url` |
| [docs/superpowers/plans/2026-09-17-obsidian-docs-vault.md](../superpowers/plans/2026-09-17-obsidian-docs-vault.md) | Plan/engineering record: Obsidian Docs Vault Implementation Plan | Maintainers/implementers | Milestone, evidence or implementation changes | Wiki links / containing section |
| [docs/superpowers/specs/2026-09-17-obsidian-docs-vault-and-md-parser-design.md](../superpowers/specs/2026-09-17-obsidian-docs-vault-and-md-parser-design.md) | Specification: Design: Obsidian-style docs vault + `mct-lang-md` Phase 2 (PKM relations) | Implementers/reviewers | Requirement or acceptance change | Wiki links / containing section |
| [docs/superpowers/specs/2026-09-18-context-overview-tool-design.md](../superpowers/specs/2026-09-18-context-overview-tool-design.md) | Specification: Design: `get_project_overview` MCP tool (compact hierarchical context) | Implementers/reviewers | Requirement or acceptance change | Wiki links / containing section |
| [docs/superpowers/specs/2026-09-18-mct-lang-md-phase-3-wikilink-anchors.md](../superpowers/specs/2026-09-18-mct-lang-md-phase-3-wikilink-anchors.md) | Specification: Design: `mct-lang-md` Phase 3 (wikilink anchors, `.md` normalization, embeds) | Implementers/reviewers | Requirement or acceptance change | Wiki links / containing section |
| [docs/superpowers/specs/2026-10-01-markdown-note-graph-design.md](../superpowers/specs/2026-10-01-markdown-note-graph-design.md) | Specification: Design: Markdown and Obsidian notes as first-class graph nodes | Implementers/reviewers | Requirement or acceptance change | `2026-10-01-qualified-relation-identity-design.md`, `../../01-architecture/adrs/005-markdown-alias-storage.md` |
| [docs/superpowers/specs/2026-10-01-qualified-relation-identity-design.md](../superpowers/specs/2026-10-01-qualified-relation-identity-design.md) | Specification: Design: qualified identity and relation resolution | Implementers/reviewers | Requirement or acceptance change | `../../01-architecture/adrs/002-qualified-relation-resolution.md` |
| [errors/HISTORY.md](../../errors/HISTORY.md) | Failure tracking: Error history | Maintainers/operators | Failure observation or verified resolution | `../project-management/checkpoints/index-parse-investigation-2026-10-18.md` |
| [errors/README.md](../../errors/README.md) | Failure tracking: Error tracking | Maintainers/operators | Failure observation or verified resolution | `REGISTRY.md`, `HISTORY.md` |
| [errors/REGISTRY.md](../../errors/REGISTRY.md) | Failure tracking: Error registry | Maintainers/operators | Failure observation or verified resolution | `../internal/corpus-progress.md`, `../CONTRIBUTING.md#language-corpus-testscorpus`, `../CONTRIBUTING.md#malformed-fixtures-and-the-working-index` |
| [internal/audit-integration-review.md](../../internal/audit-integration-review.md) | Plan/engineering record: Integration review: first audit batch | Maintainers/implementers | Milestone, evidence or implementation changes | Containing crate/section |
| [internal/checklist.md](../../internal/checklist.md) | Plan/engineering record: mini-consumes-tokens — checklist | Maintainers/implementers | Milestone, evidence or implementation changes | Wiki links / containing section |
| [internal/claude-audit/00-rules.md](../../internal/claude-audit/00-rules.md) | Plan/engineering record: Shared rules for remediation agents | Maintainers/implementers | Milestone, evidence or implementation changes | `../../Claude-Plan.md`, `../../rules.md` |
| [internal/claude-audit/01-exclusions.md](../../internal/claude-audit/01-exclusions.md) | Plan/engineering record: Agent 01 — reload exclusions | Maintainers/implementers | Milestone, evidence or implementation changes | Containing crate/section |
| [internal/claude-audit/02-lua.md](../../internal/claude-audit/02-lua.md) | Plan/engineering record: Agent 02 — enable Lua | Maintainers/implementers | Milestone, evidence or implementation changes | Containing crate/section |
| [internal/claude-audit/03-dead-code.md](../../internal/claude-audit/03-dead-code.md) | Plan/engineering record: Agent 03 — reduce false dead-code candidates | Maintainers/implementers | Milestone, evidence or implementation changes | Containing crate/section |
| [internal/claude-audit/04-relations.md](../../internal/claude-audit/04-relations.md) | Plan/engineering record: Agent 04 — reliable relation context | Maintainers/implementers | Milestone, evidence or implementation changes | Containing crate/section |
| [internal/claude-audit/05-markdown.md](../../internal/claude-audit/05-markdown.md) | Plan/engineering record: Agent 05 — note context and wikilinks | Maintainers/implementers | Milestone, evidence or implementation changes | Wiki links / containing section |
| [internal/claude-audit/06-configuration.md](../../internal/claude-audit/06-configuration.md) | Plan/engineering record: Agent 06 — shareable configuration and documentation | Maintainers/implementers | Milestone, evidence or implementation changes | Containing crate/section |
| [internal/claude-audit/07-duplication.md](../../internal/claude-audit/07-duplication.md) | Plan/engineering record: Agent 07 — share genuinely duplicated infrastructure | Maintainers/implementers | Milestone, evidence or implementation changes | Containing crate/section |
| [internal/claude-audit/08-quality.md](../../internal/claude-audit/08-quality.md) | Plan/engineering record: Agent 08 — regressions, measurements, and integration review | Maintainers/implementers | Milestone, evidence or implementation changes | Containing crate/section |
| [internal/claude-audit/09-integration-review.md](../../internal/claude-audit/09-integration-review.md) | Plan/engineering record: Reviewer agent — first-batch integration | Maintainers/implementers | Milestone, evidence or implementation changes | Containing crate/section |
| [internal/claude-hooks/README.md](../../internal/claude-hooks/README.md) | Plan/engineering record: Agent hooks (Claude Code and Codex) | Maintainers/implementers | Milestone, evidence or implementation changes | Containing crate/section |
| [internal/corpus-progress.md](../../internal/corpus-progress.md) | Plan/engineering record: Issue #74 — Long-fixture corpus progress by language | Maintainers/implementers | Milestone, evidence or implementation changes | `x` |
| [project-management/AGENT_REGISTRY.md](../../project-management/AGENT_REGISTRY.md) | Coordination/evidence: Agent Registry | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/DECISIONS.md](../../project-management/DECISIONS.md) | Coordination/evidence: Coordination Decisions | Contributors/coordinator | Task, ownership, integration or handoff changes | `../docs/01-architecture/adrs/004-grammar-version-requirements.md`, `../docs/01-architecture/adrs/005-markdown-alias-storage.md` |
| [project-management/HANDOFF.md](../../project-management/HANDOFF.md) | Coordination/evidence: Handoff | Contributors/coordinator | Task, ownership, integration or handoff changes | `../PROJECT_MANAGER.md#checkpoints-and-handoffs`, `checkpoints/TASK-207-optimize.md`, `PROJECT_STATE.md` |
| [project-management/PROJECT_STATE.md](../../project-management/PROJECT_STATE.md) | Coordination/evidence: Project State | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/REPORT.md](../../project-management/REPORT.md) | Coordination/evidence: Review Report | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/SESSION_LOG.md](../../project-management/SESSION_LOG.md) | Coordination/evidence: Session Log | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/TASKS.md](../../project-management/TASKS.md) | Coordination/evidence: Tasks | Contributors/coordinator | Task, ownership, integration or handoff changes | `../PROJECT_MANAGER.md#tasks-ownership-and-isolation`, `HANDOFF.md#task-pm-001`, `HANDOFF.md#task-lib-001` |
| [project-management/checkpoints/TASK-207-A.md](../../project-management/checkpoints/TASK-207-A.md) | Coordination/evidence: Checkpoint: TASK-207-A (issue #207) - Rust and Python token sources | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/TASK-207-B.md](../../project-management/checkpoints/TASK-207-B.md) | Coordination/evidence: Checkpoint: TASK-207-B (duplicate analysis) | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/TASK-207-C.md](../../project-management/checkpoints/TASK-207-C.md) | Coordination/evidence: Checkpoint: TASK-207-C | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/TASK-207-optimize.md](../../project-management/checkpoints/TASK-207-optimize.md) | Coordination/evidence: Checkpoint: TASK-207 optimization of the Codex evaluation work | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/TASK-CSS-NESTING-115.md](../../project-management/checkpoints/TASK-CSS-NESTING-115.md) | Coordination/evidence: TASK-CSS-NESTING-115 checkpoint | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/TASK-DOCS-MCP-PROSE.md](../../project-management/checkpoints/TASK-DOCS-MCP-PROSE.md) | Coordination/evidence: TASK-DOCS-MCP-PROSE checkpoint | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/TASK-MD-ALIAS-135.md](../../project-management/checkpoints/TASK-MD-ALIAS-135.md) | Coordination/evidence: TASK-MD-ALIAS-135 recovery checkpoint | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/TASK-PR-POLICY.md](../../project-management/checkpoints/TASK-PR-POLICY.md) | Coordination/evidence: TASK-PR-POLICY checkpoint | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/depgraph-core.md](../../project-management/checkpoints/depgraph-core.md) | Coordination/evidence: Checkpoint: depgraph-core (issue #28) | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/depgraph-cycles.md](../../project-management/checkpoints/depgraph-cycles.md) | Coordination/evidence: Checkpoint: depgraph-cycles (issue #28) | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/depgraph-docs-eval.md](../../project-management/checkpoints/depgraph-docs-eval.md) | Coordination/evidence: Checkpoint: depgraph-docs-eval | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/depgraph-languages.md](../../project-management/checkpoints/depgraph-languages.md) | Coordination/evidence: Checkpoint: depgraph-languages | Contributors/coordinator | Task, ownership, integration or handoff changes | Wiki links / containing section |
| [project-management/checkpoints/depgraph-render.md](../../project-management/checkpoints/depgraph-render.md) | Coordination/evidence: Checkpoint: depgraph-render | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/depgraph-tool.md](../../project-management/checkpoints/depgraph-tool.md) | Coordination/evidence: Checkpoint: depgraph-tool | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/index-parse-investigation-2026-10-18.md](../../project-management/checkpoints/index-parse-investigation-2026-10-18.md) | Coordination/evidence: MCP indexing and concurrent-agent isolation investigation | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/index-privacy-hygiene-review.md](../../project-management/checkpoints/index-privacy-hygiene-review.md) | Coordination/evidence: Index privacy and hygiene — independent review | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/issue-196-cli-register-persistence.md](../../project-management/checkpoints/issue-196-cli-register-persistence.md) | Coordination/evidence: issue-196-cli-register-persistence | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/issue-196-docs-and-examples.md](../../project-management/checkpoints/issue-196-docs-and-examples.md) | Coordination/evidence: Checkpoint: issue-196-docs-and-examples | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/issue-196-e2e-verification.md](../../project-management/checkpoints/issue-196-e2e-verification.md) | Coordination/evidence: issue-196-e2e-verification checkpoint | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/issue-196-server-delay-flag.md](../../project-management/checkpoints/issue-196-server-delay-flag.md) | Coordination/evidence: Checkpoint: issue-196-server-delay-flag | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/issue-196-watcher-semantics.md](../../project-management/checkpoints/issue-196-watcher-semantics.md) | Coordination/evidence: Issue 196 watcher semantics audit | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/issue-196-watcher-timing-tests.md](../../project-management/checkpoints/issue-196-watcher-timing-tests.md) | Coordination/evidence: Checkpoint: issue-196-watcher-timing-tests | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [project-management/checkpoints/storage-reliability.md](../../project-management/checkpoints/storage-reliability.md) | Coordination/evidence: TASK-STORAGE-RELIABILITY | Contributors/coordinator | Task, ownership, integration or handoff changes | Containing crate/section |
| [reporte.md](../../reporte.md) | Reference/guide: Documentation refresh report | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [reportes/architecture.md](../../reportes/architecture.md) | Reference/guide: Report: docs-architecture | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [reportes/changelog.md](../../reportes/changelog.md) | Reference/guide: Report: docs-changelog | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [reportes/commands.md](../../reportes/commands.md) | Reference/guide: Report: docs-commands | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [reportes/readme.md](../../reportes/readme.md) | Reference/guide: Report: docs-readme (README.md and internal/checklist.md audit) | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [reportes/roadmap.md](../../reportes/roadmap.md) | Reference/guide: Report: ROADMAP.md and ISSUES_PENDING.md refresh | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [research.md](../../research.md) | Reference/guide: Research — token efficiency and MCP accuracy | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [rules.md](../../rules.md) | Repository instructions: Repository rules | Contributors/agents | Repository workflow or policy changes | `PROJECT_MANAGER.md`, `errors/REGISTRY.md`, `errors/README.md` |
| [scripts/README.md](../../scripts/README.md) | Reference/guide: scripts/ | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [scripts/codex/review/README.md](../../scripts/codex/review/README.md) | Reference/guide: Revision multi-agente con Codex | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [scripts/windows/README.md](../../scripts/windows/README.md) | Reference/guide: scripts/windows/ | Users/maintainers | Behavior, commands, architecture or setup changes | Containing crate/section |
| [tasks/plan.md](../../tasks/plan.md) | Plan/engineering record: Web review database lifecycle and section controls | Maintainers/implementers | Milestone, evidence or implementation changes | `../docs/01-architecture/web-database-lifecycle.md` |
| [website/DESIGN.md](../../website/DESIGN.md) | Website content or build instructions: Code Atlas landing page | Website users/authors | Published feature/navigation/content changes | Containing crate/section |
| [website/README.md](../../website/README.md) | Website content or build instructions: Website | Website users/authors | Published feature/navigation/content changes | `DESIGN.md`, `VALIDATION.md` |
| [website/VALIDATION.md](../../website/VALIDATION.md) | Website content or build instructions: Local validation | Website users/authors | Published feature/navigation/content changes | Containing crate/section |
| [website/research/atlas-design-research.md](../../website/research/atlas-design-research.md) | Website content or build instructions: Atlas design research | Website users/authors | Published feature/navigation/content changes | `atlas-directions.html`, `atlas-directions.png` |
| [website/specs/atlas-redesign/CAPABILITY-MAP.md](../../website/specs/atlas-redesign/CAPABILITY-MAP.md) | Specification: Capability Map: Atlas 3D redesign | Implementers/reviewers | Requirement or acceptance change | `SPEC-landing-story.md`, `SPEC-atlas-cinematic.md`, `SPEC-atlas-layout.md` |
| [website/specs/atlas-redesign/SPEC-atlas-cinematic.md](../../website/specs/atlas-redesign/SPEC-atlas-cinematic.md) | Specification: Spec: atlas-cinematic | Implementers/reviewers | Requirement or acceptance change | `SPEC-landing-story.md` |
| [website/specs/atlas-redesign/SPEC-atlas-layout.md](../../website/specs/atlas-redesign/SPEC-atlas-layout.md) | Specification: Spec: atlas-layout | Implementers/reviewers | Requirement or acceptance change | `CAPABILITY-MAP.md` |
| [website/specs/atlas-redesign/SPEC-atlas-motion.md](../../website/specs/atlas-redesign/SPEC-atlas-motion.md) | Specification: Spec: atlas-motion | Implementers/reviewers | Requirement or acceptance change | `CAPABILITY-MAP.md`, `SPEC-atlas-layout.md` |
| [website/specs/atlas-redesign/SPEC-landing-story.md](../../website/specs/atlas-redesign/SPEC-landing-story.md) | Specification: Spec: landing-story | Implementers/reviewers | Requirement or acceptance change | `SPEC-tour-choreography.md`, `SPEC-atlas-motion.md` |
| [website/specs/atlas-redesign/SPEC-tour-choreography.md](../../website/specs/atlas-redesign/SPEC-tour-choreography.md) | Specification: Spec: tour-choreography | Implementers/reviewers | Requirement or acceptance change | `CAPABILITY-MAP.md`, `SPEC-atlas-layout.md`, `SPEC-atlas-motion.md` |
| [website/specs/atlas-redesign/plan.md](../../website/specs/atlas-redesign/plan.md) | Specification: Plan: Atlas 3D redesign | Implementers/reviewers | Requirement or acceptance change | `todo.md` |
| [website/specs/atlas-redesign/todo.md](../../website/specs/atlas-redesign/todo.md) | Specification: Tasks: Atlas 3D redesign | Implementers/reviewers | Requirement or acceptance change | `plan.md` |

## Phase 3 update map (2026-10-25)

No second specification or backup runbook was introduced. SDD and ADR020 own
requirements/decisions; audit owns the verified starting inventory; verification
owns this phase's test/measurement results; COMMANDS owns scheduler/recovery
operation; lifecycle and tasks/plan link them for dashboard maintainers and future
milestones. Update these records when CLI flags, safety boundaries, acceptance
results or operational preconditions change. ADR021 remains writer/generation
scope, unchanged by destination backup management. Error registry/history retain
the repeated known parser observations without claiming parser fixes.


Phase 5 update conditions: the audit now owns the verified source-to-review
lifecycle; the existing SDD owns acceptance; ADR-002 owns target-language
reference semantics; ADR-019 D16 owns coherent review analysis snapshots.
The verification record owns actual regression/benchmark outcomes. Existing
`benchmarks/token-twins-labels/labels.toon` and `score_duplicate_labels.py`
remain the labeled duplicate corpus/scorer (update only after intentional label
or analyzer policy changes). No competing review architecture document added.


Phase6 responsibilities remain unchanged: reliability SDD owns transitions,
validity and acceptance; lifecycle section17 owns API fields/visibility matrix;
ADR019 D17 records WAL snapshot/compatibility tradeoffs; audit owns verified gaps;
verification owns current executions/measurements; tasks/plan owns pending work.
Update those existing records when state/API semantics or acceptance evidence
change. No competing review persistence or documentation system was added.


Phase7 update: SDD defines comment CAS/drafts/native acceptance; ADR019 D18 owns
the aggregate-revision tradeoff and read-versus-write freshness distinction;
lifecycle section18 is the public API/UI/operator contract; verification owns
actual binaries, checks and remaining acceptance. No parallel persistence or
new document authority is introduced. Existing historical entries are retained.

Phase8.1: MCP protocol spec owns the #233 tool contract, SDD and output budget;
ADR022 records the owner-approved additive decision and alternatives. The
English issue proposal is retained at tasks/phase8-1-issue.md and published as
#233. Verification owns current test/measurement evidence; tasks/plan owns
remaining acceptance. mct-eval/suite-duplicates.json and its Rust/Python fixture
own synthetic retrieval expectations, not real-world detector precision labels.
No competing persistence specification or backup engine is introduced.


Phase9: storage SDD owns mandatory acceptance/threat boundaries; verification
owns requirement-to-test/release-gate matrix, actual failures, native evidence
and measurement limits. scripts/README owns portable isolated Python launcher /
stdio profiling usage. tasks/plan owns unfinished native/audit/operational gates.
No new ADR is needed for a bootstrap-property correction or measurement harness.

Phase9.1: existing SDD owns deadline/isolation acceptance; verification owns
new gate outcomes and native operator evidence; scripts/README.md owns
phase91-watcher.py and phase91-http-assets.py usage. Error registry preserves intermittent incidents,
including failures followed by passing retries. No new architecture document
or ADR: bounded scheduler/test-fixture corrections preserve existing contracts.
