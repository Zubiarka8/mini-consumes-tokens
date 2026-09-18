# Index

Central map of this vault. Start here.

## Sections

- [[glossary]] — terms used across this vault
- [[mcp-protocol-spec]] — MCP tool contract
- [[001-sqlite-storage]] — ADR: why SQLite
- [[mct-mcp-server]] — MCP server crate
- [[overview]] — mandatory parser contract every `mct-lang-*` crate follows
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
| .lua | Lua (acceptance-test only) | `mct-lang-lua` |

Adding a language does not need a new note here beyond a row in this table — see the templates below for the one exception.

## Templates

- [[adr-template]] — architecture decision record template
- [[crate-spec-template]] — crate note template
- [[lang-spec-template]] — language-parser note template (use only for languages with special/security-relevant behavior)

#moc
