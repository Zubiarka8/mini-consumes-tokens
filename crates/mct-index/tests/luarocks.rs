//! End-to-end behaviour of the LuaRocks analysis (issue #267): reindex a
//! project, then ask for the declared/installed/pinned detail and for the
//! origin of unresolved Lua imports.
//!
//! The tree `manifest` fixtures under `tests/fixtures/luarocks/` are files
//! written by LuaRocks 3.13.0 (`luarocks init` + `luarocks install inspect
//! lpeg`, and `luarocks make` of a `type = "none"` rockspec with
//! `build.install.lua`). The `luarocks.lock` written here is HAND-WRITTEN in
//! the format `luarocks/deplocks.lua` reads; real `luarocks build` accepted
//! such a file ("Using dependencies pinned in lockfile"), but generating one
//! with `luarocks build --pin` was not verified.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::sync::Arc;

use mct_core::{
    LanguageParser, LanguageRegistry, Location, ParseError, ParsedFile, RelationKind,
    RelationTarget, SourceFile, SymbolKind, SymbolRecord, SymbolRelation,
};
use mct_index::luarocks::{
    Completeness, Declaration, Installed, Pin, Presence, Satisfaction, SourceStatus,
};
use mct_index::{ExcludeSet, Index};

const REAL_TREE: &str = include_str!("fixtures/luarocks/real-tree/lib/luarocks/rocks-5.5/manifest");
const INSTALL_LUA_TREE: &str =
    include_str!("fixtures/luarocks/install-lua-tree/lib/luarocks/rocks-5.5/manifest");
const STORE_DIR: &str = "lua_modules/lib/luarocks/rocks-5.5";

/// Lua files: one `import NAME` per line becomes an unresolved `imports`
/// relation, which is what the real Lua parser leaves for a `require` that
/// no indexed file defines.
struct LuaImports;

impl LanguageParser for LuaImports {
    fn language_id(&self) -> &'static str {
        "lua"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["lua"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parsed = ParsedFile::default();
        for (line_no, line) in file.contents.lines().enumerate() {
            let Some(name) = line.strip_prefix("import ") else {
                continue;
            };
            let location = Location {
                line: line_no as u32 + 1,
                column: 1,
                byte_len: line.len() as u32,
                end_line: Some(line_no as u32 + 1),
            };
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name: format!("site{id}"),
                kind: SymbolKind::Function,
                location,
                parent: None,
                level: None,
            });
            parsed.relation_targets.push(RelationTarget {
                relation: parsed.relations.len(),
                ..Default::default()
            });
            parsed.relations.push(SymbolRelation {
                from: id,
                kind: RelationKind::Imports,
                to_name: name.trim().to_string(),
                location: Location {
                    end_line: None,
                    ..location
                },
            });
        }
        Ok(parsed)
    }
}

fn registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(LuaImports));
    registry
}

fn write(root: &Path, path: &str, contents: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, contents).unwrap();
}

fn index_of(root: &Path) -> Index {
    index_with(root, ExcludeSet::default())
}

fn index_with(root: &Path, exclude: ExcludeSet) -> Index {
    let mut index = Index::open_in_memory(root, exclude).unwrap();
    index.reindex(&registry(), false).unwrap();
    index
}

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, contents) in files {
        write(dir.path(), path, contents);
    }
    dir
}

const DEMO: &str = "package = 'demo'\nversion = '1.0-1'\nrockspec_format = '3.0'\n\
dependencies = { 'lua >= 5.1', 'lpeg ~> 1.0', 'inspect >= 3.0', 'absent-rock' }\n\
test_dependencies = { 'inspect' }\n";

