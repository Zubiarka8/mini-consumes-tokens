//! `project.godot` as a dependency manifest: its enabled editor plugins.
//! Its autoloads are symbols, which `mct-lang-gdscript` parses.

use super::ManifestDependency;

fn section<'a>(contents: &'a str, name: &'static str) -> impl Iterator<Item = &'a str> {
    contents
        .lines()
        .skip_while(move |line| line.trim() != name)
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
}

pub(super) fn plugins(contents: &str) -> Vec<ManifestDependency> {
    let Some(value) = section(contents, "[editor_plugins]")
        .filter_map(|line| line.split_once('='))
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
}
