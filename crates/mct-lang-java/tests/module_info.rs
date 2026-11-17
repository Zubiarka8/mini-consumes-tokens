//! A `module-info.java` declaration is a `module` symbol named after the Java
//! module, spanning the file, and each `requires` is an import of the module
//! it names: external for a JDK module (`java.*`, `jdk.*`), so another
//! module of the repository resolves by name to its declaration. `exports`
//! and `opens` name packages, not modules, and record nothing.

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_java::JavaParser;

const SOURCE: &str = r#"open module com.example.app {
    requires java.sql;
    requires transitive com.example.core;
    requires static jdk.unsupported;
    exports com.example.api;
    opens com.example.internal to com.example.core;
}
"#;

#[test]
fn a_module_declaration_is_a_module_symbol() {
    let parsed = mct_corpus::parse_source(&JavaParser, "module-info.java", "Java", SOURCE);
    let modules: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Module)
        .map(|s| (s.name.as_str(), s.location.line))
        .collect();
    assert!(modules.contains(&("com.example.app", 1)), "{modules:?}");
}

#[test]
fn a_requires_is_an_import_external_for_a_jdk_module() {
    let parsed = mct_corpus::parse_source(&JavaParser, "module-info.java", "Java", SOURCE);
    let mut imports: Vec<(String, bool)> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Imports)
        .map(|(i, r)| {
            let external = parsed
                .relation_targets
                .iter()
                .any(|t| t.relation == i && t.external);
            (r.to_name.clone(), external)
        })
        .collect();
    imports.sort();
    assert_eq!(
        imports,
        [
            ("com.example.core".to_string(), false),
            ("java.sql".to_string(), true),
            ("jdk.unsupported".to_string(), true),
        ]
    );
}
