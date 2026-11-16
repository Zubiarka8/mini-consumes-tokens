//! A type declaring the launcher's `main` method holds the program's entry
//! point, which nothing in the source calls. The type stays a dead-code
//! candidate (the build names the main class, and a project may have
//! several) and is reported with a hint, as a framework convention is.
//! The launcher accepts `void main(String[] args)`, `void main(String...
//! args)`, and since Java 25 (JEP 512) a `void main()` that need not be
//! static or public. A `main` with another return type, other parameters,
//! or another casing carries no hint. Each case was run with the JDK 25
//! `java` source launcher: the hinted ones start, `int main(String[])` and
//! `main(String, String)` fail with "can't find main(String[]) or main()".

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_lang_java::JavaParser;

fn hinted(source: &str) -> Vec<String> {
    let parsed = mct_corpus::parse_source(&JavaParser, "app/Tool.java", "Java", source);
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
fn a_type_with_a_launcher_main_is_a_possible_entry_point() {
    assert_eq!(
        hinted("public class Tool { public static void main(String[] args) { } }\n"),
        ["Tool java main-method"]
    );
    assert_eq!(
        hinted("class Tool { static void main(String... args) { } }\n"),
        ["Tool java main-method"]
    );
    assert_eq!(
        hinted("class Tool { static void main(final String args[]) { } }\n"),
        ["Tool java main-method"]
    );
    assert_eq!(
        hinted("class Tool { void main() { } }\n"),
        ["Tool java main-method"]
    );
    assert_eq!(
        hinted("interface Cli { static void main(String[] a) { } }\n"),
        ["Cli java main-method"]
    );
    assert_eq!(
        hinted("enum Mode { A; public static void main(String[] a) { } }\n"),
        ["Mode java main-method"]
    );
}

#[test]
fn only_the_type_declaring_main_carries_the_hint() {
    assert_eq!(
        hinted(
            "class Outer {\n  static class Inner { public static void main(String[] a) { } }\n}\n"
        ),
        ["Inner java main-method"]
    );
}

#[test]
fn another_main_carries_no_hint() {
    assert!(hinted("class A { int main(String[] a) { return 0; } }\n").is_empty());
    assert!(hinted("class A { static void main(int n) { } }\n").is_empty());
    assert!(hinted("class A { static void main(String a, String b) { } }\n").is_empty());
    assert!(hinted("class A { static void Main(String[] a) { } }\n").is_empty());
    assert!(hinted("class A { static void run(String[] a) { } }\n").is_empty());
}
