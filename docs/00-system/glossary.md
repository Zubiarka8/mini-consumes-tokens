# Glossary

- **MOC** — Map of Content; a note that links out to a topic's other notes instead of holding content itself. See [[00-index]].
- **Symbol** — a definable code entity (`SymbolRecord`): function, class, struct, heading, etc.
- **Relation** — a directed link from one symbol to another (`SymbolRelation`): call, import, extends/implements, reference.
- **LanguageParser** — the trait every `ccm-lang-*` crate implements to turn source text into symbols/relations. See [[overview]].
- **Blast radius** — everything that could break if a symbol changes: its callers, references, and likely-affected tests. Computed by `impact_analysis`.
- **WikiLink** — `[[Note Name]]` syntax; indexed as a `References` relation by `ccm-lang-md`.
- **Tag** — `#tag` syntax; indexed as a `tag:`-prefixed `References` relation by `ccm-lang-md`.

#glossary
