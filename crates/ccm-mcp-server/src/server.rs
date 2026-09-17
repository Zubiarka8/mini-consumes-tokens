use std::sync::Arc;

use ccm_core::LanguageRegistry;
use ccm_index::Index;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, InitializeResult, ProtocolVersion, ServerCapabilities},
    model::{CallToolResult, ContentBlock},
    tool, tool_handler, tool_router,
};
use tokio::sync::Mutex;

use crate::format;

/// Applied to any of the 5 result-returning tools below when their `limit`
/// argument is omitted. 50 is a compromise: generous enough that the common
/// case (a handful to a few dozen hits) never gets truncated, but small
/// enough that a symbol with hundreds of hits in a large repo can't blow up
/// a single response to a size that rivals the `grep` output this project
/// exists to replace.
const DEFAULT_RESULT_LIMIT: usize = 50;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListSymbolsArgs {
    /// A single file's path (e.g. `crates/ccm-lang-html/src/lib.rs`), or a
    /// directory/crate prefix with no file extension (e.g.
    /// `crates/ccm-lang-html/src`) to list every symbol under it. Relative to
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
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindSymbolArgs {
    /// Exact symbol name to look up (e.g. a function, class, struct or method name).
    pub name: String,
    /// Maximum number of definitions to return. Defaults to 50 when omitted;
    /// raise it if you expect more hits and want them all in one call.
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindReferencesArgs {
    /// Exact name of the symbol to find every reference to.
    pub symbol: String,
    /// Maximum number of references to return. Defaults to 50 when omitted;
    /// raise it if you expect more hits and want them all in one call.
    #[serde(default)]
    pub limit: Option<usize>,
    /// How many relation-graph hops to walk beyond the direct/first hop. 1
    /// (default) is the original single-hop behavior — direct referrers
    /// only. Raising it also includes referrers-of-referrers, each tagged
    /// with its hop number. Clamped to `ccm_core::MAX_QUERY_DEPTH`.
    #[serde(default)]
    pub depth: Option<u32>,
    /// Number of leading results to skip before applying `limit`, for
    /// paging through result sets larger than one `limit` page. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindCallsArgs {
    /// Exact name of the function/method whose callees you want.
    pub function: String,
    /// Maximum number of calls to return. Defaults to 50 when omitted;
    /// raise it if you expect more hits and want them all in one call.
    #[serde(default)]
    pub limit: Option<usize>,
    /// How many call-graph hops to walk forward beyond the direct/first
    /// hop. 1 (default) is the original single-hop behavior — direct
    /// callees only. Raising it also includes callees-of-callees, each
    /// tagged with its hop number. Clamped to `ccm_core::MAX_QUERY_DEPTH`.
    #[serde(default)]
    pub depth: Option<u32>,
    /// Number of leading results to skip before applying `limit`, for
    /// paging through result sets larger than one `limit` page. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindCallersArgs {
    /// Exact name of the function/method whose callers you want.
    pub function: String,
    /// Maximum number of callers to return. Defaults to 50 when omitted;
    /// raise it if you expect more hits and want them all in one call.
    #[serde(default)]
    pub limit: Option<usize>,
    /// How many call-graph hops to walk backward beyond the direct/first
    /// hop. 1 (default) is the original single-hop behavior — direct
    /// callers only. Raising it also includes callers-of-callers, each
    /// tagged with its hop number. Clamped to `ccm_core::MAX_QUERY_DEPTH`.
    #[serde(default)]
    pub depth: Option<u32>,
    /// Number of leading results to skip before applying `limit`, for
    /// paging through result sets larger than one `limit` page. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ImpactAnalysisArgs {
    /// Exact name of the symbol you're considering changing or removing.
    pub symbol: String,
    /// Maximum number of entries to return per section (callers/references/
    /// affected tests are each capped independently). Defaults to 50 when
    /// omitted.
    #[serde(default)]
    pub limit: Option<usize>,
    /// How many relation-graph hops to walk beyond the direct/first hop,
    /// applied to both the caller and reference sections. 1 (default) is
    /// the original single-hop behavior. Clamped to `ccm_core::MAX_QUERY_DEPTH`.
    #[serde(default)]
    pub depth: Option<u32>,
    /// Number of leading results to skip before applying `limit`, applied
    /// independently to each section. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct ReindexArgs {
    /// Re-parse every supported file, even if its content hash hasn't
    /// changed since the last run. Defaults to false (incremental).
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetFileSkeletonArgs {
    /// A single file's path (e.g. `crates/ccm-lang-html/src/lib.rs`),
    /// relative to the project root, forward slashes on any OS. Must be a
    /// file, not a directory/crate prefix — use list_symbols first if you
    /// don't already know which file you need.
    pub path: String,
}

#[derive(Clone)]
pub struct CcmServer {
    index: Arc<Mutex<Index>>,
    registry: LanguageRegistry,
    // Read by the #[tool_handler] macro expansion below, not by hand-written
    // code — dead_code can't see that use.
    #[allow(dead_code)]
    tool_router: ToolRouter<CcmServer>,
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

fn index_error(err: ccm_index::IndexError) -> McpError {
    McpError::internal_error(err.to_string(), None)
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

#[tool_router]
impl CcmServer {
    pub fn new(index: Index, registry: LanguageRegistry) -> Self {
        Self {
            index: Arc::new(Mutex::new(index)),
            registry,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "DISCOVERY, not a precise lookup — use this FIRST when you don't know a symbol's exact name yet. Lists symbol definitions (name, kind, line range) found under `path`: a single file (exact path) or a directory/crate (a path with no file extension, matched as a prefix), optionally narrowed to one symbol `kind` and/or one `language`. Answers \"what functions/structs/classes does this file or crate have\" without already knowing a name. Do NOT use this to locate one already-known symbol precisely, or to jump straight to its definition — use find_symbol for that; list_symbols is the discovery step that feeds find_symbol/find_references/find_calls/find_callers/impact_analysis, not a replacement for them."
    )]
    pub async fn list_symbols(
        &self,
        Parameters(ListSymbolsArgs {
            path,
            kind,
            language,
            limit,
        }): Parameters<ListSymbolsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = validate_name(&path)?;
        // Same file-vs-directory heuristic the query layer uses, computed
        // here too so the formatter knows whether to print each entry's
        // `relative_path` (needed once a directory/crate spans several
        // files) or omit it (redundant for a single-file listing).
        let is_file = path.rsplit('/').next().unwrap_or(path).contains('.');
        let kind = kind.as_deref().map(str::trim).filter(|k| !k.is_empty());
        let language = language.as_deref().map(str::trim).filter(|l| !l.is_empty());
        let index = self.index.lock().await;
        let hits = index
            .list_symbols(path, kind, language)
            .map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::list_symbols(path, is_file, &hits, limit.unwrap_or(DEFAULT_RESULT_LIMIT)),
        )]))
    }

    #[tool(
        description = "ATOMIC lookup. Find the definition location(s) of a symbol by exact name, across every indexed language in this repository (including polyglot projects). Use this instead of grepping files to answer \"where is X defined\". Do NOT use this to find who calls or references a symbol — use find_callers (direct callers only) or find_references (every reference) instead."
    )]
    pub async fn find_symbol(
        &self,
        Parameters(FindSymbolArgs { name, limit }): Parameters<FindSymbolArgs>,
    ) -> Result<CallToolResult, McpError> {
        let name = validate_name(&name)?;
        let index = self.index.lock().await;
        let hits = index.find_symbol(name).map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::symbol_hits(name, &hits, limit.unwrap_or(DEFAULT_RESULT_LIMIT)),
        )]))
    }

    #[tool(
        description = "ATOMIC lookup. Find every place a symbol is referenced: calls, imports, extends/implements, and plain references — the broadest reference search. Do NOT use this when you only want the functions that directly call a function — use find_callers instead, which is narrower and answers that question directly. Do NOT use this to find a symbol's own definition — use find_symbol."
    )]
    pub async fn find_references(
        &self,
        Parameters(FindReferencesArgs {
            symbol,
            limit,
            depth,
            offset,
        }): Parameters<FindReferencesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let symbol = validate_name(&symbol)?;
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let offset = offset.unwrap_or(0);
        let index = self.index.lock().await;
        let hits = index
            .find_references_bfs(symbol, depth.unwrap_or(1), limit, offset)
            .map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::relation_hits(symbol, "reference(s)", &hits, offset, limit),
        )]))
    }

    #[tool(
        description = "ATOMIC lookup. Find the functions/methods called by the given function — its callees. Answers \"what does this function call\". Do NOT use this to find who calls the function — that's the inverse question, answered by find_callers."
    )]
    pub async fn find_calls(
        &self,
        Parameters(FindCallsArgs {
            function,
            limit,
            depth,
            offset,
        }): Parameters<FindCallsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let function = validate_name(&function)?;
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let offset = offset.unwrap_or(0);
        let index = self.index.lock().await;
        let hits = index
            .find_calls_bfs(function, depth.unwrap_or(1), limit, offset)
            .map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::relation_hits(function, "call(s) made by this function", &hits, offset, limit),
        )]))
    }

    #[tool(
        description = "ATOMIC lookup. Find the functions/methods that call the given function — its callers. Answers \"who calls this function\"; the inverse of find_calls. Do NOT use this for a broader reference search (imports, extends/implements, non-call references) — use find_references instead. If you're about to change or remove this function and want its full blast radius (callers + references + likely tests) in one call, use impact_analysis instead of calling this separately."
    )]
    pub async fn find_callers(
        &self,
        Parameters(FindCallersArgs {
            function,
            limit,
            depth,
            offset,
        }): Parameters<FindCallersArgs>,
    ) -> Result<CallToolResult, McpError> {
        let function = validate_name(&function)?;
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let offset = offset.unwrap_or(0);
        let index = self.index.lock().await;
        let hits = index
            .find_callers_bfs(function, depth.unwrap_or(1), limit, offset)
            .map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::relation_hits(function, "caller(s) of this function", &hits, offset, limit),
        )]))
    }

    #[tool(
        description = "COMPOSITE (internally combines find_callers + find_references + a test-name heuristic — you do not need to call those separately). Reports everything a change to `symbol` could break: its direct callers, its full reference set (calls/imports/extends/implements/plain), and which of those look like tests (name starting with `test`). Use this before editing or removing a symbol to gauge blast radius in one call. Do NOT use this for a plain lookup of only direct callers or only references — that's cheaper via find_callers or find_references alone, and this tool's output is more verbose."
    )]
    pub async fn impact_analysis(
        &self,
        Parameters(ImpactAnalysisArgs {
            symbol,
            limit,
            depth,
            offset,
        }): Parameters<ImpactAnalysisArgs>,
    ) -> Result<CallToolResult, McpError> {
        let symbol = validate_name(&symbol)?;
        let limit = limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        let offset = offset.unwrap_or(0);
        let depth = depth.unwrap_or(1);
        let (callers, references) = {
            let index = self.index.lock().await;
            let callers = index
                .find_callers_bfs(symbol, depth, limit, offset)
                .map_err(index_error)?;
            let references = index
                .find_references_bfs(symbol, depth, limit, offset)
                .map_err(index_error)?;
            (callers, references)
        };
        // A test caller shows up in both `callers` and `references` (the
        // latter is a superset); dedupe by caller name so it's only counted
        // once regardless of how many relation kinds connect it to `symbol`.
        let mut seen_test_names = std::collections::HashSet::new();
        let affected_tests: Vec<&ccm_index::RelationHit> = references
            .iter()
            .chain(callers.iter())
            .filter(|hit| format::looks_like_test_name(&hit.from_symbol))
            .filter(|hit| seen_test_names.insert(hit.from_symbol.as_str()))
            .collect();
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::impact_analysis(symbol, &callers, &references, &affected_tests, offset, limit),
        )]))
    }

    #[tool(
        description = "INDEX ADMINISTRATION, not a search tool — returns a reindex summary, not symbol data. Re-scans the project and updates the index; only files whose content changed since the last run are re-parsed unless force=true. It already runs automatically at server startup, so call this manually only if you suspect the index is stale (e.g. after changes made outside this session)."
    )]
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

    #[tool(
        description = "INDEX ADMINISTRATION, not a search tool. Reports index health: files/symbols indexed per language, when it was last indexed, languages seen in the repo with no parser plugin yet, files that failed to parse, and declared dependencies detected from manifest files (Cargo.toml, package.json, requirements.txt, go.mod). Do NOT use this to search for a symbol — it returns no symbol data, only index diagnostics; use find_symbol instead."
    )]
    pub async fn get_indexing_status(&self) -> Result<CallToolResult, McpError> {
        let index = self.index.lock().await;
        let status = index.status().map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::index_status(&status),
        )]))
    }

    #[tool(
        description = "TOKEN-SAVING module overview. Returns a file's top-level declarations (functions, classes, structs, interfaces, types) with bodies collapsed to `// ...` — up to ~90% fewer tokens than reading the whole file when you just need its shape. Brace-delimited languages (Rust, Go, Java, C++, C#, PHP, JS/TS, Kotlin) get precise body elision; other languages (e.g. Python, Lua, Bash, PowerShell) get a best-effort declaration-line-only rendering. Nested members (e.g. methods inside a class) are NOT shown individually — a class/struct/interface collapses to one block regardless of what's inside it. Do NOT use this for a directory/crate — it takes a single file path; use list_symbols for that. Do NOT use this when you need a symbol's actual implementation, not just its shape — use find_symbol to locate it, then read the file directly."
    )]
    pub async fn get_file_skeleton(
        &self,
        Parameters(GetFileSkeletonArgs { path }): Parameters<GetFileSkeletonArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = validate_name(&path)?;
        let is_file = path.rsplit('/').next().unwrap_or(path).contains('.');
        if !is_file {
            return Err(McpError::invalid_params(
                "get_file_skeleton takes a single file path, not a directory/crate prefix — use list_symbols first if you don't know which file you need",
                None,
            ));
        }
        let index = self.index.lock().await;
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
}

#[tool_handler]
impl ServerHandler for CcmServer {
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
             reading a whole file when you only need its shape. reindex and get_indexing_status \
             are index maintenance, not search — they never return symbol data. The index \
             refreshes automatically at startup; call reindex manually only if you suspect it's \
             stale."
                .to_string(),
        )
    }
}
