//! Blade templates (`*.blade.php`): the dependencies a template declares.
//!
//! A template is not PHP, so it is scanned as text for the directives and
//! tags whose argument is a literal. Raw PHP inside it (`@php`, `<?php`),
//! `{{ }}` expressions and every directive with a computed argument are not
//! read: what they depend on is decided when the page renders.

use mct_core::{Location, ParsedFile, RelationKind, RelationTarget, SourceFile, SymbolKind};

use super::views::{application_root, blade_file, module_name_of};
use crate::names::namespace_alias;
use crate::Walker;

pub(crate) fn parse(file: &SourceFile) -> ParsedFile {
    let source = file.contents.as_str();
    let mut walker = Walker::new(source, &file.relative_path);
    let lines = line_starts(source);
    let end = source.lines().count().max(1) as u32;
    let module = walker.push_symbol(
        module_name_of(&file.relative_path),
        SymbolKind::Module,
        Location {
            line: 1,
            column: 1,
            byte_len: source.len() as u32,
            end_line: Some(end),
        },
        None,
    );
    walker.module_id = module;
    let root = application_root(&file.relative_path);

    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &source[i..];
        if rest.starts_with("{{--") {
            i += rest.find("--}}").map_or(rest.len(), |at| at + 4);
        } else if rest.starts_with("@@") {
            i += 2;
        } else if rest.starts_with("@verbatim") {
            i += rest.find("@endverbatim").map_or(rest.len(), |at| at + 12);
        } else if let Some(tail) = rest.strip_prefix('@') {
            let name: String = tail.chars().take_while(char::is_ascii_alphabetic).collect();
            let after = i + 1 + name.len();
            match directive_arguments(source, after) {
                Some((arguments, next)) => {
                    let loc = location_at(&lines, i, next - i);
                    directive(&mut walker, root.as_deref(), &name, &arguments, loc);
                    i = next;
                }
                None => i = after.max(i + 1),
            }
        } else if let Some(tag) = rest.strip_prefix("<x-") {
            let name: String = tag
                .chars()
                .take_while(|c| !c.is_whitespace() && !matches!(c, '/' | '>'))
                .collect();
            let loc = location_at(&lines, i, 3 + name.len());
            component(&mut walker, root.as_deref(), &name, loc);
            i += 3 + name.len();
        } else {
            i += rest.chars().next().map_or(1, char::len_utf8);
        }
    }

    ParsedFile {
        symbols: walker.symbols,
        relations: walker.relations,
        relation_targets: walker.relation_targets,
        ..Default::default()
    }
}

fn directive(walker: &mut Walker, root: Option<&str>, name: &str, args: &[String], loc: Location) {
    let view_at = |index: usize| args.get(index).and_then(|a| string_literal(a));
    let views: Vec<String> = match name {
        "extends" | "include" | "includeIf" | "component" => view_at(0).into_iter().collect(),
        "includeWhen" | "includeUnless" => view_at(1).into_iter().collect(),
        // Every candidate is recorded; only the first that exists renders.
        "includeFirst" => args
            .first()
            .map(|list| list_literals(list))
            .unwrap_or_default(),
        "each" => [view_at(0), view_at(3)].into_iter().flatten().collect(),
        _ => Vec::new(),
    };
    for view in views {
        if let Some(root) = root {
            if let Some(path) = blade_file(root, "resources/views", &view) {
                walker.push_template_import(walker.module_id, path, loc);
            }
        }
    }
    let class = match name {
        "inject" => view_at(1),
        "use" => view_at(0),
        _ => None,
    };
    if let Some(class) = class {
        class_directive(walker, name == "use", &class, loc);
    }
}

