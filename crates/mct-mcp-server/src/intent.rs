//! Structured intent extraction and tool preselection (issue #19).
//!
//! [`preselect`] turns a natural-language request into an [`Intent`] (which
//! actions, on which textual references, with which explicit parameters and
//! exclusions) using deterministic word rules, and maps it onto tools that
//! exist in the live catalog ([`crate::server::MctServer::tool_catalog`]).
//! When the rules find no action, an optional [`Embedder`] ranks the
//! catalog's own descriptions against the query; when that is unavailable or
//! not decisive, the result falls back to the full catalog — the flow every
//! client uses today — rather than guessing.
//!
//! The output is advisory: a preselected tool is only a schema shown to the
//! model, never a call, and nothing here executes or resolves anything. A
//! reference such as `parse` stays text; `find_symbol` returns every matching
//! definition. The original query always travels with the intent, so
//! constraints the rules cannot represent are not lost.
//!
//! Not yet wired into a request path: the MCP server never sees the user's
//! query before the model does. The consumer is the per-turn tool filter of
//! issue #11 (Tool Attention), still to be built.

use mct_index::{classify_query, Embedder, QueryIntent};
use rmcp::model::Tool;
use serde_json::{Map, Value};

/// Longer input is truncated (on a char boundary) before any rule runs.
pub const MAX_QUERY_CHARS: usize = 1000;

/// Embedding fallback: the best-ranked tool is kept only if its
/// description's cosine similarity to the query reaches this floor. A
/// similarity, not a calibrated probability. Set on the development queries
/// in `tests/intent_eval.rs` (bge-small-en-v1.5): non-code requests topped
/// out at 0.589, so 0.60 rejects all of them; top-1/top-2 gaps were under
/// 0.03, so only the best tool is kept.
pub const SIMILARITY_FLOOR: f32 = 0.60;

/// One thing the request asks for, each served by existing tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Callers,
    Callees,
    References,
    Impact,
    Definition,
    Search,
    List,
    Skeleton,
    Overview,
    Tree,
    Context,
    DeadCode,
    Status,
    Reindex,
    Catalog,
    Schema,
    Batch,
}

impl Action {
    /// Catalog tools that answer this action, best first.
    pub fn tools(self) -> &'static [&'static str] {
        match self {
            Action::Callers => &["find_callers"],
            Action::Callees => &["find_calls"],
            Action::References => &["find_references"],
            Action::Impact => &["impact_analysis"],
            Action::Definition => &["find_symbol"],
            Action::Search => &["hybrid_search", "search_symbols"],
            Action::List => &["list_symbols"],
            Action::Skeleton => &["get_file_skeleton"],
            Action::Overview => &["get_project_overview"],
            Action::Tree => &["get_file_tree"],
            Action::Context => &["build_context_pack"],
            Action::DeadCode => &["find_dead_code"],
            Action::Status => &["get_indexing_status"],
            Action::Reindex => &["reindex"],
            Action::Catalog => &["discover_tool_categories"],
            Action::Schema => &["get_tool_schema"],
            Action::Batch => &["batch"],
        }
    }

    fn label(self) -> &'static str {
        match self {
            Action::Callers => "callers",
            Action::Callees => "callees",
            Action::References => "references",
            Action::Impact => "impact",
            Action::Definition => "definition",
            Action::Search => "search",
            Action::List => "list",
            Action::Skeleton => "skeleton",
            Action::Overview => "overview",
            Action::Tree => "tree",
            Action::Context => "context",
            Action::DeadCode => "dead_code",
            Action::Status => "status",
            Action::Reindex => "reindex",
            Action::Catalog => "catalog",
            Action::Schema => "schema",
            Action::Batch => "batch",
        }
    }

    /// Whether the action is about one named symbol.
    fn takes_target(self) -> bool {
        matches!(
            self,
            Action::Callers
                | Action::Callees
                | Action::References
                | Action::Impact
                | Action::Definition
                | Action::Context
        )
    }
}

/// Tools offered when the request is only a code-shaped name: the action is
/// unknown, so keep the plausible lookups instead of picking one.
const LOOKUP_ALTERNATIVES: &[&str] = &["find_symbol", "search_symbols", "build_context_pack"];

/// One requested action and the textual reference it applies to, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub action: Action,
    pub target: Option<String>,
}

/// How the tool selection was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Word rules matched at least one action.
    Rules,
    /// Rules found only a code-shaped name; the lookup alternatives are kept.
    AmbiguousName,
    /// Rules found nothing; the embedding ranking was decisive.
    Embedding,
    /// Nothing reliable: the full catalog, as without preselection.
    Fallback,
}

/// The structured reading of one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intent {
    /// Requested actions, in the order they appear.
    pub steps: Vec<Step>,
    /// Actions the request explicitly rules out ("don't reindex").
    pub excluded: Vec<Action>,
    /// Exact textual references (identifiers, quoted names), unresolved.
    pub refs: Vec<String>,
    pub path: Option<String>,
    pub kind: Option<String>,
    pub language: Option<String>,
    pub depth: Option<u32>,
    pub limit: Option<u32>,
}

