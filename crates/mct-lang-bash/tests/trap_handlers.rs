//! `trap handler SIGNAL` runs `handler` when the signal arrives, so a
//! handler named by a single word is a call to that function. A command
//! string with arguments, `-`, an empty handler and the `-l`/`-p` options
//! name no function.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::RelationKind;
use mct_lang_bash::BashParser;

fn trapped(src: &str) -> Vec<String> {
    let parsed = mct_corpus::parse_source(&BashParser, "run.sh", "bash", src);
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls && r.to_name != "trap" && r.to_name != ":")
        .map(|r| r.to_name.clone())
        .collect()
}

#[test]
fn a_function_named_as_a_trap_handler_is_called() {
    assert_eq!(
        trapped("cleanup() { :; }\ntrap cleanup EXIT\n"),
        ["cleanup"]
    );
    assert_eq!(
        trapped("on_err() { :; }\ntrap 'on_err' ERR INT\n"),
        ["on_err"]
    );
    assert_eq!(trapped("trap \"stop_all\" TERM\n"), ["stop_all"]);
}

#[test]
fn a_handler_that_is_not_a_single_word_calls_nothing() {
    for src in [
        "trap 'echo bye; exit' EXIT\n",
        "trap - EXIT\n",
        "trap '' INT\n",
        "trap -p\n",
        "trap -l\n",
        "trap\n",
        "trap \"$handler\" EXIT\n",
    ] {
        assert!(trapped(src).is_empty(), "{src:?}: {:?}", trapped(src));
    }
}
