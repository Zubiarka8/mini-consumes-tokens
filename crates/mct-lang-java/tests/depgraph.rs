//! File-level `Imports` evidence: a Java import names the `Module` of the
//! file declaring the imported type, scoped by its package segment.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_index::{Index, Resolution};
use mct_lang_java::JavaParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    mct_corpus::open_fixture_indexed(&root, Arc::new(JavaParser))
}

/// The resolutions of every `imports` relation named `to` in `Main.java`.
fn import_hits(index: &Index, to: &str) -> Vec<(Resolution, Option<String>)> {
    mct_corpus::import_hits_with_path_suffix(index, "app/Main.java", to)
}

#[test]
fn type_imports_resolve_to_the_declaring_file() {
    let index = open_indexed();
    mct_corpus::assert_import_targets(
        &index,
        "app/Main.java",
        &[
            ("Money", "com/acme/model/Money.java"),
            // A static import names the class holding the member.
            ("Strings", "com/acme/util/Strings.java"),
        ],
    );
}

#[test]
fn a_jdk_import_never_resolves_to_a_local_namesake() {
    let index = open_indexed();
    // `com/acme/util/List.java` exists, but `java.util.List` is the JDK's.
    let list = import_hits(&index, "List");
    mct_corpus::assert_external_not_local(&list, "com/acme/util/List.java");
}

#[test]
fn a_package_wildcard_records_no_module_relation() {
    let parsed = {
        use mct_core::{LanguageParser, SourceFile};
        JavaParser
            .parse(&SourceFile {
                relative_path: "A.java".into(),
                contents: "import com.acme.model.*;\nclass A {}\n".into(),
            })
            .unwrap()
    };
    // Only the pre-existing name relation is left; no file evidence.
    assert!(parsed.relation_targets.is_empty());
}