#[test]
fn repeated_names_scopes_and_constraints_are_all_kept_and_the_summary_stays_a_projection() {
    let dir = project(&[(
        "x-1.0-1.rockspec",
        "dependencies = { 'luasocket >= 3', 'luasocket < 4' }\n\
         build_dependencies = { 'luasocket' }\n\
         test_dependencies = { 'busted' }\n",
    )]);
    let index = index_of(dir.path());

    let report = index.rockspec_report(None).unwrap();
    let deps = &report.rockspecs[0].dependencies;
    let sockets: Vec<_> = deps.iter().filter(|d| d.name == "luasocket").collect();
    assert_eq!(sockets.len(), 3, "{deps:?}");
    assert_eq!(sockets[0].constraint.as_deref(), Some(">= 3"));
    assert_eq!(sockets[1].constraint.as_deref(), Some("< 4"));
    assert_eq!(
        (sockets[2].scope.as_str(), sockets[2].constraint.as_deref()),
        ("build", None)
    );
    assert!(deps.iter().any(|d| d.name == "busted" && d.scope == "test"));

    // The shared summary keeps its old contract: one row per name.
    let status = index.status().unwrap();
    let summary = &status.dependencies[0].dependencies;
    assert_eq!(summary.iter().filter(|d| d.name == "luasocket").count(), 1);
    assert_eq!(
        summary
            .iter()
            .find(|d| d.name == "luasocket")
            .unwrap()
            .version
            .as_deref(),
        Some(">= 3")
    );
}

#[test]
fn installed_pinned_and_satisfied_are_three_separate_facts() {
    let dir = project(&[
        ("demo-1.0-1.rockspec", DEMO),
        (&format!("{STORE_DIR}/manifest"), REAL_TREE),
        (
            "luarocks.lock",
            "return { dependencies = { inspect = '3.1.3-0', lpeg = '1.1.0-2', 'ghost' == nil } }\n",
        ),
    ]);
    // The lockfile above is invalid on purpose (code); first check that.
    let index = index_of(dir.path());
    let spec = &index.rockspec_report(None).unwrap().rockspecs[0];
    assert!(
        matches!(spec.lockfile, SourceStatus::Corrupt(_)),
        "{:?}",
        spec.lockfile
    );
    assert!(spec
        .dependencies
        .iter()
        .all(|d| matches!(d.pin, Pin::Unknown(_))));

    // Hand-written, in the shape luarocks/deplocks.lua reads.
    write(
        dir.path(),
        "luarocks.lock",
        "return {\n dependencies = { inspect = '3.1.3-0', lpeg = '1.1.0-2', ghost = '9.9-1' },\n}\n",
    );
    let index = index_of(dir.path());
    let spec = &index.rockspec_report(None).unwrap().rockspecs[0];
    assert_eq!(spec.lockfile, SourceStatus::Read);
    let dep = |name: &str, scope: &str| {
        spec.dependencies
            .iter()
            .find(|d| d.name == name && d.scope == scope)
            .unwrap()
    };

    let lua = dep("lua", "runtime");
    assert!(matches!(lua.installed, Installed::Unknown(_)));
    assert!(matches!(lua.satisfaction, Satisfaction::Unknown(_)));

    // `~> 1.0` means 1.0.x in LuaRocks: the installed 1.1.0-2 does not meet it.
    let lpeg = dep("lpeg", "runtime");
    assert!(
        matches!(&lpeg.installed, Installed::Found { complete: true, versions } if versions[0].version == "1.1.0-2" && versions[0].lua == "5.5")
    );
    assert_eq!(
        lpeg.satisfaction,
        Satisfaction::Unsatisfied(vec!["1.1.0-2".into()])
    );
    assert!(
        matches!(&lpeg.pin, Pin::Pinned { version, installed: Presence::Yes, satisfies: Some(false) } if version == "1.1.0-2")
    );

    let inspect = dep("inspect", "runtime");
    assert_eq!(
        inspect.satisfaction,
        Satisfaction::Satisfied(vec!["3.1.3-0".into()])
    );
    assert!(matches!(
        &inspect.pin,
        Pin::Pinned {
            installed: Presence::Yes,
            satisfies: Some(true),
            ..
        }
    ));
    // The same rock under another scope is not pinned there.
    assert_eq!(dep("inspect", "test").pin, Pin::NotPinned);

    let absent = dep("absent-rock", "runtime");
    assert_eq!(absent.installed, Installed::NotFound);
    assert_eq!(absent.satisfaction, Satisfaction::NotInstalled);
    assert_eq!(absent.pin, Pin::NotPinned);
}

