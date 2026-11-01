//! What the project declares about LuaRocks, what is installed beside it, and
//! where an imported Lua module comes from — read statically (issue #267).
//!
//! The declared side comes from `rockspec_items`, written at index time. The
//! installed and pinned sides are read when asked: they live on disk outside
//! the manifest and change without the rockspec changing. Nothing is
//! executed, no process is spawned and nothing is written.
//!
//! Three rules shape every result:
//! - A source that could not be read is never reported as "absent": the
//!   answer becomes incomplete or unknown instead.
//! - A provider is never called unique unless every relevant source was read.
//! - Nothing here claims which version LuaRocks would select at run time;
//!   that depends on the loader context and tree order of the running
//!   process (`luarocks/loader.lua`).

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use crate::manifests::lua::{
    self, Lockfile, StoreError, StoreManifest, DECLARATION_SECTIONS, KIND_DEPENDENCY, KIND_FILE,
    KIND_MODULE, KIND_SECTION, STATE_COMPLETE, STATE_INCOMPLETE, STATE_UNREADABLE,
};
use crate::read_limits::read_repository_file_capped;
use crate::{DependencyGraph, Index, Result};

/// Size cap for a store `manifest` and for `luarocks.lock`.
const MAX_SOURCE_BYTES: u64 = 4 * 1024 * 1024;
/// Most `rocks-*` stores read beside one rockspec.
const MAX_STORES: usize = 8;
/// Most entries listed from one directory.
const MAX_DIR_ENTRIES: usize = 256;

/// What happened when one source was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceStatus {
    Read,
    /// The file or directory does not exist (a fact about the project-local
    /// location, not about LuaRocks trees elsewhere).
    Missing,
    Unreadable(String),
    TooLarge,
    Corrupt(String),
    /// Excluded by configuration or resolving outside the project root; the
    /// reason is deliberately not named.
    NotInspected,
}

/// Whether the declared data of a rockspec or one of its sections was read
/// completely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completeness {
    Complete,
    /// Some of it could not be proven; what was recovered is still listed.
    Incomplete(String),
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionState {
    pub section: String,
    pub completeness: Completeness,
}

