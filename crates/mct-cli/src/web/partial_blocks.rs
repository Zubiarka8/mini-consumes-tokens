//! On-demand repeated partial-block search (ADR-012) for the dashboard.
//!
//! The search is the one `mct-cli duplicates --partial-blocks` runs, with the
//! default thresholds and budgets. It starts only when the page asks for it,
//! on its own thread and its own read-only index handle, so the site thread
//! keeps answering. The finished report is kept with the snapshot: paging
//! reads it, and a reloaded snapshot starts without one.

use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use mct_index::{
    DuplicateOptions, ExcludeSet, Index, PartialBlock, PartialBlockBudget, PartialBlockReport,
};
use serde_json::{json, Value};

use super::api::{page_bounds, Params};
use super::response::Response;
use super::Site;

/// Blocks per page when the request names no size.
const PAGE_SIZE: usize = 25;

/// Where the search of one snapshot stands.
#[derive(Default)]
pub(super) enum Search {
    #[default]
    Idle,
    Running(Receiver<anyhow::Result<Finished>>),
    Done(Box<Finished>),
    /// The index was republished while the search ran: its blocks describe
    /// files the snapshot on screen does not.
    Outdated,
    Failed(String),
}

/// A finished search and the diagnostics of the analysis it ran in.
pub(super) struct Finished {
    report: PartialBlockReport,
    budget: PartialBlockBudget,
    normalization: &'static str,
    index_coverage: &'static str,
    index_syntax_errors: usize,
    index_read_failures: usize,
    /// Candidates the analysis could not read or parse; their functions were
    /// not searched.
    skipped: usize,
    generation: u64,
    elapsed_ms: u128,
}

/// The options of `mct-cli duplicates --partial-blocks`, whose `--block-*`
/// flags are `budget`.
/// No group is rendered here, so none is paged in: the search itself sees
/// every group before pagination.
fn search_options(budget: PartialBlockBudget) -> DuplicateOptions {
    DuplicateOptions {
        limit: Some(0),
        partial_blocks: Some(budget),
        ..DuplicateOptions::default()
    }
}

