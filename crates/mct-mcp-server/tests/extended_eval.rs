//! The extended labelled set (`common/extended.rs`): the set itself is sound,
//! every strategy is judged by evidence and never by "returned something",
//! and the offline adaptive simulation is reproducible.
//!
//! The numbers are printed by `examples/adaptive_eval.rs`; the assertions here
//! are about validity and safety, not about which strategy is cheapest.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::extended::{cases, corpus};
use common::retrieval::{run_cases, server_at, write_files, Outcome, Split, Strategy};
use mct_mcp_server::server::MctServer;
use serde_json::json;

struct Run {
    outcomes: Vec<Outcome>,
    server: MctServer,
    root: std::path::PathBuf,
}

impl Drop for Run {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

async fn run(strategies: &[Strategy]) -> Run {
    let files = corpus();
    let all = cases(&files);
    let root = write_files(&files);
    let server = server_at(&root).await;
    let outcomes = run_cases(&server, &all, strategies, "text", 1).await;
    Run {
        outcomes,
        server,
        root,
    }
}

#[test]
fn the_set_is_large_diverse_and_split_without_overlap() {
    let all = cases(&corpus());
    assert!(all.len() >= 50, "{} cases", all.len());
    let ids: BTreeSet<_> = all.iter().map(|c| c.id).collect();
    assert_eq!(ids.len(), all.len(), "ids must be unique");

    let categories: BTreeSet<_> = all.iter().map(|c| c.category).collect();
    for wanted in [
        "exact symbols",
        "ambiguous symbols",
        "references and dependencies",
        "small and large functions",
        "markdown sections",
        "ADR and documentation",
        "mixed code and documentation",
        "no evidence",
        "numerous results",
        "insufficient budget",
    ] {
        assert!(categories.contains(wanted), "missing category `{wanted}`");
    }

    let mut per: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for c in &all {
        let e = per.entry(c.category).or_default();
        match c.split {
            Split::Dev => e.0 += 1,
            Split::Eval => e.1 += 1,
        }
    }
    for (cat, (dev, eval)) in per {
        assert!(dev > 0 && eval > 0, "`{cat}` must have dev and eval cases");
    }
    // Evidence is labelled: only the cases that expect nothing, or an
    // explained refusal, may carry no expected evidence.
    for c in &all {
        assert!(
            !c.needs.is_empty() || c.expect_none || c.expect_signal,
            "`{}` has no labelled evidence",
            c.id
        );
    }
}

#[tokio::test]
async fn every_case_is_answerable_and_no_strategy_misses_silently() {
    let r = run(&Strategy::EXTENDED).await;
    let ids: BTreeSet<_> = r.outcomes.iter().map(|o| o.case).collect();
    for id in ids {
        assert!(
            r.outcomes.iter().any(|o| o.case == id && o.satisfied),
            "`{id}`: no strategy satisfies it, so the label may be wrong"
        );
    }
    for o in &r.outcomes {
        assert!(
            !o.silent_miss,
            "`{}` under `{}` missed evidence and said nothing",
            o.case,
            o.strategy.label()
        );
    }
}

#[tokio::test]
async fn the_unchanged_tools_answer_every_case_that_has_no_budget_of_its_own() {
    let r = run(&[Strategy::Default]).await;
    let all = cases(&corpus());
    for o in &r.outcomes {
        let case = all.iter().find(|c| c.id == o.case).unwrap();
        if case.task_budget.is_none() {
            assert!(o.satisfied, "baseline must satisfy `{}`", o.case);
        }
    }
}

#[tokio::test]
async fn escalating_from_a_lower_level_never_loses_evidence_the_default_finds() {
    let r = run(&[
        Strategy::Default,
        Strategy::OverviewEscalate,
        Strategy::AutoRules,
        Strategy::IntentRules,
    ])
    .await;
    let all = cases(&corpus());
    for case in all.iter().filter(|c| c.task_budget.is_none()) {
        let of = |s: Strategy| {
            r.outcomes
                .iter()
                .find(|o| o.case == case.id && o.strategy == s)
                .unwrap()
        };
        if of(Strategy::Default).satisfied {
            for s in [
                Strategy::OverviewEscalate,
                Strategy::AutoRules,
                Strategy::IntentRules,
            ] {
                assert!(
                    of(s).satisfied,
                    "`{}` lost evidence under `{}`",
                    case.id,
                    s.label()
                );
            }
        }
    }
}

#[tokio::test]
async fn tokens_and_answers_are_reproducible_run_to_run() {
    let strategies = [
        Strategy::Default,
        Strategy::AutoRules,
        Strategy::DetailedBudgetFollow,
    ];
    let (a, b) = (run(&strategies).await, run(&strategies).await);
    assert_eq!(a.outcomes.len(), b.outcomes.len());
    for (x, y) in a.outcomes.iter().zip(&b.outcomes) {
        assert_eq!((x.case, x.strategy), (y.case, y.strategy));
        assert_eq!(
            x.answers,
            y.answers,
            "`{}` under `{}` is not deterministic",
            x.case,
            x.strategy.label()
        );
        assert_eq!(x.estimated, y.estimated);
    }
}

#[tokio::test]
async fn the_deprecated_aliases_take_the_new_arguments_like_the_canonical_tools() {
    let r = run(&[]).await;
    let text = |result: rmcp::model::CallToolResult| {
        result
            .content
            .iter()
            .filter_map(|b| b.as_text())
            .map(|t| t.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    };
    for (alias, canonical) in [
        ("search_symbols", "search_symbols_like"),
        ("hybrid_search", "hybrid_search_like"),
    ] {
        let mut args =
            json!({"query": "handle", "format": "text", "detail": "overview", "max_tokens": 150});
        if canonical == "hybrid_search_like" {
            args["alpha"] = json!(0.0);
        }
        let a = r
            .server
            .call_read_only_tool(alias, args.as_object().cloned())
            .await
            .unwrap();
        let c = r
            .server
            .call_read_only_tool(canonical, args.as_object().cloned())
            .await
            .unwrap();
        let (a, c) = (text(a), text(c));
        assert_eq!(a, c, "`{alias}` must behave like `{canonical}`");
        assert!(a.contains("Budget: partial"), "{a}");
    }
}