impl Intent {
    /// One compact line for the model, e.g.
    /// `intent: callers(parse_config) callees(parse_config); not: reindex; depth=2`.
    pub fn render(&self, source: Source) -> String {
        let mut out = String::from("intent:");
        if self.steps.is_empty() {
            match self.refs.first() {
                Some(name) if source == Source::AmbiguousName => {
                    out.push_str(&format!(" lookup?({name})"))
                }
                _ => out.push_str(" unknown"),
            }
        }
        for step in &self.steps {
            out.push(' ');
            out.push_str(step.action.label());
            if let Some(target) = &step.target {
                out.push_str(&format!("({target})"));
            }
        }
        if !self.excluded.is_empty() {
            let labels: Vec<&str> = self.excluded.iter().map(|a| a.label()).collect();
            out.push_str(&format!("; not: {}", labels.join(",")));
        }
        for (key, value) in [
            ("path", self.path.clone()),
            ("kind", self.kind.clone()),
            ("language", self.language.clone()),
            ("depth", self.depth.map(|d| d.to_string())),
            ("limit", self.limit.map(|l| l.to_string())),
        ] {
            if let Some(value) = value {
                out.push_str(&format!("; {key}={value}"));
            }
        }
        let source = match source {
            Source::Rules => "rules",
            Source::AmbiguousName => "ambiguous",
            Source::Embedding => "embedding",
            Source::Fallback => "fallback",
        };
        out.push_str(&format!("; src={source}"));
        out
    }
}

/// Tools to put in front of the model for one request.
#[derive(Debug, Clone)]
pub struct Preselection {
    pub intent: Intent,
    pub source: Source,
    /// Catalog tool names, best first; every catalog tool on fallback.
    pub tools: Vec<String>,
    /// Suggested arguments per selected tool, only for parameters that
    /// tool's input schema declares. Never executed here.
    pub hints: Vec<(String, Map<String, Value>)>,
}

impl Preselection {
    /// The selected tools' catalog entries, in selection order.
    pub fn catalog_entries<'a>(&self, catalog: &'a [Tool]) -> Vec<&'a Tool> {
        self.tools
            .iter()
            .filter_map(|name| catalog.iter().find(|t| t.name == name.as_str()))
            .collect()
    }
}

/// Catalog descriptions embedded once, for ranking queries against them.
pub struct ToolVectors<'a> {
    embedder: &'a dyn Embedder,
    tools: Vec<(String, Vec<f32>)>,
}

impl<'a> ToolVectors<'a> {
    pub fn new(embedder: &'a dyn Embedder, catalog: &[Tool]) -> Result<Self, String> {
        let texts: Vec<String> = catalog
            .iter()
            .map(|t| format!("{}: {}", t.name, t.description.as_deref().unwrap_or("")))
            .collect();
        let vectors = embedder.embed(&texts)?;
        if vectors.len() != catalog.len() {
            return Err("embedder returned the wrong number of vectors".to_string());
        }
        let tools = catalog
            .iter()
            .map(|t| t.name.to_string())
            .zip(vectors)
            .collect();
        Ok(Self { embedder, tools })
    }