/// Runs the shared analysis with the partial-block search on.
pub(super) fn search(root: &Path, budget: PartialBlockBudget) -> anyhow::Result<Finished> {
    let started = Instant::now();
    let database = root.join(".mct-index/index.sqlite3");
    let index = Index::open_existing_read_only(root, &database, ExcludeSet::for_project(root))?;
    // The generation and the blocks must describe one published state.
    let snapshot = index.read_snapshot()?;
    let generation = snapshot.generation();
    let report = mct_index::find_duplicate_groups(
        &index,
        &mct_languages::build_token_sources(),
        search_options(budget),
    )?;
    drop(snapshot);
    let index_coverage = crate::duplicates::index_coverage(&report);
    let (Some(blocks), Some(budget)) = (report.partial_blocks, report.options.partial_blocks)
    else {
        anyhow::bail!("the analysis returned no partial-block report");
    };
    Ok(Finished {
        report: blocks,
        budget,
        normalization: report.options.normalization.as_str(),
        index_coverage,
        index_syntax_errors: report.index_syntax_errors,
        index_read_failures: report.index_read_failures,
        skipped: report.skipped.len(),
        generation,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn block_json(block: &PartialBlock) -> Value {
    json!({
        "id": block.id,
        "language": block.language,
        "tokens": block.tokens,
        "lines": block.lines,
        "occurrences": block.occurrences.iter().map(|occurrence| json!({
            "path": occurrence.path,
            "start": occurrence.start_line,
            "end": occurrence.end_line,
            "function": occurrence.function,
        })).collect::<Vec<_>>(),
    })
}

impl Finished {
    /// One page of the kept report; nothing is searched again.
    fn page(&self, params: &Params) -> Value {
        let total = self.report.blocks.len();
        let (page, size, start) = page_bounds(params, total, PAGE_SIZE);
        let items: Vec<Value> = self
            .report
            .blocks
            .iter()
            .skip(start)
            .take(size)
            .map(block_json)
            .collect();
        json!({
            "status": "done",
            "total": total,
            "page": page,
            "size": size,
            "items": items,
            "normalization": self.normalization,
            "scope": crate::duplicates::PARTIAL_BLOCKS_SCOPE,
            "warning": crate::duplicates::PARTIAL_BLOCKS_WARNING,
            "budget": {
                "minTokens": self.budget.min_tokens,
                "minLines": self.budget.min_lines,
                "maxWindows": self.budget.max_windows,
                "maxComparisons": self.budget.max_comparisons,
                "maxBlocks": self.budget.max_blocks,
                "functionsSearched": self.report.functions_searched,
                "windowsUsed": self.report.windows_used,
                "comparisonsUsed": self.report.comparisons_used,
                // `null`, or the first budget hit: the list is then partial.
                "exhausted": self.report.budget_exhausted.map(|kind| kind.as_str()),
            },
            "indexCoverage": self.index_coverage,
            "indexSyntaxErrors": self.index_syntax_errors,
            "indexReadFailures": self.index_read_failures,
            "skipped": self.skipped,
            "elapsedMs": self.elapsed_ms,
        })
    }
}

impl Site {
    /// Starts the search for this snapshot (JSON body: `csrf`). A search
    /// already running is left alone; a finished one is replaced. Answers
    /// with the state, as the GET does.
    pub(super) fn start_partial_blocks(&mut self, body: &str) -> Response {
        let Ok(Value::Object(fields)) = serde_json::from_str(body) else {
            return Response::failure(400, "invalid JSON body");
        };
        if fields.get("csrf").and_then(Value::as_str) != Some(self.csrf.as_str()) {
            return Response::error(403);
        }
        if fields.keys().any(|key| key != "csrf") {
            return Response::failure(400, "unknown field");
        }
        self.settle_partial_blocks();
        if !matches!(*self.partial_blocks.borrow(), Search::Running(_)) {
            let (send, result) = std::sync::mpsc::channel();
            let root = self.root.clone();
            let spawned = std::thread::Builder::new()
                .name("partial-blocks".into())
                .spawn(move || {
                    // The site may have reloaded meanwhile: nobody is waiting.
                    let _ = send.send(search(&root, PartialBlockBudget::default()));
                });
            *self.partial_blocks.borrow_mut() = match spawned {
                Ok(_) => Search::Running(result),
                Err(error) => Search::Failed(format!("cannot start the search thread: {error}")),
            };
        }
        Response::json(&self.partial_blocks(&Params::new()))
    }

    /// The state of the search, and one page of its blocks once it is done.
    pub(super) fn partial_blocks(&self, params: &Params) -> Value {
        self.settle_partial_blocks();
        match &*self.partial_blocks.borrow() {
            Search::Idle => json!({ "status": "idle" }),
            Search::Running(_) => json!({ "status": "running" }),
            Search::Done(finished) => finished.page(params),
            Search::Outdated => json!({ "status": "outdated" }),
            Search::Failed(error) => json!({ "status": "failed", "error": error }),
        }
    }

    /// Takes the result of a running search once its thread has sent it.
    fn settle_partial_blocks(&self) {
        let mut state = self.partial_blocks.borrow_mut();
        let Search::Running(result) = &*state else {
            return;
        };
        let sent = match result.try_recv() {
            Err(TryRecvError::Empty) => return,
            Ok(sent) => Some(sent),
            Err(TryRecvError::Disconnected) => None,
        };
        *state = self.searched(sent);
    }

    /// Blocks until the running search, if any, has finished.
    #[cfg(test)]
    pub(super) fn wait_for_partial_blocks(&self) {
        let mut state = self.partial_blocks.borrow_mut();
        if let Search::Running(result) = &*state {
            let sent = result.recv().ok();
            *state = self.searched(sent);
        }
    }

    /// The state after the search thread sent `sent`, or stopped without
    /// sending (`None`).
    fn searched(&self, sent: Option<anyhow::Result<Finished>>) -> Search {
        match sent {
            Some(Ok(finished)) if finished.generation == self.stamp.generation => {
                Search::Done(Box::new(finished))
            }
            Some(Ok(_)) => Search::Outdated,
            Some(Err(error)) => {
                eprintln!("warning: the partial-block search failed: {error:#}");
                Search::Failed(format!("{error:#}"))
            }
            None => Search::Failed("the search thread stopped".into()),
        }
    }
}
