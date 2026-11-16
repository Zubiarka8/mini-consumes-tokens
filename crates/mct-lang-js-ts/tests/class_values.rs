//! A class or enum the file declares and then names as a value (in an array,
//! an object, an argument, a return) is referenced from that use, as a
//! same-file function used as a value is. Its own name inside its body is
//! not a use, and a local that shadows it is not either.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::RelationKind;
use mct_lang_js_ts::JsTsParser;

fn references(source: &str) -> Vec<(String, String)> {
    let parsed = mct_corpus::parse_source(&JsTsParser, "src/a.ts", "TypeScript", source);
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| {
            (
                parsed.symbols[r.from as usize].name.clone(),
                r.to_name.clone(),
            )
        })
        .collect()
}

#[test]
fn a_class_or_enum_named_as_a_value_is_referenced() {
    let found = references(
        "class Widget {}\nenum Mode { A }\n\
         export const all = [Widget, Mode];\n\
         export const map = { Widget };\n\
         export function pick() { return Mode; }\n",
    );
    // A module-level initializer belongs to the file's module symbol, `a`.
    for (owner, target) in [("a", "Widget"), ("a", "Mode"), ("pick", "Mode")] {
        assert!(
            found.contains(&(owner.into(), target.into())),
            "{owner} -> {target} missing in {found:?}"
        );
    }
}

#[test]
fn a_class_naming_itself_or_a_shadowing_local_is_not_a_use() {
    let found = references(
        "class Widget { static make() { return Widget; } }\n\
         export function f(Widget) { return [Widget]; }\n",
    );
    assert!(
        !found.iter().any(|(_, t)| t == "Widget"),
        "unexpected reference in {found:?}"
    );
}

fn calls(source: &str) -> Vec<(String, String)> {
    let parsed = mct_corpus::parse_source(&JsTsParser, "src/a.ts", "TypeScript", source);
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| {
            (
                parsed.symbols[r.from as usize].name.clone(),
                r.to_name.clone(),
            )
        })
        .collect()
}

#[test]
fn a_call_in_an_enum_member_or_a_field_initializer_belongs_to_it() {
    let found = calls(
        "enum Mode { A = size() }\n\
         class Box { static x = make(); y = build(); }\n",
    );
    for (owner, target) in [("Mode", "size"), ("x", "make"), ("y", "build")] {
        assert!(
            found.contains(&(owner.into(), target.into())),
            "{owner} -> {target} missing in {found:?}"
        );
    }
}
