//! Event listeners that Laravel may discover by convention.
//!
//! Laravel 11, 12 and 13 ("Event Discovery") scan the application's
//! `Listeners` directory by default and register every listener class method
//! whose name begins with `handle` or `__invoke` for the event class
//! type-hinted in its signature (union types listen to several events).
//! Nothing in the source names such a class, so `find_dead_code` would report
//! it as unreferenced.
//!
//! What this module records is **a possibility, never a fact**: the scanned
//! directories can be changed with `withEvents(discover: [...])`, the 13.x
//! `shouldBeDiscovered()` hook can opt a listener out at run time, and the
//! framework version is a property of the project, not of the file. So the
//! hint is attached only when the file sits under `<root>/app/Listeners/`, and
//! it carries the requirement that `<root>/composer.json` declare
//! `laravel/framework` at a version whose major is not below the lowest one
//! checked; the index evaluates that when the hint is shown. The root is
//! always the path prefix before `app/Listeners/`, never a guess about an
//! enclosing project, so every application of a monorepo is judged by its own
//! `composer.json`.
//!
//! Deliberately unmarked, so a hint never over-claims: directories configured
//! through `withEvents`, abstract classes, non-public and static methods, and
//! handlers without a class type hint.

use mct_core::{EntryPointHint, PackageRequirement, SymbolId};
use tree_sitter::Node;

use crate::text;

const FRAMEWORK: &str = "laravel";
const RULE: &str = "event-listener-discovery";
const PACKAGE: &str = "laravel/framework";
/// The lowest Laravel major whose documentation was checked for this rule.
const MIN_MAJOR: u32 = 11;
const LISTENERS_DIR: &str = "app/Listeners/";
/// Evidence names at most this many handlers.
const MAX_NAMED_HANDLERS: usize = 3;

/// The application root of a file under `app/Listeners/`: the path prefix
/// before it, with its trailing slash (empty for a root application).
fn application_root(path: &str) -> Option<&str> {
    if path.starts_with(LISTENERS_DIR) {
        return Some("");
    }
    let marker = format!("/{LISTENERS_DIR}");
    path.find(&marker).map(|at| &path[..at + 1])
}

/// The hint for the class `class` (a `class_declaration`) declared in `path`,
/// when it has the shape Laravel's discovery reads.
pub(crate) fn hint(
    path: &str,
    class: Node,
    source: &str,
    symbol: SymbolId,
) -> Option<EntryPointHint> {
    let root = application_root(path)?;
    let mut cursor = class.walk();
    if class
        .children(&mut cursor)
        .any(|child| child.kind() == "abstract_modifier")
    {
        return None;
    }
    let body = class.child_by_field_name("body")?;
    let mut handlers = Vec::new();
    let mut opt_out = false;
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if member.kind() != "method_declaration" {
            continue;
        }
        let Some(name) = member.child_by_field_name("name").map(|n| text(n, source)) else {
            continue;
        };
        if name == "shouldBeDiscovered" {
            opt_out = true;
        }
        if !(name.starts_with("handle") || name.starts_with("__invoke"))
            || !is_plain_public(member, source)
        {
            continue;
        }
        let events = event_types(member, source);
        if !events.is_empty() {
            handlers.push(format!("{name}({})", events.join("|")));
        }
    }
    if handlers.is_empty() {
        return None;
    }
    let shown = handlers
        .iter()
        .take(MAX_NAMED_HANDLERS)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let more = handlers.len().saturating_sub(MAX_NAMED_HANDLERS);
    let mut evidence = format!("{shown}{} in app/Listeners", more_suffix(more));
    evidence.push_str(
        "; depends on event discovery being enabled for that directory and on the Laravel version",
    );
    if opt_out {
        evidence
            .push_str("; the class defines shouldBeDiscovered(), which can opt it out at run time");
    }
    Some(EntryPointHint {
        symbol,
        framework: FRAMEWORK.to_string(),
        rule: RULE.to_string(),
        evidence,
        requires: Some(PackageRequirement {
            manifest_path: format!("{root}composer.json"),
            package: PACKAGE.to_string(),
            min_major: MIN_MAJOR,
        }),
    })
}

fn more_suffix(more: usize) -> String {
    if more == 0 {
        String::new()
    } else {
        format!(" and {more} more")
    }
}

/// A method without `private`/`protected`/`static`/`abstract`: the shape of a
/// handler the documentation shows. Anything else is left unmarked.
fn is_plain_public(method: Node, source: &str) -> bool {
    let mut cursor = method.walk();
    for child in method.children(&mut cursor) {
        match child.kind() {
            "static_modifier" | "abstract_modifier" => return false,
            "visibility_modifier" if text(child, source).trim() != "public" => return false,
            _ => {}
        }
    }
    true
}

/// The class names type-hinted by the first parameter (single, nullable or
/// union), as written. Builtin types and `self`/`static`/`parent` are not
/// events.
fn event_types(method: Node, source: &str) -> Vec<String> {
    let Some(first) = method
        .child_by_field_name("parameters")
        .and_then(|params| params.named_child(0))
    else {
        return Vec::new();
    };
    if first.kind() != "simple_parameter" {
        return Vec::new();
    }
    let mut names = Vec::new();
    if let Some(ty) = first.child_by_field_name("type") {
        collect_class_types(ty, source, &mut names);
    }
    names
}

fn collect_class_types(node: Node, source: &str, names: &mut Vec<String>) {
    if node.kind() == "named_type" {
        let written = text(node, source).trim();
        if !matches!(
            written.to_ascii_lowercase().as_str(),
            "self" | "static" | "parent" | "object" | "mixed" | "iterable" | "callable"
        ) {
            names.push(written.to_string());
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_class_types(child, source, names);
    }
}
