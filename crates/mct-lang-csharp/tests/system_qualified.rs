//! A call through a fully qualified `System.`/`Microsoft.` name is the
//! framework's, never a same-named method of a repository class
//! (`System.Console.WriteLine` next to `Log.WriteLine`). A qualified name
//! outside those roots, and a call whose receiver is a local or a
//! repository type, keep the resolution they had.
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

const SOURCE: &str = "\
namespace Shop
{
    public class Log
    {
        public static void WriteLine(string s) {}
        public static int Max(int a, int b) => a;
    }

    public class Cart
    {
        public void WriteLine(string s) {}

        public void Run()
        {
            System.Console.WriteLine(\"a\");
            global::System.Console.WriteLine(\"b\");
            var top = System.Math.Max(1, 2);
            Log.WriteLine(\"c\");
            var other = Log.Max(1, 2);
        }
    }
}
";

fn indexed() -> (Index, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-csharp-system-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Cart.cs"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CSharpParser));
    (index, root)
}

fn calls(index: &Index, to: &str) -> Vec<RelationHit> {
    let mut hits: Vec<_> = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls")
        .collect();
    hits.sort_by_key(|h| (h.line, h.column));
    hits
}

#[test]
fn a_fully_qualified_system_call_is_external() {
    let (index, root) = indexed();
    for (name, lines) in [("WriteLine", [15, 16]), ("Max", [17, 17])] {
        let hits = calls(&index, name);
        for line in lines {
            let hit = hits.iter().find(|h| h.line == line).unwrap();
            assert_eq!(hit.resolution, Resolution::External, "{hit:?}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_call_on_a_repository_type_still_resolves_to_it() {
    let (index, root) = indexed();
    let write = calls(&index, "WriteLine");
    let hit = write.iter().find(|h| h.line == 18).unwrap();
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    assert_eq!(hit.candidates[0].line, 5, "{hit:?}");
    let max = calls(&index, "Max");
    let hit = max.iter().find(|h| h.line == 19).unwrap();
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    std::fs::remove_dir_all(root).unwrap();
}
