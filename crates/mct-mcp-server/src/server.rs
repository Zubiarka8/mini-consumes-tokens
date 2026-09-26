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

use crate::embedder::SemanticModel;
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

/// `search_symbols`' default `limit`: ranked search puts the best hits
/// first, so a short page is usually all an agent needs — unlike the graph
/// tools, whose results are exhaustive and default to
/// [`DEFAULT_RESULT_LIMIT`].
const SEARCH_DEFAULT_LIMIT: usize = 10;
/// Upper clamp on `search_symbols`' `limit`; page with `offset` beyond it.
const SEARCH_MAX_LIMIT: usize = 100;
/// Upper clamp on `search_symbols`' `snippet_lines`, per hit.
const SEARCH_MAX_SNIPPET_LINES: usize = 50;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchSymbolsArgs {
    /// Words or partial identifier to search for, in any naming style:
    /// `parse request`, `parseReq`, `parse_request_body`, `http server`.
    /// Each word prefix-matches a word of the symbol's name (split at
    /// camelCase/snake_case/kebab-case/acronym boundaries); FTS operators in
    /// the input are treated as plain text.
    pub query: String,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols). Omit to search the whole project.
    #[serde(default)]
    pub path: Option<String>,
    /// Narrow to one language id (e.g. `rust`). Omit for every language.
    #[serde(default)]
    pub language: Option<String>,
    /// Maximum number of ranked hits to return. Defaults to 10, clamped to
    /// 100 — results are best-first, so page with `offset` rather than
    /// raising it.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Number of leading results to skip before applying `limit`, for
    /// paging. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
    /// Source lines to include under each hit, starting at its definition
    /// line and never past its end line. Defaults to 0 (no snippet), clamped
    /// to 50.
    #[serde(default)]
    pub snippet_lines: Option<usize>,
    /// Response shape: `text` (default) or `toon` (a compact table — see
    /// `list_symbols`' `format` field for details).
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct HybridSearchArgs {
    /// What you're looking for, as identifier words or a plain description:
    /// `parse request`, `load config from disk`, `retry with backoff`.
    /// Wrap it in double quotes (`"parse request"`) for an exact phrase:
    /// names holding those words consecutively and in order, lexical only,
    /// a literal name match first.
    pub query: String,
    /// Weight of the semantic (embedding) ranking against the lexical
    /// (`search_symbols`) one: 0 = lexical only, 1 = semantic only, clamped
    /// to 0..=1. Omit it to let the query's shape pick: 0.1 for an
    /// identifier (`snake_case`, `camelCase`, `Type::method`,
    /// `module.fn`), 0.75 for a plain-language description, 0.5 otherwise.
    /// Ignored (forced to 0) for a `"double-quoted"` exact-phrase query.
    #[serde(default)]
    pub alpha: Option<f64>,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols). Omit to search the whole project.
    #[serde(default)]
    pub path: Option<String>,
    /// Narrow to one language id (e.g. `rust`). Omit for every language.
    #[serde(default)]
    pub language: Option<String>,
    /// Maximum number of ranked hits to return. Defaults to 10, clamped to
    /// 100 — results are best-first, so page with `offset` rather than
    /// raising it.
    #[serde(default)]
    pub top_k: Option<usize>,
    /// Number of leading results to skip before applying `top_k`, for
    /// paging. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
    /// Source lines to include under each hit, starting at its definition
    /// line and never past its end line. Defaults to 0 (no snippet), clamped
    /// to 50.
    #[serde(default)]
    pub snippet_lines: Option<usize>,
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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetFileTreeArgs {
    /// Directory to root the tree at, relative to the project root (e.g.
    /// `crates/mct-lang-html`). Omit for the whole project. Must be a
    /// directory, not a file.
    #[serde(default)]
    pub path: Option<String>,
    /// How many levels of nesting to descend below `path`. Defaults to 3.
    /// Clamped to `mct_core::MAX_QUERY_DEPTH`.
    #[serde(default)]
    pub depth: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetToolSchemaArgs {
    /// Exact tool name to fetch the input schema and description for (e.g.
    /// `find_symbol`) — see `discover_tool_categories` for the full set of
    /// registered names grouped by category.
    pub name: String,
}

/// Cap on sub-queries per `batch` call. Batching pays off by collapsing
/// per-call envelopes; past a couple dozen queries the response itself is
/// the cost, and an agent is better served by narrowing than by one giant
/// reply.
const BATCH_MAX_QUERIES: usize = 25;

/// Total bytes a `batch` response renders before the remaining sub-queries
/// are skipped (not run) and listed for a follow-up call. Each sub-query is
/// already capped at `format::DEFAULT_BYTE_BUDGET`; this keeps N of them from
/// adding up to N times that.
const BATCH_BYTE_BUDGET: usize = 2 * format::DEFAULT_BYTE_BUDGET;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BatchQuery {
    /// Name of the tool to run (e.g. `find_symbol`). Any read-only tool of
    /// this server; `batch` itself and `reindex` are rejected.
    pub tool: String,
    /// That tool's arguments object, exactly as it would be passed to the
    /// tool directly (e.g. `{"name": "parse"}` for `find_symbol`). Omit for
    /// a tool that takes no arguments.
    #[serde(default)]
    pub args: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BatchArgs {
    /// Sub-queries to run in order, 1 to 25. Each one's result (or error)
    /// is reported under its own `[n] tool` header; one failing doesn't
    /// fail the rest.
    pub queries: Vec<BatchQuery>,
}

/// Groups this server's registered tool names for `discover_tool_categories`.
/// Kept next to `ttc::KNOWN_TOOL_NAMES` in intent (both must track the live
/// `#[tool(...)]` methods below), but independent of it in shape: a category
/// is presentation grouping, not a coverage list, so nothing asserts every
/// `KNOWN_TOOL_NAMES` entry appears here exactly once the way the TTC catalog
/// test does for `tools.ttc`.
const TOOL_CATEGORIES: &[(&str, &[&str])] = &[
    (
        "discovery",
        &[
            "list_symbols",
            "get_file_skeleton",
            "get_project_overview",
            "get_file_tree",
        ],
    ),
    ("lookup", &["find_symbol", "search_symbols", "hybrid_search"]),
    (
        "relations",
        &["find_callers", "find_calls", "find_references", "impact_analysis"],
    ),
    (
        "maintenance",
        &["reindex", "get_indexing_status", "find_dead_code"],
    ),
    (
        "meta",
        &["discover_tool_categories", "get_tool_schema", "batch"],
    ),
];

#[derive(Clone)]
pub struct MctServer {
    index: Arc<Mutex<Index>>,
    registry: LanguageRegistry,
    semantic: Arc<SemanticModel>,
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

/// `get_file_tree`'s error mapping: a bad `path` (not a directory, escapes
/// the root, or doesn't exist) is the caller's mistake, not a server fault —
/// unlike [`index_error`]'s blanket `internal_error`, these map to
/// `invalid_params` so the agent sees an actionable message.
fn file_tree_error(err: mct_index::IndexError) -> McpError {
    use mct_index::IndexError::*;
    match err {
        NotADirectory(_) | PathEscapesRoot(_) => McpError::invalid_params(err.to_string(), None),
        Io { .. } => McpError::invalid_params(
            format!("{err} — is `path` a real directory under the indexed project root?"),
            None,
        ),
        other => index_error(other),
    }
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

/// Source snippets for the `offset`/`limit` page of a ranked search result
/// (`search_symbols`/`hybrid_search`). Only the page actually shown pays for
/// file reads, each file read at most once. A file that vanished since the
/// last reindex just gets no snippet rather than failing the whole search.
fn page_snippets(
    index: &Index,
    hits: &[mct_index::SymbolHit],
    offset: usize,
    limit: usize,
    snippet_lines: usize,
) -> Vec<Option<String>> {
    let mut sources: std::collections::HashMap<&str, Option<String>> =
        std::collections::HashMap::new();
    hits.iter()
        .skip(offset)
        .take(limit)
        .map(|hit| {
            if snippet_lines == 0 {
                return None;
            }
            let source = sources
                .entry(hit.relative_path.as_str())
                .or_insert_with(|| read_source_file(index, &hit.relative_path).ok());
            source
                .as_deref()
                .map(|src| format::symbol_snippet(src, hit.line, hit.end_line, snippet_lines))
        })
        .collect()
}

/// `get_project_overview`/`find_dead_code`'s path resolution — thin wrapper
/// over `Index::list_symbols_all` mapping its error type for the MCP
/// surface.
fn list_symbols_for_overview(
    index: &Index,
    path: Option<&str>,
    language: Option<&str>,
) -> Result<Vec<mct_index::SymbolListEntry>, McpError> {
    index.list_symbols_all(path, language).map_err(index_error)
}

/// Deserializes one `batch` sub-query's `args` into `tool`'s own argument
/// struct, so a sub-query is validated exactly as the direct call would be.
/// Omitted `args` is an empty object — every tool's required fields still
/// get reported as missing, by name.
fn batch_params<T: serde::de::DeserializeOwned>(
    tool: &str,
    args: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<Parameters<T>, McpError> {
    serde_json::from_value(serde_json::Value::Object(args.unwrap_or_default()))
        .map(Parameters)
        .map_err(|err| McpError::invalid_params(format!("invalid args for `{tool}`: {err}"), None))
}

/// Every text block of a sub-query's result, joined — today each tool
/// returns exactly one, but a batch shouldn't silently drop a second.
fn result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|text| text.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
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
            semantic: Arc::new(SemanticModel::default()),
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

    /// Runs one read-only tool by name with JSON `args`, exactly as a `batch`
    /// sub-query does (same validation, defaults and output as a direct
    /// call; `reindex` and `batch` refused). Not itself an MCP tool — it is
    /// how `mct-eval` drives the server by name.
    pub async fn call_read_only_tool(
        &self,
        tool: &str,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, McpError> {
        self.run_batch_query(tool, args).await
    }

    /// Runs one `batch` sub-query through the same tool method a direct call
    /// would hit, so its validation, defaults and output are identical.
    /// `reindex` is refused (a batch stays read-only, and a mid-batch
    /// reindex would change what later sub-queries see), as is a nested
    /// `batch`.
    async fn run_batch_query(
        &self,
        tool: &str,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, McpError> {
        match tool {
            "list_symbols" => self.list_symbols(batch_params(tool, args)?).await,
            "find_symbol" => self.find_symbol(batch_params(tool, args)?).await,
            "search_symbols" => self.search_symbols(batch_params(tool, args)?).await,
            "hybrid_search" => self.hybrid_search(batch_params(tool, args)?).await,
            "find_references" => self.find_references(batch_params(tool, args)?).await,
            "find_calls" => self.find_calls(batch_params(tool, args)?).await,
            "find_callers" => self.find_callers(batch_params(tool, args)?).await,
            "impact_analysis" => self.impact_analysis(batch_params(tool, args)?).await,
            "get_indexing_status" => self.get_indexing_status(batch_params(tool, args)?).await,
            "get_file_skeleton" => self.get_file_skeleton(batch_params(tool, args)?).await,
            "get_project_overview" => self.get_project_overview(batch_params(tool, args)?).await,
            "get_file_tree" => self.get_file_tree(batch_params(tool, args)?).await,
            "find_dead_code" => self.find_dead_code(batch_params(tool, args)?).await,
            "discover_tool_categories" => self.discover_tool_categories().await,
            "get_tool_schema" => self.get_tool_schema(batch_params(tool, args)?).await,
            "batch" => Err(McpError::invalid_params("`batch` can't be nested inside a batch", None)),
            "reindex" => Err(McpError::invalid_params(
                "`reindex` isn't allowed in a batch (batches are read-only) — call it directly",
                None,
            )),
            other => Err(McpError::invalid_params(
                format!("no tool named `{other}` — call discover_tool_categories for the full list"),
                None,
            )),
        }
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
    #[tool(description = "Ranked search over split symbol names; see tools.ttc")]
    pub async fn search_symbols(
        &self,
        Parameters(SearchSymbolsArgs {
            query,
            path,
            language,
            limit,
            offset,
            snippet_lines,
            format,
        }): Parameters<SearchSymbolsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let query = validate_name(&query)?;
        let output_format = parse_output_format(format.as_deref())?;
        let scope = query_scope(optional_arg(path.as_ref()), optional_arg(language.as_ref()));
        let limit = limit.unwrap_or(SEARCH_DEFAULT_LIMIT).clamp(1, SEARCH_MAX_LIMIT);
        let offset = offset.unwrap_or(0);
        let snippet_lines = snippet_lines.unwrap_or(0).min(SEARCH_MAX_SNIPPET_LINES);
        let index = self.index.lock().await;
        let hits = index.search_symbols(query, scope).map_err(index_error)?;
        let snippets = page_snippets(&index, &hits, offset, limit, snippet_lines);
        let text = match output_format {
            OutputFormat::Text => format::search_hits(query, &hits, offset, limit, &snippets),
            OutputFormat::Toon => format::search_hits_toon(query, &hits, offset, limit, &snippets),
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Lexical + semantic ranked symbol search; see tools.ttc")]
    pub async fn hybrid_search(
        &self,
        Parameters(HybridSearchArgs {
            query,
            alpha,
            path,
            language,
            top_k,
            offset,
            snippet_lines,
            format,
        }): Parameters<HybridSearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let query = validate_name(&query)?;
        let output_format = parse_output_format(format.as_deref())?;
        let scope = query_scope(optional_arg(path.as_ref()), optional_arg(language.as_ref()));
        // A quoted query is an exact phrase: lexical only, whatever `alpha`.
        let (alpha, alpha_source) = match alpha.filter(|a| a.is_finite()) {
            _ if mct_index::exact_phrase(query).is_some() => (0.0, " (exact phrase)".to_string()),
            Some(alpha) => (alpha.clamp(0.0, 1.0), String::new()),
            None => {
                let intent = mct_index::classify_query(query);
                (intent.alpha(), format!(" (auto: {intent:?} query)"))
            }
        };
        let limit = top_k.unwrap_or(SEARCH_DEFAULT_LIMIT).clamp(1, SEARCH_MAX_LIMIT);
        let offset = offset.unwrap_or(0);
        let snippet_lines = snippet_lines.unwrap_or(0).min(SEARCH_MAX_SNIPPET_LINES);
        let index = self.index.lock().await;

        // Lexical-only whenever the semantic side can't contribute: alpha 0
        // (the model is never loaded), no model in this build or it failed to
        // load, or embedding the pending symbols failed. Each case is named
        // in the output's first line instead of erroring.
        let (embedder, note) = if alpha == 0.0 {
            (None, format!("hybrid: alpha 0{alpha_source}, lexical ranking only"))
        } else {
            match self.semantic.get(index.root()) {
                Err(reason) => (None, format!("hybrid: lexical ranking only — {reason}")),
                Ok(embedder) => match index.refresh_embeddings(embedder) {
                    Err(e) => (None, format!("hybrid: lexical ranking only — {e}")),
                    Ok(_) => {
                        let coverage = index
                            .embedding_coverage(embedder.model_id())
                            .map_err(index_error)?;
                        (
                            Some(embedder),
                            format!(
                                "hybrid: alpha {alpha:.2}{alpha_source}, {}/{} symbols embedded ({})",
                                coverage.embedded,
                                coverage.total,
                                embedder.model_id()
                            ),
                        )
                    }
                },
            }
        };
        let fused = match index.hybrid_search(query, embedder, alpha, scope) {
            Ok(fused) => fused,
            Err(e) if embedder.is_some() => {
                return Err(McpError::internal_error(format!("hybrid_search failed: {e}"), None))
            }
            Err(e) => return Err(index_error(e)),
        };
        let hits: Vec<mct_index::SymbolHit> = fused.into_iter().map(|h| h.hit).collect();
        let snippets = page_snippets(&index, &hits, offset, limit, snippet_lines);
        let mut text = match output_format {
            OutputFormat::Text => format::search_hits(query, &hits, offset, limit, &snippets),
            OutputFormat::Toon => format::search_hits_toon(query, &hits, offset, limit, &snippets),
        };
        // An exact phrase also searches prose string literals (error/log
        // messages), appended as their own section when any hold it.
        if let Some(phrase) = mct_index::exact_phrase(query) {
            let literals = index.search_literals(phrase, scope).map_err(index_error)?;
            let section = match output_format {
                OutputFormat::Text => format::literal_hits(phrase, &literals, offset, limit),
                OutputFormat::Toon => format::literal_hits_toon(phrase, &literals, offset, limit),
            };
            if !section.is_empty() {
                text = format!("{}\n{section}", text.trim_end());
            }
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "{note}\n{text}"
        ))]))
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
            .filter(|hit| mct_index::looks_like_test_name(&hit.from_symbol, &hit.relative_path))
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
    #[tool(description = "Directory/file tree listing; see tools.ttc")]
    pub async fn get_file_tree(
        &self,
        Parameters(GetFileTreeArgs { path, depth }): Parameters<GetFileTreeArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = path.as_deref().map(str::trim).filter(|p| !p.is_empty());
        let depth = depth.unwrap_or(3).max(1);
        let index = self.index.lock().await;
        let tree = index.file_tree(path, depth).map_err(file_tree_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::file_tree(path.unwrap_or("."), &tree, depth),
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
        // Deliberately not filtered on `parent.is_none()` the way
        // `get_project_overview`'s digest is: a `LanguageParser` uses `parent`
        // for more than "nested inside another declaration" — e.g.
        // `mct-lang-go` sets every top-level function's `parent` to its
        // enclosing package name, not `None`. Filtering on it here would
        // silently exclude every Go (and similarly-modeled) top-level
        // function from consideration. `mct_index::DEAD_CODE_KIND_ALLOWLIST`
        // already excludes `method`/`module`/`field`, which is the
        // distinction that actually matters for this tool.
        let candidates =
            mct_index::find_dead_code_candidates(&index, path, language).map_err(index_error)?;

        let text = match output_format {
            OutputFormat::Text => format::find_dead_code(path.unwrap_or("."), &candidates, offset, limit),
            OutputFormat::Toon => {
                format::find_dead_code_toon(path.unwrap_or("."), &candidates, offset, limit)
            }
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "List tool names/categories without schemas; see tools.ttc")]
    pub async fn discover_tool_categories(&self) -> Result<CallToolResult, McpError> {
        let catalog = self.tool_router.list_all();
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::tool_categories(&catalog, TOOL_CATEGORIES),
        )]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Full input schema for one named tool; see tools.ttc")]
    pub async fn get_tool_schema(
        &self,
        Parameters(GetToolSchemaArgs { name }): Parameters<GetToolSchemaArgs>,
    ) -> Result<CallToolResult, McpError> {
        let name = validate_name(&name)?;
        let catalog = self.tool_router.list_all();
        let tool = catalog.iter().find(|t| t.name == name).ok_or_else(|| {
            McpError::invalid_params(
                format!(
                    "no tool named `{name}` — call discover_tool_categories for the full list"
                ),
                None,
            )
        })?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::tool_schema(tool),
        )]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Run several read-only queries in one call; see tools.ttc")]
    pub async fn batch(
        &self,
        Parameters(BatchArgs { queries }): Parameters<BatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        if queries.is_empty() {
            return Err(McpError::invalid_params("`queries` must not be empty", None));
        }
        if queries.len() > BATCH_MAX_QUERIES {
            return Err(McpError::invalid_params(
                format!(
                    "a batch takes at most {BATCH_MAX_QUERIES} queries, got {} — split it",
                    queries.len()
                ),
                None,
            ));
        }
        // Sequential on purpose: every sub-query takes the same index lock,
        // so running them concurrently would only queue on it. The budget is
        // checked before each sub-query runs, so the response overshoots it
        // by at most one sub-result (itself capped by DEFAULT_BYTE_BUDGET).
        let mut outcomes = Vec::with_capacity(queries.len());
        let mut rendered = 0usize;
        for BatchQuery { tool, args } in queries {
            let tool = tool.trim().to_string();
            let outcome = if rendered >= BATCH_BYTE_BUDGET {
                format::BatchOutcome::Skipped
            } else {
                match self.run_batch_query(&tool, args).await {
                    Ok(result) => format::BatchOutcome::Ok(result_text(&result)),
                    Err(err) => format::BatchOutcome::Err(err.message.to_string()),
                }
            };
            rendered += outcome.len();
            outcomes.push((tool, outcome));
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::batch(&outcomes),
        )]))
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
             get_file_skeleton + find_calls by hand. get_file_tree returns a plain directory/file \
             tree (no symbol data) for layout navigation before you know which file or crate to \
             look at — cheaper than get_project_overview when you only need the shape of the \
             filesystem, not what's defined in it. find_dead_code reports top-level symbols \
             with zero indexed references anywhere in the project — a heuristic starting point \
             for cleanup, not a certainty; sanity-check a hit with find_references or \
             impact_analysis before deleting anything. reindex and get_indexing_status \
             are index maintenance, not search — they never return symbol data. The index \
             refreshes automatically at startup and silently in the background as changes settle \
             on disk; call reindex manually only if you need an immediate refresh right now, or \
             force=true to bypass the incremental hash check. discover_tool_categories and \
             get_tool_schema are progressive discovery: discover_tool_categories lists every \
             tool's name and one-line purpose grouped by category with no input schemas, and \
             get_tool_schema returns one named tool's full input schema and description on \
             demand — useful for a client choosing to defer loading full schemas instead of \
             requesting every tool's schema up front. batch runs up to 25 read-only queries \
             (any tool above except reindex) in one call, each reported under its own header — \
             prefer it over several separate calls whenever you already know the queries you \
             need, since it drops the per-call overhead."
                .to_string(),
        )
    }
}
