//! Godot project settings that participate in the index.

use mct_core::{
    Location, ParsedFile, RelationKind, RelationTarget, SymbolKind, SymbolRecord, SymbolRelation,
};

use super::ManifestDependency;

fn section<'a>(contents: &'a str, name: &'static str) -> impl Iterator<Item = (usize, &'a str)> {
    contents
        .lines()
        .enumerate()
        .skip_while(move |(_, line)| line.trim() != name)
        .skip(1)
        .take_while(|(_, line)| !line.trim_start().starts_with('['))
}

pub(super) fn plugins(contents: &str) -> Vec<ManifestDependency> {
    let Some(value) = section(contents, "[editor_plugins]")
        .filter_map(|(_, line)| line.split_once('='))
        .find_map(|(key, value)| (key.trim() == "enabled").then_some(value.trim()))
        .and_then(|value| value.strip_prefix("PackedStringArray(")?.strip_suffix(')'))
    else {
        return Vec::new();
    };
    toml::from_str::<std::collections::BTreeMap<String, Vec<String>>>(&format!(
        "enabled = [{value}]"
    ))
    .ok()
    .and_then(|mut config| config.remove("enabled"))
    .unwrap_or_default()
    .into_iter()
    .map(|name| ManifestDependency {
        name,
        version: None,
    })
    .collect()
}

/// Globally named autoloads are declared by the leading `*` in project.godot.
/// The underlying script remains a separate symbol with its own members.
///
/// An autoload is an instance of its script, so it also `extends` the script
/// class: a call through the autoload name (`Game.start()`) reaches the
/// script's members through the index's ancestor walk. The script class is
/// named after the file stem; a script that declares a different
/// `class_name` is not reached that way.
pub(crate) fn autoloads(contents: &str, directory: &str) -> ParsedFile {
    let mut parsed = ParsedFile::default();
    for (line_number, line) in section(contents, "[autoload]") {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || !name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
            continue;
        }
        let Ok(value) = serde_json::from_str::<String>(value.trim()) else {
            continue;
        };
        let Some(resource) = value.strip_prefix("*res://") else {
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
        if let Some(script) = resource.strip_suffix(".gd") {
            let path = if directory.is_empty() {
                resource.to_owned()
            } else {
                format!("{directory}/{resource}")
            };
            let file_name = resource.rsplit('/').next().unwrap_or(resource);
            let class = script.rsplit('/').next().unwrap_or(script);
            for (kind, to_name, target_kind) in [
                (RelationKind::Imports, file_name, SymbolKind::Module),
                (RelationKind::Extends, class, SymbolKind::Class),
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
                    path: Some(path.clone()),
                    kind: Some(target_kind),
                    ..Default::default()
                });
            }
        }
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_names(contents: &str) -> Vec<String> {
        plugins(contents).into_iter().map(|dep| dep.name).collect()
    }

    #[test]
    fn enabled_editor_plugins_are_dependencies_without_versions() {
        let contents = "[editor_plugins]\n\nenabled=PackedStringArray(\"res://addons/a/plugin.cfg\", \"res://addons/b/plugin.cfg\")\n";
        assert_eq!(
            plugin_names(contents),
            ["res://addons/a/plugin.cfg", "res://addons/b/plugin.cfg"]
        );
        assert!(plugins(contents).iter().all(|dep| dep.version.is_none()));
    }

    #[test]
    fn missing_empty_or_malformed_plugin_lists_declare_nothing() {
        for contents in [
            "config_version=5\n",
            "[editor_plugins]\nenabled=PackedStringArray()\n",
            "[editor_plugins]\nenabled=PackedStringArray(\"unterminated)\n",
            "[editor_plugins]\nenabled=[\"res://not/a/packed/array.cfg\"]\n",
            // `enabled` in another section is not the plugin list.
            "[editor_plugins]\n[other]\nenabled=PackedStringArray(\"res://x.cfg\")\n",
        ] {
            assert!(plugin_names(contents).is_empty(), "{contents:?}");
        }
    }

    fn autoload_rows(parsed: &ParsedFile) -> Vec<(&str, u32, Option<&str>)> {
        parsed
            .symbols
            .iter()
            .map(|symbol| {
                let path = parsed
                    .relations
                    .iter()
                    .position(|relation| relation.from == symbol.id)
                    .and_then(|index| parsed.relation_targets[index].path.as_deref());
                (symbol.name.as_str(), symbol.location.line, path)
            })
            .collect()
    }

    #[test]
    fn singleton_autoloads_are_classes_that_import_their_script() {
        let contents = concat!(
            "config_version=5\n",
            "[autoload]\n",
            "Game=\"*res://core/game.gd\"\n",
            "Music=\"*res://audio/music.tscn\"\n",
            "[input]\n",
            "Late=\"*res://late.gd\"\n",
        );
        let parsed = autoloads(contents, "client");
        assert_eq!(
            autoload_rows(&parsed),
            [
                ("Game", 3, Some("client/core/game.gd")),
                // A scene autoload is a global name without a script import.
                ("Music", 4, None),
            ]
        );
        let at_root = autoloads("[autoload]\nGame=\"*res://core/game.gd\"\n", "");
        assert_eq!(autoload_rows(&at_root), [("Game", 2, Some("core/game.gd"))]);
        assert!(at_root
            .symbols
            .iter()
            .all(|symbol| symbol.kind == SymbolKind::Class));
        assert_eq!(at_root.relations[0].kind, RelationKind::Imports);
        assert_eq!(at_root.relations[0].to_name, "game.gd");
    }

    #[test]
    fn a_script_autoload_extends_the_class_named_after_its_file() {
        let parsed = autoloads("[autoload]\nGame=\"*res://core/game_state.gd\"\n", "client");
        let extends: Vec<_> = parsed
            .relations
            .iter()
            .zip(&parsed.relation_targets)
            .filter(|(relation, _)| relation.kind == RelationKind::Extends)
            .map(|(relation, target)| {
                (
                    relation.to_name.as_str(),
                    target.kind,
                    target.path.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            extends,
            [(
                "game_state",
                Some(SymbolKind::Class),
                Some("client/core/game_state.gd")
            )]
        );
        let scene = autoloads("[autoload]\nMusic=\"*res://music.tscn\"\n", "");
        assert!(scene.relations.is_empty(), "a scene has no script class");
    }

    #[test]
    fn autoloads_without_a_global_name_or_with_unsafe_paths_are_skipped() {
        let contents = concat!(
            "[autoload]\n",
            "NotGlobal=\"res://plain.gd\"\n",
            "Bad-Name=\"*res://bad.gd\"\n",
            "=\"*res://nameless.gd\"\n",
            "Escape=\"*res://../outside.gd\"\n",
            "Dot=\"*res://./here.gd\"\n",
            "Double=\"*res://a//b.gd\"\n",
            "Empty=\"*res://\"\n",
            "Unquoted=*res://raw.gd\n",
            "Elsewhere=\"*user://save.gd\"\n",
        );
        assert!(autoloads(contents, "").symbols.is_empty());
    }
}
