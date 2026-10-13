use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::Index;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ToolAnnotations},
    model::{Implementation, InitializeResult, ProtocolVersion, ServerCapabilities},
    service::RequestContext,
    tool, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler,
};
use tokio::sync::Mutex;

use crate::cache::{self, CacheConfig, CacheStats, CachedResult, HitKind, QueryCache};
use crate::embedder::SemanticModel;
use crate::format;
use crate::intent;
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
    /// `variable`, `constant`, `field`, `element`, `rule`, `asset`,
    /// `model_node`, `material`, `animation`, `finding`). Omit to include
    /// every kind.
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
    /// Response shape: `toon` (default; also used when omitted or null)
    /// renders the data as a compact TOON table (one header row of column
    /// names, then one row per symbol, no repeated labels) — fewer tokens
    /// for a large result. `text` is this server's plain human/agent-skimmable
    /// text, with per-kind grouping, kept as an explicit option. Any other
    /// value is rejected.
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
    /// Response shape: `toon` (default) or `text` (plain text, opt-in) — see
    /// `list_symbols`' `format` field for details.
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
    /// Response shape: `toon` (default) or `text` (plain text, opt-in) — see
    /// `list_symbols`' `format` field for details.
    #[serde(default)]
    pub format: Option<String>,
    /// `false` skips the query cache for this call and always returns the
    /// full result — a repeated, unchanged call otherwise gets a one-line
    /// "unchanged" reply. Omit it to use the cache.
    #[serde(default)]
    pub cache: Option<bool>,
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
    /// Response shape: `toon` (default) or `text` (plain text, opt-in) — see
    /// `list_symbols`' `format` field for details.
    #[serde(default)]
    pub format: Option<String>,
    /// `false` skips the query cache for this call and always returns the
    /// full result — a repeated, unchanged call otherwise gets a one-line
    /// "unchanged" reply. Omit it to use the cache.
    #[serde(default)]
    pub cache: Option<bool>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SemanticSearchArgs {
    /// What the code does, in plain words: `validate user authentication`,
    /// `load config from disk`.
    pub query: String,
    /// Maximum number of hits, most similar first. Defaults to 10, clamped
    /// to 100.
    #[serde(default)]
    pub top_k: Option<usize>,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols). Omit to search the whole project.
    #[serde(default)]
    pub path: Option<String>,
    /// Narrow to one language id (e.g. `rust`). Omit for every language.
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindReferencesArgs {
    /// Exact name of the symbol to find every reference to.
    pub symbol: String,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols), applied to the start and first hop only. Omit to search the whole project.
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
    /// Response shape: `toon` (default) or `text` (plain text, opt-in) — see
    /// `list_symbols`' `format` field for details.
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindCallsArgs {
    /// Exact name of the function/method whose callees you want.
    pub function: String,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols), applied to the start and first hop only. Omit to search the whole project.
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
    /// Response shape: `toon` (default) or `text` (plain text, opt-in) — see
    /// `list_symbols`' `format` field for details.
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindCallersArgs {
    /// Exact name of the function/method whose callers you want.
    pub function: String,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols), applied to the start and first hop only. Omit to search the whole project.
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
    /// Response shape: `toon` (default) or `text` (plain text, opt-in) — see
    /// `list_symbols`' `format` field for details.
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ImpactAnalysisArgs {
    /// Exact name of the symbol you're considering changing or removing.
    pub symbol: String,
    /// Narrow to one file or directory/crate prefix (same matching as
    /// list_symbols), applied to the start and first hop only. Omit to search the whole project.
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
    /// Response shape: `toon` (default; a compact table per section) or
    /// `text` (plain text, opt-in) — see `list_symbols`' `format` field.
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

/// The parts of a file's symbol list that decide [`is_top_level`].
struct FileShape {
    /// Names of `module` symbols: a Go package, a C# namespace, or the
    /// synthetic file module.
    modules: HashSet<String>,
    /// How many non-module symbols in the file carry each name.
    type_names: HashMap<String, usize>,
}

fn file_shape(entries: &[mct_index::SymbolListEntry]) -> FileShape {
    let mut shape = FileShape {
        modules: HashSet::new(),
        type_names: HashMap::new(),
    };
    for entry in entries {
        if entry.kind == "module" {
            shape.modules.insert(entry.name.clone());
        } else {
            *shape.type_names.entry(entry.name.clone()).or_insert(0) += 1;
        }
    }
    shape
}

/// Whether `entry` is a top-level declaration of its file.
///
/// No parent: yes. A parent that is not a module: no, it is a member (a method
/// of a class, a field of a struct). A module parent: yes, except when a class
/// or struct in the same file has the module's name. Then the child is that
/// type's member, unless it is a `function`, which stays at package level
/// (`package ledger` beside `type ledger`: Go's `NewLedger` vs `Ledger.Add`).
fn is_top_level(entry: &mct_index::SymbolListEntry, shape: &FileShape) -> bool {
    let Some(parent) = &entry.parent else {
        return true;
    };
    if !shape.modules.contains(parent) {
        return false;
    }
    // The entry counts toward its own name when it is itself the type named
    // like its parent; that is not a clash with anything else.
    let own = usize::from(entry.kind != "module" && entry.name == *parent);
    let clashes = shape.type_names.get(parent).copied().unwrap_or(0) > own;
    !clashes || entry.kind == "function"
}

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
    /// Exact package name (e.g. `three`): also list each manifest that
    /// declares it, with its version. Omit for the usual report only.
    #[serde(default)]
    pub dependency: Option<String>,
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
    /// Response shape: `toon` (default) or `text` (plain text, opt-in) — see
    /// `list_symbols`' `format` field for details.
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetDependencyGraphArgs {
    /// Repo-relative file or directory path (forward slashes) whose imports
    /// to follow. A directory counts as one module.
    pub module: String,
    /// Import hops to follow from `module`. Defaults to 2; clamped to
    /// `mct_core::MAX_QUERY_DEPTH`.
    #[serde(default)]
    pub depth: Option<u32>,
    /// Maximum number of modules to return. Defaults to 50 when omitted;
    /// the response says when the walk was cut short.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Also return the graph rendered as `dot` (Graphviz) or `mermaid`.
    #[serde(default)]
    pub export: Option<String>,
    /// Response shape: `toon` (default) or `text` (plain text, opt-in) — see
    /// `list_symbols`' `format` field for details.
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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SelectToolsArgs {
    /// The user's request, verbatim (e.g. `who calls parse_config, but don't
    /// reindex`). Longer than `intent::MAX_QUERY_CHARS` is truncated.
    pub query: String,
}

/// Env var opting into Tool Attention (issue #11): `1` turns it on.
pub const TOOL_ATTENTION_ENV: &str = "MCT_TOOL_ATTENTION";

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
    /// this server; `batch` itself, `reindex` and `select_tools` are rejected.
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

/// `build_context_pack`'s default cap on related-symbol rows. Lower than
/// [`DEFAULT_RESULT_LIMIT`]: the pack also carries source, and a symbol with
/// more related symbols than this wants a narrower question, not a longer
/// pack.
const CONTEXT_PACK_DEFAULT_LIMIT: usize = 30;
/// Default source lines shown per definition of the packed symbol, after its
/// doc comment.
const CONTEXT_PACK_DEFAULT_SOURCE_LINES: usize = 40;
/// Upper clamp on `source_lines` — a pack shows a definition, never a file.
const CONTEXT_PACK_MAX_SOURCE_LINES: usize = 200;
/// Definitions of an ambiguous name rendered with source; the rest are only
/// counted, with a hint to narrow by `path`/`language`.
const CONTEXT_PACK_MAX_DEFINITIONS: usize = 3;
/// Bound on each multi-hop walk behind a pack (`depth > 1`), so a hub
/// symbol's transitive callers can't turn one call into thousands of rows.
const CONTEXT_PACK_MAX_RELATIONS: usize = 500;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BuildContextPackArgs {
    /// Exact name of the symbol to build the pack around.
    pub symbol: String,
    /// Which definition(s) to pack when the name is ambiguous: a file or
    /// directory/crate prefix, as list_symbols. Callers/tests stay project-wide.
    #[serde(default)]
    pub path: Option<String>,
    /// Which definition(s) to pack, by language id — like `path`.
    #[serde(default)]
    pub language: Option<String>,
    /// Call-graph hops to include, both ways. Defaults to 1 (direct only).
    #[serde(default)]
    pub depth: Option<u32>,
    /// Maximum related symbols listed. Defaults to 30.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Source lines per definition, doc comment not counted. Defaults to 40,
    /// clamped to 1..=200.
    #[serde(default)]
    pub source_lines: Option<usize>,
    /// `toon` (default; related symbols as one table) or `text` (opt-in).
    #[serde(default)]
    pub format: Option<String>,
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
    (
        "lookup",
        &[
            "find_symbol",
            "search_symbols",
            "hybrid_search",
            "semantic_search",
        ],
    ),
    (
        "relations",
        &[
            "find_callers",
            "find_calls",
            "find_references",
            "impact_analysis",
            "build_context_pack",
            "get_dependency_graph",
        ],
    ),
    (
        "maintenance",
        &["reindex", "get_indexing_status", "find_dead_code"],
    ),
    (
        "meta",
        &[
            "discover_tool_categories",
            "get_tool_schema",
            "select_tools",
            "batch",
        ],
    ),
];

/// Cache version of `search_symbols`' ranking — bump when it changes.
const SEARCH_SYMBOLS_CACHE_VERSION: &str = "search_symbols/1";
/// Cache version of `hybrid_search`'s ranking — bump when it changes.
const HYBRID_SEARCH_CACHE_VERSION: &str = "hybrid_search/1";

/// Stands in for the query in the rendering a response's "already sent"
/// digest is taken over, so an equivalent query that reuses a ranking
/// compares equal to the response its source query produced.
const QUERY_PLACEHOLDER: &str = "\u{0}query\u{0}";

/// How an [`MctServer`] is built beyond its index and registry.
pub struct ServerOptions {
    pub cache: CacheConfig,
    /// Embedder for `hybrid_search`'s semantic side; `None` loads the build's
    /// own model lazily (see `crate::embedder`).
    pub embedder: Option<Box<dyn mct_index::Embedder>>,
    /// Share an existing cache instead of starting an empty one.
    pub shared_cache: Option<Arc<std::sync::Mutex<QueryCache>>>,
    /// Tool Attention (issue #11): `tools/list` starts with `select_tools`
    /// only and is narrowed per request. Off: the full catalog, no
    /// `select_tools`.
    pub tool_attention: bool,
}

impl ServerOptions {
    /// The defaults `mct-mcp-server` runs with: cache configured from the
    /// `MCT_CACHE*` environment variables, the build's own model.
    pub fn from_env() -> Self {
        Self {
            cache: CacheConfig::from_env(),
            embedder: None,
            shared_cache: None,
            tool_attention: std::env::var(TOOL_ATTENTION_ENV).is_ok_and(|v| v == "1"),
        }
    }
}

/// Re-ranks `query` against the current index for the query cache's
/// validation: `None` when the ranking can't be reproduced (no vector for a
/// model, a failed refresh or search), which the cache treats as a miss.
fn rerank_hits(
    index: &Index,
    embedder: Option<&dyn mct_index::Embedder>,
    alpha: f64,
    scope: mct_index::QueryScope<'_>,
    query: &str,
    vector: Option<&[f32]>,
) -> Option<Vec<mct_index::SymbolHit>> {
    let fused = match (embedder, vector) {
        (Some(e), Some(v)) => {
            index.refresh_embeddings(e).ok()?;
            index.hybrid_search_with_vector(query, Some((v, e.model_id())), alpha, scope)
        }
        (None, _) => index.hybrid_search_with_vector(query, None, alpha, scope),
        (Some(_), None) => return None,
    };
    fused.ok().map(|f| f.into_iter().map(|h| h.hit).collect())
}

/// The search constants a ranking depends on, as one cache-key field.
fn search_config() -> String {
    format!(
        "rrf_k={};semantic_candidates={};phrase_boost={};embedding_text={}",
        mct_index::RRF_K,
        mct_index::SEMANTIC_CANDIDATES,
        mct_index::EXACT_PHRASE_BOOST,
        mct_index::EMBEDDING_TEXT_VERSION
    )
}

/// The reply for a cache hit this session already received in full — kept
/// to one short line, since it is the whole point of the token saving.
fn reference_reply(kind: &HitKind) -> String {
    match kind {
        HitKind::Exact => {
            "cache: unchanged since your identical earlier call (cache:false resends)".to_string()
        }
        HitKind::Semantic {
            similarity,
            source_query,
        } => format!(
            "cache: same as your earlier `{source_query}` (sim {similarity:.3}); cache:false resends"
        ),
    }
}

#[derive(Clone)]
pub struct MctServer {
    index: Arc<Mutex<Index>>,
    /// The index's project root, kept here so the embedding model can be
    /// loaded without taking the index mutex (`Index::root` is immutable).
    root: std::path::PathBuf,
    registry: LanguageRegistry,
    semantic: Arc<SemanticModel>,
    cache: Arc<std::sync::Mutex<QueryCache>>,
    // Built once in `MctServer::new` with the `tools.ttc` descriptions
    // applied. The `#[tool_handler(router = self.tool_router)]` below serves
    // `tools/list`/`tools/call` from this field; the macro's default would
    // build a fresh, unpatched router.
    tool_router: ToolRouter<MctServer>,
    /// With Tool Attention on, the tool names `tools/list` currently
    /// advertises; `None` when it is off (the full catalog).
    // ponytail: one set per server, i.e. per stdio session; key it by
    // session if a multi-session transport is ever added.
    attention: Option<Arc<std::sync::Mutex<Vec<String>>>>,
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

/// Parses a tool's optional `format` argument (`toon`, the default when it is
/// omitted or null, or `text` — see `crate::toon`). An unrecognized value is
/// rejected the same way `parse_match_mode` rejects a typo'd `match`, rather
/// than silently falling back to the default.
fn parse_output_format(raw: Option<&str>) -> Result<OutputFormat, McpError> {
    OutputFormat::parse(raw).map_err(|msg| McpError::invalid_params(msg, None))
}

/// Validates `get_dependency_graph`'s `module`: repo-relative, forward or
/// back slashes, no absolute path and no `..` component. Returns it with
/// forward slashes and without a leading `./` or trailing `/`.
fn validate_module_path(raw: &str) -> Result<String, McpError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(McpError::invalid_params("`module` must not be empty", None));
    }
    let normalized = trimmed.replace('\\', "/");
    let is_windows_drive = normalized.as_bytes().get(1) == Some(&b':');
    if normalized.starts_with('/') || is_windows_drive {
        return Err(McpError::invalid_params(
            format!("`module` must be relative to the project root, got `{trimmed}`"),
            None,
        ));
    }
    if normalized.split('/').any(|part| part == "..") {
        return Err(McpError::invalid_params(
            format!("`module` must not contain `..`, got `{trimmed}`"),
            None,
        ));
    }
    let cleaned: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if cleaned.is_empty() {
        return Err(McpError::invalid_params(
            "`module` must name a file or directory",
            None,
        ));
    }
    Ok(cleaned.join("/"))
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
/// starts with its compiled-in fallback descriptions intact. Each entry's
/// `EFFECTS` class also sets the MCP `readOnlyHint`/`destructiveHint`
/// annotations (issue #17); `batch` refuses any tool not marked read-only.
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
            Some(route) => {
                route.attr.description = Some(entry.expand().into());
                // Only `reindex` (derived data) and `select_tools` (the
                // advertised list) write — not destructive, which MCP
                // otherwise assumes for a non-read-only tool.
                route.attr.annotations = Some(match entry.effect() {
                    ttc::Effect::None | ttc::Effect::Read => ToolAnnotations::new().read_only(true),
                    ttc::Effect::Write => {
                        ToolAnnotations::new().read_only(false).destructive(false)
                    }
                });
            }
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
    mct_index::read_repository_file(&canonical)
        .and_then(|bytes| {
            String::from_utf8(bytes)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })
        .map_err(|source| {
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

/// Every tool's POST contract floor (issue #17): a successful call returns
/// some text. Checked on every direct call and `batch` sub-query, so a tool
/// that regresses to an empty answer fails loudly instead of leaving the
/// agent to guess what nothing means.
fn check_postcondition(tool: &str, result: &CallToolResult) -> Result<(), McpError> {
    if result.is_error == Some(true) || !result_text(result).trim().is_empty() {
        return Ok(());
    }
    Err(McpError::internal_error(
        format!("`{tool}` broke its contract: a successful call returned no text"),
        None,
    ))
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

/// One definition of the symbol a `build_context_pack` is built around: its
/// location and its doc comment + source, already cut to `source_lines`.
pub struct PackedDefinition {
    pub hit: mct_index::SymbolHit,
    pub snippet: String,
    /// Lines of the definition past the `source_lines` cut.
    pub hidden_lines: u32,
}

/// One symbol related to the packed one, listed once however many relations
/// connect them: every role it plays (`callee`, `caller`, `test`, or a
/// dependency's relation kind such as `imports`), the closest hop it was
/// found at, and where it is defined.
pub struct PackedRelated {
    pub name: String,
    pub roles: Vec<String>,
    pub hop: u32,
    /// The exact definition: a callee/dependency's resolved target, or the
    /// referring symbol itself for a caller/test.
    pub definition: Option<mct_index::SymbolHit>,
    pub signature: Option<String>,
}

/// A relation whose target the index could not prove: listed apart from the
/// related symbols, and never followed further.
pub struct UncertainRelated {
    pub name: String,
    pub roles: Vec<String>,
    pub hop: u32,
    /// For a caller/test, its own definition (it is its call's target that
    /// is uncertain); `None` for a callee/dependency.
    pub definition: Option<mct_index::SymbolHit>,
    /// For a callee/dependency, the first few remaining candidates.
    pub candidates: Vec<mct_index::SymbolHit>,
    /// How many definitions the uncertain relation could denote.
    pub candidate_count: usize,
}

/// Everything `build_context_pack` renders, deduplicated.
pub struct ContextPack {
    pub symbol: String,
    pub depth: u32,
    pub definitions: Vec<PackedDefinition>,
    pub omitted_definitions: usize,
    pub related: Vec<PackedRelated>,
    /// Ambiguous relations, kept apart from `related`.
    pub uncertain: Vec<UncertainRelated>,
    /// Files referencing the symbol at file level (a top-of-file `use`/
    /// `import`) rather than from inside one of their symbols, sorted.
    pub file_level: Vec<String>,
    /// Called/referenced names the parser proved external (std/third-party),
    /// sorted.
    pub external: Vec<String>,
    /// Called/referenced names with no indexed definition and no external
    /// evidence — simply unknown — sorted.
    pub unresolved: Vec<String>,
}

/// How many candidates of an ambiguous callee a pack lists.
const CONTEXT_PACK_MAX_CANDIDATES: usize = 3;

/// A related symbol while a pack is being assembled: its roles, closest hop,
/// and what identifies it — an exact symbol row, or for an ambiguous callee
/// the relation whose candidates stand in for it.
struct RelatedDraft {
    name: String,
    roles: Vec<String>,
    hop: u32,
    /// The exact row: a resolved target, or a caller/test itself.
    symbol_id: Option<i64>,
    /// Whether the relation is proven (resolved) — a caller is certain once
    /// any one of its relations to the packed symbol resolved.
    certain: bool,
    /// One relation behind this draft, for an ambiguous callee's candidates.
    relation_id: i64,
    candidate_count: usize,
}

/// Collects related symbols in first-seen order, merging repeat sightings of
/// the same one into one [`RelatedDraft`] — the deduplication behind
/// `build_context_pack`. Keyed by symbol id where known, by name for an
/// ambiguous callee.
#[derive(Default)]
struct RelatedSet {
    drafts: Vec<RelatedDraft>,
    positions: std::collections::HashMap<String, usize>,
}

impl RelatedSet {
    /// A callee/dependency sighting. Only resolved and ambiguous relations
    /// become drafts; any other is returned for the external/unresolved
    /// footers, still carrying its resolution.
    fn add_target<'h>(
        &mut self,
        role: &str,
        hit: &'h mct_index::RelationHit,
    ) -> Option<&'h mct_index::RelationHit> {
        match hit.resolution {
            mct_index::Resolution::Resolved => {
                let id = hit.target_id?;
                self.add(format!("#{id}"), &hit.to_name, role, hit, Some(id), true);
            }
            mct_index::Resolution::Ambiguous => {
                let key = format!("?{}", hit.to_name);
                self.add(key, &hit.to_name, role, hit, None, false);
            }
            _ => return Some(hit),
        }
        None
    }

    /// A caller/test sighting: the referring symbol is always exact.
    fn add_referrer(&mut self, role: &str, hit: &mct_index::RelationHit) {
        let certain = hit.resolution == mct_index::Resolution::Resolved;
        let id = hit.from_symbol_id;
        self.add(
            format!("#{id}"),
            &hit.from_symbol,
            role,
            hit,
            Some(id),
            certain,
        );
    }

    fn add(
        &mut self,
        key: String,
        name: &str,
        role: &str,
        hit: &mct_index::RelationHit,
        symbol_id: Option<i64>,
        certain: bool,
    ) {
        match self
            .positions
            .get(&key)
            .and_then(|&i| self.drafts.get_mut(i))
        {
            Some(draft) => {
                if !draft.roles.iter().any(|r| r == role) {
                    draft.roles.push(role.to_string());
                }
                draft.hop = draft.hop.min(hit.depth);
                draft.certain |= certain;
            }
            None => {
                self.positions.insert(key, self.drafts.len());
                self.drafts.push(RelatedDraft {
                    name: name.to_string(),
                    roles: vec![role.to_string()],
                    hop: hit.depth,
                    symbol_id,
                    certain,
                    relation_id: hit.relation_id,
                    candidate_count: hit.candidate_count,
                });
            }
        }
    }
}

/// Drops `module` entries (a file's synthetic whole-file symbol) when the
/// name also has a real declaration — packing a whole file is exactly what
/// this tool exists to avoid.
fn without_modules(defs: Vec<mct_index::SymbolHit>) -> Vec<mct_index::SymbolHit> {
    if defs.iter().all(|d| d.kind == "module") {
        return defs;
    }
    defs.into_iter().filter(|d| d.kind != "module").collect()
}

#[tool_router]
impl MctServer {
    /// A server with [`ServerOptions::from_env`].
    pub fn new(index: Index, registry: LanguageRegistry) -> Self {
        Self::with_options(index, registry, ServerOptions::from_env())
    }

    pub fn with_options(index: Index, registry: LanguageRegistry, options: ServerOptions) -> Self {
        let mut tool_router = Self::tool_router();
        apply_ttc_catalog(&mut tool_router, ttc::CATALOG_SOURCE);
        if !options.tool_attention {
            tool_router.remove_route("select_tools");
        }
        let attention = options
            .tool_attention
            .then(|| Arc::new(std::sync::Mutex::new(vec!["select_tools".to_string()])));
        let semantic = match options.embedder {
            Some(embedder) => SemanticModel::preloaded(embedder),
            None => SemanticModel::default(),
        };
        let cache = options
            .shared_cache
            .unwrap_or_else(|| Arc::new(std::sync::Mutex::new(QueryCache::new(options.cache))));
        Self {
            root: index.root().to_path_buf(),
            index: Arc::new(Mutex::new(index)),
            registry,
            semantic: Arc::new(semantic),
            cache,
            tool_router,
            attention,
        }
    }

    /// This server's query cache, to share with another server (tests).
    pub fn cache_handle(&self) -> Arc<std::sync::Mutex<QueryCache>> {
        Arc::clone(&self.cache)
    }

    /// The query cache's counters and latencies so far.
    pub fn cache_stats(&self) -> CacheStats {
        self.cache_guard().stats()
    }

    /// The cache, even if a panicking holder poisoned the lock: every
    /// mutation leaves it consistent, and a miss is always safe.
    fn cache_guard(&self) -> std::sync::MutexGuard<'_, QueryCache> {
        self.cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether this call goes through the cache at all.
    fn cache_on(&self, cache_arg: Option<bool>) -> bool {
        cache_arg != Some(false) && self.cache_guard().config().enabled
    }

    /// The embedding model, or why there is none. Loading it (an ONNX
    /// session plus a possible first-use download) takes seconds, so it runs
    /// on a blocking thread that this call awaits — not a detached task — and
    /// callers must not hold the index mutex across it. Concurrent callers
    /// wait on the same once-cell: the model is built exactly once, a failed
    /// load is remembered, and no caller ever sees a half-built model.
    ///
    /// Lock order, everywhere in this server: model (this call) and query
    /// embedding first with no lock held, then the index mutex, then the
    /// cache mutex (a short `std` mutex never held across an `.await` or
    /// model work). Never the reverse, so the three cannot deadlock.
    async fn semantic_model(&self) -> Result<&dyn mct_index::Embedder, String> {
        let model = Arc::clone(&self.semantic);
        let root = self.root.clone();
        let loaded = tokio::task::spawn_blocking(move || {
            model.get(&root).map(|_| ()).map_err(str::to_string)
        })
        .await;
        match loaded {
            Ok(Ok(())) => self.semantic.get(&self.root).map_err(str::to_string),
            Ok(Err(reason)) => Err(reason),
            Err(e) => Err(format!("the embedding model failed to load: {e}")),
        }
    }

    /// [`Index::embed_query`] on a blocking thread, with no lock held: ONNX
    /// inference on the query takes tens of milliseconds and an ordinary
    /// query must not queue behind it. Errors come back as text for the
    /// caller to turn into a tool error or a cache miss.
    async fn embed_query_unlocked(&self, query: &str) -> Result<Vec<f32>, String> {
        let model = Arc::clone(&self.semantic);
        let root = self.root.clone();
        let query = query.to_string();
        tokio::task::spawn_blocking(move || {
            let embedder = model.get(&root).map_err(str::to_string)?;
            Index::embed_query(embedder, &query).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| format!("query embedding failed: {e}"))?
    }

    /// Embeds the symbols that have no vector yet, holding the index mutex
    /// only for the snapshot and for each batch's commit: the model runs on a
    /// blocking thread with no lock. A cold refresh after a reindex embeds
    /// thousands of symbols; under the mutex it froze every other tool. A
    /// reindex may land between batches: the commit skips rows that changed
    /// since the snapshot, so nothing stale is stored.
    async fn refresh_pending_unlocked(&self, model_id: &str) -> Result<usize, String> {
        let mut pending = self
            .index
            .lock()
            .await
            .snapshot_pending_embeddings(model_id)
            .map_err(|e| e.to_string())?;
        let mut committed = 0;
        for batch in 0..pending.batch_count() {
            let model = Arc::clone(&self.semantic);
            let root = self.root.clone();
            // The snapshot travels into the closure and back: it owns the
            // symbol texts the batch embeds, and the commit needs it again.
            let (returned, vectors) = tokio::task::spawn_blocking(move || {
                let vectors = model
                    .get(&root)
                    .map_err(str::to_string)
                    .and_then(|embedder| {
                        pending
                            .embed_batch(batch, embedder)
                            .map_err(|e| e.to_string())
                    });
                (pending, vectors)
            })
            .await
            .map_err(|e| format!("embedding failed: {e}"))?;
            pending = returned;
            committed += self
                .index
                .lock()
                .await
                .commit_embedding_batch(&pending, batch, vectors?)
                .map_err(|e| e.to_string())?;
        }
        Ok(committed)
    }

    /// The response for a call: `full` as rendered, or — for a hit whose
    /// query-independent rendering `digest_view` this session already
    /// received — the one-line reference reply. A semantic hit shown in full
    /// says whose ranking it reuses.
    fn cached_reply(
        &self,
        cache_on: bool,
        full: String,
        digest_view: &str,
        hit: Option<&CachedResult>,
    ) -> CallToolResult {
        if !cache_on {
            return CallToolResult::success(vec![ContentBlock::text(full)]);
        }
        let mut cache = self.cache_guard();
        let text = match hit {
            Some(hit)
                if cache.config().response == cache::ResponseMode::Reference
                    && cache.was_sent(digest_view) =>
            {
                let reply = reference_reply(&hit.kind);
                if reply.len() < full.len() {
                    cache.record_reference();
                    reply
                } else {
                    full
                }
            }
            Some(CachedResult {
                kind:
                    HitKind::Semantic {
                        similarity,
                        source_query,
                    },
                ..
            }) => format!(
                "{}\ncache: ranking reused from earlier query `{source_query}` (similarity {similarity:.3}), re-validated against the current index\n",
                full.trim_end()
            ),
            _ => full,
        };
        cache.record_sent(digest_view);
        CallToolResult::success(vec![ContentBlock::text(text)])
    }

    /// The tool catalog as it will be sent to an MCP client: names,
    /// TTC-expanded descriptions and input schemas. Exposed for tests and
    /// for measuring the catalog's token/byte footprint; not itself an MCP
    /// tool.
    pub fn tool_catalog(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }

    /// What `tools/list` returns now: the full catalog, or with Tool
    /// Attention on, only the tools the last `select_tools` picked (plus
    /// `select_tools` itself). Every catalog tool stays callable either way.
    pub fn advertised_tools(&self) -> Vec<rmcp::model::Tool> {
        let mut tools = self.tool_catalog();
        if let Some(active) = &self.attention {
            let active = active
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            tools.retain(|t| active.iter().any(|name| name == t.name.as_ref()));
        }
        tools
    }

    /// `select_tools` minus the `tools/list_changed` notification: picks the
    /// tools for `query` and makes them the advertised set: the word rules of
    /// `intent::preselect`, or the full catalog when they find nothing. No
    /// embedding step: in benchmarks/intent-selection.md it lost a required
    /// tool and guessed on requests no tool answers.
    pub async fn attend(&self, query: &str) -> Result<CallToolResult, McpError> {
        let Some(active) = &self.attention else {
            return Err(McpError::invalid_params(
                format!("Tool Attention is off — set {TOOL_ATTENTION_ENV}=1"),
                None,
            ));
        };
        let query = query.trim();
        if query.is_empty() {
            return Err(McpError::invalid_params("query must not be empty", None));
        }
        let catalog: Vec<rmcp::model::Tool> = self
            .tool_catalog()
            .into_iter()
            .filter(|t| t.name != "select_tools")
            .collect();
        let languages = self.registry.language_ids();
        let selection = intent::preselect(query, &languages, &catalog, None);
        let mut names = vec!["select_tools".to_string()];
        names.extend(selection.tools.iter().cloned());
        *active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = names;

        let mut text = selection.intent.render(selection.source);
        text.push_str(&format!("\ntools: {}", selection.tools.join(", ")));
        for (name, args) in &selection.hints {
            let args: Vec<String> = args.iter().map(|(k, v)| format!("{k}={v}")).collect();
            text.push_str(&format!("\nhint {name}: {}", args.join(" ")));
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
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

    /// Applies an edit to the project's ignore files before a tool reads the
    /// index ([`Index::apply_exclusion_changes`]): a path that `.mctignore`
    /// (or an imported `.gitignore`) now excludes is never served from rows
    /// stored before the edit, even with the watcher disabled or still
    /// debouncing. Fails closed: if the reindex fails, so does the call. The
    /// index lock is released before the tool runs, keeping the lock order
    /// documented on [`MctServer::semantic_model`].
    async fn apply_exclusion_changes(&self) -> Result<(), McpError> {
        self.index
            .lock()
            .await
            .apply_exclusion_changes(&self.registry)
            .map(|_| ())
            .map_err(index_error)
    }

    /// Calls any tool by name with a JSON argument object, through the same
    /// method (and so the same validation, defaults and output) an MCP client
    /// reaches over stdio. Used by `mct-cli call`; unlike a `batch`
    /// sub-query it also accepts `reindex` and `batch` themselves.
    pub async fn call_tool_by_name(
        &self,
        tool: &str,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, McpError> {
        match tool {
            "reindex" => self.reindex(batch_params(tool, args)?).await,
            "batch" => self.batch(batch_params(tool, args)?).await,
            other => self.run_batch_query(other, args).await,
        }
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
        self.apply_exclusion_changes().await?;
        if let Some(route) = self.tool_router.map.get(tool) {
            let read_only = route
                .attr
                .annotations
                .as_ref()
                .and_then(|a| a.read_only_hint)
                == Some(true);
            if !read_only {
                return Err(McpError::invalid_params(
                    format!(
                        "`{tool}` isn't allowed in a batch: its contract has write effects \
                         (batches are read-only) — call it directly"
                    ),
                    None,
                ));
            }
        }
        let result = match tool {
            "list_symbols" => self.list_symbols(batch_params(tool, args)?).await,
            "find_symbol" => self.find_symbol(batch_params(tool, args)?).await,
            "search_symbols" => self.search_symbols(batch_params(tool, args)?).await,
            "hybrid_search" => self.hybrid_search(batch_params(tool, args)?).await,
            "semantic_search" => self.semantic_search(batch_params(tool, args)?).await,
            "find_references" => self.find_references(batch_params(tool, args)?).await,
            "find_calls" => self.find_calls(batch_params(tool, args)?).await,
            "find_callers" => self.find_callers(batch_params(tool, args)?).await,
            "impact_analysis" => self.impact_analysis(batch_params(tool, args)?).await,
            "build_context_pack" => self.build_context_pack(batch_params(tool, args)?).await,
            "get_indexing_status" => self.get_indexing_status(batch_params(tool, args)?).await,
            "get_file_skeleton" => self.get_file_skeleton(batch_params(tool, args)?).await,
            "get_project_overview" => self.get_project_overview(batch_params(tool, args)?).await,
            "get_file_tree" => self.get_file_tree(batch_params(tool, args)?).await,
            "find_dead_code" => self.find_dead_code(batch_params(tool, args)?).await,
            "get_dependency_graph" => self.get_dependency_graph(batch_params(tool, args)?).await,
            "discover_tool_categories" => self.discover_tool_categories().await,
            "get_tool_schema" => self.get_tool_schema(batch_params(tool, args)?).await,
            "batch" => Err(McpError::invalid_params(
                "`batch` can't be nested inside a batch",
                None,
            )),
            other => Err(McpError::invalid_params(
                format!(
                    "no tool named `{other}` — call discover_tool_categories for the full list"
                ),
                None,
            )),
        }?;
        check_postcondition(tool, &result)?;
        Ok(result)
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
            cache,
        }): Parameters<SearchSymbolsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let query = validate_name(&query)?;
        let output_format = parse_output_format(format.as_deref())?;
        let scope = query_scope(optional_arg(path.as_ref()), optional_arg(language.as_ref()));
        let limit = limit
            .unwrap_or(SEARCH_DEFAULT_LIMIT)
            .clamp(1, SEARCH_MAX_LIMIT);
        let offset = offset.unwrap_or(0);
        let snippet_lines = snippet_lines.unwrap_or(0).min(SEARCH_MAX_SNIPPET_LINES);
        let index = self.index.lock().await;
        let cache_on = self.cache_on(cache);
        let mut hit = None;
        let hits = if cache_on {
            let root = index.root().display().to_string();
            let key = cache::RankingKey {
                repository: &root,
                tool: "search_symbols",
                tool_version: SEARCH_SYMBOLS_CACHE_VERSION,
                params: vec![
                    ("path", scope.path.unwrap_or_default().to_string()),
                    ("language", scope.language.unwrap_or_default().to_string()),
                ],
                search_config: search_config(),
                model: None,
            };
            let normalized = cache::normalize_query(query);
            let mut rerank = |q: &str, _: Option<&[f32]>| index.search_symbols(q, scope).ok();
            let mut guard = self.cache_guard();
            hit = guard.lookup_exact(&index, &key, &normalized, &mut rerank);
            match &hit {
                Some(found) => found.hits.clone(),
                None => {
                    let hits = index.search_symbols(query, scope).map_err(index_error)?;
                    guard.insert(&index, &key, &normalized, None, hits.clone());
                    hits
                }
            }
        } else {
            index.search_symbols(query, scope).map_err(index_error)?
        };
        let snippets = page_snippets(&index, &hits, offset, limit, snippet_lines);
        let render = |query: &str| match output_format {
            OutputFormat::Text => format::search_hits(query, &hits, offset, limit, &snippets),
            OutputFormat::Toon => format::search_hits_toon(query, &hits, offset, limit, &snippets),
        };
        let text = render(query);
        let digest_view = if cache_on {
            render(QUERY_PLACEHOLDER)
        } else {
            String::new()
        };
        Ok(self.cached_reply(cache_on, text, &digest_view, hit.as_ref()))
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
            cache,
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
        let limit = top_k
            .unwrap_or(SEARCH_DEFAULT_LIMIT)
            .clamp(1, SEARCH_MAX_LIMIT);
        let offset = offset.unwrap_or(0);
        let snippet_lines = snippet_lines.unwrap_or(0).min(SEARCH_MAX_SNIPPET_LINES);
        let cache_on = self.cache_on(cache);

        // The embedder the semantic side would use: none at alpha 0 (the
        // model is never loaded), or why it's unavailable. Loaded before the
        // index mutex is taken, so a cold start never blocks other queries.
        let model = if alpha != 0.0 {
            Some(self.semantic_model().await)
        } else {
            None
        };
        let embedder = model.as_ref().and_then(|m| m.as_ref().ok()).copied();
        let semantic_cache = cache_on
            && embedder.is_some()
            && mct_index::exact_phrase(query).is_none()
            && self.cache_guard().config().semantic;

        let mut index = self.index.lock().await;
        let root = index.root().display().to_string();
        let key = cache::RankingKey {
            repository: &root,
            tool: "hybrid_search",
            tool_version: HYBRID_SEARCH_CACHE_VERSION,
            params: vec![
                ("path", scope.path.unwrap_or_default().to_string()),
                ("language", scope.language.unwrap_or_default().to_string()),
                ("alpha", format!("{alpha:?}")),
            ],
            search_config: search_config(),
            model: embedder.map(|e| e.model_id().to_string()),
        };
        let normalized = cache::normalize_query(query);
        let mut hit = None;
        if cache_on {
            let mut rerank =
                |q: &str, v: Option<&[f32]>| rerank_hits(&index, embedder, alpha, scope, q, v);
            hit = self
                .cache_guard()
                .lookup_exact(&index, &key, &normalized, &mut rerank);
        }

        // A miss needs the query's vector (for the semantic cache and for the
        // ranking itself). Embed it with the index mutex released, then take
        // the mutex again: lookups below validate against the index as it is
        // now, and a ranking built from the earlier view is never reused.
        let mut query_vector: Option<Result<Vec<f32>, String>> = None;
        if hit.is_none() && embedder.is_some() {
            drop(index);
            let start = std::time::Instant::now();
            let vector = self.embed_query_unlocked(query).await;
            index = self.index.lock().await;
            // The semantic cache path: never for an exact phrase (its words
            // must match literally), only with a model to embed the query.
            if let (Ok(vector), true) = (&vector, semantic_cache) {
                let mut guard = self.cache_guard();
                guard.record_embedding(start.elapsed());
                let mut rerank =
                    |q: &str, v: Option<&[f32]>| rerank_hits(&index, embedder, alpha, scope, q, v);
                hit = guard.lookup_semantic(&index, &key, &normalized, vector, &mut rerank);
            }
            query_vector = Some(vector);
        }

        // Lexical-only whenever the semantic side can't contribute: alpha 0
        // (the model is never loaded), no model in this build or it failed to
        // load, or embedding the pending symbols failed. Each case is named
        // in the output's first line instead of erroring.
        let (ranked_with, note) = match model {
            None => (
                None,
                format!("hybrid: alpha 0{alpha_source}, lexical ranking only"),
            ),
            Some(Err(reason)) => (None, format!("hybrid: lexical ranking only — {reason}")),
            Some(Ok(embedder)) => {
                // A hit's ranking was validated against the current index;
                // only a miss needs the pending symbols embedded, and that
                // runs with the index mutex released (see
                // `refresh_pending_unlocked`).
                let refreshed = match hit {
                    Some(_) => Ok(0),
                    None => {
                        drop(index);
                        let refreshed = self.refresh_pending_unlocked(embedder.model_id()).await;
                        index = self.index.lock().await;
                        refreshed
                    }
                };
                match refreshed {
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
                }
            }
        };
        let hits: Vec<mct_index::SymbolHit> = match &hit {
            Some(found) => found.hits.clone(),
            None => {
                // `ranked_with` is only `Some` with an embedder, and a miss
                // with an embedder always embedded its query above.
                let vector = match (ranked_with, query_vector.take()) {
                    (Some(_), Some(Ok(v))) => Some(v),
                    (Some(_), Some(Err(err))) => {
                        return Err(McpError::internal_error(
                            format!("hybrid_search failed: {err}"),
                            None,
                        ));
                    }
                    (Some(_), None) => {
                        return Err(McpError::internal_error(
                            "hybrid_search failed: the query was not embedded".to_string(),
                            None,
                        ));
                    }
                    (None, _) => None,
                };
                let semantic = ranked_with.zip(vector.as_deref());
                let fused = match index.hybrid_search_with_vector(
                    query,
                    semantic.map(|(e, v)| (v, e.model_id())),
                    alpha,
                    scope,
                ) {
                    Ok(fused) => fused,
                    Err(e) if ranked_with.is_some() => {
                        return Err(McpError::internal_error(
                            format!("hybrid_search failed: {e}"),
                            None,
                        ));
                    }
                    Err(e) => return Err(index_error(e)),
                };
                let hits: Vec<mct_index::SymbolHit> = fused.into_iter().map(|h| h.hit).collect();
                // Cached only when the semantic side ran exactly as keyed — a
                // refresh failure fell back to lexical under a model's key.
                if cache_on && ranked_with.is_some() == key.model.is_some() {
                    self.cache_guard().insert(
                        &index,
                        &key,
                        &normalized,
                        vector.as_deref(),
                        hits.clone(),
                    );
                }
                hits
            }
        };
        let snippets = page_snippets(&index, &hits, offset, limit, snippet_lines);
        let render = |query: &str| match output_format {
            OutputFormat::Text => format::search_hits(query, &hits, offset, limit, &snippets),
            OutputFormat::Toon => format::search_hits_toon(query, &hits, offset, limit, &snippets),
        };
        let mut text = render(query);
        // An exact phrase also searches prose string literals (error/log
        // messages), appended as their own section when any hold it.
        let mut literal_section = String::new();
        if let Some(phrase) = mct_index::exact_phrase(query) {
            let literals = index.search_literals(phrase, scope).map_err(index_error)?;
            literal_section = match output_format {
                OutputFormat::Text => format::literal_hits(phrase, &literals, offset, limit),
                OutputFormat::Toon => format::literal_hits_toon(phrase, &literals, offset, limit),
            };
            if !literal_section.is_empty() {
                text = format!("{}\n{literal_section}", text.trim_end());
            }
        }
        let digest_view = if cache_on {
            format!("{note}\n{}\n{literal_section}", render(QUERY_PLACEHOLDER))
        } else {
            String::new()
        };
        Ok(self.cached_reply(
            cache_on,
            format!("{note}\n{text}"),
            &digest_view,
            hit.as_ref(),
        ))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Meaning-based symbol search by embedding similarity; see tools.ttc")]
    pub async fn semantic_search(
        &self,
        Parameters(SemanticSearchArgs {
            query,
            top_k,
            path,
            language,
        }): Parameters<SemanticSearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let query = validate_name(&query)?;
        let scope = query_scope(optional_arg(path.as_ref()), optional_arg(language.as_ref()));
        let limit = top_k
            .unwrap_or(SEARCH_DEFAULT_LIMIT)
            .clamp(1, SEARCH_MAX_LIMIT);
        // Same lock order as hybrid_search: model, pending symbols and query
        // embedding with no lock held, then the index mutex for the scan.
        // ponytail: no query cache here, unlike hybrid_search; add one if
        // repeated identical queries show up in measurements.
        let embedder = match self.semantic_model().await {
            Ok(embedder) => embedder,
            Err(reason) => {
                return Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                    "semantic_search unavailable — {reason}; use search_symbols or hybrid_search"
                ))]))
            }
        };
        let failed =
            |e: String| McpError::internal_error(format!("semantic_search failed: {e}"), None);
        self.refresh_pending_unlocked(embedder.model_id())
            .await
            .map_err(failed)?;
        let vector = self.embed_query_unlocked(query).await.map_err(failed)?;
        let index = self.index.lock().await;
        let hits = index
            .semantic_search(&vector, embedder.model_id(), scope, limit)
            .map_err(index_error)?;
        let coverage = index
            .embedding_coverage(embedder.model_id())
            .map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "semantic: {}/{} symbols embedded ({})\n{}",
            coverage.embedded,
            coverage.total,
            embedder.model_id(),
            format::semantic_hits(query, &hits)
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
            .relation_page(
                symbol,
                mct_index::RelationDirection::References,
                depth.unwrap_or(1),
                (limit, offset),
                scope,
            )
            .map_err(index_error)?;
        let text = format::relation_page_hits(
            symbol,
            "reference(s)",
            &hits,
            output_format == OutputFormat::Toon,
        );
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
            .relation_page(
                function,
                mct_index::RelationDirection::Calls,
                depth.unwrap_or(1),
                (limit, offset),
                scope,
            )
            .map_err(index_error)?;
        let text = format::relation_page_hits(
            function,
            "call(s) made by this function",
            &hits,
            output_format == OutputFormat::Toon,
        );
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
            .relation_page(
                function,
                mct_index::RelationDirection::Callers,
                depth.unwrap_or(1),
                (limit, offset),
                scope,
            )
            .map_err(index_error)?;
        let text = format::relation_page_hits(
            function,
            "caller(s) of this function",
            &hits,
            output_format == OutputFormat::Toon,
        );
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
        let (callers, references, tests) = {
            let index = self.index.lock().await;
            let callers = index
                .relation_page(
                    symbol,
                    mct_index::RelationDirection::Callers,
                    depth,
                    (limit, offset),
                    scope,
                )
                .map_err(index_error)?;
            let references = index
                .relation_page(
                    symbol,
                    mct_index::RelationDirection::References,
                    depth,
                    (limit, offset),
                    scope,
                )
                .map_err(index_error)?;
            let tests = index
                .affected_test_page(symbol, depth, (limit, offset), scope)
                .map_err(index_error)?;
            (callers, references, tests)
        };
        let text = format::impact_pages(
            symbol,
            &callers,
            &references,
            &tests,
            output_format == OutputFormat::Toon,
        );
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "One-call context bundle for a symbol; see tools.ttc")]
    pub async fn build_context_pack(
        &self,
        Parameters(BuildContextPackArgs {
            symbol,
            path,
            language,
            depth,
            limit,
            source_lines,
            format,
        }): Parameters<BuildContextPackArgs>,
    ) -> Result<CallToolResult, McpError> {
        let symbol = validate_name(&symbol)?;
        let output_format = parse_output_format(format.as_deref())?;
        let path = optional_arg(path.as_ref());
        let language = optional_arg(language.as_ref());
        let depth = depth.unwrap_or(1).clamp(1, mct_core::MAX_QUERY_DEPTH);
        let limit = limit.unwrap_or(CONTEXT_PACK_DEFAULT_LIMIT).max(1);
        let source_lines = source_lines
            .unwrap_or(CONTEXT_PACK_DEFAULT_SOURCE_LINES)
            .clamp(1, CONTEXT_PACK_MAX_SOURCE_LINES);
        let index = self.index.lock().await;

        let definitions = without_modules(
            index
                .find_symbol_scoped(symbol, query_scope(path, language))
                .map_err(index_error)?,
        );
        if definitions.is_empty() {
            return Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "No symbol named `{symbol}` found in the index — try search_symbols for a partial name."
            ))]));
        }

        // Every relation is walked from these exact rows, so a same-named
        // symbol elsewhere (another language, another type's method) never
        // contributes callees, and a callee is only followed — or listed as
        // a related symbol — once its target is proven.
        let definition_ids: Vec<i64> = definitions.iter().map(|d| d.id).collect();
        let callees = index
            .find_calls_from_symbols(&definition_ids, depth, CONTEXT_PACK_MAX_RELATIONS)
            .map_err(index_error)?;
        let dependencies = index
            .find_dependencies_of_symbols(&definition_ids)
            .map_err(index_error)?;
        let callers = index
            .find_callers_of_symbols(&definition_ids, depth, CONTEXT_PACK_MAX_RELATIONS)
            .map_err(index_error)?;
        let references = index
            .find_references_of_symbols(&definition_ids, depth, CONTEXT_PACK_MAX_RELATIONS)
            .map_err(index_error)?;

        let mut related = RelatedSet::default();
        let mut external = std::collections::BTreeSet::new();
        let mut unresolved = std::collections::BTreeSet::new();
        let targets = callees
            .iter()
            .map(|hit| ("callee", hit))
            .chain(dependencies.iter().map(|hit| (hit.kind.as_str(), hit)));
        for (role, hit) in targets {
            if let Some(hit) = related.add_target(role, hit) {
                if hit.resolution == mct_index::Resolution::External {
                    external.insert(hit.to_name.clone());
                } else {
                    unresolved.insert(hit.to_name.clone());
                }
            }
        }
        for hit in &callers {
            related.add_referrer("caller", hit);
        }
        // Backlinks by resolved identity — links and embeds reaching a
        // note/section (`module`/`element`), so a same-named note elsewhere
        // never shares them. Other kinds keep their callers-only pack.
        let linkable: Vec<i64> = definitions
            .iter()
            .filter(|d| matches!(d.kind.as_str(), "module" | "element"))
            .map(|d| d.id)
            .collect();
        if !linkable.is_empty() {
            let backlinks = index
                .find_references_of_symbols(&linkable, 1, CONTEXT_PACK_MAX_RELATIONS)
                .map_err(index_error)?;
            for hit in backlinks
                .iter()
                .filter(|hit| matches!(hit.kind.as_str(), "references" | "imports"))
            {
                related.add_referrer(&hit.kind, hit);
            }
        }
        // Same test heuristic as impact_analysis, over every relation
        // reaching the symbol — a test importing it counts, not just one
        // calling it.
        for hit in references.iter().chain(callers.iter()) {
            if mct_index::looks_like_test_name(&hit.from_symbol, &hit.relative_path) {
                related.add_referrer("test", hit);
            }
        }

        // Every file read at most once, shared by the definitions' source and
        // the related symbols' signatures. A file that vanished since the
        // last reindex just contributes no source rather than failing.
        let mut sources: std::collections::HashMap<String, Option<String>> =
            std::collections::HashMap::new();
        let mut source_of = |relative_path: &str| -> Option<String> {
            sources
                .entry(relative_path.to_string())
                .or_insert_with(|| read_source_file(&index, relative_path).ok())
                .clone()
        };

        let omitted_definitions = definitions
            .len()
            .saturating_sub(CONTEXT_PACK_MAX_DEFINITIONS);
        let mut packed_definitions = Vec::new();
        for hit in definitions.into_iter().take(CONTEXT_PACK_MAX_DEFINITIONS) {
            // An asset is binary or glTF JSON: no source lines to show.
            let source = if hit.kind == "asset" {
                None
            } else {
                source_of(&hit.relative_path)
            };
            let (snippet, hidden_lines) = match source {
                Some(source) => {
                    let start = format::leading_comment_start(&source, hit.line);
                    let doc_lines = hit.line.saturating_sub(start) as usize;
                    let snippet = format::symbol_snippet(
                        &source,
                        start,
                        hit.end_line,
                        doc_lines + source_lines,
                    );
                    let shown_end = hit.line as usize + source_lines - 1;
                    let hidden = hit
                        .end_line
                        .map_or(0, |end| (end as usize).saturating_sub(shown_end));
                    (snippet, hidden as u32)
                }
                None => (String::new(), 0),
            };
            packed_definitions.push(PackedDefinition {
                hit,
                snippet,
                hidden_lines,
            });
        }

        let ids: Vec<i64> = related.drafts.iter().filter_map(|d| d.symbol_id).collect();
        let by_id: std::collections::HashMap<i64, mct_index::SymbolHit> = index
            .symbols_by_ids(&ids)
            .map_err(index_error)?
            .into_iter()
            .map(|hit| (hit.id, hit))
            .collect();
        let mut packed_related = Vec::new();
        let mut uncertain = Vec::new();
        let mut file_level = std::collections::BTreeSet::new();
        for draft in related.drafts {
            // Recursion (the symbol calling itself) isn't a related symbol.
            if draft
                .symbol_id
                .is_some_and(|id| definition_ids.contains(&id))
            {
                continue;
            }
            let definition = draft.symbol_id.and_then(|id| by_id.get(&id).cloned());
            if !draft.certain {
                let candidates = match draft.symbol_id {
                    Some(_) => Vec::new(),
                    None => index
                        .relation_candidates(draft.relation_id)
                        .map_err(index_error)?
                        .into_iter()
                        .take(CONTEXT_PACK_MAX_CANDIDATES)
                        .collect(),
                };
                uncertain.push(UncertainRelated {
                    name: draft.name,
                    roles: draft.roles,
                    hop: draft.hop,
                    definition,
                    candidates,
                    candidate_count: draft.candidate_count,
                });
                continue;
            }
            let Some(definition) = definition else {
                continue; // its file was reindexed away mid-query
            };
            // A relation made by a file's synthetic module symbol is a
            // file-level `use`/`import` of the packed symbol: the file is
            // worth naming, its whole-file "definition" isn't worth a row.
            if definition.kind == "module"
                && draft.roles.iter().all(|r| r == "caller" || r == "test")
            {
                file_level.insert(definition.relative_path);
                continue;
            }
            let signature = source_of(&definition.relative_path)
                .and_then(|source| format::signature_line(&source, definition.line));
            packed_related.push(PackedRelated {
                name: draft.name,
                roles: draft.roles,
                hop: draft.hop,
                definition: Some(definition),
                signature,
            });
        }

        let pack = ContextPack {
            symbol: symbol.to_string(),
            depth,
            definitions: packed_definitions,
            omitted_definitions,
            related: packed_related,
            uncertain,
            file_level: file_level.into_iter().collect(),
            external: external.into_iter().collect(),
            unresolved: unresolved.into_iter().collect(),
        };
        let text = match output_format {
            OutputFormat::Text => format::context_pack(&pack, limit),
            OutputFormat::Toon => format::context_pack_toon(&pack, limit),
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
            dependency,
        }): Parameters<GetIndexingStatusArgs>,
    ) -> Result<CallToolResult, McpError> {
        let index = self.index.lock().await;
        let status = index.status().map_err(index_error)?;
        let mut text = format::index_status(&status, verbose_dependencies);
        if let Some(name) = optional_arg(dependency.as_ref()) {
            text.push_str(&format::dependency_declarations(&status, name));
        }
        // The query cache's counters, once it has answered anything — an idle
        // or disabled cache leaves the report as it always was.
        let (config, stats) = {
            let cache = self.cache_guard();
            (cache.config().clone(), cache.stats())
        };
        if config.enabled && stats.exact_hit + stats.exact_miss > 0 {
            text = format!(
                "{}\n\nQuery cache (semantic {}, threshold {:.2}, margin {:.2}):\n{stats}\n",
                text.trim_end(),
                if config.semantic { "on" } else { "off" },
                config.threshold,
                config.min_margin
            );
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
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
        let all = index.list_symbols(path, None, None).map_err(index_error)?;
        let shape = file_shape(&all);
        let entries: Vec<_> = all
            .into_iter()
            // Not every top-level declaration lacks a parent: Go, C#, Bash
            // and PowerShell parent theirs to the package, namespace or file
            // module (see `is_top_level`). The module entry itself is
            // also dropped: the user asked for classes/structs/interfaces/
            // functions/types, not the synthetic whole-file wrapper.
            .filter(|entry| entry.kind != "module" && is_top_level(entry, &shape))
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
            let shape = file_shape(&entries);
            let mut candidates: Vec<mct_index::SymbolListEntry> = entries
                .into_iter()
                .filter(|e| {
                    is_top_level(e, &shape) && OVERVIEW_KIND_ALLOWLIST.contains(&e.kind.as_str())
                })
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
            format::overview(path.unwrap_or("."), &digests, max_symbols_per_module as u32),
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
            OutputFormat::Text => {
                format::find_dead_code(path.unwrap_or("."), &candidates, offset, limit)
            }
            OutputFormat::Toon => {
                format::find_dead_code_toon(path.unwrap_or("."), &candidates, offset, limit)
            }
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Module import graph with cycles; see tools.ttc")]
    pub async fn get_dependency_graph(
        &self,
        Parameters(GetDependencyGraphArgs {
            module,
            depth,
            limit,
            export,
            format,
        }): Parameters<GetDependencyGraphArgs>,
    ) -> Result<CallToolResult, McpError> {
        let module = validate_module_path(&module)?;
        let depth = depth.unwrap_or(2).clamp(1, mct_core::MAX_QUERY_DEPTH);
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let output_format = parse_output_format(format.as_deref())?;
        let export = match export.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
            None => None,
            Some("dot") => Some("dot"),
            Some("mermaid") => Some("mermaid"),
            Some(other) => {
                return Err(McpError::invalid_params(
                    format!("export must be one of dot, mermaid — got `{other}`"),
                    None,
                ))
            }
        };

        let graph = {
            let index = self.index.lock().await;
            index
                .dependency_graph(&module, depth, limit)
                .map_err(file_tree_error)?
        };
        let edges: Vec<(String, String)> = graph
            .edges
            .iter()
            .map(|e| (e.from.clone(), e.to.clone()))
            .collect();
        let cycles = mct_index::find_cycles(&edges);
        let rendered = export.map(|kind| {
            let body = match kind {
                "dot" => crate::graph_render::render_dot(&graph.root, &edges, &cycles),
                _ => crate::graph_render::render_mermaid(&graph.root, &edges, &cycles),
            };
            (kind, body)
        });
        let export_arg = rendered.as_ref().map(|(k, b)| (*k, b.as_str()));
        let text = match output_format {
            OutputFormat::Text => format::dependency_graph(&graph, &cycles, export_arg),
            OutputFormat::Toon => format::dependency_graph_toon(&graph, &cycles, export_arg),
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
                format!("no tool named `{name}` — call discover_tool_categories for the full list"),
                None,
            )
        })?;
        let contract = ttc::parse(ttc::CATALOG_SOURCE)
            .ok()
            .and_then(|entries| entries.get(name).map(ttc::TtcEntry::contract))
            .unwrap_or_else(|| "(contract unavailable)".to_string());
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "{}\nContract:\n{contract}\n",
            format::tool_schema(tool)
        ))]))
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Pick the tools for one user request; see tools.ttc")]
    pub async fn select_tools(
        &self,
        Parameters(SelectToolsArgs { query }): Parameters<SelectToolsArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let result = self.attend(&query).await?;
        // Best effort: a client that ignores it still gets the names in the
        // reply, and every tool stays callable.
        let _ = context.peer.notify_tool_list_changed().await;
        Ok(result)
    }

    // Fallback only — see tools.ttc / MctServer::new.
    #[tool(description = "Run several read-only queries in one call; see tools.ttc")]
    pub async fn batch(
        &self,
        Parameters(BatchArgs { queries }): Parameters<BatchArgs>,
    ) -> Result<CallToolResult, McpError> {
        if queries.is_empty() {
            return Err(McpError::invalid_params(
                "`queries` must not be empty",
                None,
            ));
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

// `router = self.tool_router`, not the macro's default `Self::tool_router()`:
// the default rebuilds the router from the `#[tool]` attributes on every
// request and serves their fallback descriptions (issue #70).
#[tool_handler(router = self.tool_router)]
impl ServerHandler for MctServer {
    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, McpError> {
        Ok(rmcp::model::ListToolsResult {
            tools: self.advertised_tools(),
            ..Default::default()
        })
    }

    // The macro's default `call_tool`, plus the POST contract check — and,
    // first, the current ignore rules applied to the index.
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, McpError> {
        self.apply_exclusion_changes().await?;
        let tool = request.name.clone();
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        let response = self.tool_router.call(call).await?;
        if let rmcp::model::CallToolResponse::Complete(result) = &response {
            check_postcondition(&tool, result)?;
        }
        Ok(response)
    }

    fn get_info(&self) -> InitializeResult {
        let tools = ServerCapabilities::builder().enable_tools();
        let (capabilities, attention) = if self.attention.is_some() {
            (
                tools.enable_tool_list_changed().build(),
                "Tool Attention is on: tools/list starts with select_tools only. Call \
                 select_tools with the user's request first; it narrows tools/list to the tools \
                 that request needs and sends tools/list_changed. Every tool stays callable by \
                 name. ",
            )
        } else {
            (tools.build(), "")
        };
        InitializeResult::new(capabilities)
            .with_server_info(Implementation::from_build_env())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(format!(
                "{attention}Indexes this repository's source code (any of the supported languages, including \
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
             impact_analysis before deleting anything. build_context_pack is the composite \
             for working on one symbol: its definition with doc comment and capped source, \
             plus every caller, callee, dependency and related test listed once with its \
             signature — one call in place of find_symbol + find_calls + find_callers + \
             impact_analysis + reading the files. reindex and get_indexing_status \
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
            ))
    }
}

#[cfg(test)]
mod contract_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn postcondition_rejects_an_empty_success_only() {
        let empty = CallToolResult::success(vec![ContentBlock::text("  ")]);
        let err = check_postcondition("find_symbol", &empty).unwrap_err();
        assert!(
            err.message.contains("broke its contract"),
            "{}",
            err.message
        );

        assert!(
            check_postcondition("t", &CallToolResult::success(vec![ContentBlock::text("x")]))
                .is_ok()
        );
        assert!(check_postcondition("t", &CallToolResult::error(vec![])).is_ok());
    }
}
