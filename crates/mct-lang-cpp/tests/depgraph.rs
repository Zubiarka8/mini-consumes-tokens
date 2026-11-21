//! File-level `Imports` evidence: a quoted `#include` is pinned to the exact
//! file it names, so a header is never confused with its same-named source.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_index::{Index, Resolution};
use mct_lang_cpp::CppParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    mct_corpus::open_fixture_indexed(&root, Arc::new(CppParser))
}

/// Resolution and first candidate of the `imports` relation named `to` in
/// `from_file`.
fn import_hit(index: &Index, to: &str, from_file: &str) -> Vec<(Resolution, Option<String>)> {
    mct_corpus::import_hits_from(index, from_file, to)
}

#[test]
fn quoted_includes_resolve_to_the_exact_header() {
    let index = open_indexed();
    // `math.h` and `math.cpp` both hold a `math` module; the path decides.
    let hits = import_hit(&index, "math", "src/main.cpp");
    assert_eq!(
        hits,
        [(Resolution::Resolved, Some("src/util/math.h".to_string()))]
    );
    let hits = import_hit(&index, "math", "src/util/math.cpp");
    assert_eq!(
        hits,
        [(Resolution::Resolved, Some("src/util/math.h".to_string()))]
    );
    let hits = import_hit(&index, "config", "src/main.cpp");
    assert_eq!(
        hits,
        [(Resolution::Resolved, Some("include/config.h".to_string()))]
    );
}

#[test]
fn includes_found_only_through_an_include_path_stay_unresolved() {
    let index = open_indexed();
    let hits = import_hit(&index, "missing_on_include_path", "src/main.cpp");
    assert_eq!(hits, [(Resolution::Unresolved, None)]);
    // An `<angle>` include proves nothing: only the plain name relation
    // exists, with no file evidence.
    let hits = import_hit(&index, "vector", "src/main.cpp");
    assert_eq!(hits, [(Resolution::Unresolved, None)]);
}

/// Issue #567: a quoted include written with Windows separators names the
/// same header (MSVC reads `\` as a separator).
#[test]
fn a_quoted_include_with_backslashes_resolves_to_the_header() {
    let index = open_indexed();
    let hits = import_hit(&index, "math", "src/windows.cpp");
    assert_eq!(
        hits,
        [(Resolution::Resolved, Some("src/util/math.h".to_string()))]
    );
    let hits = import_hit(&index, "config", "src/windows.cpp");
    assert_eq!(
        hits,
        [(Resolution::Resolved, Some("include/config.h".to_string()))]
    );
}

/// Issue #563 (C/C++): a quoted include found only through an include
/// directory that a `CMakeLists.txt` declares literally resolves there,
/// for files under that CMakeLists' directory only. A header next to the
/// including file still wins. Removing the declaration unresolves it again.
#[test]
fn a_cmake_include_directory_resolves_a_quoted_include() {
    let dir = std::env::temp_dir().join(format!("mct-cpp-incdirs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let write = |path: &str, text: &str| {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write(
        "lib/CMakeLists.txt",
        "add_library(core src/core.cpp)\n\
         target_include_directories(core PUBLIC\n  $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>\n  $<INSTALL_INTERFACE:include>)\n\
         include_directories(${SOME_VAR}/x missing)\n",
    );
    write("lib/include/core/api.h", "int api();\n");
    write(
        "lib/src/core.cpp",
        "#include \"core/api.h\"\n#include \"local.h\"\nint api() { return 1; }\n",
    );
    write("lib/src/local.h", "int local();\n");
    write("lib/include/local.h", "int local();\n");
    write(
        "app/main.cpp",
        "#include \"core/api.h\"\nint main() { return 0; }\n",
    );

    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(CppParser));
    let mut index = Index::open_in_memory(&dir, mct_index::ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    assert_eq!(
        import_hit(&index, "api", "lib/src/core.cpp"),
        [(
            Resolution::Resolved,
            Some("lib/include/core/api.h".to_string())
        )]
    );
    assert_eq!(
        import_hit(&index, "local", "lib/src/core.cpp"),
        [(Resolution::Resolved, Some("lib/src/local.h".to_string()))]
    );
    assert_eq!(
        import_hit(&index, "api", "app/main.cpp"),
        [(Resolution::Unresolved, None)]
    );

    write("lib/CMakeLists.txt", "add_library(core src/core.cpp)\n");
    index.reindex(&registry, false).unwrap();
    assert_eq!(
        import_hit(&index, "api", "lib/src/core.cpp"),
        [(Resolution::Unresolved, None)]
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// Issue #563 (C/C++): `-I` and `-iquote` flags of a `compile_commands.json`
/// entry, relative to its `directory` or absolute inside the repository,
/// resolve a quoted include for files in the compiled file's directory.
/// Both `arguments` and `command` forms count; a flag outside the
/// repository is dropped.
#[test]
fn a_compile_commands_include_flag_resolves_a_quoted_include() {
    let dir = std::env::temp_dir().join(format!("mct-cpp-ccdb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let write = |path: &str, text: &str| {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    let root = dir.to_str().unwrap();
    write(
        "build/compile_commands.json",
        &format!(
            r#"[
  {{"directory": "{root}/build", "file": "../app/main.cpp",
    "arguments": ["c++", "-I../lib/include", "-iquote", "/usr/include/outside", "-c", "../app/main.cpp"]}},
  {{"directory": "{root}/build", "file": "{root}/tools/tool.cpp",
    "command": "c++ -I {root}/lib/include -c {root}/tools/tool.cpp"}}
]"#
        ),
    );
    write("lib/include/core/api.h", "int api();\n");
    write(
        "app/main.cpp",
        "#include \"core/api.h\"\nint main() { return 0; }\n",
    );
    write(
        "tools/tool.cpp",
        "#include \"core/api.h\"\nint tool() { return 0; }\n",
    );
    write(
        "other/x.cpp",
        "#include \"core/api.h\"\nint x() { return 0; }\n",
    );

    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(CppParser));
    let mut index = Index::open_in_memory(&dir, mct_index::ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let api = || Some("lib/include/core/api.h".to_string());
    assert_eq!(
        import_hit(&index, "api", "app/main.cpp"),
        [(Resolution::Resolved, api())]
    );
    assert_eq!(
        import_hit(&index, "api", "tools/tool.cpp"),
        [(Resolution::Resolved, api())]
    );
    assert_eq!(
        import_hit(&index, "api", "other/x.cpp"),
        [(Resolution::Unresolved, None)]
    );

    write("build/compile_commands.json", "[]");
    index.reindex(&registry, false).unwrap();
    assert_eq!(
        import_hit(&index, "api", "app/main.cpp"),
        [(Resolution::Unresolved, None)]
    );
    std::fs::remove_dir_all(dir).unwrap();
}