#[test]
fn a_pin_for_a_rock_that_is_not_installed_is_reported_as_a_difference() {
    let dir = project(&[
        ("demo-1.0-1.rockspec", "dependencies = { 'ghost >= 1' }\n"),
        (&format!("{STORE_DIR}/manifest"), REAL_TREE),
        (
            "luarocks.lock",
            "return { dependencies = { ghost = '9.9-1' } }\n",
        ),
    ]);
    let spec = index_of(dir.path())
        .rockspec_report(None)
        .unwrap()
        .rockspecs
        .remove(0);
    let ghost = &spec.dependencies[0];
    assert!(matches!(
        &ghost.pin,
        Pin::Pinned {
            installed: Presence::No,
            satisfies: Some(true),
            ..
        }
    ));
}

#[test]
fn no_lockfile_and_no_tree_are_distinct_states_not_errors() {
    let dir = project(&[("demo-1.0-1.rockspec", "dependencies = { 'lpeg' }\n")]);
    let spec = index_of(dir.path())
        .rockspec_report(None)
        .unwrap()
        .rockspecs
        .remove(0);
    assert_eq!(spec.lockfile, SourceStatus::Missing);
    assert!(spec.stores.is_empty());
    assert_eq!(spec.dependencies[0].pin, Pin::NoLockfile);
    assert_eq!(spec.dependencies[0].installed, Installed::NotFound);
}

#[test]
fn an_unreadable_store_makes_absence_and_uniqueness_unknown() {
    let dir = project(&[
        ("demo-1.0-1.rockspec", DEMO),
        (&format!("{STORE_DIR}/manifest"), REAL_TREE),
        (
            "lua_modules/lib/luarocks/rocks-5.4/manifest",
            "repository = os.getenv('X')\n",
        ),
    ]);
    let spec = index_of(dir.path())
        .rockspec_report(None)
        .unwrap()
        .rockspecs
        .remove(0);
    assert_eq!(spec.stores.len(), 2);
    assert!(spec
        .stores
        .iter()
        .any(|s| s.lua == "5.4" && matches!(s.status, SourceStatus::Corrupt(_))));
    let lpeg = spec.dependencies.iter().find(|d| d.name == "lpeg").unwrap();
    // Found in the readable store, but more versions may sit in the other.
    assert!(matches!(
        &lpeg.installed,
        Installed::Found {
            complete: false,
            ..
        }
    ));
    // Not found in the readable store, but the other could not be read: unknown.
    let absent = spec
        .dependencies
        .iter()
        .find(|d| d.name == "absent-rock")
        .unwrap();
    assert!(
        matches!(absent.installed, Installed::Unknown(_)),
        "{:?}",
        absent.installed
    );
    assert!(matches!(absent.satisfaction, Satisfaction::Unknown(_)));
}

#[test]
fn an_oversized_manifest_is_dropped_whole() {
    let big = format!("{REAL_TREE}\n-- {}\n", "x".repeat(5 * 1024 * 1024));
    let dir = project(&[
        ("demo-1.0-1.rockspec", DEMO),
        (&format!("{STORE_DIR}/manifest"), &big),
    ]);
    let spec = index_of(dir.path())
        .rockspec_report(None)
        .unwrap()
        .rockspecs
        .remove(0);
    assert_eq!(spec.stores[0].status, SourceStatus::TooLarge);
    assert!(spec
        .dependencies
        .iter()
        .all(|d| matches!(d.installed, Installed::Unknown(_))));
}

#[test]
fn an_excluded_tree_is_never_read_and_is_not_reported_as_absent() {
    let dir = project(&[
        ("demo-1.0-1.rockspec", DEMO),
        (&format!("{STORE_DIR}/manifest"), REAL_TREE),
    ]);
    let index = index_with(dir.path(), ExcludeSet::new(&["lua_modules".to_string()]));
    let spec = index.rockspec_report(None).unwrap().rockspecs.remove(0);
    assert!(matches!(spec.stores[0].status, SourceStatus::NotInspected));
    assert!(spec
        .dependencies
        .iter()
        .all(|d| matches!(d.installed, Installed::Unknown(_))));
    assert!(
        !format!("{spec:?}").contains("3.1.3-0"),
        "excluded content leaked"
    );
}

