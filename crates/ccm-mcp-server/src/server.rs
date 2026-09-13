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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindSymbolArgs {
    /// Exact symbol name to look up (e.g. a function, class, struct or method name).
    pub name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindReferencesArgs {
    /// Exact name of the symbol to find every reference to.
    pub symbol: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindCallsArgs {
    /// Exact name of the function/method whose callees you want.
    pub function: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FindCallersArgs {
    /// Exact name of the function/method whose callers you want.
    pub function: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ImpactAnalysisArgs {
    /// Exact name of the symbol you're considering changing or removing.
    pub symbol: String,
}

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct ReindexArgs {
    /// Re-parse every supported file, even if its content hash hasn't
    /// changed since the last run. Defaults to false (incremental).
    #[serde(default)]
    pub force: bool,
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
        description = "ATOMIC lookup. Find the definition location(s) of a symbol by exact name, across every indexed language in this repository (including polyglot projects). Use this instead of grepping files to answer \"where is X defined\". Do NOT use this to find who calls or references a symbol — use find_callers (direct callers only) or find_references (every reference) instead."
    )]
    pub async fn find_symbol(
        &self,
        Parameters(FindSymbolArgs { name }): Parameters<FindSymbolArgs>,
    ) -> Result<CallToolResult, McpError> {
        let name = validate_name(&name)?;
        let index = self.index.lock().await;
        let hits = index.find_symbol(name).map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::symbol_hits(name, &hits),
        )]))
    }

    #[tool(
        description = "ATOMIC lookup. Find every place a symbol is referenced: calls, imports, extends/implements, and plain references — the broadest reference search. Do NOT use this when you only want the functions that directly call a function — use find_callers instead, which is narrower and answers that question directly. Do NOT use this to find a symbol's own definition — use find_symbol."
    )]
    pub async fn find_references(
        &self,
        Parameters(FindReferencesArgs { symbol }): Parameters<FindReferencesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let symbol = validate_name(&symbol)?;
        let index = self.index.lock().await;
        let hits = index.find_references(symbol).map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::relation_hits(symbol, "reference(s)", &hits),
        )]))
    }

    #[tool(
        description = "ATOMIC lookup. Find the functions/methods called by the given function — its callees. Answers \"what does this function call\". Do NOT use this to find who calls the function — that's the inverse question, answered by find_callers."
    )]
    pub async fn find_calls(
        &self,
        Parameters(FindCallsArgs { function }): Parameters<FindCallsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let function = validate_name(&function)?;
        let index = self.index.lock().await;
        let hits = index.find_calls(function).map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::relation_hits(function, "call(s) made by this function", &hits),
        )]))
    }

    #[tool(
        description = "ATOMIC lookup. Find the functions/methods that call the given function — its callers. Answers \"who calls this function\"; the inverse of find_calls. Do NOT use this for a broader reference search (imports, extends/implements, non-call references) — use find_references instead. If you're about to change or remove this function and want its full blast radius (callers + references + likely tests) in one call, use impact_analysis instead of calling this separately."
    )]
    pub async fn find_callers(
        &self,
        Parameters(FindCallersArgs { function }): Parameters<FindCallersArgs>,
    ) -> Result<CallToolResult, McpError> {
        let function = validate_name(&function)?;
        let index = self.index.lock().await;
        let hits = index.find_callers(function).map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::relation_hits(function, "caller(s) of this function", &hits),
        )]))
    }

    #[tool(
        description = "COMPOSITE (internally combines find_callers + find_references + a test-name heuristic — you do not need to call those separately). Reports everything a change to `symbol` could break: its direct callers, its full reference set (calls/imports/extends/implements/plain), and which of those look like tests (name starting with `test`). Use this before editing or removing a symbol to gauge blast radius in one call. Do NOT use this for a plain lookup of only direct callers or only references — that's cheaper via find_callers or find_references alone, and this tool's output is more verbose."
    )]
    pub async fn impact_analysis(
        &self,
        Parameters(ImpactAnalysisArgs { symbol }): Parameters<ImpactAnalysisArgs>,
    ) -> Result<CallToolResult, McpError> {
        let symbol = validate_name(&symbol)?;
        let (callers, references) = {
            let index = self.index.lock().await;
            let callers = index.find_callers(symbol).map_err(index_error)?;
            let references = index.find_references(symbol).map_err(index_error)?;
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
            format::impact_analysis(symbol, &callers, &references, &affected_tests),
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
        description = "INDEX ADMINISTRATION, not a search tool. Reports index health: files/symbols indexed per language, when it was last indexed, languages seen in the repo with no parser plugin yet, and files that failed to parse. Do NOT use this to search for a symbol — it returns no symbol data, only index diagnostics; use find_symbol instead."
    )]
    pub async fn get_indexing_status(&self) -> Result<CallToolResult, McpError> {
        let index = self.index.lock().await;
        let status = index.status().map_err(index_error)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            format::index_status(&status),
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
             relationships — it costs far fewer tokens. Search tools (find_symbol, find_calls, \
             find_callers, find_references) are atomic lookups, each answering one specific \
             question — see each tool's description for which one to use and which NOT to. \
             impact_analysis is composite: it combines find_callers + find_references + a test \
             heuristic internally, for when you need the full blast radius of a change in one \
             call. reindex and get_indexing_status are index maintenance, not search — they \
             never return symbol data. The index refreshes automatically at startup; call \
             reindex manually only if you suspect it's stale."
                .to_string(),
        )
    }
}
