//! Text scenes (`.tscn`) and text resources (`.tres`): Godot's own
//! `[section key="value"]` format, read statically.
//!
//! The file is a `Module`. Each external resource it loads is an `Imports` of
//! that file, and a script is also a `References` to its class, which is how a
//! script attached only in the editor stays alive. A signal connection made in
//! the editor (`[connection … to="Node" method="_on_x"]`) is a `Calls` of that
//! method on the script attached to the target node.

use std::collections::HashMap;

use mct_core::{
    Location, ParsedFile, RelationKind, RelationTarget, SourceFile, SymbolKind, SymbolRecord,
    SymbolRelation,
};

/// The module symbol every relation of the file comes from.
const MODULE: u32 = 0;

/// `key="value"` (or a bare `key=value`) attributes of a `[section …]` line.
fn attributes(header: &str) -> HashMap<&str, &str> {
    let mut found = HashMap::new();
    let mut rest = header;
    while let Some((key, after)) = rest.split_once('=') {
        let key = key.rsplit(' ').next().unwrap_or(key).trim();
        let (value, tail) = if let Some(quoted) = after.strip_prefix('"') {
            quoted.split_once('"').unwrap_or((quoted, ""))
        } else if after.starts_with("ExtResource(") {
            after.split_once(')').unwrap_or((after, ""))
        } else {
            after.split_once(' ').unwrap_or((after, ""))
        };
        found.insert(key, value);
        rest = tail;
    }
    found
}

/// The id inside `ExtResource("id")` (Godot 4) or `ExtResource( 1 )` (Godot 3).
fn ext_resource_id(value: &str) -> Option<&str> {
    let inner = value
        .trim()
        .strip_prefix("ExtResource(")?
        .trim_end_matches(')')
        .trim();
    Some(inner.trim_matches('"'))
}

/// A `res://` path to a local script, scene or resource, kept `res://`-rooted
/// for the index to root at the project.
fn resource_path(value: &str) -> Option<&str> {
    let path = value.strip_prefix("res://")?;
    let safe = !path.is_empty()
        && !path.contains(['\\', ':'])
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."));
    let known = crate::RESOURCE_EXTENSIONS
        .iter()
        .any(|extension| path.ends_with(&format!(".{extension}")));
    (safe && known).then_some(value)
}

/// The scene-relative path of a node, as `[connection]` names it.
fn node_path(attributes: &HashMap<&str, &str>) -> Option<String> {
    let name = attributes.get("name")?;
    Some(match attributes.get("parent") {
        None => ".".into(),
        Some(&".") => (*name).into(),
        Some(parent) => format!("{parent}/{name}"),
    })
}

struct Writer {
    parsed: ParsedFile,
}

impl Writer {
    fn emit(&mut self, kind: RelationKind, to_name: &str, line: u32, target: RelationTarget) {
        self.parsed.relation_targets.push(RelationTarget {
            relation: self.parsed.relations.len(),
            ..target
        });
        self.parsed.relations.push(SymbolRelation {
            from: MODULE,
            kind,
            to_name: to_name.into(),
            location: Location {
                line,
                column: 1,
                byte_len: 1,
                end_line: None,
            },
        });
    }

    fn in_file(
        &mut self,
        kind: RelationKind,
        to_name: &str,
        line: u32,
        path: &str,
        of: SymbolKind,
    ) {
        self.emit(
            kind,
            to_name,
            line,
            RelationTarget {
                path: Some(path.into()),
                kind: Some(of),
                ..Default::default()
            },
        );
    }
}

pub(crate) fn parse(file: &SourceFile) -> ParsedFile {
    let lines: Vec<&str> = file.contents.lines().collect();
    let mut writer = Writer {
        parsed: ParsedFile::default(),
    };
    writer.parsed.symbols.push(SymbolRecord {
        id: MODULE,
        name: crate::basename(&file.relative_path).into(),
        kind: SymbolKind::Module,
        location: Location {
            line: 1,
            column: 1,
            byte_len: file.contents.len() as u32,
            end_line: Some(lines.len().max(1) as u32),
        },
        parent: None,
        level: None,
    });

    let mut resources: HashMap<&str, &str> = HashMap::new();
    let mut node_scripts: HashMap<String, &str> = HashMap::new();
    let mut connections = Vec::new();
    let mut node: Option<String> = None;
    for (index, raw) in lines.iter().enumerate() {
        let line = index as u32 + 1;
        let text = raw.trim();
        if let Some(header) = text.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            let (tag, rest) = header.split_once(' ').unwrap_or((header, ""));
            let attributes = attributes(rest);
            node = None;
            match tag {
                "ext_resource" => {
                    let Some(path) = attributes.get("path").and_then(|p| resource_path(p)) else {
                        continue;
                    };
                    if let Some(id) = attributes.get("id") {
                        resources.insert(id, path);
                    }
                    writer.in_file(
                        RelationKind::Imports,
                        crate::basename(path),
                        line,
                        path,
                        SymbolKind::Module,
                    );
                    if path.ends_with(".gd") {
                        writer.in_file(
                            RelationKind::References,
                            crate::script_class_name(path),
                            line,
                            path,
                            SymbolKind::Class,
                        );
                    }
                }
                "node" => node = node_path(&attributes),
                "connection" => connections.push((line, attributes)),
                _ => {}
            }
        } else if let Some((key, value)) = text.split_once('=') {
            if key.trim() == "script" {
                if let (Some(node), Some(path)) = (
                    node.as_ref(),
                    ext_resource_id(value).and_then(|id| resources.get(id)),
                ) {
                    node_scripts.insert(node.clone(), path);
                }
            }
        }
    }

    for (line, attributes) in connections {
        let (Some(to), Some(method)) = (attributes.get("to"), attributes.get("method")) else {
            continue;
        };
        if let Some(path) = node_scripts.get(*to).filter(|path| path.ends_with(".gd")) {
            writer.in_file(RelationKind::Calls, method, line, path, SymbolKind::Method);
        }
        // A custom signal is declared by the script of the emitting node.
        if let (Some(signal), Some(path)) = (
            attributes.get("signal"),
            attributes
                .get("from")
                .and_then(|from| node_scripts.get(*from))
                .filter(|path| path.ends_with(".gd")),
        ) {
            writer.in_file(
                RelationKind::References,
                signal,
                line,
                path,
                SymbolKind::Field,
            );
        }
    }
    writer.parsed
}
