//! A method written in an object literal (`{ tokenizer(src) { … } }`) may be
//! called by a library through the object, never by name in the project
//! (issue #475). It stays a dead-code candidate and carries a "possible
//! entry point" hint, never a rule by name; a class method, a free function
//! and an arrow property are not marked.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, SourceFile};
use mct_lang_js_ts::JsTsParser;

/// `(symbol name, framework, rule)` of every hint.
fn marked(src: &str) -> Vec<(String, String, String)> {
    let parsed = JsTsParser
        .parse(&SourceFile {
            relative_path: "src/ext.ts".to_string(),
            contents: src.to_string(),
        })
        .expect("valid source should parse");
    parsed
        .entry_points
        .iter()
        .map(|hint| {
            let symbol = parsed
                .symbols
                .iter()
                .find(|s| s.id == hint.symbol)
                .expect("a hint names a symbol of its file");
            (
                symbol.name.clone(),
                hint.framework.clone(),
                hint.rule.clone(),
            )
        })
        .collect()
}

#[test]
fn a_method_of_an_object_literal_is_a_possible_entry_point() {
    let found = marked(
        "export const ext = { extensions: [{ name: 'embed', tokenizer(src) { return src; } }],\n\
         renderer(token) { return token; } };\n",
    );
    let names: Vec<&str> = found.iter().map(|(n, _, _)| n.as_str()).collect();
    assert_eq!(names, ["tokenizer", "renderer"], "{found:?}");
    assert!(
        found
            .iter()
            .all(|(_, f, r)| f == "javascript" && r == "object-literal-callback"),
        "{found:?}"
    );
}

#[test]
fn a_class_method_a_function_and_an_arrow_property_are_not_marked() {
    let found =
        marked("class K { run() {} }\nfunction free() {}\nexport const o = { go: () => 1 };\n");
    assert!(found.is_empty(), "{found:?}");
}
