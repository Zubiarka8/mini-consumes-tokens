use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::Index;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, InitializeResult, ProtocolVersion, ServerCapabilities},
    model::{CallToolResult, ContentBlock},
    tool, tool_handler, tool_router,
};
use tokio::sync::Mutex;

use crate::format;
use crate::toon::OutputFormat;
use crate::ttc;

/// Applied to any of the 5 result-returning tools below when their `limit`
/// argument is omitted. 50 is a compromise: generous enough that the common
/// case (a handful to a few dozen hits) never gets truncated, but small
/// enough that a symbol with hundreds of hits in a large repo can't blow up
/// a single response to a size that rivals the `grep` output this project
/// exists to replace.
const DEFAULT_RESULT_LIMIT: usize = 50;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListSymbolsArgs {
    /// A single file's path (e.g. `crates/mct-lang-html/src/lib.rs`), or a
    /// directory/crate prefix with no file extension (e.g.
    /// `crates/mct-lang-html/src`) to list every symbol under it. Relative to
    /// the project root, forward slashes on any OS.
    pub path: String,
    /// Exact symbol kind to keep (e.g. `function`, `method`, `class`,
    /// `struct`, `interface`, `enum`, `trait`, `type_alias`, `module`,
    /// `variable`, `constant`, `field`). Omit to include every kind.
    #[serde(default)]
    pub kind: Option<String>,
    /// Exact language id to keep (e.g. `rust`, `python`, `go`). Omit to
    /// include every language — only useful when `path` is a directory that
    /// mixes languages.
    #[serde(default)]
    pub language: Option<String>,
    /// Maximum number of symbols to return. Defaults to 50 when omitted;
    /// raise it if you expect more hits and want them all in one call.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Response shape: `text` (default) is this server's existing
    /// human/agent-skimmable plain text. `toon` renders the same data as a
    /// compact TOON table (one header row of column names, then one row per
    /// symbol, no repeated labels) — fewer tokens for a large result, at the
    /// cost of the text version's per-kind grouping.
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindSymbolArgs {
    /// Symbol name to look up (e.g. a function, class, struct or method
    /// name). Matched exactly, as a prefix, or as a substring depending on
    /// `match` — see that field.
    pub name: String,
    /// How `name` is matched: `exact` (default) requires the full name,
    /// byte for byte. `prefix` returns every symbol whose name starts with
    /// `name`. `fuzzy` returns every symbol whose name contains `name`
    /// anywhere, case-insensitively — use this when you're not sure of the
    /// exact/full name instead of guessing and re-querying. Unrecognized
    /// values are rejected rather than silently falling back to `exact`.
    #[serde(default, rename = "match")]
    pub match_mode: Option<String>,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols). Omit to search the whole project.
    #[serde(default)]
    pub path: Option<String>,
    /// Narrow to one language id (e.g. `rust`). Omit for every language.
    #[serde(default)]
    pub language: Option<String>,
    /// Maximum number of definitions to return. Defaults to 50 when omitted;
    /// raise it if you expect more hits and want them all in one call.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Response shape: `text` (default) or `toon` (a compact table — see
    /// `list_symbols`' `format` field for details).
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindReferencesArgs {
    /// Exact name of the symbol to find every reference to.
    pub symbol: String,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols), applied at every hop. Omit to search the whole project.
    #[serde(default)]
    pub path: Option<String>,
    /// Narrow to one language id (e.g. `rust`). Omit for every language.
    #[serde(default)]
    pub language: Option<String>,
    /// Maximum number of references to return. Defaults to 50 when omitted;
    /// raise it if you expect more hits and want them all in one call.
    #[serde(default)]
    pub limit: Option<usize>,
    /// How many relation-graph hops to walk beyond the direct/first hop. 1
    /// (default) is the original single-hop behavior — direct referrers
    /// only. Raising it also includes referrers-of-referrers, each tagged
    /// with its hop number. Clamped to `mct_core::MAX_QUERY_DEPTH`.
    #[serde(default)]
    pub depth: Option<u32>,
    /// Number of leading results to skip before applying `limit`, for
    /// paging through result sets larger than one `limit` page. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
    /// Response shape: `text` (default) or `toon` (a compact table — see
    /// `list_symbols`' `format` field for details).
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindCallsArgs {
    /// Exact name of the function/method whose callees you want.
    pub function: String,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols), applied at every hop. Omit to search the whole project.
    #[serde(default)]
    pub path: Option<String>,
    /// Narrow to one language id (e.g. `rust`). Omit for every language.
    #[serde(default)]
    pub language: Option<String>,
    /// Maximum number of calls to return. Defaults to 50 when omitted;
    /// raise it if you expect more hits and want them all in one call.
    #[serde(default)]
    pub limit: Option<usize>,
    /// How many call-graph hops to walk forward beyond the direct/first
    /// hop. 1 (default) is the original single-hop behavior — direct
    /// callees only. Raising it also includes callees-of-callees, each
    /// tagged with its hop number. Clamped to `mct_core::MAX_QUERY_DEPTH`.
    #[serde(default)]
    pub depth: Option<u32>,
    /// Number of leading results to skip before applying `limit`, for
    /// paging through result sets larger than one `limit` page. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
    /// Response shape: `text` (default) or `toon` (a compact table — see
    /// `list_symbols`' `format` field for details).
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindCallersArgs {
    /// Exact name of the function/method whose callers you want.
    pub function: String,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols), applied at every hop. Omit to search the whole project.
    #[serde(default)]
    pub path: Option<String>,
    /// Narrow to one language id (e.g. `rust`). Omit for every language.
    #[serde(default)]
    pub language: Option<String>,
    /// Maximum number of callers to return. Defaults to 50 when omitted;
    /// raise it if you expect more hits and want them all in one call.
    #[serde(default)]
    pub limit: Option<usize>,
    /// How many call-graph hops to walk backward beyond the direct/first
    /// hop. 1 (default) is the original single-hop behavior — direct
    /// callers only. Raising it also includes callers-of-callers, each
    /// tagged with its hop number. Clamped to `mct_core::MAX_QUERY_DEPTH`.
    #[serde(default)]
    pub depth: Option<u32>,
    /// Number of leading results to skip before applying `limit`, for
    /// paging through result sets larger than one `limit` page. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
    /// Response shape: `text` (default) or `toon` (a compact table — see
    /// `list_symbols`' `format` field for details).
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ImpactAnalysisArgs {
    /// Exact name of the symbol you're considering changing or removing.
    pub symbol: String,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols), applied at every hop. Omit to search the whole project.
    #[serde(default)]
    pub path: Option<String>,
    /// Narrow to one language id (e.g. `rust`). Omit for every language.
    #[serde(default)]
    pub language: Option<String>,
    /// Maximum number of entries to return per section (callers/references/
    /// affected tests are each capped independently). Defaults to 50 when
    /// omitted.
    #[serde(default)]
    pub limit: Option<usize>,
    /// How many relation-graph hops to walk beyond the direct/first hop,
    /// applied to both the caller and reference sections. 1 (default) is
    /// the original single-hop behavior. Clamped to `mct_core::MAX_QUERY_DEPTH`.
    #[serde(default)]
    pub depth: Option<u32>,
    /// Number of leading results to skip before applying `limit`, applied
    /// independently to each section. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
    /// Response shape: `text` (default) or `toon` (a compact table per
    /// section — see `list_symbols`' `format` field for details).
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct ReindexArgs {
    /// Re-parse every supported file, even if its content hash hasn't
    /// changed since the last run. Defaults to false (incremental).
    #[serde(default)]
    pub force: bool,
}

/// Cap on how many of a surfaced symbol's callers are shown in
/// `get_project_overview`'s relations section — deliberately smaller than
/// `DEFAULT_RESULT_LIMIT` since this tool already fans out one `find_callers`
/// call per surfaced symbol and must stay a cheap digest, not a full report.
const OVERVIEW_RELATIONS_PER_SYMBOL: usize = 3;

/// Symbol kinds surfaced by `get_project_overview`'s module digest — every
/// kind that reads as a "declaration" an agent would want to know exists,
/// excluding lower-signal top-level items (`variable`, `constant`, `field`)
/// that would mostly be noise in a coarse first-pass overview.
const OVERVIEW_KIND_ALLOWLIST: &[&str] = &[
    "function",
    "method",
    "class",
    "struct",
    "interface",
    "enum",
    "trait",
    "type_alias",
    "module",
];

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetProjectOverviewArgs {
    /// File, directory, or crate prefix — same matching semantics as
    /// list_symbols. Omit for the whole project root.
    #[serde(default)]
    pub path: Option<String>,
    /// Exact language id to keep. Same semantics as list_symbols.
    #[serde(default)]
    pub language: Option<String>,
    /// Cap on top-level symbols surfaced per file before truncating.
    /// Defaults to 8.
    #[serde(default)]
    pub max_symbols_per_module: Option<u32>,
    /// Whether to include each surfaced symbol's top callers. Defaults to
    /// false — the caller lines cost roughly three quarters of this tool's
    /// response, so ask for them only once the digest has told you which
    /// part of the project you care about.
    #[serde(default)]
    pub include_relations: Option<bool>,
}

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct GetIndexingStatusArgs {
    /// List every manifest's declared dependencies individually instead of
    /// a one-line summary. Defaults to false — the full listing is by far
    /// the largest part of this tool's output and is rarely what the
    /// "is the index healthy?" question needs.
    #[serde(default)]
    pub verbose_dependencies: bool,
}

/// Symbol kinds `find_dead_code` considers as candidates. Deliberately
/// narrower than `OVERVIEW_KIND_ALLOWLIST`: `method` is excluded because
/// trait/interface implementations are routinely called only through dynamic
/// dispatch, never by name — including it would make every implemented
/// interface method a false positive. `module` is excluded because a file's
/// own synthetic module entry is never itself "referenced".
const DEAD_CODE_KIND_ALLOWLIST: &[&str] = &[
    "function",
    "class",
    "struct",
    "interface",
    "enum",
    "trait",
    "type_alias",
];

/// Symbol names `find_dead_code` never flags, regardless of reference count —
/// language entry points invoked by the runtime/toolchain itself, never by an
/// in-repo caller.
const DEAD_CODE_ENTRY_POINT_NAMES: &[&str] = &["main"];

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindDeadCodeArgs {
    /// File, directory, or crate prefix — same matching semantics as
    /// list_symbols. Omit for the whole project root.
    #[serde(default)]
    pub path: Option<String>,
    /// Exact language id to keep. Same semantics as list_symbols.
    #[serde(default)]
    pub language: Option<String>,
    /// Maximum number of candidates to return. Defaults to 50 when omitted;
    /// raise it if you expect more hits and want them all in one call.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Number of leading results to skip before applying `limit`, for
    /// paging through result sets larger than one `limit` page. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
    /// Response shape: `text` (default) or `toon` (a compact table — see
    /// `list_symbols`' `format` field for details).
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetFileSkeletonArgs {
    /// A single file's path (e.g. `crates/mct-lang-html/src/lib.rs`),
    /// relative to the project root, forward slashes on any OS. Must be a
    /// file, not a directory/crate prefix — use list_symbols first if you
    /// don't already know which file you need.
    pub path: String,
}

#[derive(Clone)]
pub struct MctServer {
    index: Arc<Mutex<Index>>,
    registry: LanguageRegistry,
    // Read by the #[tool_handler] macro expansion below, not by hand-written
    // code — dead_code can't see that use.
    #[allow(dead_code)]
    tool_router: ToolRouter<MctServer>,
}

fn validate_name(raw: &str) -> Result<&str, McpError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(McpError::invalid_params(
            "symbol name must not be empty",
            None,
        ));
    }
    Ok(trimmed)
}

