//! File-level `Imports` evidence: a Java import names the `Module` of the
//! file declaring the imported type, scoped by its package segment.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_java::JavaParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(JavaParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    index
}

/// The resolutions of every `imports` relation named `to` in `Main.java`.
fn import_hits(index: &Index, to: &str) -> Vec<(Resolution, Option<String>)> {
    index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "imports" && h.relative_path.ends_with("app/Main.java"))
        .map(|h| {
            (
                h.resolution,
                h.candidates.first().map(|c| c.relative_path.clone()),
            )
        })
        .collect()
}

#[test]
fn type_imports_resolve_to_the_declaring_file() {
    let index = open_indexed();
    let money = import_hits(&index, "Money");
    assert!(
        money.contains(&(
            Resolution::Resolved,
            Some("com/acme/model/Money.java".into())
        )),
        "{money:?}"
    );
    // A static import names the class holding the member.
    let strings = import_hits(&index, "Strings");
    assert!(
        strings.contains(&(
            Resolution::Resolved,
            Some("com/acme/util/Strings.java".into())
        )),
        "{strings:?}"
    );
}

#[test]
fn a_jdk_import_never_resolves_to_a_local_namesake() {
    let index = open_indexed();
    // `com/acme/util/List.java` exists, but `java.util.List` is the JDK's.
    let list = import_hits(&index, "List");
    assert!(
        list.iter().any(|(r, _)| *r == Resolution::External),
        "{list:?}"
    );
    assert!(!list.iter().any(|(r, f)| {
        *r == Resolution::Resolved && f.as_deref() == Some("com/acme/util/List.java")
    }));
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
