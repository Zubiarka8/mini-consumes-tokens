# Index

Central map of this vault. Start here.

## Sections

- [[glossary]] — terms used across this vault
- [[mcp-protocol-spec]] — MCP tool contract
- [[001-sqlite-storage]] — ADR: why SQLite
- [[002-qualified-relation-resolution]] — ADR: how relation targets are resolved
- [[003-web-3d-indexing]] — ADR: web / 3D portfolio indexing foundation
- [[004-grammar-version-requirements]] — ADR: grammar version requirements
- [[005-markdown-alias-storage]] — ADR: Markdown frontmatter alias storage
- [[006-dependency-graph-tool]] — ADR: module dependency graph tool
- [[007-duplicate-function-report]] — ADR: on-demand duplicate function report
- [[008-token-twins-comparison-policies]] — ADR: precise token twins and comparison policies
- [[009-duplicate-group-aligned-evidence]] — ADR: aligned evidence for duplicate groups
- [[010-duplicate-group-verdicts]] — ADR: explainable verdicts for duplicate groups
- [[011-duplicate-subgroups]] — ADR: evidence-based duplicate subgroups
- [[012-repeated-partial-blocks]] — ADR: repeated partial blocks
- [[013-obsidian-vault-location]] — ADR: Obsidian vault location and generation
- [[014-local-project-web]] — ADR: one local website for atlas and duplicate review
- [[015-duplicate-review-decisions]] — ADR: reversible review decisions for duplicate groups
- [[016-context-budget]] — ADR: estimated token budget and progressive detail
- [[019-shared-review-decisions]] — ADR: shared review decisions across dashboard, CLI and MCP
- [[021-index-writer-and-generations]] — ADR: project writer ownership and explicit index completion
- [[022-mcp-duplicate-findings]] — ADR: review-aware duplicate findings over MCP
- [[023-mcp-dead-code-reviews]] — ADR: shared review decisions in MCP dead-code findings
- [[024-mcp-source-revisions]] — ADR: verify source revisions before rendering MCP ranges
- [[library-support]] — library and framework support: manifests, per-library tests and extension steps
- [[web-dashboard]] — the local `mct-cli web` dashboard
- [[dashboard-review-acceptance]] — acceptance record for the dashboard review decisions
- [[query-cache]] — multi-hop query cache design (issue #16)
- [[mct-mcp-server]] — MCP server crate
- [[overview]] — mandatory parser contract every `mct-lang-*` crate follows
- [[symbol-coverage]] — what each parser indexes and which tools (search, dead code, duplicates, import graph…) work per language, with a checklist per language
- [[mct-lang-php]] — PHP parser exception (security-relevant)
- [[limits-spec]] — recursion/fuzzing/benchmark limits
- [[sec-001-php-stack-overflow]] — security advisory
- [[roadmap]] — backlog

## Extension → Crate map

| Extension | Language | Crate |
|---|---|---|
| .rs | Rust | `mct-lang-rust` |
| .py | Python | `mct-lang-python` |
| .js/.jsx/.mjs/.cjs/.ts/.mts/.cts/.tsx | JS/TS | `mct-lang-js-ts` |
| .java | Java | `mct-lang-java` |
| .cs | C# | `mct-lang-csharp` |
| .kt | Kotlin | `mct-lang-kotlin` |
| .cpp/.cc/.cxx/.hpp/.hh/.h | C++ | `mct-lang-cpp` |
| .go | Go | `mct-lang-go` |
| .html | HTML | `mct-lang-html` |
| .css | CSS | `mct-lang-css` |
| .xml | XML | `mct-lang-xml` |
| .xaml | XAML | `mct-lang-xaml` |
| .sh | Bash | `mct-lang-bash` |
| .ps1/.psm1 | PowerShell | `mct-lang-powershell` |
| .php | PHP | `mct-lang-php` |
| .md | Markdown | `mct-lang-md` |
| .lua | Lua | `mct-lang-lua` |

Adding a language does not need a new note here beyond a row in this table — see the templates below for the one exception.

## Templates

- [[adr-template]] — architecture decision record template
- [[crate-spec-template]] — crate note template
- [[lang-crate-base]] — base template for a new `mct-lang-*` crate
- [[library-module-template]] — contribution flow and test skeleton for library/framework support
- [[lang-spec-template]] — language-parser note template (use only for languages with special/security-relevant behavior)

#moc