/// Parses `find_symbol`'s `match` argument. `None` (the field omitted) is
/// `Exact`; an explicit but unrecognized string is rejected rather than
/// silently treated as `Exact`, since a typo'd mode should surface as an
/// error, not a query that quietly ran narrower than intended.
fn parse_match_mode(raw: Option<&str>) -> Result<mct_index::SymbolMatchMode, McpError> {
    match raw {
        None => Ok(mct_index::SymbolMatchMode::Exact),
        Some("exact") => Ok(mct_index::SymbolMatchMode::Exact),
        Some("prefix") => Ok(mct_index::SymbolMatchMode::Prefix),
        Some("fuzzy") => Ok(mct_index::SymbolMatchMode::Fuzzy),
        Some(other) => Err(McpError::invalid_params(
            format!("match must be one of exact, prefix, fuzzy — got `{other}`"),
            None,
        )),
    }
}

/// Parses a tool's optional `format` argument (`text`, the default, or
/// `toon` — see `crate::toon`). An unrecognized value is rejected the same
/// way `parse_match_mode` rejects a typo'd `match`, rather than silently
/// falling back to `text`.
fn parse_output_format(raw: Option<&str>) -> Result<OutputFormat, McpError> {
    OutputFormat::parse(raw).map_err(|msg| McpError::invalid_params(msg, None))
}