/// One store (`lib/luarocks/rocks-<lua>`) of a project-local tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreStatus {
    /// The tree directory, relative to the project root.
    pub tree: String,
    /// The Lua version of the store, as named on disk (`5.5`).
    pub lua: String,
    pub status: SourceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledVersion {
    pub version: String,
    pub lua: String,
    pub tree: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// Nothing can be said (no readable store, or an interpreter constraint).
    Unknown(String),
    /// Seen in at least one readable store; `complete` is false when another
    /// relevant source could not be read, so more versions may exist.
    Found {
        versions: Vec<InstalledVersion>,
        complete: bool,
    },
    /// Every project-local store was read and none lists it. User and system
    /// trees are never inspected, so this is not "not installed".
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Yes,
    No,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pin {
    NoLockfile,
    Unknown(String),
    /// A readable lockfile that does not pin this rock.
    NotPinned,
    Pinned {
        version: String,
        installed: Presence,
        /// Whether the pinned version meets the declared constraint; `None`
        /// when the constraint text is not one LuaRocks accepts.
        satisfies: Option<bool>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Satisfaction {
    Unknown(String),
    NotInstalled,
    /// Installed versions that meet the constraint. Says nothing about which
    /// one LuaRocks would pick.
    Satisfied(Vec<String>),
    Unsatisfied(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredDependency {
    /// `runtime`, `build` or `test`.
    pub scope: String,
    /// Set when the entry sits under `platforms.<p>`; never claimed active.
    pub platform: Option<String>,
    /// `name` or `namespace/name`, lowercase.
    pub name: String,
    pub constraint: Option<String>,
    pub constraint_recognised: bool,
    pub installed: Installed,
    pub pin: Pin,
    pub satisfaction: Satisfaction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectModule {
    pub name: String,
    /// The file (the first source of a native module), as written.
    pub path: Option<String>,
    pub native: bool,
    /// `build.modules` or `build.install.lua`. The latter's interpretation
    /// is validated only for LuaRocks 3.13.0 with `type = "none"`.
    pub source: String,
    pub platform: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RockspecDetail {
    pub path: String,
    pub package: Option<String>,
    pub version: Option<String>,
    pub format: Option<String>,
    pub completeness: Completeness,
    pub sections: Vec<SectionState>,
    pub dependencies: Vec<DeclaredDependency>,
    pub modules: Vec<ProjectModule>,
    pub stores: Vec<StoreStatus>,
    pub lockfile: SourceStatus,
    /// Why the rockspec could not be read on the last run, when it could not.
    /// Everything above then comes from an earlier read of the file.
    pub last_read_failure: Option<String>,
    /// Limits of this report that the reader must see.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RockspecReport {
    pub rockspecs: Vec<RockspecDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaration {
    Declared {
        scopes: Vec<String>,
        constraints: Vec<String>,
    },
    DeclaredUnderPlatform {
        platforms: Vec<String>,
    },
    /// Every declaration section of the rockspec was read completely and
    /// none names this rock.
    Undeclared,
    /// The rockspec's declarations were not read completely, so absence of
    /// a declaration proves nothing.
    Unproven(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    pub rock: String,
    pub versions: Vec<InstalledVersion>,
    /// The module name the store lists (`m` or `m.init`).
    pub via: String,
    pub declaration: Declaration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RockspecOrigin {
    pub rockspec: String,
    pub project_modules: Vec<ProjectModule>,
    pub providers: Vec<Provider>,
    /// True only when every relevant source was read, so the candidates
    /// listed are all there are.
    pub complete: bool,
    /// Declared rocks whose module lists could not be read: any of them
    /// might provide the module.
    pub unmapped_declared: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalOrigin {
    pub name: String,
    /// The Lua files that import it.
    pub importers: Vec<String>,
    /// One entry per rockspec whose directory contains an importer. Empty
    /// when no rockspec covers the importers.
    pub rockspecs: Vec<RockspecOrigin>,
}

// ------------------------------------------------------------ reading

#[derive(Debug)]
struct Row {
    kind: String,
    scope: Option<String>,
    platform: Option<String>,
    namespace: Option<String>,
    name: String,
    constraint: Option<String>,
    path: Option<String>,
    native: bool,
    source: Option<String>,
    state: String,
    detail: Option<String>,
}

struct Store {
    tree: String,
    lua: String,
    status: SourceStatus,
    manifest: Option<StoreManifest>,
}

enum LockState {
    Absent,
    Unknown(String),
    Pins(Lockfile),
}

struct Project {
    stores: Vec<Store>,
    lock: LockState,
    lock_status: SourceStatus,
    /// Why trees beyond the ones read may exist, if known.
    extra_unknown: Option<String>,
}

fn join(dir: &str, rest: &str) -> String {
    if dir.is_empty() {
        rest.to_string()
    } else {
        format!("{dir}/{rest}")
    }
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn describe(error: &io::Error) -> String {
    error.kind().to_string()
}

impl Index {
    /// Reads `relative` (a project-relative path) only if it is a file that
    /// resolves inside the project root and is not excluded.
    fn read_inside(&self, relative: &str) -> std::result::Result<Vec<u8>, SourceStatus> {
        let canonical = self.resolve_inside(relative)?;
        read_repository_file_capped(&canonical, MAX_SOURCE_BYTES).map_err(|error| {
            if error.kind() == io::ErrorKind::InvalidData {
                SourceStatus::TooLarge
            } else {
                SourceStatus::Unreadable(describe(&error))
            }
        })
    }

    /// The canonical path of `relative` once it is known to lie inside the
    /// root and outside every exclusion. The exclusion test runs on the text
    /// of the path before the disk is touched, and again on the canonical
    /// path, as the indexer does.
    fn resolve_inside(
        &self,
        relative: &str,
    ) -> std::result::Result<std::path::PathBuf, SourceStatus> {
        if self.exclude.is_excluded(relative) {
            return Err(SourceStatus::NotInspected);
        }
        let canonical = match self.root.join(relative).canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(SourceStatus::Missing)
            }
            Err(error) => return Err(SourceStatus::Unreadable(describe(&error))),
        };
        if !canonical.starts_with(&self.root) {
            return Err(SourceStatus::NotInspected);
        }
        match crate::indexer::to_relative_slash_path(&self.root, &canonical) {
            Some(rel) if !self.exclude.is_excluded(&rel) => Ok(canonical),
            _ => Err(SourceStatus::NotInspected),
        }
    }

    fn list_inside(&self, relative: &str) -> std::result::Result<Vec<String>, SourceStatus> {
        let canonical = self.resolve_inside(relative)?;
        let entries = std::fs::read_dir(&canonical)
            .map_err(|error| SourceStatus::Unreadable(describe(&error)))?;
        let mut names = Vec::new();
        for entry in entries.take(MAX_DIR_ENTRIES) {
            let entry = entry.map_err(|error| SourceStatus::Unreadable(describe(&error)))?;
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
        names.sort();
        Ok(names)
    }

    fn load_project(&self, dir: &str) -> Project {
        let tree = join(dir, "lua_modules");
        let libdir = join(&tree, "lib/luarocks");
        let mut stores = Vec::new();
        match self.list_inside(&libdir) {
            Ok(names) => {
                let mut candidates = names
                    .iter()
                    .filter(|n| n.starts_with("rocks"))
                    .collect::<Vec<_>>();
                let overflow = candidates.len() > MAX_STORES;
                candidates.truncate(MAX_STORES);
                for name in candidates {
                    stores.push(self.load_store(&tree, &libdir, name));
                }
                if overflow {
                    stores.push(Store {
                        tree: tree.clone(),
                        lua: "?".into(),
                        status: SourceStatus::Corrupt(format!("more than {MAX_STORES} stores")),
                        manifest: None,
                    });
                }
            }
            // No project-local tree at all: a fact about this location.
            Err(SourceStatus::Missing) => {}
            Err(status) => stores.push(Store {
                tree: tree.clone(),
                lua: "?".into(),
                status,
                manifest: None,
            }),
        }
        let extra_unknown = match self.list_inside(&join(dir, ".luarocks")) {
            Ok(names) if names.iter().any(|n| n.starts_with("config")) => Some(
                "a project LuaRocks config exists; the trees it names are not read".to_string(),
            ),
            Ok(_) | Err(SourceStatus::Missing) => None,
            Err(_) => Some("the project LuaRocks config location was not inspected".to_string()),
        };
        let (lock, lock_status) = self.load_lock(dir);
        Project {
            stores,
            lock,
            lock_status,
            extra_unknown,
        }
    }

    fn load_store(&self, tree: &str, libdir: &str, name: &str) -> Store {
        let lua = name.strip_prefix("rocks-").unwrap_or("?").to_string();
        let mut store = Store {
            tree: tree.to_string(),
            lua,
            status: SourceStatus::Read,
            manifest: None,
        };
        if !name.starts_with("rocks-") {
            store.status = SourceStatus::Corrupt("unrecognised store layout".into());
            return store;
        }
        let bytes = match self.read_inside(&join(libdir, &format!("{name}/manifest"))) {
            Ok(bytes) => bytes,
            Err(status) => {
                store.status = status;
                return store;
            }
        };
        match String::from_utf8(bytes) {
            Ok(text) => match lua::parse_store_manifest(&text) {
                Ok(manifest) => store.manifest = Some(manifest),
                Err(StoreError::TooLarge) => store.status = SourceStatus::TooLarge,
                Err(StoreError::Corrupt(reason)) => store.status = SourceStatus::Corrupt(reason),
            },
            Err(_) => store.status = SourceStatus::Corrupt("not valid UTF-8".into()),
        }
        store
    }

    fn load_lock(&self, dir: &str) -> (LockState, SourceStatus) {
        match self.read_inside(&join(dir, "luarocks.lock")) {
            Err(SourceStatus::Missing) => (LockState::Absent, SourceStatus::Missing),
            Err(SourceStatus::NotInspected) => (
                LockState::Unknown("lockfile location not inspected".into()),
                SourceStatus::NotInspected,
            ),
            Err(status) => (
                LockState::Unknown("lockfile could not be read".into()),
                status,
            ),
            Ok(bytes) => match String::from_utf8(bytes)
                .map_err(|_| "not valid UTF-8".to_string())
                .and_then(|text| lua::parse_lockfile(&text))
            {
                Ok(lock) => (LockState::Pins(lock), SourceStatus::Read),
                Err(reason) => (
                    LockState::Unknown(format!("lockfile is not a verifiable pin table: {reason}")),
                    SourceStatus::Corrupt(reason),
                ),
            },
        }
    }

    /// Rockspecs whose last read failed (unreadable, over the read limit):
    /// their stored rows are from an earlier read.
    fn rockspec_read_failures(&self) -> Result<BTreeMap<String, String>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT relative_path, detail FROM index_issues WHERE issue_kind = 'read_failure'",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (path, detail): (String, String) = row?;
            out.insert(path, detail);
        }
        Ok(out)
    }

    fn rockspec_rows(&self) -> Result<BTreeMap<String, Vec<Row>>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT manifest_path, kind, scope, platform, namespace, name,
                    version_constraint, path, native, source, state, detail
               FROM rockspec_items ORDER BY manifest_path, position",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                Row {
                    kind: row.get(1)?,
                    scope: row.get(2)?,
                    platform: row.get(3)?,
                    namespace: row.get(4)?,
                    name: row.get(5)?,
                    constraint: row.get(6)?,
                    path: row.get(7)?,
                    native: row.get(8)?,
                    source: row.get(9)?,
                    state: row.get(10)?,
                    detail: row.get(11)?,
                },
            ))
        })?;
        let mut out: BTreeMap<String, Vec<Row>> = BTreeMap::new();
        for row in rows {
            let (path, row) = row?;
            out.entry(path).or_default().push(row);
        }
        Ok(out)
    }

    /// Every rockspec's declared dependencies, each set against what is
    /// installed beside it and what its lockfile pins. `dependency` keeps
    /// only entries with that exact (lowercase) name.
    pub fn rockspec_report(&self, dependency: Option<&str>) -> Result<RockspecReport> {
        let wanted = dependency.map(str::to_lowercase);
        let mut report = RockspecReport::default();
        let failures = self.rockspec_read_failures()?;
        for (path, rows) in self.rockspec_rows()? {
            let project = self.load_project(parent_dir(&path));
            let mut detail = build_detail(&path, &rows, &project, failures.get(&path));
            if let Some(name) = &wanted {
                detail.dependencies.retain(|d| &d.name == name);
                detail.modules.clear();
                if detail.dependencies.is_empty() {
                    continue;
                }
            }
            report.rockspecs.push(detail);
        }
        Ok(report)
    }

    /// [`Index::dependency_graph`] plus the origin of every external import
    /// made by a Lua file in the walk. The graph is the same value.
    pub fn dependency_graph_with_origins(
        &self,
        module: &str,
        depth: u32,
        limit: usize,
    ) -> Result<(DependencyGraph, Vec<ExternalOrigin>)> {
        let (graph, importers) = self.walk_dependency_graph(module, depth, limit)?;
        let lua_importers: BTreeMap<String, Vec<String>> = importers
            .into_iter()
            .filter_map(|(name, files)| {
                let files: Vec<String> =
                    files.into_iter().filter(|f| f.ends_with(".lua")).collect();
                (!files.is_empty()).then_some((name, files))
            })
            .collect();
        if lua_importers.is_empty() {
            return Ok((graph, Vec::new()));
        }
        let rows = self.rockspec_rows()?;
        let failures = self.rockspec_read_failures()?;
        let mut projects: BTreeMap<String, Project> = BTreeMap::new();
        let mut origins = Vec::new();
        for (name, files) in lua_importers {
            let mut rockspecs = Vec::new();
            for (path, rows) in &rows {
                let dir = parent_dir(path);
                let covers = files
                    .iter()
                    .any(|f| dir.is_empty() || f.starts_with(&format!("{dir}/")));
                if !covers || !rows.iter().any(|r| r.kind == KIND_FILE) {
                    continue;
                }
                let project = projects
                    .entry(dir.to_string())
                    .or_insert_with(|| self.load_project(dir));
                rockspecs.push(origin_in(path, rows, project, &name, failures.get(path)));
            }
            origins.push(ExternalOrigin {
                name,
                importers: files,
                rockspecs,
            });
        }
        Ok((graph, origins))
    }
}

// ------------------------------------------------------ pure assembly

fn completeness(state: &str, detail: &Option<String>) -> Completeness {
    let reason = detail.clone().unwrap_or_default();
    match state {
        STATE_COMPLETE => Completeness::Complete,
        STATE_UNREADABLE => Completeness::Unreadable(reason),
        STATE_INCOMPLETE => Completeness::Incomplete(reason),
        other => Completeness::Incomplete(format!("unknown state `{other}`")),
    }
}

fn dependency_name(row: &Row) -> String {
    match &row.namespace {
        Some(ns) => format!("{ns}/{}", row.name),
        None => row.name.clone(),
    }
}

fn project_module(row: &Row) -> ProjectModule {
    ProjectModule {
        name: row.name.clone(),
        path: row.path.clone(),
        native: row.native,
        source: row.source.clone().unwrap_or_default(),
        platform: row.platform.clone(),
    }
}

/// Installed versions of `rock` across the stores that were read. A source
/// that could not be read (or a config that may name more trees) makes the
/// answer incomplete.
fn installed_in(project: &Project, rock: &str) -> Installed {
    let mut versions = Vec::new();
    let mut unread = usize::from(project.extra_unknown.is_some());
    for store in &project.stores {
        match &store.manifest {
            Some(manifest) => {
                for version in manifest.rocks.get(rock).into_iter().flat_map(|v| v.keys()) {
                    versions.push(InstalledVersion {
                        version: version.clone(),
                        lua: store.lua.clone(),
                        tree: store.tree.clone(),
                    });
                }
            }
            None => unread += 1,
        }
    }
    if !versions.is_empty() {
        Installed::Found {
            versions,
            complete: unread == 0,
        }
    } else if unread == 0 {
        Installed::NotFound
    } else {
        Installed::Unknown(format!("{unread} source(s) could not be read"))
    }
}

fn satisfaction(installed: &Installed, constraint: Option<&str>) -> Satisfaction {
    match installed {
        Installed::Unknown(reason) => Satisfaction::Unknown(reason.clone()),
        Installed::NotFound => Satisfaction::NotInstalled,
        Installed::Found { versions, complete } => {
            let mut ok = Vec::new();
            for v in versions {
                match lua::constraint_accepts(constraint, &v.version) {
                    Some(true) => ok.push(v.version.clone()),
                    Some(false) => {}
                    None => return Satisfaction::Unknown("constraint not recognised".into()),
                }
            }
            if !ok.is_empty() {
                Satisfaction::Satisfied(ok)
            } else if *complete {
                Satisfaction::Unsatisfied(versions.iter().map(|v| v.version.clone()).collect())
            } else {
                Satisfaction::Unknown(
                    "no readable store has a matching version; others were not read".into(),
                )
            }
        }
    }
}

fn pin_for(
    project: &Project,
    scope: &str,
    name: &str,
    constraint: Option<&str>,
    installed: &Installed,
) -> Pin {
    match &project.lock {
        LockState::Absent => Pin::NoLockfile,
        LockState::Unknown(reason) => Pin::Unknown(reason.clone()),
        LockState::Pins(lock) => {
            let Some((_, _, version)) = lock.pins.iter().find(|(s, n, _)| *s == scope && n == name)
            else {
                return Pin::NotPinned;
            };
            let presence = match installed {
                Installed::Found { versions, complete } => {
                    if versions.iter().any(|v| &v.version == version) {
                        Presence::Yes
                    } else if *complete {
                        Presence::No
                    } else {
                        Presence::Unknown
                    }
                }
                Installed::NotFound => Presence::No,
                Installed::Unknown(_) => Presence::Unknown,
            };
            Pin::Pinned {
                version: version.clone(),
                installed: presence,
                satisfies: lua::constraint_accepts(constraint, version),
            }
        }
    }
}

fn build_detail(
    path: &str,
    rows: &[Row],
    project: &Project,
    read_failure: Option<&String>,
) -> RockspecDetail {
    let file = rows.iter().find(|r| r.kind == KIND_FILE);
    let mut detail = RockspecDetail {
        path: path.to_string(),
        package: file.map(|f| f.name.clone()).filter(|n| !n.is_empty()),
        version: file.and_then(|f| f.constraint.clone()),
        format: file
            .filter(|f| f.state != STATE_UNREADABLE)
            .and_then(|f| f.detail.clone()),
        completeness: file.map_or(Completeness::Incomplete("not analysed yet".into()), |f| {
            completeness(&f.state, &f.detail)
        }),
        sections: rows
            .iter()
            .filter(|r| r.kind == KIND_SECTION)
            .map(|r| SectionState {
                section: r.name.clone(),
                completeness: completeness(&r.state, &r.detail),
            })
            .collect(),
        dependencies: Vec::new(),
        modules: rows
            .iter()
            .filter(|r| r.kind == KIND_MODULE)
            .map(project_module)
            .collect(),
        stores: project
            .stores
            .iter()
            .map(|s| StoreStatus {
                tree: s.tree.clone(),
                lua: s.lua.clone(),
                status: s.status.clone(),
            })
            .collect(),
        lockfile: project.lock_status.clone(),
        last_read_failure: read_failure.cloned(),
        notes: vec!["user and system LuaRocks trees are never inspected".into()],
    };
    if let Some(why) = read_failure {
        // The rows are the last good read, not the file as it is now.
        detail.completeness = Completeness::Incomplete(format!(
            "the rockspec could not be read on the last run ({why}); this is from an earlier read"
        ));
    }
    if let Some(extra) = &project.extra_unknown {
        detail.notes.push(extra.clone());
    }
    for row in rows.iter().filter(|r| r.kind == KIND_DEPENDENCY) {
        let scope = row.scope.clone().unwrap_or_default();
        let name = dependency_name(row);
        let interpreter = row.namespace.is_none() && row.name == "lua";
        let installed = if interpreter {
            Installed::Unknown("interpreter constraint, not a rock".into())
        } else {
            installed_in(project, &row.name)
        };
        // The pin follows the same rules for every entry: an unreadable
        // lockfile is unknown for `lua` too.
        let pin = pin_for(
            project,
            &scope,
            &row.name,
            row.constraint.as_deref(),
            &installed,
        );
        let satisfaction = if interpreter {
            Satisfaction::Unknown("interpreter version not inspected".into())
        } else {
            satisfaction(&installed, row.constraint.as_deref())
        };
        detail.dependencies.push(DeclaredDependency {
            scope,
            platform: row.platform.clone(),
            name,
            constraint: row.constraint.clone(),
            constraint_recognised: row.state == STATE_COMPLETE,
            installed,
            pin,
            satisfaction,
        });
    }
    detail
}

fn origin_in(
    path: &str,
    rows: &[Row],
    project: &Project,
    module: &str,
    read_failure: Option<&String>,
) -> RockspecOrigin {
    let init = format!("{module}.init");
    let project_modules: Vec<ProjectModule> = rows
        .iter()
        .filter(|r| r.kind == KIND_MODULE && (r.name == module || r.name == init))
        .map(project_module)
        .collect();

    let declarations_complete = {
        let file_ok = rows
            .iter()
            .find(|r| r.kind == KIND_FILE)
            .is_some_and(|f| f.state == STATE_COMPLETE);
        let sections_ok = DECLARATION_SECTIONS.iter().all(|section| {
            rows.iter()
                .any(|r| r.kind == KIND_SECTION && r.name == *section && r.state == STATE_COMPLETE)
        });
        file_ok && sections_ok && read_failure.is_none()
    };

    let mut by_rock: BTreeMap<String, (Vec<InstalledVersion>, String)> = BTreeMap::new();
    let mut unread = usize::from(project.extra_unknown.is_some());
    for store in &project.stores {
        let Some(manifest) = &store.manifest else {
            unread += 1;
            continue;
        };
        for (rock, versions) in &manifest.rocks {
            for (version, modules) in versions {
                let via = if modules.contains_key(module) {
                    module
                } else if modules.contains_key(&init) {
                    &init
                } else {
                    continue;
                };
                let slot = by_rock
                    .entry(rock.clone())
                    .or_insert_with(|| (Vec::new(), via.to_string()));
                slot.0.push(InstalledVersion {
                    version: version.clone(),
                    lua: store.lua.clone(),
                    tree: store.tree.clone(),
                });
            }
        }
    }

    let declared: Vec<&Row> = rows.iter().filter(|r| r.kind == KIND_DEPENDENCY).collect();
    let providers: Vec<Provider> = by_rock
        .into_iter()
        .map(|(rock, (versions, via))| {
            let matching: Vec<&&Row> = declared.iter().filter(|r| r.name == rock).collect();
            let top: Vec<&&Row> = matching
                .iter()
                .copied()
                .filter(|r| r.platform.is_none())
                .collect();
            let declaration = if !top.is_empty() {
                let mut scopes: Vec<String> = top.iter().filter_map(|r| r.scope.clone()).collect();
                scopes.sort();
                scopes.dedup();
                Declaration::Declared {
                    scopes,
                    constraints: top.iter().filter_map(|r| r.constraint.clone()).collect(),
                }
            } else if !matching.is_empty() {
                let mut platforms: Vec<String> =
                    matching.iter().filter_map(|r| r.platform.clone()).collect();
                platforms.sort();
                platforms.dedup();
                Declaration::DeclaredUnderPlatform { platforms }
            } else if declarations_complete {
                Declaration::Undeclared
            } else {
                Declaration::Unproven(if read_failure.is_some() {
                    "the rockspec could not be read on the last run; its declarations may be out of date"
                        .into()
                } else {
                    "the rockspec's declarations were not read completely".into()
                })
            };
            Provider {
                rock,
                versions,
                via,
                declaration,
            }
        })
        .collect();

    let complete = unread == 0;
    let provider_rocks: BTreeSet<&str> = providers.iter().map(|p| p.rock.as_str()).collect();
    let mut unmapped_declared: Vec<String> = Vec::new();
    for row in &declared {
        if row.namespace.is_none() && row.name == "lua" {
            continue;
        }
        let known = matches!(installed_in(project, &row.name), Installed::Found { .. });
        let name = dependency_name(row);
        if !known
            && !provider_rocks.contains(row.name.as_str())
            && !unmapped_declared.contains(&name)
        {
            unmapped_declared.push(name);
        }
    }
    let reason = if providers.is_empty() && project_modules.is_empty() {
        Some(
            if project.stores.is_empty() && project.extra_unknown.is_none() {
                "no project-local LuaRocks tree was found".to_string()
            } else if complete {
                "no project-local store lists this module".to_string()
            } else {
                format!("{unread} source(s) could not be read")
            },
        )
    } else if !complete {
        Some(format!(
            "{unread} source(s) could not be read, so other providers may exist"
        ))
    } else {
        None
    };
    RockspecOrigin {
        rockspec: path.to_string(),
        project_modules,
        providers,
        complete,
        unmapped_declared,
        reason,
    }
}