/// `@use('App\Models\Order')` imports a file, `@inject('x', 'App\Y')` uses
/// the class.
fn class_directive(walker: &mut Walker, import: bool, class: &str, loc: Location) {
    let full = class.trim_matches('\\');
    let (namespace, short) = full.rsplit_once('\\').unwrap_or(("", full));
    if short.is_empty() {
        return;
    }
    let relation = walker.relations.len();
    let kind = if import {
        RelationKind::Imports
    } else {
        RelationKind::References
    };
    walker.push_relation(walker.module_id, kind, short.to_string(), loc);
    walker.relation_targets.push(RelationTarget {
        relation,
        kind: import.then_some(SymbolKind::Module),
        target_module: Some(namespace_alias(namespace)),
        ..Default::default()
    });
}

/// `<x-forms.input-text>`: the class `App\View\Components\Forms\InputText` and
/// the anonymous template `resources/views/components/forms/input-text.blade.php`.
/// Which one renders depends on what exists, so both are recorded as
/// references (not as imports: the one that does not exist must not read as
/// a missing dependency) and the index resolves the one that does. `App` is Laravel's default root
/// namespace; a renamed one leaves the class reference unresolved.
fn component(walker: &mut Walker, root: Option<&str>, tag: &str, loc: Location) {
    if tag.is_empty() || tag.contains(':') || tag.starts_with("dynamic-component") {
        return;
    }
    if let Some(root) = root {
        if let Some(path) = blade_file(root, "resources/views/components", tag) {
            walker.push_template_relation(walker.module_id, RelationKind::References, path, loc);
        }
    }
    let segments: Vec<String> = tag.split('.').map(studly).collect();
    let Some((short, namespace)) = segments.split_last() else {
        return;
    };
    if short.is_empty() || segments.iter().any(String::is_empty) {
        return;
    }
    let mut full = String::from("App\\View\\Components");
    for segment in namespace {
        full.push('\\');
        full.push_str(segment);
    }
    let relation = walker.relations.len();
    walker.push_relation(
        walker.module_id,
        RelationKind::References,
        short.clone(),
        loc,
    );
    walker.relation_targets.push(RelationTarget {
        relation,
        kind: Some(SymbolKind::Class),
        target_module: Some(namespace_alias(&full)),
        ..Default::default()
    });
}

fn studly(segment: &str) -> String {
    segment
        .split('-')
        .flat_map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
        })
        .collect()
}

/// The top-level arguments of a directive written as `@name(...)` starting
/// at byte `at`, and the offset after the closing parenthesis. Quotes and
/// nested brackets are respected; whitespace may separate name and `(`.
fn directive_arguments(source: &str, at: usize) -> Option<(Vec<String>, usize)> {
    let bytes = source.as_bytes();
    let mut open = at;
    while bytes.get(open).is_some_and(|b| *b == b' ') {
        open += 1;
    }
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut args = Vec::new();
    let mut start = open + 1;
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == b'\\' {
                    i += 1;
                } else if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'\'' | b'"' => quote = Some(b),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        args.push(source[start..i].trim().to_string());
                        return Some((args, i + 1));
                    }
                }
                b',' if depth == 1 => {
                    args.push(source[start..i].trim().to_string());
                    start = i + 1;
                }
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// The text of a plain quoted string with no interpolation or escapes.
fn string_literal(argument: &str) -> Option<String> {
    let inner = argument
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| argument.strip_prefix('"').and_then(|s| s.strip_suffix('"')))?;
    (!inner.is_empty() && !inner.contains(['\'', '"', '$', '{']))
        .then(|| inner.replace("\\\\", "\\"))
}

/// The literals of `['a', 'b']`.
fn list_literals(list: &str) -> Vec<String> {
    list.trim()
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .map(|inner| {
            inner
                .split(',')
                .filter_map(|s| string_literal(s.trim()))
                .collect()
        })
        .unwrap_or_default()
}

