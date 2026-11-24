//! Blade templates (`*.blade.php`): the dependencies a template declares.
//!
//! The template scanner reads literal directive arguments and component tags.
//! Blade echoes and PHP blocks are parsed as PHP fragments for their references.

use mct_core::{Location, ParsedFile, RelationKind, RelationTarget, SourceFile, SymbolKind};

use super::keys::KeyKind;
use super::views::{application_root, blade_file, module_name_of};
use crate::names::{namespace_alias, ImportKind};
use crate::Walker;
use tree_sitter::Parser;

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
    let mut parser = None;
    let mut i = 0;
    while i < bytes.len() {
        let rest = &source[i..];
        if rest.starts_with("{{--") {
            i += rest.find("--}}").map_or(rest.len(), |at| at + 4);
        } else if rest.starts_with("@@") {
            i += 2;
        } else if rest.starts_with("@verbatim") {
            i += rest.find("@endverbatim").map_or(rest.len(), |at| at + 12);
        } else if rest.starts_with("@{{") || rest.starts_with("@{!!") {
            i += echo_fragment(&rest[1..]).map_or(1, |(_, _, next)| next + 1);
        } else if rest.starts_with("{{") || rest.starts_with("{!!") {
            let Some((body, start, next)) = echo_fragment(rest) else {
                break;
            };
            php_fragment(
                &mut walker,
                &mut parser,
                body,
                location_at(&lines, i + start, 0),
                true,
            );
            i += next;
        } else if let Some(body) = rest.strip_prefix("<?php") {
            let end = body.find("?>").unwrap_or(body.len());
            php_fragment(
                &mut walker,
                &mut parser,
                &body[..end],
                location_at(&lines, i + 5, 0),
                false,
            );
            i += 5 + (end + 2).min(body.len());
        } else if let Some(body) = rest.strip_prefix("@php").filter(|body| {
            body.starts_with(char::is_whitespace) && !body.trim_start().starts_with('(')
        }) {
            let end = body.find("@endphp").unwrap_or(body.len());
            php_fragment(
                &mut walker,
                &mut parser,
                &body[..end],
                location_at(&lines, i + 4, 0),
                false,
            );
            i += 4 + (end + 7).min(body.len());
        } else if let Some(tail) = rest.strip_prefix('@') {
            let name: String = tail.chars().take_while(char::is_ascii_alphabetic).collect();
            let after = i + 1 + name.len();
            match directive_arguments(source, after) {
                Some((arguments, next)) => {
                    let loc = location_at(&lines, i, next - i);
                    if name == "php" {
                        let open = source[after..].trim_start_matches(' ');
                        let start = source.len() - open.len() + 1;
                        php_fragment(
                            &mut walker,
                            &mut parser,
                            &source[start..next - 1],
                            location_at(&lines, start, 0),
                            true,
                        );
                    } else {
                        directive(&mut walker, &name, &arguments, loc);
                    }
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

/// Find the closing echo delimiter outside strings and nested PHP expressions.
fn echo_fragment(rest: &str) -> Option<(&str, usize, usize)> {
    let (open, close) = if rest.starts_with("{{") {
        ("{{", "}}")
    } else if rest.starts_with("{!!") {
        ("{!!", "!!}")
    } else {
        return None;
    };
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut comment = None;
    let mut chars = rest[open.len()..].char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        let next = chars.peek().map(|(_, c)| *c);
        if let Some(kind) = comment {
            match (kind, ch, next) {
                ('/', '\n', _) => comment = None,
                ('*', '*', Some('/')) => {
                    chars.next();
                    comment = None;
                }
                _ => {}
            }
            continue;
        }
        if let Some(end) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == end {
                quote = None;
            }
            continue;
        }
        let at = open.len() + offset;
        if depth == 0 && rest[at..].starts_with(close) {
            return Some((&rest[open.len()..at], open.len(), at + close.len()));
        }
        match (ch, next) {
            ('\'' | '"' | '`', _) => quote = Some(ch),
            ('/', Some('/' | '*')) => {
                comment = next;
                chars.next();
            }
            ('#', n) if n != Some('[') => comment = Some('/'),
            ('(' | '[' | '{', _) => depth += 1,
            (')' | ']' | '}', _) => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    None
}

fn php_fragment(
    walker: &mut Walker,
    parser: &mut Option<Parser>,
    body: &str,
    start: Location,
    expression: bool,
) {
    let parser = parser.get_or_insert_with(|| {
        let mut parser = Parser::new();
        // The PHP-only grammar belongs to the same pinned crate as PHP.
        let _ = parser.set_language(&tree_sitter_php::LANGUAGE_PHP_ONLY.into());
        parser
    });
    let expression_source;
    let source = if expression {
        expression_source = format!("{body};");
        expression_source.as_str()
    } else {
        body
    };
    let Some(tree) = parser.parse(source, None) else {
        return;
    };
    let root = tree.root_node();
    if root.has_error() {
        return;
    }
    let mut fragment = Walker::new(source, walker.path);
    fragment.scope = walker.scope.clone();
    fragment.next_id = 1;
    fragment.visit_children(root, fragment.module_id, None, 0);
    let offset = walker.relations.len();
    walker
        .relations
        .extend(fragment.relations.into_iter().map(|mut relation| {
            relation.from = walker.module_id;
            if relation.location.line == 1 {
                relation.location.column += start.column - 1;
            }
            relation.location.line += start.line - 1;
            relation
        }));
    walker
        .relation_targets
        .extend(fragment.relation_targets.into_iter().map(|mut target| {
            target.relation += offset;
            target
        }));
}

fn directive(walker: &mut Walker, name: &str, args: &[String], loc: Location) {
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
        walker.push_named_view(walker.module_id, &view, loc, RelationKind::Imports);
    }
    if name == "lang" {
        if let Some(key) = view_at(0) {
            walker.laravel_key_reference(walker.module_id, KeyKind::Translation, &key, loc);
        }
    }
    let class = match name {
        "inject" => view_at(1),
        "use" => view_at(0),
        _ => None,
    };
    if let Some(class) = class {
        if name == "use" {
            walker
                .scope
                .import(ImportKind::Class, &class, view_at(1).as_deref());
        }
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
