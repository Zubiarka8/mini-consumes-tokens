//! Exports the Next.js App Router may load by file convention.
//!
//! Since Next.js 13 the App Router reads the files of an `app` directory
//! (`<root>/app/` or `<root>/src/app/`) by name: the default export of a
//! `page`, `layout`, `template`, `loading`, `error`, `global-error`,
//! `not-found` or `default` file is rendered, a `route` file's `GET`, `HEAD`,
//! `POST`, `PUT`, `PATCH`, `DELETE` and `OPTIONS` exports handle requests,
//! and `generateStaticParams`, `generateMetadata` and `generateViewport` are
//! called at build or request time. Nothing in the source names these
//! exports, so `find_dead_code` would report them as unreferenced.
//!
//! What this module records is **a possibility, never a fact**: a project
//! can use the Pages Router only, or a `next.config` that changes what is
//! served, and the framework version is a property of the project, not of
//! the file. So the hint is attached only to an exported declaration with a
//! convention name in a convention file, and it carries the requirement that
//! `<root>/package.json` declare `next` at a major not below 13; the index
//! evaluates that when the hint is shown. The root is always the path prefix
//! before `app/` (or `src/app/`), never a guess about an enclosing project.
//!
//! Deliberately unmarked, so a hint never over-claims: files in a private
//! folder (`_name`), metadata files (`icon`, `sitemap`, `opengraph-image`),
//! `.mjs`/`.cjs` files, re-exports (`export { GET }`) and the Pages Router.

use mct_core::{EntryPointHint, PackageRequirement, SymbolKind, SymbolRecord};
use tree_sitter::Node;

const FRAMEWORK: &str = "next.js";
const RULE: &str = "app-router-file-convention";
const PACKAGE: &str = "next";
/// The release that introduced the App Router.
const MIN_MAJOR: u32 = 13;
const EXTENSIONS: &[&str] = &["js", "jsx", "ts", "tsx"];
const ROUTE_HANDLERS: &[&str] = &["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];

/// The application root and the file name of `path`, when it is a file the
/// App Router reads by name.
fn convention_file(path: &str) -> Option<(String, &str)> {
    let segments: Vec<&str> = path.split('/').collect();
    let (file, folders) = segments.split_last()?;
    let app = folders.iter().position(|segment| *segment == "app")?;
    if folders[app + 1..].iter().any(|s| s.starts_with('_')) {
        return None;
    }
    let (stem, extension) = file.rsplit_once('.')?;
    if !EXTENSIONS.contains(&extension) || exports_of(stem).is_none() {
        return None;
    }
    let root_len = if app > 0 && folders[app - 1] == "src" {
        app - 1
    } else {
        app
    };
    let root: String = folders[..root_len]
        .iter()
        .map(|segment| format!("{segment}/"))
        .collect();
    Some((root, file))
}

/// The named exports a convention file `stem` is read for, besides its
/// default export (`None` when `stem` is no convention file).
fn exports_of(stem: &str) -> Option<&'static [&'static str]> {
    match stem {
        "page" | "layout" => Some(&[
            "generateStaticParams",
            "generateMetadata",
            "generateViewport",
        ]),
        "route" => Some(&[
            "GET",
            "HEAD",
            "POST",
            "PUT",
            "PATCH",
            "DELETE",
            "OPTIONS",
            "generateStaticParams",
        ]),
        "template" | "loading" | "error" | "global-error" | "not-found" | "default" => Some(&[]),
        _ => None,
    }
}

/// Whether a `stem` file's default export is read (a route has none).
fn reads_default(stem: &str) -> bool {
    stem != "route"
}

/// The hints for the exported declarations of `root` (a `program`) that the
/// App Router may load from `path`, against the `symbols` parsed from it.
pub(crate) fn hints(
    path: &str,
    root: Node,
    source: &str,
    symbols: &[SymbolRecord],
) -> Vec<EntryPointHint> {
    let Some((app_root, file)) = convention_file(path) else {
        return Vec::new();
    };
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let named = exports_of(stem).unwrap_or_default();
    let mut out = Vec::new();
    let mut cursor = root.walk();
    for statement in root.named_children(&mut cursor) {
        if statement.kind() != "export_statement" {
            continue;
        }
        let Some(declaration) = statement.child_by_field_name("declaration") else {
            continue;
        };
        let is_default = has_default_keyword(statement);
        for name in declared_names(declaration, source) {
            let export = if is_default { "default" } else { name.0 };
            let read = if is_default {
                reads_default(stem)
            } else {
                named.contains(&export)
            };
            if !read {
                continue;
            }
            let Some(symbol) = symbols.iter().find(|s| {
                s.parent.is_none()
                    && s.kind == SymbolKind::Function
                    && s.name == name.0
                    && s.location.line == name.1
            }) else {
                continue;
            };
            let evidence = if is_default {
                format!("default export of {file} under app/")
            } else if ROUTE_HANDLERS.contains(&export) {
                format!("{export} route handler exported from {file} under app/")
            } else {
                format!("{export} exported from {file} under app/")
            };
            out.push(EntryPointHint {
                symbol: symbol.id,
                framework: FRAMEWORK.to_string(),
                rule: RULE.to_string(),
                evidence: format!(
                    "{evidence}; depends on the project using the App Router for that directory"
                ),
                requires: Some(PackageRequirement {
                    manifest_path: format!("{app_root}package.json"),
                    package: PACKAGE.to_string(),
                    min_major: MIN_MAJOR,
                }),
            });
        }
    }
    out
}

fn has_default_keyword(statement: Node) -> bool {
    let mut cursor = statement.walk();
    let found = statement
        .children(&mut cursor)
        .any(|child| child.kind() == "default");
    found
}

/// `(name, 1-based line)` of each function a declaration declares: a
/// function declaration, or a `const` bound to an arrow or function
/// expression.
fn declared_names<'s>(declaration: Node, source: &'s str) -> Vec<(&'s str, u32)> {
    let name_of = |node: Node| {
        source
            .get(node.byte_range())
            .map(|name| (name, node.start_position().row as u32 + 1))
    };
    match declaration.kind() {
        "function_declaration" | "generator_function_declaration" => declaration
            .child_by_field_name("name")
            .and_then(name_of)
            .into_iter()
            .collect(),
        "lexical_declaration" | "variable_declaration" => {
            let mut cursor = declaration.walk();
            let names = declaration
                .named_children(&mut cursor)
                .filter(|d| d.kind() == "variable_declarator")
                .filter(|d| {
                    d.child_by_field_name("value").is_some_and(|v| {
                        matches!(v.kind(), "arrow_function" | "function_expression")
                    })
                })
                .filter_map(|d| d.child_by_field_name("name"))
                .filter(|n| n.kind() == "identifier")
                .filter_map(name_of)
                .collect();
            names
        }
        _ => Vec::new(),
    }
}