fn line_starts(source: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(source.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

fn location_at(lines: &[usize], offset: usize, len: usize) -> Location {
    let line = lines.partition_point(|start| *start <= offset);
    let start = lines.get(line - 1).copied().unwrap_or(0);
    Location {
        line: line as u32,
        column: (offset - start) as u32 + 1,
        byte_len: len as u32,
        end_line: Some(line as u32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relations(path: &str, contents: &str) -> Vec<(RelationKind, String, Option<String>)> {
        let parsed = parse(&SourceFile {
            relative_path: path.to_string(),
            contents: contents.to_string(),
        });
        parsed
            .relations
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let target = parsed.relation_targets.iter().find(|t| t.relation == i);
                (
                    r.kind,
                    r.to_name.clone(),
                    target.and_then(|t| t.path.clone().or_else(|| t.target_module.clone())),
                )
            })
            .collect()
    }

    fn paths(path: &str, contents: &str) -> Vec<String> {
        relations(path, contents)
            .into_iter()
            .filter_map(|(_, _, target)| target)
            .collect()
    }

    const PAGE: &str = "resources/views/page.blade.php";

    #[test]
    fn literal_view_arguments_name_template_files() {
        let found = paths(
            PAGE,
            "@extends('layouts.app')\n@include(\"parts.row\", ['a' => 1])\n@includeWhen($x, 'parts.maybe')\n@each('parts.item', $items, 'item', 'parts.empty')\n@includeFirst(['a.one', 'a.two'])",
        );
        assert_eq!(
            found,
            [
                "resources/views/layouts/app.blade.php",
                "resources/views/parts/row.blade.php",
                "resources/views/parts/maybe.blade.php",
                "resources/views/parts/item.blade.php",
                "resources/views/parts/empty.blade.php",
                "resources/views/a/one.blade.php",
                "resources/views/a/two.blade.php",
            ]
        );
    }

    #[test]
    fn computed_escaped_commented_and_verbatim_directives_record_nothing() {
        let found = relations(
            PAGE,
            "@include($view)\n@include('a.' . $b)\n@@include('escaped')\n{{-- @include('commented') --}}\n@verbatim @include('literal') @endverbatim\n@include(\"interp.{$x}\")",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn malformed_templates_never_panic_and_keep_what_was_read() {
        for broken in [
            "@include('a.b'",
            "@include(",
            "@extends('a.b') {{-- unterminated",
            "<x-",
            "@verbatim @include('x')",
            "@",
            "é @include('é.ñ') <x-é />",
        ] {
            let _ = relations(PAGE, broken);
        }
        let kept = paths(PAGE, "@extends('a.b')\n@include('c.d'");
        assert_eq!(kept, ["resources/views/a/b.blade.php"]);
    }

    #[test]
    fn a_template_outside_the_conventional_directories_names_no_file() {
        assert!(paths("docs/example.blade.php", "@extends('layouts.app')").is_empty());
    }

    #[test]
    fn locations_point_at_the_directive() {
        let parsed = parse(&SourceFile {
            relative_path: PAGE.to_string(),
            contents: "<p>\n  @include('x.y')\n".to_string(),
        });
        let loc = &parsed.relations[0].location;
        assert_eq!((loc.line, loc.column), (2, 3));
    }

    #[test]
    fn component_tags_name_a_class_and_an_anonymous_template() {
        let found = relations(PAGE, "<x-forms.input-text name=\"a\" />\n<x-slot:title>\n<x-dynamic-component :component=\"$c\" />\n<x-pkg::button />");
        assert_eq!(
            found,
            [
                (
                    RelationKind::References,
                    "input-text.blade".into(),
                    Some("resources/views/components/forms/input-text.blade.php".into())
                ),
                (
                    RelationKind::References,
                    "InputText".into(),
                    Some("\\App\\View\\Components\\Forms".into())
                ),
            ]
        );
    }

    #[test]
    fn inject_and_use_name_classes() {
        let found = relations(
            PAGE,
            "@inject('m', 'App\\Services\\Metrics')\n@use('App\\Models\\Order')",
        );
        assert_eq!(
            found,
            [
                (
                    RelationKind::References,
                    "Metrics".into(),
                    Some("\\App\\Services".into())
                ),
                (
                    RelationKind::Imports,
                    "Order".into(),
                    Some("\\App\\Models".into())
                ),
            ]
        );
    }
}