    /// Every catalog tool with its cosine similarity to `query`, best first.
    pub fn rank(&self, query: &str) -> Result<Vec<(String, f32)>, String> {
        let q = self.embedder.embed_query(query)?;
        let mut ranked: Vec<(String, f32)> = self
            .tools
            .iter()
            .map(|(name, v)| (name.clone(), cosine(&q, v)))
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        Ok(ranked)
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// Extracts the intent of `query` and preselects catalog tools for it.
/// `languages` are the registry's language ids
/// ([`mct_core::LanguageRegistry::language_ids`]); `vectors`, when given,
/// enables the embedding fallback. Never panics on any input.
pub fn preselect(
    query: &str,
    languages: &[&str],
    catalog: &[Tool],
    vectors: Option<&ToolVectors>,
) -> Preselection {
    let intent = extract(query, languages);
    let available = |name: &str| catalog.iter().any(|t| t.name == name);
    let excluded: Vec<&str> = intent
        .excluded
        .iter()
        .flat_map(|a| a.tools().iter().copied())
        .collect();

    let mut tools: Vec<String> = Vec::new();
    let push = |name: &str, tools: &mut Vec<String>| {
        if available(name) && !excluded.contains(&name) && !tools.iter().any(|t| t == name) {
            tools.push(name.to_string());
        }
    };
    let mut source = Source::Rules;
    if !intent.steps.is_empty() {
        for step in &intent.steps {
            for name in step.action.tools() {
                push(name, &mut tools);
            }
        }
    } else if !intent.refs.is_empty() {
        source = Source::AmbiguousName;
        for name in LOOKUP_ALTERNATIVES {
            push(name, &mut tools);
        }
    } else if let Some(ranked) = vectors.and_then(|v| v.rank(&truncate(query)).ok()) {
        // An embedding error lands here as `None`: same as no model.
        if let Some((name, score)) = ranked.first() {
            if *score >= SIMILARITY_FLOOR {
                push(name, &mut tools);
                source = Source::Embedding;
            }
        }
    }
    if tools.is_empty() {
        source = Source::Fallback;
        for tool in catalog {
            push(&tool.name, &mut tools);
        }
    }

    let hints = if source == Source::Fallback {
        Vec::new()
    } else {
        tools
            .iter()
            .filter_map(|name| {
                let tool = catalog.iter().find(|t| t.name == name.as_str())?;
                let args = hint_args(&intent, tool, query);
                (!args.is_empty()).then(|| (name.clone(), args))
            })
            .collect()
    };
    Preselection {
        intent,
        source,
        tools,
        hints,
    }
}

/// Arguments the intent supplies for `tool`, restricted to the properties
/// its input schema declares.
fn hint_args(intent: &Intent, tool: &Tool, query: &str) -> Map<String, Value> {
    let mut args = Map::new();
    let Some(Value::Object(props)) = tool.input_schema.get("properties") else {
        return args;
    };
    let target = intent
        .steps
        .iter()
        .find(|s| s.action.tools().contains(&&*tool.name))
        .and_then(|s| s.target.clone())
        .or_else(|| intent.refs.first().cloned());
    let name_key = ["function", "symbol", "name"]
        .into_iter()
        .find(|k| props.contains_key(*k));
    if let (Some(target), Some(key)) = (target, name_key) {
        args.insert(key.to_string(), Value::String(target));
    }
    if props.contains_key("query") {
        args.insert("query".to_string(), Value::String(truncate(query)));
    }
    let mut put = |key: &str, value: Option<Value>| match value {
        Some(value) if props.contains_key(key) => {
            args.insert(key.to_string(), value);
        }
        _ => {}
    };
    put("path", intent.path.clone().map(Value::String));
    put("kind", intent.kind.clone().map(Value::String));
    put("language", intent.language.clone().map(Value::String));
    put("depth", intent.depth.map(Value::from));
    put("limit", intent.limit.map(Value::from));
    put("top_k", intent.limit.map(Value::from));
    args
}

fn truncate(query: &str) -> String {
    query.chars().take(MAX_QUERY_CHARS).collect()
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

/// Multi-word triggers, matched before single words (longest first wins
/// because the table is scanned in order and matched tokens are consumed).
const PHRASES: &[(&[&str], Action)] = &[
    (&["never", "called"], Action::DeadCode),
    (&["never", "used"], Action::DeadCode),
    (&["never", "referenced"], Action::DeadCode),
    (&["calls", "made", "by"], Action::Callees),
    (&["calls", "from"], Action::Callees),
    (&["called", "by"], Action::Callers),
    (&["calls", "to"], Action::Callers),
    (&["who", "calls"], Action::Callers),
    (&["who", "call"], Action::Callers),
    (&["who", "invokes"], Action::Callers),
    (&["who", "uses"], Action::References),
    (&["what", "tools"], Action::Catalog),
    (&["which", "tools"], Action::Catalog),
    (&["available", "tools"], Action::Catalog),
    (&["what", "can", "you", "do"], Action::Catalog),
    (&["blast", "radius"], Action::Impact),
    (&["safe", "to"], Action::Impact),
    (&["depends", "on"], Action::Impact),
    (&["depend", "on"], Action::Impact),
    (&["up", "to", "date"], Action::Status),
    (&["about", "to"], Action::Context),
    (&["work", "on"], Action::Context),
    (&["everything", "about"], Action::Context),
    (&["folder", "structure"], Action::Tree),
    (&["directory", "structure"], Action::Tree),
    (&["file", "tree"], Action::Tree),
    (&["code", "that"], Action::Search),
    (&["function", "that"], Action::Search),
    (&["functions", "that"], Action::Search),
    (&["method", "that"], Action::Search),
    (&["something", "that"], Action::Search),
    (&["anything", "that"], Action::Search),
    (&["look", "for"], Action::Search),
    (&["looking", "for"], Action::Search),
    (&["at", "once"], Action::Batch),
    (&["in", "one", "go"], Action::Batch),
    (&["in", "one", "call"], Action::Batch),
    (&["several", "lookups"], Action::Batch),
    (&["multiple", "lookups"], Action::Batch),
];

const WORDS: &[(&[&str], Action)] = &[
    (&["callers", "caller"], Action::Callers),
    (&["callees", "callee"], Action::Callees),
    (
        &[
            "usages",
            "usage",
            "uses",
            "used",
            "references",
            "referenced",
            "referencing",
            "refs",
            "imports",
            "imported",
        ],
        Action::References,
    ),
    (
        &[
            "impact",
            "breaks",
            "break",
            "affected",
            "affects",
            "dependents",
        ],
        Action::Impact,
    ),
    (
        &[
            "defined",
            "definition",
            "definitions",
            "define",
            "defines",
            "declared",
            "declaration",
            "locate",
            "implemented",
        ],
        Action::Definition,
    ),
    (&["search", "named", "matching"], Action::Search),
    (&["list", "enumerate"], Action::List),
    (&["outline", "skeleton", "shape"], Action::Skeleton),
    (
        &[
            "overview",
            "summarize",
            "summarise",
            "summary",
            "architecture",
            "orient",
        ],
        Action::Overview,
    ),
    (
        &["tree", "layout", "folders", "folder", "directories"],
        Action::Tree,
    ),
    (&["context", "source"], Action::Context),
    (
        &["unused", "dead", "unreferenced", "orphaned"],
        Action::DeadCode,
    ),
    (
        &["status", "stale", "healthy", "health", "outdated"],
        Action::Status,
    ),
    (
        &[
            "reindex",
            "reindexing",
            "reindexed",
            "re-index",
            "rebuild",
            "rebuilding",
            "refresh",
            "refreshing",
        ],
        Action::Reindex,
    ),
    (&["schema", "schemas"], Action::Schema),
    (&["batch"], Action::Batch),
];

/// Call verbs whose direction depends on where the name sits.
const CALL_VERBS: &[&str] = &["call", "calls", "invoke", "invokes", "calling", "invoking"];
const NEGATIONS: &[&str] = &[
    "don't", "dont", "not", "never", "without", "no", "avoid", "skip", "doesn't",
];
const PRONOUNS: &[&str] = &["it", "its", "this", "that", "them"];
const CLAUSE_WORDS: &[&str] = &["and", "but", "then", "also", "plus"];
/// Values the `kind` argument of list_symbols/find_symbol accepts.
const KINDS: &[&str] = &[
    "function",
    "method",
    "class",
    "struct",
    "interface",
    "enum",
    "trait",
    "type_alias",
    "module",
    "variable",
    "constant",
    "field",
    "element",
    "rule",
];
const NUMBERS: &[&str] = &[
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
];
const DEPTH_WORDS: &[&str] = &["depth", "deep", "levels", "level", "hops", "hop"];
const LIMIT_WORDS: &[&str] = &["top", "first", "limit", "results", "max", "at most"];
/// Words never taken as a plain-word reference.
const STOPWORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "is",
    "are",
    "was",
    "be",
    "of",
    "to",
    "in",
    "on",
    "at",
    "by",
    "for",
    "from",
    "with",
    "and",
    "or",
    "but",
    "what",
    "which",
    "who",
    "whom",
    "where",
    "when",
    "how",
    "why",
    "does",
    "do",
    "did",
    "can",
    "could",
    "would",
    "should",
    "will",
    "i",
    "i'm",
    "me",
    "my",
    "we",
    "you",
    "your",
    "it",
    "its",
    "this",
    "that",
    "these",
    "those",
    "there",
    "here",
    "if",
    "so",
    "all",
    "any",
    "every",
    "each",
    "some",
    "show",
    "give",
    "find",
    "tell",
    "get",
    "just",
    "please",
    "let",
    "see",
    "up",
    "back",
    "first",
    "scratch",
    "everything",
    "anything",
    "something",
    "code",
    "index",
    "file",
    "files",
    "crate",
    "crates",
    "project",
    "repo",
    "test",
    "tests",
    "tool",
    "tools",
    "change",
    "changing",
    "rename",
    "renaming",
    "delete",
    "deleting",
    "remove",
    "removing",
    "edit",
    "editing",
    "modify",
    "into",
    "under",
    "inside",
    "within",
    "also",
    "then",
    "only",
    "not",
    "no",
    "don't",
    "dont",
    "anything",
    "lookups",
    "once",
    "several",
    "multiple",
    "go",
    "call",
    "calls",
    "invoke",
    "invokes",
    "hops",
    "hop",
    "levels",
    "level",
    "deep",
    "depth",
    "top",
    "limit",
    "results",
    "max",
    "most",
    "exist",
    "exists",
    "defined",
    "used",
    "about",
    "safe",
    "need",
    "want",
    "know",
    "look",
    "make",
    "write",
];

#[derive(Debug, Clone)]
struct Tok {
    raw: String,
    low: String,
    quoted: bool,
    /// Followed by `,` / `;` / `.` / `?` / `!` in the original text.
    clause_end: bool,
}

fn tokenize(query: &str) -> Vec<Tok> {
    let trim: &[char] = &[
        '?', ',', ';', '!', '(', ')', '[', ']', '{', '}', '"', '\'', '`', '.', ':',
    ];
    query
        .split_whitespace()
        .filter_map(|piece| {
            let quoted = piece.starts_with('`') || piece.starts_with('"');
            let clause_end = piece.ends_with([',', ';', '.', '?', '!']);
            let raw = piece.trim_matches(trim);
            if raw.is_empty() {
                return None;
            }
            let mut low = raw.to_lowercase();
            if let Some(stem) = low.strip_suffix("'s") {
                low = stem.to_string();
            }
            Some(Tok {
                raw: raw.to_string(),
                low,
                quoted,
                clause_end,
            })
        })
        .collect()
}

fn number(word: &str) -> Option<u32> {
    word.parse::<u32>()
        .ok()
        .or_else(|| NUMBERS.iter().position(|n| *n == word).map(|i| i as u32))
}

fn is_path(raw: &str) -> bool {
    raw.contains('/') || raw.contains('\\')
}

fn kind_of(word: &str) -> Option<&'static str> {
    KINDS.iter().copied().find(|k| {
        word == *k || word.strip_suffix('s') == Some(k) || word.strip_suffix("es") == Some(k)
    })
}

/// A word naming one of `languages`, by its full id or one `_`-separated
/// part of it (`typescript` for `javascript_typescript`).
fn language_of<'a>(word: &str, languages: &[&'a str]) -> Option<&'a str> {
    languages
        .iter()
        .copied()
        .find(|id| *id == word || id.split('_').any(|part| part == word))
}