#[cfg(unix)]
#[test]
fn a_tree_symlinked_outside_the_project_is_not_followed() {
    let outside = project(&[("lib/luarocks/rocks-5.5/manifest", REAL_TREE)]);
    let dir = project(&[("demo-1.0-1.rockspec", DEMO)]);
    std::os::unix::fs::symlink(outside.path(), dir.path().join("lua_modules")).unwrap();
    let spec = index_of(dir.path())
        .rockspec_report(None)
        .unwrap()
        .rockspecs
        .remove(0);
    assert!(
        matches!(spec.stores[0].status, SourceStatus::NotInspected),
        "{:?}",
        spec.stores
    );
    assert!(
        !format!("{spec:?}").contains("3.1.3-0"),
        "content outside the root leaked"
    );
}

#[test]
fn a_project_luarocks_config_may_name_other_trees_so_absence_is_not_claimed() {
    let dir = project(&[
        ("demo-1.0-1.rockspec", "dependencies = { 'lpeg' }\n"),
        (
            ".luarocks/config-5.5.lua",
            "rocks_trees = { 'elsewhere' }\n",
        ),
    ]);
    let spec = index_of(dir.path())
        .rockspec_report(None)
        .unwrap()
        .rockspecs
        .remove(0);
    assert!(matches!(
        spec.dependencies[0].installed,
        Installed::Unknown(_)
    ));
    assert!(spec.notes.iter().any(|n| n.contains("config")));
}

#[test]
fn dynamic_rockspec_code_makes_every_section_incomplete() {
    let dir = project(&[(
        "x-1.0-1.rockspec",
        "dependencies = { 'lpeg' }\nif os.getenv('X') then dependencies[#dependencies + 1] = 'evil' end\n",
    )]);
    let spec = index_of(dir.path())
        .rockspec_report(None)
        .unwrap()
        .rockspecs
        .remove(0);
    assert!(matches!(spec.completeness, Completeness::Incomplete(_)));
    assert_eq!(spec.sections.len(), 6);
    assert!(spec
        .sections
        .iter()
        .all(|s| matches!(s.completeness, Completeness::Incomplete(_))));
    assert!(
        spec.dependencies.iter().any(|d| d.name == "lpeg"),
        "recovered data is kept"
    );
}

#[test]
fn install_lua_modules_are_recorded_with_their_source_and_resolved_against_a_real_tree() {
    let dir = project(&[
        (
            "demo-1.0-1.rockspec",
            "package = 'demo'\nversion = '1.0-1'\ndependencies = { 'lua >= 5.1' }\n\
             build = { type = 'none', install = { lua = { ['demo.pkg'] = 'src/pkg/init.lua' } } }\n",
        ),
        (&format!("{STORE_DIR}/manifest"), INSTALL_LUA_TREE),
        ("app/main.lua", "import demo.pkg\n"),
    ]);
    let index = index_of(dir.path());
    let spec = &index.rockspec_report(None).unwrap().rockspecs[0];
    assert_eq!(spec.modules.len(), 1);
    assert_eq!(spec.modules[0].source, "build.install.lua");
    assert_eq!(spec.modules[0].path.as_deref(), Some("src/pkg/init.lua"));

    let (_, origins) = index
        .dependency_graph_with_origins("app/main.lua", 1, 50)
        .unwrap();
    let origin = &origins[0].rockspecs[0];
    assert_eq!(origin.project_modules[0].name, "demo.pkg");
    assert_eq!(origin.providers[0].rock, "demo");
    assert_eq!(origin.providers[0].via, "demo.pkg");
}

fn origin_of(index: &Index, importer: &str) -> mct_index::luarocks::ExternalOrigin {
    let (graph, mut origins) = index
        .dependency_graph_with_origins(importer, 1, 50)
        .unwrap();
    assert_eq!(graph.external.len(), origins.len());
    origins.remove(0)
}

#[test]
fn an_import_is_related_to_the_declared_rock_that_provides_it() {
    // `re` is a module of the `lpeg` rock (lpeg.so + re.lua).
    let dir = project(&[
        ("demo-1.0-1.rockspec", "dependencies = { 'lpeg >= 1.0' }\n"),
        (&format!("{STORE_DIR}/manifest"), REAL_TREE),
        ("app/main.lua", "import re\n"),
    ]);
    let origin = origin_of(&index_of(dir.path()), "app/main.lua");
    assert_eq!(origin.name, "re");
    let spec = &origin.rockspecs[0];
    assert!(spec.complete);
    assert_eq!(spec.providers.len(), 1);
    assert_eq!(spec.providers[0].rock, "lpeg");
    assert!(matches!(
        &spec.providers[0].declaration,
        Declaration::Declared { scopes, constraints } if scopes == &["runtime"] && constraints == &[">= 1.0"]
    ));
}