fn index_error(err: mct_index::IndexError) -> McpError {
    McpError::internal_error(err.to_string(), None)
}

/// Applies the TTC-expanded description from `source` to every tool route
/// already present in `router`, in place, overwriting the compiled-in
/// `#[tool(description = "...")]` fallback with the shorter
/// `WHEN | NOT: ERR | TAGS: tags` catalog text (see `crate::ttc`). Never
/// panics: a TTC parse failure, or a `tools.ttc` entry naming a tool that
/// isn't registered, is logged and otherwise ignored so the server still
/// starts with its compiled-in fallback descriptions intact.
fn apply_ttc_catalog(router: &mut ToolRouter<MctServer>, source: &str) {
    let entries = match ttc::parse(source) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "failed to parse TTC tool catalog; keeping compiled-in fallback descriptions"
            );
            return;
        }
    };
    for (name, entry) in &entries {
        match router.map.get_mut(name.as_str()) {
            Some(route) => route.attr.description = Some(entry.expand().into()),
            None => {
                tracing::warn!(tool = %name, "tools.ttc has an entry for an unregistered tool");
            }
        }
    }
}

/// Trims an optional tool argument and drops it when it's blank, so a caller
/// passing `""` (or a stray `"  "`) gets the unscoped behavior rather than a
/// filter that matches nothing.
fn optional_arg(raw: Option<&String>) -> Option<&str> {
    raw.map(|s| s.trim()).filter(|s| !s.is_empty())
}