/// A capitalized word mid-sentence (`Index`, `Embedder`): a type name more
/// often than prose.
fn type_like(tok: &Tok) -> bool {
    tok.raw.chars().next().is_some_and(char::is_uppercase)
        && tok.raw.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !STOPWORDS.contains(&tok.low.as_str())
}

fn code_shaped(raw: &str) -> bool {
    !raw.contains(char::is_whitespace) && classify_query(raw) == QueryIntent::Identifier
}

/// Reads `query` into an [`Intent`] with word rules only. Pure, bounded by
/// [`MAX_QUERY_CHARS`], never panics.
pub fn extract(query: &str, languages: &[&str]) -> Intent {
    let query = truncate(query);
    let toks = tokenize(&query);
    let n = toks.len();
    let mut consumed = vec![false; n];
    // (token index, action) for every trigger, in text order once sorted.
    let mut hits: Vec<(usize, Action)> = Vec::new();

    for (phrase, action) in PHRASES {
        let len = phrase.len();
        let mut i = 0;
        while i + len <= n {
            if (0..len).all(|k| !consumed[i + k] && toks[i + k].low == phrase[k]) {
                (i..i + len).for_each(|k| consumed[k] = true);
                hits.push((i, *action));
                i += len;
            } else {
                i += 1;
            }
        }
    }
    for (i, tok) in toks.iter().enumerate() {
        if consumed[i] {
            continue;
        }
        let low = tok.low.as_str();
        if let Some((_, action)) = WORDS.iter().find(|(words, _)| words.contains(&low)) {
            consumed[i] = true;
            hits.push((i, *action));
        } else if CALL_VERBS.contains(&low) {
            consumed[i] = true;
            hits.push((i, call_direction(&toks, i)));
        }
    }
    hits.sort_by_key(|(i, _)| *i);

    // Negation: a negator scopes over the triggers in its next three tokens.
    let mut excluded: Vec<Action> = Vec::new();
    for (i, tok) in toks.iter().enumerate() {
        let negator = NEGATIONS.contains(&tok.low.as_str())
            || (tok.low == "do" && toks.get(i + 1).is_some_and(|t| t.low == "not"));
        if !negator {
            continue;
        }
        for (pos, action) in &hits {
            if *pos > i
                && *pos <= i + 3
                && *action != Action::DeadCode
                && !excluded.contains(action)
            {
                excluded.push(*action);
            }
        }
    }

    // Parameters.
    let mut path = None;
    let mut kind = None;
    let mut language = None;
    let mut depth = None;
    let mut limit = None;
    for (i, tok) in toks.iter().enumerate() {
        let low = tok.low.as_str();
        if path.is_none() && is_path(&tok.raw) {
            path = Some(tok.raw.trim_end_matches(['/', '\\']).to_string());
            consumed[i] = true;
            continue;
        }
        if kind.is_none() && !tok.quoted {
            if let Some(k) = kind_of(low) {
                kind = Some(k.to_string());
                consumed[i] = true;
                // "Rust functions", "Go interfaces": a language right before a kind.
                let prev = i.checked_sub(1).and_then(|p| toks.get(p).map(|t| (p, t)));
                if let Some((p, id)) =
                    prev.and_then(|(p, t)| language_of(&t.low, languages).map(|id| (p, id)))
                {
                    language = Some(id.to_string());
                    consumed[p] = true;
                }
                continue;
            }
        }
        let after_in = i > 0 && matches!(toks[i - 1].low.as_str(), "in" | "only");
        if language.is_none() && after_in {
            if let Some(id) = language_of(low, languages) {
                language = Some(id.to_string());
                consumed[i] = true;
                continue;
            }
        }
        if let Some(value) = number(low) {
            let near = |words: &[&str]| {
                [i.checked_sub(2), i.checked_sub(1), Some(i + 1)]
                    .into_iter()
                    .flatten()
                    .filter_map(|j| toks.get(j))
                    .any(|t| words.contains(&t.low.as_str()))
            };
            if depth.is_none() && near(DEPTH_WORDS) {
                depth = Some(value);
                consumed[i] = true;
            } else if limit.is_none() && near(LIMIT_WORDS) {
                limit = Some(value);
                consumed[i] = true;
            }
        }
    }

    // References: code-shaped or quoted words first; plain words only when
    // there are none and the request has an action needing a name.
    let mut refs: Vec<(usize, String)> = toks
        .iter()
        .enumerate()
        .filter(|(i, t)| {
            !consumed[*i] && (t.quoted || code_shaped(&t.raw) || (*i > 0 && type_like(t)))
        })
        .map(|(i, t)| (i, t.raw.clone()))
        .collect();
    let wants_target = hits.iter().any(|(_, a)| a.takes_target());
    if refs.is_empty() && wants_target {
        refs = toks
            .iter()
            .enumerate()
            .filter(|(i, t)| {
                !consumed[*i]
                    && !STOPWORDS.contains(&t.low.as_str())
                    && number(&t.low).is_none()
                    && language_of(&t.low, languages).is_none()
                    && t.raw.len() >= 2
                    && t.raw.chars().all(|c| c.is_alphanumeric() || c == '_')
            })
            .map(|(i, t)| (i, t.raw.clone()))
            .take(1)
            .collect();
    }

    // Steps: excluded actions dropped, duplicates merged, each target-taking
    // action bound to the nearest reference after it in its clause, else the
    // nearest one before it.
    let mut steps: Vec<Step> = Vec::new();
    for (pos, action) in &hits {
        if excluded.contains(action) {
            continue;
        }
        let target = if action.takes_target() {
            nearest_ref(&toks, &refs, *pos)
        } else {
            None
        };
        if !steps
            .iter()
            .any(|s| s.action == *action && s.target == target)
        {
            steps.push(Step {
                action: *action,
                target,
            });
        }
    }
    // A definition question about a kind in a place, with no name, is a listing.
    if path.is_some() || kind.is_some() {
        for step in &mut steps {
            if step.action == Action::Definition && step.target.is_none() {
                step.action = Action::List;
            }
        }
    }
    // "where is X" with nothing more specific is a definition lookup.
    if steps.is_empty() && toks.iter().any(|t| t.low == "where") && !refs.is_empty() {
        steps.push(Step {
            action: Action::Definition,
            target: refs.first().map(|(_, r)| r.clone()),
        });
    }
    // A context pack already holds the definition, callers and callees.
    if steps.iter().any(|s| s.action == Action::Context) {
        steps.retain(|s| {
            !matches!(
                s.action,
                Action::Definition | Action::Callers | Action::Callees
            )
        });
    }
    let mut seen: Vec<Action> = Vec::new();
    steps.retain(|s| {
        let dup = seen.contains(&s.action) && s.target.is_none();
        seen.push(s.action);
        !dup
    });

    Intent {
        steps,
        excluded,
        refs: refs.into_iter().map(|(_, r)| r).collect(),
        path,
        kind,
        language,
        depth,
        limit,
    }
}