#[test]
fn an_installed_but_undeclared_provider_is_named_only_when_declarations_are_complete() {
    let tree = (&format!("{STORE_DIR}/manifest") as &str, REAL_TREE);
    let complete = project(&[
        ("demo-1.0-1.rockspec", "dependencies = { 'inspect' }\n"),
        tree,
        ("main.lua", "import re\n"),
    ]);
    let origin = origin_of(&index_of(complete.path()), "main.lua");
    assert_eq!(
        origin.rockspecs[0].providers[0].declaration,
        Declaration::Undeclared
    );

    // Same project, but the rockspec builds its dependencies with code: the
    // missing declaration proves nothing.
    let dynamic = project(&[
        (
            "demo-1.0-1.rockspec",
            "dependencies = { 'inspect' }\nlocal extra = compute()\n",
        ),
        tree,
        ("main.lua", "import re\n"),
    ]);
    let origin = origin_of(&index_of(dynamic.path()), "main.lua");
    assert!(matches!(
        origin.rockspecs[0].providers[0].declaration,
        Declaration::Unproven(_)
    ));
}

#[test]
fn platform_declarations_are_never_called_active() {
    let dir = project(&[
        (
            "demo-1.0-1.rockspec",
            "dependencies = {}\nplatforms = { unix = { dependencies = { 'lpeg' } } }\n",
        ),
        (&format!("{STORE_DIR}/manifest"), REAL_TREE),
        ("main.lua", "import re\n"),
    ]);
    let index = index_of(dir.path());
    let origin = origin_of(&index, "main.lua");
    assert_eq!(
        origin.rockspecs[0].providers[0].declaration,
        Declaration::DeclaredUnderPlatform {
            platforms: vec!["unix".into()]
        }
    );
    let spec = &index.rockspec_report(None).unwrap().rockspecs[0];
    assert_eq!(spec.dependencies[0].platform.as_deref(), Some("unix"));
}

#[test]
fn two_providers_are_two_candidates_and_an_unread_store_removes_uniqueness() {
    let two = "repository = {\n a = { ['1-1'] = { { arch = 'installed', modules = { shared = 'a.lua' } } } },\n \
               b = { ['2-1'] = { { arch = 'installed', modules = { shared = 'b.lua' } } } },\n}\n";
    let dir = project(&[
        ("demo-1.0-1.rockspec", "dependencies = { 'a' }\n"),
        (&format!("{STORE_DIR}/manifest"), two),
        ("main.lua", "import shared\n"),
    ]);
    let spec = origin_of(&index_of(dir.path()), "main.lua")
        .rockspecs
        .remove(0);
    assert_eq!(spec.providers.len(), 2);
    assert!(spec.complete);

    let one = "repository = { a = { ['1-1'] = { { arch = 'installed', modules = { only = 'a.lua' } } } } }\n";
    let sole = project(&[
        ("demo-1.0-1.rockspec", "dependencies = { 'a' }\n"),
        (&format!("{STORE_DIR}/manifest"), one),
        ("main.lua", "import only\n"),
    ]);
    let spec = origin_of(&index_of(sole.path()), "main.lua")
        .rockspecs
        .remove(0);
    assert!(spec.complete && spec.providers.len() == 1);

    let partial = project(&[
        ("demo-1.0-1.rockspec", "dependencies = { 'a' }\n"),
        (&format!("{STORE_DIR}/manifest"), one),
        ("lua_modules/lib/luarocks/rocks-5.4/manifest", "garbage ("),
        ("main.lua", "import only\n"),
    ]);
    let spec = origin_of(&index_of(partial.path()), "main.lua")
        .rockspecs
        .remove(0);
    assert_eq!(spec.providers.len(), 1);
    assert!(
        !spec.complete,
        "one provider found is not uniqueness when a store was unread"
    );
    assert!(spec.reason.is_some());
}

