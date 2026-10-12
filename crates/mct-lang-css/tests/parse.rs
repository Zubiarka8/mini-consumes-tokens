// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-css/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_css::CssParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    CssParser
        .parse(&SourceFile {
            relative_path: "style.css".to_string(),
            contents: src.to_string(),
        })
        .expect("valid CSS source should parse")
}

#[test]
fn extracts_simple_class_selector() {
    let parsed = parse(".nav { display: flex; }");
    let rule = parsed.symbols.iter().find(|s| s.name == ".nav").unwrap();
    assert_eq!(rule.kind, SymbolKind::Rule);
}

#[test]
fn extracts_simple_id_selector() {
    let parsed = parse("#header { color: navy; }");
    let rule = parsed.symbols.iter().find(|s| s.name == "#header").unwrap();
    assert_eq!(rule.kind, SymbolKind::Rule);
}

#[test]
fn comma_separated_selectors_each_become_their_own_rule() {
    let parsed = parse(".a, .b { color: red; }");
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == ".a" && s.kind == SymbolKind::Rule));
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == ".b" && s.kind == SymbolKind::Rule));
}

fn rule_names(parsed: &mct_core::ParsedFile) -> Vec<&str> {
    parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Rule)
        .map(|s| s.name.as_str())
        .collect()
}

#[test]
fn compound_selector_indexes_full_and_atomic_names() {
    let parsed = parse("div.card { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&"div.card"), "{names:?}");
    assert!(names.contains(&"div"), "{names:?}");
    assert!(names.contains(&".card"), "{names:?}");
}

#[test]
fn compound_class_selector_indexes_full_and_atomic_names() {
    let parsed = parse(".btn.btn-primary { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&".btn.btn-primary"), "{names:?}");
    assert!(names.contains(&".btn"), "{names:?}");
    assert!(names.contains(&".btn-primary"), "{names:?}");
}

#[test]
fn descendant_combinator_indexes_full_and_atomic_names() {
    let parsed = parse(".card .title { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&".card .title"), "{names:?}");
    assert!(names.contains(&".card"), "{names:?}");
    assert!(names.contains(&".title"), "{names:?}");
}

#[test]
fn child_combinator_indexes_full_and_atomic_names() {
    let parsed = parse("#id > .c { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&"#id > .c"), "{names:?}");
    assert!(names.contains(&"#id"), "{names:?}");
    assert!(names.contains(&".c"), "{names:?}");
}

#[test]
fn bare_tag_selector_is_indexed() {
    let parsed = parse("div { color: red; }");
    assert_eq!(rule_names(&parsed), vec!["div"]);
}

#[test]
fn pseudo_class_selector_indexes_full_and_base_names() {
    let parsed = parse(".btn:hover { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&".btn:hover"), "{names:?}");
    assert!(names.contains(&".btn"), "{names:?}");
}

#[test]
fn pseudo_element_selector_is_indexed_alone() {
    let parsed = parse("::before { color: red; }");
    assert_eq!(rule_names(&parsed), vec!["::before"]);
}

#[test]
fn attribute_selector_is_indexed() {
    let parsed = parse("[data-bs-toggle] { color: red; }");
    assert_eq!(rule_names(&parsed), vec!["[data-bs-toggle]"]);
}

#[test]
fn tag_plus_attribute_selector_indexes_full_and_atomic_names() {
    let parsed = parse("input[type=\"text\"] { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&"input[type=\"text\"]"), "{names:?}");
    assert!(names.contains(&"input"), "{names:?}");
}

#[test]
fn escaped_tailwind_class_names_are_unescaped() {
    let parsed = parse(".md\\:flex { display: flex; }");
    assert_eq!(rule_names(&parsed), vec![".md:flex"]);

    let parsed = parse(".hover\\:bg-blue-500 { color: blue; }");
    assert_eq!(rule_names(&parsed), vec![".hover:bg-blue-500"]);

    let parsed = parse(".w-1\\/2 { width: 50%; }");
    assert_eq!(rule_names(&parsed), vec![".w-1/2"]);
}

#[test]
fn hex_escapes_decode_to_their_code_point() {
    // CSS Syntax 3 §4.3.7: `\` + 1–6 hex digits is one code point. An
    // identifier can't start with a digit, so Tailwind writes `3xl:` as
    // `\33xl\:` — the class `3xl:grid-cols-6` an HTML `class` attribute uses.
    let parsed = parse(".\\33xl\\:grid-cols-6 { display: grid; }");
    assert_eq!(rule_names(&parsed), vec![".3xl:grid-cols-6"]);

    let parsed = parse(".icon-\\1F600 { content: ''; }");
    assert_eq!(rule_names(&parsed), vec![".icon-😀"]);

    // Six digits at most: the seventh is a literal character.
    let parsed = parse(".\\00003A7 { color: red; }");
    assert_eq!(rule_names(&parsed), vec![".:7"]);

    // Zero, surrogates and values past U+10FFFF become U+FFFD.
    let parsed = parse(".a\\0 { color: red; }");
    assert_eq!(rule_names(&parsed), vec![".a\u{FFFD}"]);
}

#[test]
fn rules_nested_in_a_media_query_are_still_indexed() {
    let parsed = parse("@media (min-width: 600px) { .nav { display: flex; } }");
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == ".nav" && s.kind == SymbolKind::Rule));
}

#[test]
fn at_import_with_a_plain_string_creates_an_import_relation() {
    let parsed = parse("@import \"reset.css\";");
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"reset.css"), "{imports:?}");
}

#[test]
fn at_import_with_url_function_creates_an_import_relation() {
    let parsed = parse("@import url(\"theme.css\");");
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"theme.css"), "{imports:?}");
}

