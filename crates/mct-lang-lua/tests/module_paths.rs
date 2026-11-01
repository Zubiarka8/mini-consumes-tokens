//! `require` against project layouts: module roots such as `src/`, the
//! `?.lua` and `?/init.lua` templates, several roots, and the cases where the
//! layout cannot be proven. The index matches a module path against the path
//! components of the files it holds, so a root needs no configuration; what a
//! file is *named* still has to follow the `?.lua` / `?/init.lua` templates.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_lua::LuaParser;

struct Project {
    root: PathBuf,
    index: Index,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-lua-paths-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        for (path, contents) in files {
            let full: &Path = &root.join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, contents).unwrap();
        }
        let mut registry = LanguageRegistry::new();
        registry.register(Arc::new(LuaParser));
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        Self { root, index }
    }

    /// Resolution and candidate files of the import `to` made by `main.lua`.
    fn import(&self, to: &str) -> (Resolution, Vec<String>) {
        let hits: Vec<_> = self
            .index
            .find_references(to)
            .unwrap()
            .into_iter()
            .filter(|h| h.kind == "imports" && h.relative_path == "main.lua" && h.to_name == to)
            .collect();
        assert_eq!(hits.len(), 1, "{to}: {hits:?}");
        let mut paths: Vec<String> = hits[0]
            .candidates
            .iter()
            .map(|c| c.relative_path.clone())
            .collect();
        paths.sort();
        (hits[0].resolution, paths)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const MODULE: &str = "return {}\n";

#[test]
fn a_src_root_needs_no_configuration() {
    let project = Project::new(&[
        ("main.lua", "local s = require(\"app.service\")\n"),
        ("src/app/service.lua", MODULE),
        ("other/service.lua", MODULE),
    ]);
    assert_eq!(
        project.import("service"),
        (
            Resolution::Resolved,
            vec!["src/app/service.lua".to_string()]
        )
    );
}

#[test]
fn the_init_template_resolves_a_package_directory_under_any_root() {
    let project = Project::new(&[
        (
            "main.lua",
            "local u = require(\"lib.util\")\nlocal p = require(\"pkg\")\n",
        ),
        ("packages/lib/util/init.lua", MODULE),
        ("src/pkg/init.lua", MODULE),
    ]);
    assert_eq!(
        project.import("util"),
        (
            Resolution::Resolved,
            vec!["packages/lib/util/init.lua".to_string()]
        )
    );
    assert_eq!(
        project.import("pkg"),
        (Resolution::Resolved, vec!["src/pkg/init.lua".to_string()])
    );
}

#[test]
fn the_same_module_under_two_roots_is_ambiguous_not_guessed() {
    let project = Project::new(&[
        ("main.lua", "local s = require(\"app.service\")\n"),
        ("src/app/service.lua", MODULE),
        ("lib/app/service.lua", MODULE),
    ]);
    let (resolution, paths) = project.import("service");
    assert_eq!(resolution, Resolution::Ambiguous);
    assert_eq!(paths, ["lib/app/service.lua", "src/app/service.lua"]);
}

#[test]
fn a_module_file_named_differently_from_its_module_is_not_linked() {
    // A rockspec `modules = { ["app.service"] = "src/impl/svc.lua" }` or a
    // `package.path` template such as `src/?/main.lua` would make this file
    // the module, but the index sees only the file's name. It stays
    // unresolved rather than guessed.
    let project = Project::new(&[
        (
            "main.lua",
            "package.path = package.path .. \";src/?/main.lua\"\nlocal s = require(\"app.service\")\n",
        ),
        ("src/impl/svc.lua", MODULE),
    ]);
    assert_eq!(project.import("service").0, Resolution::Unresolved);
}