#[test]
fn without_any_provider_the_declared_rocks_are_kept_as_candidates() {
    let dir = project(&[
        (
            "demo-1.0-1.rockspec",
            "dependencies = { 'lua >= 5.1', 'lpeg', 'luasocket' }\n",
        ),
        ("main.lua", "import socket.http\n"),
    ]);
    let spec = origin_of(&index_of(dir.path()), "main.lua")
        .rockspecs
        .remove(0);
    assert!(spec.providers.is_empty() && spec.project_modules.is_empty());
    assert_eq!(spec.unmapped_declared, vec!["lpeg", "luasocket"]);
    assert!(spec.reason.is_some());
}

#[test]
fn each_importer_is_related_to_the_rockspecs_above_it_without_merging_them() {
    let dir = project(&[
        ("root-1.0-1.rockspec", "dependencies = { 'lpeg' }\n"),
        ("pkg/pkg-1.0-1.rockspec", "dependencies = { 'inspect' }\n"),
        ("pkg/pkg-scm-1.rockspec", "dependencies = { 'penlight' }\n"),
        (&format!("{STORE_DIR}/manifest"), REAL_TREE),
        ("pkg/src/mod.lua", "import inspect\n"),
        ("elsewhere/other.lua", "import inspect\n"),
    ]);
    let index = index_of(dir.path());
    let nested = origin_of(&index, "pkg/src/mod.lua");
    let names: Vec<&str> = nested
        .rockspecs
        .iter()
        .map(|r| r.rockspec.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "pkg/pkg-1.0-1.rockspec",
            "pkg/pkg-scm-1.rockspec",
            "root-1.0-1.rockspec"
        ]
    );
    let (_, origins) = index
        .dependency_graph_with_origins("elsewhere/other.lua", 1, 50)
        .unwrap();
    let only: Vec<&str> = origins[0]
        .rockspecs
        .iter()
        .map(|r| r.rockspec.as_str())
        .collect();
    assert_eq!(only, vec!["root-1.0-1.rockspec"]);
}

#[test]
fn an_import_with_no_rockspec_above_it_is_reported_without_an_origin() {
    let dir = project(&[("main.lua", "import inspect\n")]);
    let origin = origin_of(&index_of(dir.path()), "main.lua");
    assert!(origin.rockspecs.is_empty());
}

#[test]
fn both_tables_follow_the_rockspec_through_unreadable_removed_and_empty_states() {
    let dir = project(&[
        ("a-1.0-1.rockspec", "dependencies = { 'lpeg' }\n"),
        ("empty-1.0-1.rockspec", "package = 'empty'\n"),
    ]);
    let mut index = index_of(dir.path());
    assert_eq!(index.rockspec_report(None).unwrap().rockspecs.len(), 2);
    assert_eq!(index.status().unwrap().dependencies.len(), 1);

    // The file stops being analysable text: neither table keeps old data as current.
    fs::write(
        dir.path().join("a-1.0-1.rockspec"),
        [0xff, 0xfe, 0x00, 0x9f],
    )
    .unwrap();
    index.reindex(&registry(), false).unwrap();
    let report = index.rockspec_report(None).unwrap();
    let a = report
        .rockspecs
        .iter()
        .find(|r| r.path == "a-1.0-1.rockspec")
        .unwrap();
    assert!(matches!(a.completeness, Completeness::Unreadable(_)));
    assert!(a.dependencies.is_empty());
    assert!(index.status().unwrap().dependencies.is_empty());

    // Removed: a rockspec that declares nothing is swept too.
    fs::remove_file(dir.path().join("a-1.0-1.rockspec")).unwrap();
    fs::remove_file(dir.path().join("empty-1.0-1.rockspec")).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(index.rockspec_report(None).unwrap().rockspecs.is_empty());
    assert!(index.status().unwrap().dependencies.is_empty());
}