#[test]
fn module_pseudo_symbol_owns_the_import_relation() {
    let parsed = parse("@import \"reset.css\";");
    let module = parsed
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Module)
        .unwrap();
    let import = parsed
        .relations
        .iter()
        .find(|r| r.kind == RelationKind::Imports)
        .unwrap();
    assert_eq!(import.from, module.id);
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = CssParser.parse(&SourceFile {
        relative_path: "broken.css".to_string(),
        contents: ".nav {{{ color".to_string(),
    });
    assert!(
        matches!(result, Err(mct_core::ParseError::Syntax { .. })),
        "{result:?}"
    );
}

#[test]
fn deeply_nested_selectors_do_not_overflow_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let source = format!("{}{{}}", ".a ".repeat(8000));
            let parsed = parse(&source);
            assert!(!parsed.symbols.is_empty());
        })
        .unwrap()
        .join()
        .unwrap();
}

// --- CSS Nesting (#115): nested selectors resolved against their parent ---

/// Every `Rule` symbol named `name`, as `(line, end_line)`.
fn rule_lines(parsed: &mct_core::ParsedFile, name: &str) -> Vec<(u32, Option<u32>)> {
    parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Rule && s.name == name)
        .map(|s| (s.location.line, s.location.end_line))
        .collect()
}

#[test]
fn nesting_resolves_parents_and_keeps_source_ranges() {
    let parsed = parse(".a, .b {\n &:hover { color: red; }\n .title {\n  @media (min-width: 600px) { &:focus { color: blue; } }\n }\n}\n");
    let names = rule_names(&parsed);
    for expected in [
        ".a:hover",
        ".b:hover",
        ".a .title",
        ".b .title",
        ".a .title:focus",
        ".b .title:focus",
    ] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    let hover = parsed
        .symbols
        .iter()
        .find(|s| s.name == ".a:hover")
        .unwrap();
    assert_eq!(hover.location.line, 2);
    assert_eq!(hover.location.end_line, Some(2));
    assert!(names.contains(&".title"));
}