/// Builds the `mct-index` scope from a tool's optional `path`/`language`
/// arguments. Borrowed, so the caller must keep the trimmed strs alive for
/// the duration of the query.
fn query_scope<'a>(path: Option<&'a str>, language: Option<&'a str>) -> mct_index::QueryScope<'a> {
    mct_index::QueryScope { path, language }
}

/// Reads `relative_path`'s contents off disk, for tools (currently only
/// `get_file_skeleton`) that need the file's actual source text rather than
/// just its indexed symbol metadata. Rejects anything that canonicalizes
/// outside the project root — the same traversal guard `Index`'s own
/// reindex walk uses — so a path like `../../etc/passwd` is refused rather
/// than read.
fn read_source_file(index: &Index, relative_path: &str) -> Result<String, McpError> {
    let candidate = index.root().join(relative_path);
    let canonical = candidate.canonicalize().map_err(|_| {
        McpError::invalid_params(
            format!("`{relative_path}` was not found under the indexed project root"),
            None,
        )
    })?;
    if !canonical.starts_with(index.root()) {
        return Err(McpError::invalid_params(
            format!("`{relative_path}` resolves outside the indexed project root"),
            None,
        ));
    }
    std::fs::read_to_string(&canonical).map_err(|source| {
        McpError::internal_error(format!("failed to read `{relative_path}`: {source}"), None)
    })
}

/// `get_project_overview`'s path resolution: when the caller names a
/// `path`, this is identical to `Index::list_symbols`. When `path` is
/// omitted (whole-project overview), `Index::list_symbols`'s directory-prefix
/// matching has no way to express "everything" — its `LIKE` pattern is
/// always anchored to a specific prefix, and an empty prefix produces
/// `/%%` which matches nothing, since stored `relative_path`s never start
/// with a leading slash. So instead this walks the project root's own
/// top-level directory entries (skipping dotfiles/dot-directories, e.g.
/// `.git`/`.mct-index`) and unions one `list_symbols` call per entry —
/// each entry is itself a valid file-or-prefix path, so no new query
/// semantics are needed in `mct-index`.
fn list_symbols_for_overview(
    index: &Index,
    path: Option<&str>,
    language: Option<&str>,
) -> Result<Vec<mct_index::SymbolListEntry>, McpError> {
    if let Some(path) = path {
        return index.list_symbols(path, None, language).map_err(index_error);
    }
    let mut top_level: Vec<String> = std::fs::read_dir(index.root())
        .map_err(|source| {
            McpError::internal_error(format!("failed to read project root: {source}"), None)
        })?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (!name.starts_with('.')).then_some(name)
        })
        .collect();
    top_level.sort();

    let mut entries = Vec::new();
    for name in &top_level {
        entries.extend(index.list_symbols(name, None, language).map_err(index_error)?);
    }
    Ok(entries)
}

/// One file/module's slice of `get_project_overview`'s digest: its surfaced
/// top-level symbols (already capped to `max_symbols_per_module`, ranked by
/// fan-in when truncated), how many were left out, and — when
/// `include_relations` is set — each surfaced symbol's top callers.
pub struct ModuleDigest {
    pub relative_path: String,
    pub symbols: Vec<mct_index::SymbolListEntry>,
    pub omitted: usize,
    pub relations: Vec<(String, Vec<mct_index::RelationHit>)>,
}

