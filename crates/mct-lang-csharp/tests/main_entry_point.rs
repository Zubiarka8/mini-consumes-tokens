//! A class with a `static Main` method holds the program's entry point,
//! which nothing in the source calls. The class stays a dead-code candidate
//! (the runtime picks the entry point, a project may name another) and is
//! reported with a hint, as a framework convention is. An instance `Main`,
//! a `Main` with another casing and a class without one carry no hint.

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_lang_csharp::CSharpParser;

fn hinted(source: &str) -> Vec<String> {
    let parsed = mct_corpus::parse_source(&CSharpParser, "Program.cs", "C#", source);
    parsed
        .entry_points
        .iter()
        .map(|hint| {
            let symbol = parsed.symbols.iter().find(|s| s.id == hint.symbol).unwrap();
            format!("{} {} {}", symbol.name, hint.framework, hint.rule)
        })
        .collect()
}

#[test]
fn a_class_with_a_static_main_is_a_possible_entry_point() {
    assert_eq!(
        hinted("class Program { static void Main(string[] args) { } }\n"),
        ["Program dotnet static-main"]
    );
    assert_eq!(
        hinted("public static class App { public static async Task<int> Main() => 0; }\n"),
        ["App dotnet static-main"]
    );
}

#[test]
fn another_main_carries_no_hint() {
    assert!(hinted("class A { void Main() { } }\n").is_empty());
    assert!(hinted("class A { static void main() { } }\n").is_empty());
    assert!(hinted("class A { static void Run() { } }\n").is_empty());
}
