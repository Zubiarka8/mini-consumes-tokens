//! An inline event handler attribute (`onclick="save()"`) runs its value as
//! JavaScript, so each function it calls by name is a `calls` relation to a
//! JavaScript function, through the real parse -> SQLite -> query pipeline
//! with both parsers: `save` in `app.js` is no dead code. A member call
//! (`form.reset()`), a keyword (`return`, `if`), text inside a string
//! literal and a non-event attribute name nothing. Only a JavaScript
//! `function` qualifies: a CSS rule or an HTML element of the same name
//! never does. The HTML standard runs an event handler content attribute's
//! value as the body of a function called on the event.
//!
//! The project is written to a temporary directory, so its names never
//! become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_core::{LanguageRegistry, RelationKind};
use mct_index::{Index, Resolution};
use mct_lang_css::CssParser;
use mct_lang_html::HtmlParser;
use mct_lang_js_ts::JsTsParser;

const HTML: &str = r#"<html><head><script src="app.js"></script></head>
<body>
<button onclick="save()">Save</button>
<form id="f" onsubmit="return validate(this) && track('submit()');">
  <input onchange="form.reset(); if (ready) { refresh(1) }" title="clear()">
</form>
<div class="clear" ONMOUSEOVER = "highlight ( event )"></div>
</body></html>
"#;

const JS: &str = "\
function save() { return 1; }
function validate(f) { return !!f; }
function track(name) { return name; }
function refresh(n) { return n; }
function highlight(e) { return e; }
function clear() { return 0; }
function reset() { return 0; }
";

fn calls_in(source: &str) -> Vec<String> {
    let parsed = mct_corpus::parse_source(&HtmlParser, "index.html", "HTML", source);
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .map(|(i, r)| {
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            let language = target.and_then(|t| t.language.clone()).unwrap_or_default();
            format!("{} {} {language}", r.location.line, r.to_name)
        })
        .collect()
}

#[test]
fn an_event_handler_calls_each_function_it_names() {
    assert_eq!(
        calls_in(HTML),
        [
            "3 save javascript_typescript",
            "4 validate javascript_typescript",
            "4 track javascript_typescript",
            "5 refresh javascript_typescript",
            "7 highlight javascript_typescript",
        ]
    );
}

#[test]
fn no_other_attribute_or_markup_calls_anything() {
    assert!(calls_in("<p title=\"save()\" data-on=\"x()\">save()</p>\n").is_empty());
    assert!(calls_in("<a href=\"javascript:void(0)\" on=\"y()\">a</a>\n").is_empty());
}

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-html-event-handlers-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("index.html"), HTML).unwrap();
    std::fs::write(root.join("app.js"), JS).unwrap();
    std::fs::write(root.join("style.css"), ".clear { color: red; }\n").unwrap();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(HtmlParser));
    registry.register(Arc::new(CssParser));
    registry.register(Arc::new(JsTsParser));
    let index = mct_corpus::open_indexed_with_registry(
        &root,
        &registry,
        3,
        "index.html, app.js, style.css",
    );
    (index, root)
}

#[test]
fn a_handler_call_resolves_to_the_javascript_function() {
    let (index, root) = indexed();
    for name in ["save", "validate", "track", "refresh", "highlight"] {
        let hits: Vec<_> = index
            .find_references(name)
            .unwrap()
            .into_iter()
            .filter(|h| h.relative_path == "index.html" && h.kind == "calls")
            .collect();
        assert_eq!(hits.len(), 1, "{name}: {hits:?}");
        assert_eq!(hits[0].resolution, Resolution::Resolved, "{name}: {hits:?}");
        let target = index.symbols_by_ids(&[hits[0].target_id.unwrap()]).unwrap();
        assert_eq!(target[0].relative_path, "app.js", "{name}");
    }
    for name in ["clear", "reset"] {
        let calls: Vec<_> = index
            .find_references(name)
            .unwrap()
            .into_iter()
            .filter(|h| h.kind == "calls")
            .collect();
        assert!(calls.is_empty(), "{name}: {calls:?}");
    }
    std::fs::remove_dir_all(root).ok();
}
