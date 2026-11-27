//! `project.godot`: its singleton autoloads are global names.
//!
//! An autoload is a `Class` that imports its script and also `extends` the
//! script class, so a call through the autoload name (`Game.start()`) reaches
//! the script's members through the index's ancestor walk. The script class is
//! named after the file stem; a script that declares a different
//! `class_name` is not reached that way. Paths stay `res://` paths, which the
//! index roots at this file's own directory.

use mct_core::{
    Location, ParsedFile, RelationKind, RelationTarget, SymbolKind, SymbolRecord, SymbolRelation,
};

fn section<'a>(contents: &'a str, name: &'static str) -> impl Iterator<Item = (usize, &'a str)> {
    contents
        .lines()
        .enumerate()
        .skip_while(move |(_, line)| line.trim() != name)
        .skip(1)
        .take_while(|(_, line)| !line.trim_start().starts_with('['))
}

/// The text between a value's double quotes, when it has no escapes.
fn quoted(value: &str) -> Option<&str> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.contains(['"', '\\'])).then_some(inner)
}

/// Globally named autoloads are the ones with a leading `*`.
pub(crate) fn autoloads(contents: &str) -> ParsedFile {
    let mut parsed = ParsedFile::default();
    for (line_number, line) in section(contents, "[autoload]") {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || !name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
            continue;
        }
        let Some(resource) = quoted(value.trim()).and_then(|v| v.strip_prefix("*res://")) else {
            continue;
        };
        if resource.is_empty()
            || resource
                .split('/')
                .any(|part| matches!(part, "" | "." | ".."))
        {
            continue;
        }
        let line = line_number as u32 + 1;
        let id = parsed.symbols.len() as u32;
        let location = Location {
            line,
            column: 1,
            byte_len: name.len() as u32,
            end_line: Some(line),
        };
        parsed.symbols.push(SymbolRecord {
            id,
            name: name.into(),
            kind: SymbolKind::Class,
            location,
            parent: None,
            level: None,
        });
        if !resource.ends_with(".gd") {
            // A scene autoload is a global name with no script class.
            continue;
        }
        for (kind, to_name, target_kind) in [
            (
                RelationKind::Imports,
                crate::basename(resource),
                SymbolKind::Module,
            ),
            (
                RelationKind::Extends,
                crate::script_class_name(resource),
                SymbolKind::Class,
            ),
        ] {
            let relation = parsed.relations.len();
            parsed.relations.push(SymbolRelation {
                from: id,
                to_name: to_name.into(),
                kind,
                location: Location {
                    end_line: None,
                    ..location
                },
            });
            parsed.relation_targets.push(RelationTarget {
                relation,
                path: Some(format!("res://{resource}")),
                kind: Some(target_kind),
                ..Default::default()
            });
        }
    }
    parsed
}
