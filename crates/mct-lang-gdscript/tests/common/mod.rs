//! Shared helpers for the GDScript parser tests.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{
    LanguageParser, ParsedFile, RelationKind, RelationTarget, SourceFile, SymbolDetail, SymbolKind,
    SymbolRecord, SymbolRelation,
};
use mct_lang_gdscript::GdscriptParser;

pub fn parse(path: &str, contents: &str) -> ParsedFile {
    GdscriptParser
        .parse(&SourceFile {
            relative_path: path.into(),
            contents: contents.into(),
        })
        .unwrap_or_else(|error| panic!("{path} must parse: {error}"))
}

/// The only symbol with this name and kind.
pub fn symbol<'a>(file: &'a ParsedFile, name: &str, kind: SymbolKind) -> &'a SymbolRecord {
    let mut hits = file
        .symbols
        .iter()
        .filter(|s| s.name == name && s.kind == kind);
    let hit = hits
        .next()
        .unwrap_or_else(|| panic!("no {kind:?} {name}: {:#?}", file.symbols));
    assert!(hits.next().is_none(), "several {kind:?} {name}");
    hit
}

pub fn has_symbol(file: &ParsedFile, name: &str, kind: SymbolKind) -> bool {
    file.symbols
        .iter()
        .any(|s| s.name == name && s.kind == kind)
}

pub fn detail<'a>(file: &'a ParsedFile, name: &str, kind: SymbolKind) -> &'a SymbolDetail {
    let id = symbol(file, name, kind).id;
    file.symbol_details
        .iter()
        .find(|d| d.symbol == id)
        .unwrap_or_else(|| panic!("no detail for {name}"))
}

/// Every relation of `kind` to `to`, with the name of its owner and its target.
pub fn relations<'a>(
    file: &'a ParsedFile,
    kind: RelationKind,
    to: &str,
) -> Vec<(&'a str, &'a SymbolRelation, &'a RelationTarget)> {
    file.relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == kind && r.to_name == to)
        .map(|(index, relation)| {
            let target = file
                .relation_targets
                .iter()
                .find(|t| t.relation == index)
                .expect("every GDScript relation carries a target");
            let owner = file.symbols[relation.from as usize].name.as_str();
            (owner, relation, target)
        })
        .collect()
}

/// The only relation of `kind` from `from` to `to`.
pub fn relation<'a>(
    file: &'a ParsedFile,
    from: &str,
    kind: RelationKind,
    to: &str,
) -> (&'a SymbolRelation, &'a RelationTarget) {
    let hits: Vec<_> = relations(file, kind, to)
        .into_iter()
        .filter(|(owner, _, _)| *owner == from)
        .collect();
    match hits.as_slice() {
        [(_, relation, target)] => (relation, target),
        _ => panic!(
            "expected one {kind:?} {from} -> {to}, got {}: {:#?}",
            hits.len(),
            file.relations
        ),
    }
}
