//! Code that runs when a class is built belongs to that class: a primary
//! constructor's parameter defaults, a property initializer, an `init`
//! block and a secondary constructor's body. A Kotlin constructor is no
//! symbol, so these calls come from the class, as its header's
//! `: Base(…)` call does, never from the file. A function declared in the
//! class still owns its own body.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::RelationKind;
use mct_lang_kotlin::KotlinParser;

const SOURCE: &str = r#"package zoo

class B(val x: Int = seed()) {
    val y = compute()
    init { setup() }
    constructor(s: String) : this(1) { log(s) }
    fun run() { work() }
    object Inner { val z = inner() }
}

val top = topLevel()
"#;

#[test]
fn a_class_owns_the_code_that_builds_it() {
    let parsed = mct_corpus::parse_source(&KotlinParser, "src/zoo/B.kt", "Kotlin", SOURCE);
    let owner = |id| {
        let s = parsed.symbols.iter().find(|s| s.id == id).unwrap();
        format!("{} {:?}", s.name, s.kind)
    };
    let calls: Vec<(&str, String)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| (r.to_name.as_str(), owner(r.from)))
        .collect();
    let call = |to, from: &str| (to, from.to_string());
    assert_eq!(
        calls,
        [
            call("seed", "B Class"),
            call("compute", "B Class"),
            call("setup", "B Class"),
            call("log", "B Class"),
            call("work", "run Method"),
            call("inner", "Inner Class"),
            call("topLevel", "B Module"),
        ]
    );
}
