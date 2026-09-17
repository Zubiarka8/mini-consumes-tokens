# Index

Central map of this vault. Start here.

## Sections

- [[glossary]] — terms used across this vault
- [[mcp-protocol-spec]] — MCP tool contract
- [[001-sqlite-storage]] — ADR: why SQLite
- [[ccm-mcp-server]] — MCP server crate
- [[overview]] — mandatory parser contract every `ccm-lang-*` crate follows
- [[ccm-lang-php]] — PHP parser exception (security-relevant)
- [[limits-spec]] — recursion/fuzzing/benchmark limits
- [[sec-001-php-stack-overflow]] — security advisory
- [[roadmap]] — backlog

## Extension → Crate map

| Extension | Language | Crate |
|---|---|---|
| .rs | Rust | `ccm-lang-rust` |
| .py | Python | `ccm-lang-python` |
| .js/.jsx/.mjs/.cjs/.ts/.mts/.cts/.tsx | JS/TS | `ccm-lang-js-ts` |
| .java | Java | `ccm-lang-java` |
| .cs | C# | `ccm-lang-csharp` |
| .kt | Kotlin | `ccm-lang-kotlin` |
| .cpp/.cc/.cxx/.hpp/.hh/.h | C++ | `ccm-lang-cpp` |
| .go | Go | `ccm-lang-go` |
| .html | HTML | `ccm-lang-html` |
| .css | CSS | `ccm-lang-css` |
| .xml | XML | `ccm-lang-xml` |
| .xaml | XAML | `ccm-lang-xaml` |
| .sh | Bash | `ccm-lang-bash` |
| .ps1/.psm1 | PowerShell | `ccm-lang-powershell` |
| .php | PHP | `ccm-lang-php` |
| .md | Markdown | `ccm-lang-md` |
| .lua | Lua (acceptance-test only) | `ccm-lang-lua` |

Adding a language does not need a new note here beyond a row in this table — see the templates below for the one exception.

## Templates

- [[adr-template]] — architecture decision record template
- [[crate-spec-template]] — crate note template
- [[lang-spec-template]] — language-parser note template (use only for languages with special/security-relevant behavior)

#moc