#[test]
fn nesting_does_not_replace_literal_or_escaped_ampersands() {
    let parsed = parse(r#".a { &[data-label="&"] { color: red; } .icon\& { color: blue; } }"#);
    let names = rule_names(&parsed);
    assert!(names.contains(&r#".a[data-label="&"]"#), "{names:?}");
    assert!(names.contains(&".a .icon&"), "{names:?}");
    // The `&` inside the string and the escaped `\&` are not nesting
    // selectors: nothing is substituted into them.
    assert!(
        !names
            .iter()
            .any(|n| n.contains(r#""".a""#) || n.contains(".icon.a")),
        "{names:?}"
    );
}

#[test]
fn explicit_nesting_substitutes_the_parent_for_each_ampersand() {
    let parsed = parse(".card { &:hover {} &.active {} & > .t {} & + & {} .x:not(&) {} }");
    let names = rule_names(&parsed);
    for expected in [
        ".card:hover",
        ".card.active",
        ".card > .t",
        ".card + .card",
        ".x:not(.card)",
    ] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    // A selector containing `&` gets no implicit descendant prefix.
    assert!(!names.contains(&".card .x:not(.card)"), "{names:?}");
}

#[test]
fn implicit_nesting_prefixes_the_parent_and_keeps_leading_combinators() {
    let parsed = parse(".a { .b {} > .c {} + .d {} ~ .e {} div {} }");
    let names = rule_names(&parsed);
    for expected in [".a .b", ".a > .c", ".a + .d", ".a ~ .e", ".a div"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    // The child's own atoms are still indexed for HTML matching.
    for atom in [".b", ".c", ".d", ".e", "div"] {
        assert!(names.contains(&atom), "missing atom {atom}: {names:?}");
    }
}

#[test]
fn comma_lists_on_parent_and_child_expand_to_their_cartesian_product() {
    let parsed = parse(".a, .b { & .x, .y {} }");
    let names = rule_names(&parsed);
    for expected in [".a .x", ".b .x", ".a .y", ".b .y"] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    // One rule per parent, never a `:is()` list.
    assert!(!names.iter().any(|n| n.contains(":is(")), "{names:?}");
}

#[test]
fn deep_nesting_resolves_through_every_level() {
    let parsed = parse(".a { .b { &:hover { .c { & > .d {} } } } }");
    let names = rule_names(&parsed);
    for expected in [
        ".a .b",
        ".a .b:hover",
        ".a .b:hover .c",
        ".a .b:hover .c > .d",
    ] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
}

#[test]
fn parent_context_crosses_a_media_query_inside_a_rule() {
    let parsed = parse(".a { @media (min-width: 1px) { &:hover {} .b {} } }");
    let names = rule_names(&parsed);
    assert!(names.contains(&".a:hover"), "{names:?}");
    assert!(names.contains(&".a .b"), "{names:?}");
}

#[test]
fn parent_context_crosses_a_supports_block_inside_a_rule() {
    let parsed = parse(".a { @supports (display: grid) { &:hover {} .b {} } }");
    let names = rule_names(&parsed);
    assert!(names.contains(&".a:hover"), "{names:?}");
    assert!(names.contains(&".a .b"), "{names:?}");
}

#[test]
fn parent_context_crosses_named_and_anonymous_layers_inside_a_rule() {
    let parsed = parse(".a { @layer base { &:hover {} } @layer { .b {} } }");
    let names = rule_names(&parsed);
    assert!(names.contains(&".a:hover"), "{names:?}");
    assert!(names.contains(&".a .b"), "{names:?}");
}

#[test]
fn parent_context_crosses_a_container_query_inside_a_rule() {
    // tree-sitter-css 0.25 accepts a named `@container` inside a rule; a
    // parenthesized condition there is rejected by the grammar (#116).
    let parsed = parse(".a { @container sidebar { &:hover {} .b {} } }");
    let names = rule_names(&parsed);
    assert!(names.contains(&".a:hover"), "{names:?}");
    assert!(names.contains(&".a .b"), "{names:?}");
}

#[test]
fn at_rules_around_a_rule_do_not_break_its_nested_resolution() {
    for src in [
        "@media (min-width: 1px) { .a { &:hover {} } }",
        "@supports (display: grid) { .a { &:hover {} } }",
        "@layer components { .a { &:hover {} } }",
        "@container (min-width: 1px) { .a { &:hover {} } }",
    ] {
        let parsed = parse(src);
        let names = rule_names(&parsed);
        assert!(names.contains(&".a:hover"), "{src}: {names:?}");
    }
}

#[test]
fn nested_rule_literal_ampersand_selector_is_not_indexed() {
    // User decision (#115): only the resolved names are indexed, never the
    // literal `&` selector alongside them.
    let parsed = parse(".a { &:hover {} & > .b {} &[x] {} }");
    let names = rule_names(&parsed);
    for literal in ["&:hover", "& > .b", "&[x]"] {
        assert!(!names.contains(&literal), "{literal} kept: {names:?}");
    }
    assert!(names.contains(&".a:hover"), "{names:?}");
    assert!(names.contains(&".a > .b"), "{names:?}");
    assert!(names.contains(&".a[x]"), "{names:?}");
}

#[test]
fn ampersand_without_a_parent_rule_keeps_its_literal_selector() {
    // `@utility`/`@custom-variant` blocks are not style rules: there is no
    // parent selector to substitute, so the selector is indexed as written.
    let parsed = parse("@utility scrollbar-hidden { &::-webkit-scrollbar { display: none; } }");
    let names = rule_names(&parsed);
    assert!(names.contains(&"&::-webkit-scrollbar"), "{names:?}");
}

#[test]
fn every_resolved_name_keeps_the_nested_rule_range() {
    let parsed = parse(".a,\n.b {\n  color: red;\n  & .x,\n  .y {\n    color: blue;\n  }\n}\n");
    // Each name spans from its own selector of the list to the rule's `}`.
    for name in [".a .x", ".b .x"] {
        assert_eq!(rule_lines(&parsed, name), [(4, Some(7))], "{name}");
    }
    for name in [".a .y", ".b .y"] {
        assert_eq!(rule_lines(&parsed, name), [(5, Some(7))], "{name}");
    }
    // The parent's own range is untouched.
    assert_eq!(rule_lines(&parsed, ".a"), [(1, Some(8))]);
    // A nested selector without `&` also keeps its name as written, with
    // the nested rule's range (only literal `&` names are dropped).
    assert_eq!(rule_lines(&parsed, ".y"), [(5, Some(7))]);
    // An atom keeps the range of its own token.
    assert_eq!(rule_lines(&parsed, ".x"), [(4, Some(4))]);
}

#[test]
fn self_doubling_nesting_stays_bounded() {
    // Each level doubles the resolved length (`& + &`); 64 levels would be
    // 2^64 parents long. Parsing must stay bounded and still succeed.
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let source = format!(".a {{ {}{} }}", "& + & { ".repeat(64), "} ".repeat(64));
            let parsed = parse(&source);
            let bytes: usize = parsed.symbols.iter().map(|s| s.name.len()).sum();
            assert!(bytes < 16 * 1024 * 1024, "{bytes} bytes of names");
        })
        .unwrap()
        .join()
        .unwrap();
}

/// Every full `Rule` name that contains `needle`, sorted.
fn full_names_with(parsed: &mct_core::ParsedFile, needle: &str) -> Vec<String> {
    let mut names: Vec<String> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Rule && s.name.contains(needle))
        .map(|s| s.name.clone())
        .collect();
    names.sort();
    names
}

#[test]
fn repeated_ampersands_combine_every_parent_selector() {
    // `&` is `:is(.a, .b)` (CSS Nesting §4), so `& + &` matches every pair.
    let parsed = parse(".a, .b { & + & {} }");
    assert_eq!(
        full_names_with(&parsed, " + "),
        [".a + .a", ".a + .b", ".b + .a", ".b + .b"]
    );
    assert!(!rule_names(&parsed).iter().any(|n| n.contains('&')));
}

#[test]
fn repeated_ampersands_combine_across_nesting_levels() {
    let parsed = parse(".a, .b { .c { & ~ &:hover {} } }");
    assert_eq!(
        full_names_with(&parsed, " ~ "),
        [
            ".a .c ~ .a .c:hover",
            ".a .c ~ .b .c:hover",
            ".b .c ~ .a .c:hover",
            ".b .c ~ .b .c:hover",
        ]
    );
    // A third `&` takes the full product as well: 2^3 names.
    let parsed = parse(".a, .b { & + & + & {} }");
    assert_eq!(full_names_with(&parsed, " + ").len(), 8);
    assert!(full_names_with(&parsed, " + ").contains(&".b + .a + .b".to_string()));
}

#[test]
fn several_children_with_several_parents_expand_per_selector() {
    let parsed = parse(".a, .b { &:hover, & + &, .c {} }");
    let names = rule_names(&parsed);
    for expected in [
        ".a:hover", ".b:hover", ".a + .a", ".a + .b", ".b + .a", ".b + .b", ".a .c", ".b .c",
    ] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
}