/// `who calls X` / `which functions call X` are callers; `what does X call`
/// / `everything X calls` are callees.
fn call_direction(toks: &[Tok], i: usize) -> Action {
    let clause_start = (0..i)
        .rev()
        .find(|&j| toks[j].clause_end || CLAUSE_WORDS.contains(&toks[j].low.as_str()))
        .map_or(0, |j| j + 1);
    let asked_with_do = toks[clause_start..i]
        .iter()
        .any(|t| matches!(t.low.as_str(), "does" | "do" | "did"));
    if asked_with_do {
        return Action::Callees;
    }
    let next_is_name = !toks[i].clause_end
        && toks.get(i + 1).is_some_and(|t| {
            !CLAUSE_WORDS.contains(&t.low.as_str()) && !DEPTH_WORDS.contains(&t.low.as_str())
        });
    if next_is_name {
        Action::Callers
    } else {
        Action::Callees
    }
}

fn nearest_ref(toks: &[Tok], refs: &[(usize, String)], pos: usize) -> Option<String> {
    let clause_end = (pos..toks.len())
        .find(|&j| toks[j].clause_end || (j > pos && CLAUSE_WORDS.contains(&toks[j].low.as_str())))
        .unwrap_or(toks.len());
    let after = refs.iter().find(|(i, _)| *i > pos && *i <= clause_end);
    let before = refs.iter().rev().find(|(i, _)| *i < pos);
    // A pronoun right after the trigger ("does it call", "its source")
    // points back, so the earlier name wins.
    let pronoun_next = toks
        .get(pos + 1)
        .is_some_and(|t| PRONOUNS.contains(&t.low.as_str()));
    let pick = if pronoun_next {
        before.or(after)
    } else {
        after.or(before)
    };
    pick.or(refs.first()).map(|(_, r)| r.clone())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    const LANGS: &[&str] = &["go", "javascript_typescript", "python", "rust"];

    fn actions(q: &str) -> Vec<Action> {
        extract(q, LANGS).steps.iter().map(|s| s.action).collect()
    }

    // Development examples: rules are tuned on these only, never on
    // tests/fixtures/intent_eval.toon.

    #[test]
    fn who_calls_is_callers_with_the_exact_name() {
        let i = extract("Who calls parse_config?", LANGS);
        assert_eq!(
            i.steps,
            vec![Step {
                action: Action::Callers,
                target: Some("parse_config".into())
            }]
        );
    }

    #[test]
    fn what_does_x_call_is_callees_and_keeps_depth() {
        let i = extract("What does parse_config call, up to depth 2?", LANGS);
        assert_eq!(
            actions("What does parse_config call, up to depth 2?"),
            vec![Action::Callees]
        );
        assert_eq!(i.steps[0].target.as_deref(), Some("parse_config"));
        assert_eq!(i.depth, Some(2));
    }

    #[test]
    fn listing_keeps_kind_language_and_path() {
        let i = extract("List Rust functions under crates/mct-index", LANGS);
        assert_eq!(
            actions("List Rust functions under crates/mct-index"),
            vec![Action::List]
        );
        assert_eq!(i.kind.as_deref(), Some("function"));
        assert_eq!(i.language.as_deref(), Some("rust"));
        assert_eq!(i.path.as_deref(), Some("crates/mct-index"));
    }

    #[test]
    fn a_plain_word_name_stays_a_textual_reference() {
        let i = extract("Where is parse defined?", LANGS);
        assert_eq!(
            i.steps,
            vec![Step {
                action: Action::Definition,
                target: Some("parse".into())
            }]
        );
    }

    #[test]
    fn compound_request_keeps_both_actions_and_the_negation() {
        let i = extract(
            "Show callers and callees of parse_config, but don't reindex",
            LANGS,
        );
        let acts: Vec<Action> = i.steps.iter().map(|s| s.action).collect();
        assert_eq!(acts, vec![Action::Callers, Action::Callees]);
        assert!(i
            .steps
            .iter()
            .all(|s| s.target.as_deref() == Some("parse_config")));
        assert_eq!(i.excluded, vec![Action::Reindex]);
    }

    #[test]
    fn pronoun_binds_to_the_earlier_name() {
        let i = extract("who calls load_config and what does it call", LANGS);
        assert_eq!(
            i.steps,
            vec![
                Step {
                    action: Action::Callers,
                    target: Some("load_config".into())
                },
                Step {
                    action: Action::Callees,
                    target: Some("load_config".into())
                },
            ]
        );
    }

    #[test]
    fn each_action_binds_its_own_name() {
        let i = extract("definition of Foo and callers of bar_baz", LANGS);
        assert_eq!(i.steps[0].target.as_deref(), Some("Foo"));
        assert_eq!(i.steps[1].target.as_deref(), Some("bar_baz"));
    }

    #[test]
    fn paraphrases_map_to_their_actions() {
        assert_eq!(actions("callers of render_page"), vec![Action::Callers]);
        assert_eq!(
            actions("which handlers invoke dispatch_event"),
            vec![Action::Callers]
        );
        assert_eq!(
            actions("every function save_user calls"),
            vec![Action::Callees]
        );
        assert_eq!(
            actions("where is UserStore referenced"),
            vec![Action::References]
        );
        assert_eq!(
            actions("what would break if I change UserStore"),
            vec![Action::Impact]
        );
        assert_eq!(
            actions("locate the definition of UserStore"),
            vec![Action::Definition]
        );
        assert_eq!(
            actions("search for code that parses dates"),
            vec![Action::Search]
        );
        assert_eq!(actions("outline of src/main.rs"), vec![Action::Skeleton]);
        assert_eq!(
            actions("give me an overview of the server crate"),
            vec![Action::Overview]
        );
        assert_eq!(actions("directory layout of src"), vec![Action::Tree]);
        assert_eq!(actions("find dead code in src/"), vec![Action::DeadCode]);
        assert_eq!(actions("is the index stale?"), vec![Action::Status]);
        assert_eq!(actions("please reindex"), vec![Action::Reindex]);
        assert_eq!(actions("which tools are available?"), vec![Action::Catalog]);
        assert_eq!(
            actions("input schema of the list_symbols tool"),
            vec![Action::Schema]
        );
    }

    #[test]
    fn a_context_pack_subsumes_definition_and_callers() {
        assert_eq!(
            actions("I want to work on save_user: show its source and callers"),
            vec![Action::Context]
        );
    }

    #[test]
    fn negation_without_another_action_leaves_no_step() {
        let i = extract("do not reindex", LANGS);
        assert!(i.steps.is_empty());
        assert_eq!(i.excluded, vec![Action::Reindex]);
    }

    #[test]
    fn never_called_is_dead_code_not_a_negation() {
        let i = extract("functions that are never called", LANGS);
        assert!(i.steps.iter().any(|s| s.action == Action::DeadCode));
        assert!(i.excluded.is_empty());
    }

    #[test]
    fn limit_and_number_words() {
        let i = extract("first ten callers of render_page", LANGS);
        assert_eq!(i.limit, Some(10));
        let i = extract("callees of render_page three levels deep", LANGS);
        assert_eq!(i.depth, Some(3));
    }

    #[test]
    fn small_talk_has_no_intent() {
        for q in ["thanks!", "good morning", "", "   ", "???"] {
            let i = extract(q, LANGS);
            assert!(i.steps.is_empty() && i.refs.is_empty(), "{q}");
        }
    }

    #[test]
    fn go_as_a_verb_is_not_a_language() {
        assert_eq!(extract("let's go", LANGS).language, None);
        assert_eq!(
            extract("list Go structs", LANGS).language.as_deref(),
            Some("go")
        );
    }

    #[test]
    fn huge_or_odd_input_never_panics() {
        let big = "who calls x ".repeat(10_000);
        let _ = extract(&big, LANGS);
        let _ = extract(
            "ünïcödé `` \"\" ' / \\ :: . 99999999999999999999 depth",
            LANGS,
        );
    }

    #[test]
    fn every_action_maps_to_known_tools() {
        let all = [
            Action::Callers,
            Action::Callees,
            Action::References,
            Action::Impact,
            Action::Definition,
            Action::Search,
            Action::List,
            Action::Skeleton,
            Action::Overview,
            Action::Tree,
            Action::Context,
            Action::DeadCode,
            Action::Status,
            Action::Reindex,
            Action::Catalog,
            Action::Schema,
            Action::Batch,
        ];
        for action in all {
            for tool in action.tools() {
                assert!(crate::ttc::KNOWN_TOOL_NAMES.contains(tool), "{tool}");
            }
        }
        for tool in LOOKUP_ALTERNATIVES {
            assert!(crate::ttc::KNOWN_TOOL_NAMES.contains(tool), "{tool}");
        }
    }

    fn tool(name: &'static str, props: &[&str]) -> Tool {
        let props: Map<String, Value> = props
            .iter()
            .map(|p| (p.to_string(), serde_json::json!({"type": "string"})))
            .collect();
        let mut schema = Map::new();
        schema.insert("type".into(), "object".into());
        schema.insert("properties".into(), Value::Object(props));
        Tool::new(
            name,
            format!("{name} description"),
            std::sync::Arc::new(schema),
        )
    }

    fn catalog() -> Vec<Tool> {
        vec![
            tool("find_symbol", &["name", "path", "language", "kind"]),
            tool("find_callers", &["function", "depth", "limit"]),
            tool("find_calls", &["function", "depth"]),
            tool("hybrid_search", &["query", "top_k"]),
            tool("search_symbols", &["query", "limit"]),
            tool("build_context_pack", &["symbol"]),
            tool("reindex", &[]),
        ]
    }

    fn names(p: &Preselection) -> Vec<&str> {
        p.tools.iter().map(String::as_str).collect()
    }

    #[test]
    fn preselection_hints_only_parameters_the_schema_declares() {
        let p = preselect("Who calls parse_config, depth 2?", LANGS, &catalog(), None);
        assert_eq!(names(&p), ["find_callers"]);
        assert_eq!(p.source, Source::Rules);
        let (tool, args) = &p.hints[0];
        assert_eq!(tool, "find_callers");
        assert_eq!(args["function"], "parse_config");
        assert_eq!(args["depth"], 2);
        assert!(!args.contains_key("path"));
    }

    #[test]
    fn tools_missing_from_the_catalog_are_never_offered() {
        // impact_analysis is not in this catalog.
        let p = preselect("what breaks if I change Foo_bar", LANGS, &catalog(), None);
        assert_eq!(p.source, Source::Fallback);
        assert!(p
            .tools
            .iter()
            .all(|t| catalog().iter().any(|c| c.name == t.as_str())));
    }

    #[test]
    fn unknown_request_without_a_model_falls_back_to_the_catalog_minus_exclusions() {
        let p = preselect("hmm, don't reindex", LANGS, &catalog(), None);
        assert_eq!(p.source, Source::Fallback);
        assert_eq!(p.tools.len(), catalog().len() - 1);
        assert!(!p.tools.iter().any(|t| t == "reindex"));
        assert!(p.hints.is_empty());
        assert!(p
            .intent
            .render(p.source)
            .ends_with("not: reindex; src=fallback"));
    }

    #[test]
    fn a_bare_identifier_keeps_the_lookup_alternatives() {
        let p = preselect("parse_config", LANGS, &catalog(), None);
        assert_eq!(p.source, Source::AmbiguousName);
        assert_eq!(
            names(&p),
            ["find_symbol", "search_symbols", "build_context_pack"]
        );
    }

    /// Embeds a text as one-hot on whether it mentions "search".
    struct Fake;
    impl Embedder for Fake {
        fn model_id(&self) -> &str {
            "fake"
        }
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            Ok(texts
                .iter()
                .map(|t| {
                    if t.contains("search") {
                        vec![1.0, 0.0]
                    } else {
                        vec![0.0, 1.0]
                    }
                })
                .collect())
        }
    }

    struct Broken;
    impl Embedder for Broken {
        fn model_id(&self) -> &str {
            "broken"
        }
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            if texts.len() > 1 {
                Ok(texts.iter().map(|_| vec![1.0]).collect())
            } else {
                Err("model crashed".into())
            }
        }
    }

    #[test]
    fn embeddings_decide_only_when_rules_find_nothing_and_above_the_floor() {
        let cat = catalog();
        let vectors = ToolVectors::new(&Fake, &cat).unwrap();
        // "researching" is no rule word, but the fake embedder sees "search".
        let p = preselect("hmm, researching", LANGS, &cat, Some(&vectors));
        assert_eq!(p.source, Source::Embedding);
        assert_eq!(names(&p), ["hybrid_search"]);
        // Rules still win when they match.
        let p = preselect("who calls parse_config", LANGS, &cat, Some(&vectors));
        assert_eq!(names(&p), ["find_callers"]);
    }

    #[test]
    fn a_failing_embedder_degrades_to_the_full_catalog() {
        let cat = catalog();
        let vectors = ToolVectors::new(&Broken, &cat).unwrap();
        let p = preselect("hmm", LANGS, &cat, Some(&vectors));
        assert_eq!(p.source, Source::Fallback);
        assert_eq!(p.tools.len(), cat.len());
    }

    #[test]
    fn render_keeps_negations_and_parameters() {
        let i = extract(
            "What does parse_config call, up to depth 2? Don't reindex",
            LANGS,
        );
        assert_eq!(
            i.render(Source::Rules),
            "intent: callees(parse_config); not: reindex; depth=2; src=rules"
        );
    }
}