#[tool_router]
impl MctServer {
    pub fn new(index: Index, registry: LanguageRegistry) -> Self {
        let mut tool_router = Self::tool_router();
        apply_ttc_catalog(&mut tool_router, ttc::CATALOG_SOURCE);
        Self {
            index: Arc::new(Mutex::new(index)),
            registry,
            tool_router,
        }
    }

    /// The tool catalog as it will be sent to an MCP client: names,
    /// TTC-expanded descriptions and input schemas. Exposed for tests and
    /// for measuring the catalog's token/byte footprint; not itself an MCP
    /// tool.
    pub fn tool_catalog(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }

    /// Shares this server's index handle with the background auto-reindex
    /// watcher (see `background::spawn_watcher`), without exposing the
    /// `index` field itself.
    pub fn index_handle(&self) -> Arc<Mutex<Index>> {
        Arc::clone(&self.index)
    }

    /// Shares this server's language registry with the background
    /// auto-reindex watcher, without exposing the `registry` field itself.
    pub fn registry_handle(&self) -> LanguageRegistry {
        self.registry.clone()
    }

    // Fallback only — the live description installed on this tool comes
    // from crate::ttc's expansion of tools.ttc, applied in `MctServer::new`.
    #[tool(description = "Discover symbols in a file or directory/crate; see tools.ttc")]
    pub async fn list_symbols(
        &self,
        Parameters(ListSymbolsArgs {
            path,
            kind,
            language,
            limit,
            format,
        }): Parameters<ListSymbolsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = validate_name(&path)?;
        let kind = optional_arg(kind.as_ref());
        let language = optional_arg(language.as_ref());
        let output_format = parse_output_format(format.as_deref())?;
        let index = self.index.lock().await;
        // Asks the index the same file-vs-directory question it resolves
        // internally, so the formatter knows whether to print each entry's
        // `relative_path` (needed once a directory/crate spans several
        // files) or omit it (redundant for a single-file listing). Decided
        // from the filesystem, not from whether the last segment happens to
        // contain a dot — `.claude`/`.github` are directories.
        let is_file = !index.path_is_directory(path);
        let hits = index
            .list_symbols(path, kind, language)
            .map_err(index_error)?;
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let text = match output_format {
            OutputFormat::Text => format::list_symbols(path, is_file, &hits, limit),
            OutputFormat::Toon => format::list_symbols_toon(path, is_file, &hits, limit),
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Find where a symbol is defined; see tools.ttc")]
    pub async fn find_symbol(
        &self,
        Parameters(FindSymbolArgs {
            name,
            match_mode,
            path,
            language,
            limit,
            format,
        }): Parameters<FindSymbolArgs>,
    ) -> Result<CallToolResult, McpError> {
        let name = validate_name(&name)?;
        let mode = parse_match_mode(match_mode.as_deref())?;
        let output_format = parse_output_format(format.as_deref())?;
        let scope = query_scope(optional_arg(path.as_ref()), optional_arg(language.as_ref()));
        let index = self.index.lock().await;
        let hits = index
            .find_symbol_matching_scoped(name, mode, scope)
            .map_err(index_error)?;
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let text = match output_format {
            OutputFormat::Text => format::symbol_hits(name, &hits, limit),
            OutputFormat::Toon => format::symbol_hits_toon(name, &hits, limit),
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Find every reference to a symbol; see tools.ttc")]
    pub async fn find_references(
        &self,
        Parameters(FindReferencesArgs {
            symbol,
            path,
            language,
            limit,
            depth,
            offset,
            format,
        }): Parameters<FindReferencesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let symbol = validate_name(&symbol)?;
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let offset = offset.unwrap_or(0);
        let output_format = parse_output_format(format.as_deref())?;
        let scope = query_scope(optional_arg(path.as_ref()), optional_arg(language.as_ref()));
        let index = self.index.lock().await;
        let hits = index
            .find_references_bfs_scoped(symbol, depth.unwrap_or(1), limit, offset, scope)
            .map_err(index_error)?;
        let text = match output_format {
            OutputFormat::Text => format::relation_hits(symbol, "reference(s)", &hits, offset, limit),
            OutputFormat::Toon => format::relation_hits_toon(symbol, "reference(s)", &hits, offset, limit),
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Find what a function calls; see tools.ttc")]
    pub async fn find_calls(
        &self,
        Parameters(FindCallsArgs {
            function,
            path,
            language,
            limit,
            depth,
            offset,
            format,
        }): Parameters<FindCallsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let function = validate_name(&function)?;
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let offset = offset.unwrap_or(0);
        let output_format = parse_output_format(format.as_deref())?;
        let scope = query_scope(optional_arg(path.as_ref()), optional_arg(language.as_ref()));
        let index = self.index.lock().await;
        let hits = index
            .find_calls_bfs_scoped(function, depth.unwrap_or(1), limit, offset, scope)
            .map_err(index_error)?;
        let text = match output_format {
            OutputFormat::Text => {
                format::relation_hits(function, "call(s) made by this function", &hits, offset, limit)
            }
            OutputFormat::Toon => {
                format::relation_hits_toon(function, "call(s) made by this function", &hits, offset, limit)
            }
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Find who calls a function; see tools.ttc")]
    pub async fn find_callers(
        &self,
        Parameters(FindCallersArgs {
            function,
            path,
            language,
            limit,
            depth,
            offset,
            format,
        }): Parameters<FindCallersArgs>,
    ) -> Result<CallToolResult, McpError> {
        let function = validate_name(&function)?;
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let offset = offset.unwrap_or(0);
        let output_format = parse_output_format(format.as_deref())?;
        let scope = query_scope(optional_arg(path.as_ref()), optional_arg(language.as_ref()));
        let index = self.index.lock().await;
        let hits = index
            .find_callers_bfs_scoped(function, depth.unwrap_or(1), limit, offset, scope)
            .map_err(index_error)?;
        let text = match output_format {
            OutputFormat::Text => {
                format::relation_hits(function, "caller(s) of this function", &hits, offset, limit)
            }
            OutputFormat::Toon => {
                format::relation_hits_toon(function, "caller(s) of this function", &hits, offset, limit)
            }
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Full blast radius of changing a symbol; see tools.ttc")]
    pub async fn impact_analysis(
        &self,
        Parameters(ImpactAnalysisArgs {
            symbol,
            path,
            language,
            limit,
            depth,
            offset,
            format,
        }): Parameters<ImpactAnalysisArgs>,
    ) -> Result<CallToolResult, McpError> {
        let symbol = validate_name(&symbol)?;
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let offset = offset.unwrap_or(0);
        let depth = depth.unwrap_or(1);
        let output_format = parse_output_format(format.as_deref())?;
        let scope = query_scope(optional_arg(path.as_ref()), optional_arg(language.as_ref()));
        let (callers, references) = {
            let index = self.index.lock().await;
            let callers = index
                .find_callers_bfs_scoped(symbol, depth, limit, offset, scope)
                .map_err(index_error)?;
            let references = index
                .find_references_bfs_scoped(symbol, depth, limit, offset, scope)
                .map_err(index_error)?;
            (callers, references)
        };
        // A test caller shows up in both `callers` and `references` (the
        // latter is a superset); dedupe by caller name so it's only counted
        // once regardless of how many relation kinds connect it to `symbol`.
        let mut seen_test_names = std::collections::HashSet::new();
        let affected_tests: Vec<&mct_index::RelationHit> = references
            .iter()
            .chain(callers.iter())
            .filter(|hit| format::looks_like_test_name(&hit.from_symbol, &hit.relative_path))
            .filter(|hit| seen_test_names.insert(hit.from_symbol.as_str()))
            .collect();
        let text = match output_format {
            OutputFormat::Text => {
                format::impact_analysis(symbol, &callers, &references, &affected_tests, offset, limit)
            }
            OutputFormat::Toon => {
                format::impact_analysis_toon(symbol, &callers, &references, &affected_tests, offset, limit)
            }
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Force an index refresh; see tools.ttc")]
    pub async fn reindex(
        &self,
        Parameters(ReindexArgs { force }): Parameters<ReindexArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut index = self.index.lock().await;
        let report = index.reindex(&self.registry, force).map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::reindex_report(&report),
        )]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Report index health; see tools.ttc")]
    pub async fn get_indexing_status(
        &self,
        Parameters(GetIndexingStatusArgs {
            verbose_dependencies,
        }): Parameters<GetIndexingStatusArgs>,
    ) -> Result<CallToolResult, McpError> {
        let index = self.index.lock().await;
        let status = index.status().map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::index_status(&status, verbose_dependencies),
        )]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "A file's shape, bodies collapsed; see tools.ttc")]
    pub async fn get_file_skeleton(
        &self,
        Parameters(GetFileSkeletonArgs { path }): Parameters<GetFileSkeletonArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = validate_name(&path)?;
        let index = self.index.lock().await;
        if index.path_is_directory(path) {
            return Err(McpError::invalid_params(
                "get_file_skeleton takes a single file path, not a directory/crate prefix — use list_symbols first if you don't know which file you need",
                None,
            ));
        }
        let source = read_source_file(&index, path)?;
        let entries: Vec<_> = index
            .list_symbols(path, None, None)
            .map_err(index_error)?
            .into_iter()
            // `parent.is_none()` alone isn't "top-level declaration" — every
            // file also gets a synthetic whole-file `module` entry spanning
            // its entire line range with no parent of its own. Rendering
            // that would collapse the file into one bogus self-referential
            // block; the user asked for classes/structs/interfaces/
            // functions/types, not the module wrapper.
            .filter(|entry| entry.parent.is_none() && entry.kind != "module")
            .collect();
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::file_skeleton(path, &entries, &source),
        )]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Capped hierarchical project digest; see tools.ttc")]
    pub async fn get_project_overview(
        &self,
        Parameters(GetProjectOverviewArgs {
            path,
            language,
            max_symbols_per_module,
            include_relations,
        }): Parameters<GetProjectOverviewArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = path.as_deref().map(str::trim).filter(|p| !p.is_empty());
        let language = language.as_deref().map(str::trim).filter(|l| !l.is_empty());
        let max_symbols_per_module = max_symbols_per_module.unwrap_or(8).max(1) as usize;
        // Defaults to false: the per-symbol caller lines were measured at
        // roughly three quarters of this tool's whole response on this repo
        // (167_864 -> 45_760 bytes with them off). A digest that costs more
        // than reading the files isn't a digest.
        let include_relations = include_relations.unwrap_or(false);

        let index = self.index.lock().await;
        let all_entries = list_symbols_for_overview(&index, path, language)?;

        // `list_symbols_for_overview`'s entries are already ordered by
        // `relative_path` within each call and the calls themselves are made
        // in sorted top-level-entry order, so grouping consecutive runs by
        // `relative_path` reproduces the same grouping a full re-sort would.
        let mut modules: Vec<(String, Vec<mct_index::SymbolListEntry>)> = Vec::new();
        for entry in all_entries {
            match modules.last_mut() {
                Some((last_path, group)) if *last_path == entry.relative_path => {
                    group.push(entry);
                }
                _ => modules.push((entry.relative_path.clone(), vec![entry])),
            }
        }

        // Direct-caller counts for every called name, in one grouped query,
        // hoisted out of the module loop below. This replaces one
        // `find_callers` per candidate symbol — over 1800 queries on this
        // repo — with a single aggregate the ranking reads from.
        let fan_in_counts = index.fan_in_counts().map_err(index_error)?;

        let mut digests = Vec::with_capacity(modules.len());
        for (relative_path, entries) in modules {
            let mut candidates: Vec<mct_index::SymbolListEntry> = entries
                .into_iter()
                .filter(|e| e.parent.is_none() && OVERVIEW_KIND_ALLOWLIST.contains(&e.kind.as_str()))
                .collect();

            let omitted = candidates.len().saturating_sub(max_symbols_per_module);
            if omitted > 0 {
                // Rank by fan-in (direct caller count) descending; ties keep
                // their existing line-ascending order (`sort_by_key` is
                // stable) since a zero fan-in for an uncalled symbol is a
                // legitimate, common case, not a tie-break bug. A name
                // absent from the map has no direct callers at all.
                let mut ranked: Vec<usize> = (0..candidates.len()).collect();
                ranked.sort_by_key(|&i| {
                    std::cmp::Reverse(
                        fan_in_counts
                            .get(candidates[i].name.as_str())
                            .copied()
                            .unwrap_or(0),
                    )
                });
                let keep: std::collections::HashSet<usize> =
                    ranked.into_iter().take(max_symbols_per_module).collect();
                let mut kept = Vec::with_capacity(max_symbols_per_module);
                for (i, candidate) in candidates.into_iter().enumerate() {
                    if keep.contains(&i) {
                        kept.push(candidate);
                    }
                }
                candidates = kept;
            }

            let mut relations = Vec::new();
            if include_relations {
                for symbol in &candidates {
                    let callers = index.find_callers(&symbol.name).map_err(index_error)?;
                    let top: Vec<mct_index::RelationHit> = callers
                        .into_iter()
                        .take(OVERVIEW_RELATIONS_PER_SYMBOL)
                        .collect();
                    relations.push((symbol.name.clone(), top));
                }
            }

            digests.push(ModuleDigest {
                relative_path,
                symbols: candidates,
                omitted,
                relations,
            });
        }

        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::overview(
                path.unwrap_or("."),
                &digests,
                max_symbols_per_module as u32,
            ),
        )]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Heuristic dead-code candidates; see tools.ttc")]
    pub async fn find_dead_code(
        &self,
        Parameters(FindDeadCodeArgs {
            path,
            language,
            limit,
            offset,
            format,
        }): Parameters<FindDeadCodeArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = path.as_deref().map(str::trim).filter(|p| !p.is_empty());
        let language = language.as_deref().map(str::trim).filter(|l| !l.is_empty());
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let offset = offset.unwrap_or(0);
        let output_format = parse_output_format(format.as_deref())?;

        let index = self.index.lock().await;
        let all_entries = list_symbols_for_overview(&index, path, language)?;
        let reference_counts = index.reference_counts().map_err(index_error)?;

        // Deliberately not filtered on `parent.is_none()` the way
        // `get_project_overview`'s digest is: a `LanguageParser` uses `parent`
        // for more than "nested inside another declaration" — e.g.
        // `mct-lang-go` sets every top-level function's `parent` to its
        // enclosing package name, not `None`. Filtering on it here would
        // silently exclude every Go (and similarly-modeled) top-level
        // function from consideration. `DEAD_CODE_KIND_ALLOWLIST` already
        // excludes `method`/`module`/`field`, which is the distinction that
        // actually matters for this tool.
        let candidates: Vec<mct_index::SymbolListEntry> = all_entries
            .into_iter()
            .filter(|e| DEAD_CODE_KIND_ALLOWLIST.contains(&e.kind.as_str()))
            .filter(|e| !DEAD_CODE_ENTRY_POINT_NAMES.contains(&e.name.as_str()))
            .filter(|e| !format::looks_like_test_name(&e.name, &e.relative_path))
            .filter(|e| !reference_counts.contains_key(e.name.as_str()))
            .collect();

        let text = match output_format {
            OutputFormat::Text => format::find_dead_code(path.unwrap_or("."), &candidates, offset, limit),
            OutputFormat::Toon => {
                format::find_dead_code_toon(path.unwrap_or("."), &candidates, offset, limit)
            }
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }
}

#[tool_handler]
impl ServerHandler for MctServer {
    fn get_info(&self) -> InitializeResult {
        InitializeResult::new(
            ServerCapabilities::builder().enable_tools().build(),
        )
        .with_server_info(Implementation::from_build_env())
        .with_protocol_version(ProtocolVersion::V_2024_11_05)
        .with_instructions(
            "Indexes this repository's source code (any of the supported languages, including \
             polyglot repos) into a symbol graph. Prefer these tools over reading whole files \
             with grep or a file reader when you need to locate a definition or understand call \
             relationships — it costs far fewer tokens. list_symbols is the discovery tool: use \
             it first when you don't know a symbol's exact name, to list what a file or \
             directory/crate contains. Search tools (find_symbol, find_calls, find_callers, \
             find_references) are atomic lookups, each answering one specific question — see \
             each tool's description for which one to use and which NOT to. impact_analysis is \
             composite: it combines find_callers + find_references + a test heuristic \
             internally, for when you need the full blast radius of a change in one call. \
             find_references/find_calls/find_callers/impact_analysis also accept an optional \
             `depth` to walk multiple relation-graph hops (1, the default, is today's direct-hit \
             behavior) and `offset` to page past `limit`. get_file_skeleton returns a file's \
             top-level declarations with bodies collapsed to `// ...` — reach for it instead of \
             reading a whole file when you only need its shape. get_project_overview is the \
             cheapest way to get oriented in an unfamiliar file/directory/crate/project: a \
             capped, ranked hierarchical digest in one call, in place of chaining list_symbols + \
             get_file_skeleton + find_calls by hand. find_dead_code reports top-level symbols \
             with zero indexed references anywhere in the project — a heuristic starting point \
             for cleanup, not a certainty; sanity-check a hit with find_references or \
             impact_analysis before deleting anything. reindex and get_indexing_status \
             are index maintenance, not search — they never return symbol data. The index \
             refreshes automatically at startup and silently in the background as changes settle \
             on disk; call reindex manually only if you need an immediate refresh right now, or \
             force=true to bypass the incremental hash check."
                .to_string(),
        )
    }
}