#[test]
fn a_rockspec_over_the_read_limit_keeps_its_old_data_but_marked_as_possibly_stale() {
    let tree = (&format!("{STORE_DIR}/manifest") as &str, REAL_TREE);
    let dir = project(&[
        ("demo-1.0-1.rockspec", "dependencies = { 'inspect' }\n"),
        tree,
        ("main.lua", "import re\n"),
    ]);
    let mut index = index_of(dir.path());
    let fresh = index.rockspec_report(None).unwrap();
    assert_eq!(fresh.rockspecs[0].last_read_failure, None);
    assert_eq!(fresh.rockspecs[0].completeness, Completeness::Complete);
    assert_eq!(
        origin_of(&index, "main.lua").rockspecs[0].providers[0].declaration,
        Declaration::Undeclared
    );

    // Over the 16 MiB global read limit (not the 4 MiB LuaRocks-source cap).
    // The padding is a comment, so were it read it would still declare only
    // `inspect`: the file changed and was not read, which is the point.
    let mut huge = String::from("dependencies = { 'inspect', 'lpeg' }\n-- ");
    huge.push_str(&"x".repeat(17 * 1024 * 1024));
    fs::write(dir.path().join("demo-1.0-1.rockspec"), huge).unwrap();
    index.reindex(&registry(), false).unwrap();

    let report = index.rockspec_report(None).unwrap();
    let spec = &report.rockspecs[0];
    let why = spec.last_read_failure.as_deref().expect("flagged as stale");
    assert!(!why.is_empty());
    // The earlier read is still served, never presented as current.
    let names: Vec<_> = spec.dependencies.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["inspect"], "the last good read is kept");
    assert!(
        matches!(&spec.completeness, Completeness::Incomplete(m) if m.contains("earlier read")),
        "{:?}",
        spec.completeness
    );
    // So an absent declaration is no longer proof of `Undeclared`.
    assert!(matches!(
        origin_of(&index, "main.lua").rockspecs[0].providers[0].declaration,
        Declaration::Unproven(_)
    ));
    // The existing status lists the same path as unreadable, and its
    // dependency projection is the same last good read.
    let status = index.status().unwrap();
    assert!(status
        .read_failures
        .iter()
        .any(|f| f.relative_path == "demo-1.0-1.rockspec"));
    assert_eq!(status.dependencies[0].dependencies.len(), 1);

    // Readable again: the flag goes and the new content replaces the old.
    fs::write(
        dir.path().join("demo-1.0-1.rockspec"),
        "dependencies = { 'inspect', 'lpeg' }\n",
    )
    .unwrap();
    index.reindex(&registry(), false).unwrap();
    let spec = &index.rockspec_report(None).unwrap().rockspecs[0];
    assert_eq!(spec.last_read_failure, None);
    assert_eq!(spec.completeness, Completeness::Complete);
    assert_eq!(spec.dependencies.len(), 2);
    assert!(index.status().unwrap().read_failures.is_empty());
}

#[test]
fn a_dependency_filter_keeps_only_that_name() {
    let dir = project(&[("demo-1.0-1.rockspec", DEMO)]);
    let report = index_of(dir.path())
        .rockspec_report(Some("INSPECT"))
        .unwrap();
    assert_eq!(report.rockspecs[0].dependencies.len(), 2);
    assert!(index_of(dir.path())
        .rockspec_report(Some("nope"))
        .unwrap()
        .rockspecs
        .is_empty());
}

/// Needs a LuaRocks 3 install on `PATH`; builds a real tree offline and checks
/// that the report agrees with it. Ignored by default: it runs an external
/// tool, which the analysis itself never does.
#[test]
#[ignore = "not a bug: needs the luarocks CLI (3.x); run with --ignored"]
fn a_tree_built_by_the_real_luarocks_cli_is_read_correctly() {
    let dir = project(&[
        (
            "demo-1.0-1.rockspec",
            "package = 'demo'\nversion = '1.0-1'\nsource = { url = 'file:///dev/null' }\n\
         dependencies = { 'lua >= 5.1' }\n\
         build = { type = 'none', install = { lua = { ['demo.pkg'] = 'src/pkg/init.lua' } } }\n",
        ),
        ("src/pkg/init.lua", "return {}\n"),
        ("app/main.lua", "import demo.pkg\n"),
    ]);
    let status = std::process::Command::new("luarocks")
        .args(["--tree", "lua_modules", "make", "demo-1.0-1.rockspec"])
        .current_dir(dir.path())
        .status()
        .expect("luarocks must be installed for this test");
    assert!(status.success());
    let index = index_of(dir.path());
    let (_, origins) = index
        .dependency_graph_with_origins("app/main.lua", 1, 50)
        .unwrap();
    assert_eq!(origins[0].rockspecs[0].providers[0].rock, "demo");
}
