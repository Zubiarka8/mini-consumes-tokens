//! The LuaRocks analysis must never run a process, open a socket or write to
//! the project (issue #267). Two complementary checks, because neither alone
//! proves it:
//!
//! - a scan of the analyser's sources for process and network APIs, which
//!   covers every code path (including ones this test never drives);
//! - a run over hostile rockspec, manifest and lockfile content with fake
//!   `luarocks`/`lua` executables first on `PATH` that leave a marker file,
//!   plus a before/after snapshot of the project tree. This proves those named
//!   executables are not run and nothing is written; it cannot prove that no
//!   process of any other kind exists, which is what the source scan is for.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: [&str; 8] = [
    "process::Command",
    "Command::new",
    "std::net",
    "TcpStream",
    "UdpSocket",
    "libc::fork",
    "libc::exec",
    "posix_spawn",
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_analyser_sources_use_no_process_or_network_api() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![src.join("luarocks.rs"), src.join("manifests/lua.rs")];
    rust_files(&src.join("manifests/lua"), &mut files);
    assert!(
        files.len() >= 6,
        "the scan must cover every analyser file: {files:?}"
    );
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for api in FORBIDDEN {
            assert!(!text.contains(api), "{} uses `{api}`", file.display());
        }
    }
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut files = Vec::new();
    rust_files_any(root, &mut files);
    for file in files {
        let rel = file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        out.insert(rel, fs::read(&file).unwrap());
    }
    out
}

fn rust_files_any(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files_any(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[cfg(unix)]
#[test]
fn hostile_inputs_with_fake_executables_on_path_run_nothing_and_write_nothing() {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;

    use mct_core::LanguageRegistry;
    use mct_index::{ExcludeSet, Index};

    let outside = tempfile::tempdir().unwrap();
    let marker = outside.path().join("EXECUTED");
    let bin = outside.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    for name in ["luarocks", "lua", "luajit", "sh", "touch"] {
        let script = bin.join(name);
        fs::write(
            &script,
            format!("#!/bin/sh\necho {name} >> '{}'\n", marker.display()),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let original = std::env::var_os("PATH").unwrap_or_default();
    let mut path = std::ffi::OsString::from(&bin);
    path.push(":");
    path.push(&original);
    std::env::set_var("PATH", &path);

    let project = tempfile::tempdir().unwrap();
    let hostile = format!(
        "os.execute('touch {m}')\nio.popen('luarocks install evil')\n\
         dependencies = {{ 'lpeg' }}\n",
        m = marker.display()
    );
    for (rel, text) in [
        ("evil-1.0-1.rockspec", hostile.as_str()),
        (
            "lua_modules/lib/luarocks/rocks-5.5/manifest",
            "repository = io.popen('luarocks list')\nos.execute('touch x')\n",
        ),
        (
            "luarocks.lock",
            "os.execute('luarocks install evil')\nreturn {}\n",
        ),
        ("app/main.lua", "import lpeg\n"),
    ] {
        let file = project.path().join(rel);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    let before = snapshot(project.path());

    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(NoopLua));
    let mut index = Index::open_in_memory(project.path(), ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let report = index.rockspec_report(None).unwrap();
    let _ = index
        .dependency_graph_with_origins("app/main.lua", 2, 50)
        .unwrap();
    assert_eq!(
        report.rockspecs.len(),
        1,
        "the hostile rockspec was analysed"
    );

    std::env::set_var("PATH", original);
    assert!(!marker.exists(), "an executable named on PATH was run");
    assert_eq!(
        snapshot(project.path()),
        before,
        "the analysis wrote to the project"
    );
}

#[cfg(unix)]
struct NoopLua;

#[cfg(unix)]
impl mct_core::LanguageParser for NoopLua {
    fn language_id(&self) -> &'static str {
        "lua"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["lua"]
    }

    fn parse(
        &self,
        _: &mct_core::SourceFile,
    ) -> Result<mct_core::ParsedFile, mct_core::ParseError> {
        Ok(mct_core::ParsedFile::default())
    }
}
