//! A single `using static A.Fmt;` proves the type and namespace a bare call
//! reaches: `Render(1)` is `A.Fmt.Render`, never `B.Fmt.Render`, and a
//! `using static System.Math;` makes `Abs(2)` the framework's. Several
//! `using static` in one file (which type?), a method the class declares, a
//! class with a base list or a `partial` one (the member may live elsewhere)
//! and a qualified call prove nothing.
//!
//! The project is written to a temporary directory, so its clashing names
//! never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_csharp::CSharpParser;

const FILES: [(&str, &str); 12] = [
    (
        "A.cs",
        "namespace A\n{\n    public static class Fmt\n    {\n        public static string Render(int x) => \"a\";\n    }\n}\n",
    ),
    (
        "B.cs",
        "namespace B\n{\n    public static class Fmt\n    {\n        public static string Render(int x) => \"b\";\n    }\n}\n",
    ),
    (
        "Main.cs",
        "using static A.Fmt;\n\nnamespace App\n{\n    class Main\n    {\n        void Run()\n        {\n            Render(1);\n        }\n    }\n}\n",
    ),
    (
        "Framework.cs",
        "using static System.Math;\n\nnamespace App\n{\n    class Framework\n    {\n        void Run()\n        {\n            Abs(2);\n        }\n    }\n}\n",
    ),
    (
        "Two.cs",
        "using static A.Fmt;\nusing static B.Fmt;\n\nnamespace App\n{\n    class Two\n    {\n        void Run()\n        {\n            Render(3);\n        }\n    }\n}\n",
    ),
    (
        "Own.cs",
        "using static A.Fmt;\n\nnamespace App\n{\n    class Own\n    {\n        string Render(int x) => \"own\";\n        void Run()\n        {\n            Render(4);\n        }\n    }\n}\n",
    ),
    (
        "Child.cs",
        "using static A.Fmt;\n\nnamespace App\n{\n    class Child : Base\n    {\n        void Run()\n        {\n            Render(5);\n        }\n    }\n}\n",
    ),
    (
        "Part.cs",
        "using static A.Fmt;\n\nnamespace App\n{\n    partial class Part\n    {\n        void Run()\n        {\n            Render(6);\n        }\n    }\n}\n",
    ),
    (
        "Plain.cs",
        "namespace App\n{\n    class Plain\n    {\n        void Run()\n        {\n            Render(7);\n        }\n    }\n}\n",
    ),
    (
        "Bound.cs",
        "using static A.Fmt;\n\nnamespace App\n{\n    class Bound\n    {\n        void Run(System.Func<int, string> Render)\n        {\n            Render(9);\n        }\n\n        void Other()\n        {\n            string Render(int x) => \"local\";\n            Render(10);\n        }\n    }\n}\n",
    ),
    (
        "BoundFramework.cs",
        "using static System.Math;\n\nnamespace App\n{\n    class BoundFramework\n    {\n        System.Func<int, int> Abs;\n\n        int Run(System.Func<int, int> Max)\n        {\n            Abs(1);\n            return Max(2);\n        }\n    }\n}\n",
    ),
    (
        "Qualified.cs",
        "using static A.Fmt;\n\nnamespace App\n{\n    class Qualified\n    {\n        void Run(object o)\n        {\n            o.Render(8);\n        }\n    }\n}\n",
    ),
];

fn indexed() -> (Index, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-csharp-using-static-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (path, contents) in FILES {
        std::fs::write(root.join(path), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CSharpParser));
    (index, root)
}

fn call(index: &Index, name: &str, file: &str) -> RelationHit {
    index
        .find_references(name)
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "calls" && h.relative_path == file)
        .unwrap()
}

#[test]
fn a_using_static_resolves_a_bare_call_to_its_type() {
    let (index, root) = indexed();
    let hit = call(&index, "Render", "Main.cs");
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    assert_eq!(hit.candidates[0].relative_path, "A.cs", "{hit:?}");
    let framework = call(&index, "Abs", "Framework.cs");
    assert_eq!(framework.resolution, Resolution::External, "{framework:?}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_bare_call_with_no_proving_using_static_stays_unproven() {
    let (index, root) = indexed();
    for file in [
        "Two.cs",
        "Own.cs",
        "Child.cs",
        "Part.cs",
        "Plain.cs",
        "Qualified.cs",
    ] {
        let hit = call(&index, "Render", file);
        assert!(
            hit.resolution != Resolution::Resolved || hit.candidates[0].relative_path == file,
            "{file}: {hit:?}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_name_the_code_binds_itself_is_not_the_using_static() {
    let (index, root) = indexed();
    // A delegate parameter, a local function and a delegate field shadow it.
    for hit in index.find_references("Render").unwrap() {
        if hit.kind == "calls" && hit.relative_path == "Bound.cs" {
            assert_ne!(hit.resolution, Resolution::Resolved, "{hit:?}");
        }
    }
    for name in ["Abs", "Max"] {
        for hit in index.find_references(name).unwrap() {
            if hit.kind == "calls" && hit.relative_path == "BoundFramework.cs" {
                assert_ne!(hit.resolution, Resolution::External, "{hit:?}");
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
