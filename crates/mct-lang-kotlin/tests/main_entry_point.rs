//! An `object` with `@JvmStatic fun main`, or a class whose companion
//! object has one, compiles to a JVM class with a static `main`: the class
//! the `java` launcher starts, which nothing in the source calls. It stays
//! a dead-code candidate (the build names the main class) and is reported
//! with a hint, as a framework convention is. A `main` without
//! `@JvmStatic`, with other parameters, or in a plain class body carries no
//! hint. No `kotlinc` was available to run the cases; they follow the
//! Kotlin documentation for `@JvmStatic` and the `java` launcher.

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_lang_kotlin::KotlinParser;

fn hinted(source: &str) -> Vec<String> {
    let parsed = mct_corpus::parse_source(&KotlinParser, "app/App.kt", "Kotlin", source);
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
fn an_object_with_a_jvm_static_main_is_a_possible_entry_point() {
    assert_eq!(
        hinted("object App {\n    @JvmStatic\n    fun main(args: Array<String>) {}\n}\n"),
        ["App jvm jvm-static-main"]
    );
    assert_eq!(
        hinted("object App {\n    @JvmStatic fun main(vararg args: String) {}\n}\n"),
        ["App jvm jvm-static-main"]
    );
    assert_eq!(
        hinted("object App {\n    @kotlin.jvm.JvmStatic fun main() {}\n}\n"),
        ["App jvm jvm-static-main"]
    );
}

#[test]
fn a_class_whose_companion_has_a_jvm_static_main_is_a_possible_entry_point() {
    assert_eq!(
        hinted(
            "class Tool {\n    companion object {\n        @JvmStatic fun main(args: Array<String>) {}\n    }\n}\n"
        ),
        ["Tool jvm jvm-static-main"]
    );
}

#[test]
fn another_main_carries_no_hint() {
    assert!(hinted("object App {\n    fun main(args: Array<String>) {}\n}\n").is_empty());
    assert!(hinted("class App {\n    @JvmStatic fun main(args: Array<String>) {}\n}\n").is_empty());
    assert!(hinted("object App {\n    @JvmStatic fun main(n: Int) {}\n}\n").is_empty());
    assert!(hinted("object App {\n    @JvmStatic fun main(vararg a: Int) {}\n}\n").is_empty());
    assert!(hinted("object App {\n    @JvmStatic fun run(args: Array<String>) {}\n}\n").is_empty());
}
