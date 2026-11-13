//! D9 for C++: a call through `std::` can only reach the standard library.
//! A program may not add declarations to namespace `std` ([namespace.std]),
//! so `std::sort(…)` or `::std::move(…)` never reaches a function of the
//! repository. A file that opens `namespace std` itself (to specialize
//! `std::hash`) proves nothing, and neither does a bare `sort(…)` after
//! `using namespace std;`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_lang_cpp::CppParser;

#[test]
fn a_call_through_std_is_external() {
    let root = std::env::temp_dir().join(format!("mct-cpp-std-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/app.cpp"),
        "#include <algorithm>\nusing namespace std;\n\nvoid run(int* a, int* b) {\n    std::sort(a, b); std /* comment */ ::sort(a, b);\n    ::std::move(a);\n    std::to_string<int>(1);\n    sort(a, b);\n    util::sort(a, b);\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/hash.hpp"),
        "namespace std {\ntemplate <> struct hash<Key> {};\n}\n\nvoid mix(int* a) {\n    std::move(a);\n}\n",
    )
    .unwrap();
    // A repository function of each name, so a call that isn't external
    // finds a candidate.
    std::fs::write(
        root.join("src/util.cpp"),
        "namespace util {\nvoid sort(int* a, int* b) {}\nvoid move(int* a) {}\nvoid to_string(int v) {}\n}\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CppParser));
    let mut hits: Vec<String> = ["sort", "move", "to_string"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| {
            format!(
                "{}:{} {} {:?}",
                h.relative_path, h.line, h.to_name, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "src/app.cpp:5 sort External",
            "src/app.cpp:5 sort External",
            "src/app.cpp:6 move External",
            "src/app.cpp:7 to_string External",
            // `using namespace std;` proves nothing about a bare name.
            "src/app.cpp:8 sort Resolved",
            "src/app.cpp:9 sort Resolved",
            // This file opens `namespace std`.
            "src/hash.hpp:6 move Resolved",
        ]
    );
}
