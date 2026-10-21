//! Retrieval quality over the labelled cases in `common/retrieval.rs`.
//!
//! A strategy is judged by whether the labelled evidence is in its answers,
//! not by whether it returned something. Run
//! `cargo test -p mct-mcp-server --test retrieval_quality -- --nocapture` to
//! see the per-case table.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::retrieval::{cases, run_all, server_at, write_project, Kind, Outcome, Strategy};

async fn outcomes() -> Vec<Outcome> {
    let root = write_project();
    let server = server_at(&root).await;
    let all = run_all(&server).await;
    std::fs::remove_dir_all(root).ok();
    all
}

fn of(all: &[Outcome], strategy: Strategy) -> Vec<&Outcome> {
    all.iter().filter(|o| o.strategy == strategy).collect()
}

#[tokio::test]
async fn the_unchanged_tools_find_every_labelled_evidence() {
    let all = outcomes().await;
    for o in of(&all, Strategy::Default) {
        assert!(o.satisfied, "baseline must satisfy `{}`", o.case);
    }
}

#[tokio::test]
async fn one_budgeted_detailed_call_keeps_the_recall_of_the_default() {
    let all = outcomes().await;
    for o in of(&all, Strategy::DetailedBudget) {
        assert!(o.satisfied, "`{}` lost evidence under a budget", o.case);
    }
}

#[tokio::test]
async fn following_the_continuation_recovers_every_search_case_a_small_budget_cut() {
    let all = outcomes().await;
    let follow = of(&all, Strategy::OverviewBudgetFollow);
    assert!(!follow.is_empty());
    for o in follow {
        assert!(
            o.satisfied,
            "`{}` not reached after {} call(s)",
            o.case, o.calls
        );
    }
}

#[tokio::test]
async fn nothing_is_ever_missed_silently_by_a_lowered_or_cut_answer() {
    let all = outcomes().await;
    for o in all.iter().filter(|o| o.strategy != Strategy::Default) {
        assert!(
            !o.silent_miss,
            "`{}` / {}: evidence missing and the answer did not say it was cut or lowered",
            o.case,
            o.strategy.label()
        );
    }
}

#[tokio::test]
async fn a_level_that_cannot_show_the_answer_says_so_and_the_next_level_recovers_it() {
    let all = outcomes().await;
    let needs_body = ["needs-the-body", "markdown-section"];
    for id in needs_body {
        let overview = all
            .iter()
            .find(|o| o.case == id && o.strategy == Strategy::Overview)
            .unwrap();
        assert!(!overview.satisfied, "`{id}` needs source an overview lacks");
        assert!(
            overview.signalled,
            "`{id}`: overview must say it is a lower level"
        );
        let escalated = all
            .iter()
            .find(|o| o.case == id && o.strategy == Strategy::OverviewThenDetailed)
            .unwrap();
        assert!(escalated.satisfied, "`{id}` not recovered by escalating");
        assert_eq!(escalated.calls, 2);
    }
}

#[tokio::test]
async fn no_evidence_is_reported_as_none_not_invented_in_every_strategy() {
    let all = outcomes().await;
    for o in all.iter().filter(|o| o.case == "no-evidence") {
        assert!(
            o.satisfied,
            "{}: must say no match and list no row",
            o.strategy.label()
        );
    }
}

#[tokio::test]
async fn lower_levels_cost_fewer_estimated_tokens_than_the_default_for_packs() {
    let all = outcomes().await;
    let pack_ids: Vec<&str> = cases()
        .into_iter()
        .filter(|c| c.kind == Kind::Pack)
        .map(|c| c.id)
        .collect();
    let sum = |s: Strategy| -> usize {
        all.iter()
            .filter(|o| o.strategy == s && pack_ids.contains(&o.case))
            .map(|o| o.estimated)
            .sum()
    };
    assert!(sum(Strategy::Overview) < sum(Strategy::Default));
    assert!(sum(Strategy::Summary) < sum(Strategy::Default));
    // Escalating costs more than asking once at the right level: the reason
    // the footer prices the levels above.
    assert!(sum(Strategy::OverviewThenDetailed) > sum(Strategy::DetailedBudget));
}

#[tokio::test]
async fn print_the_table() {
    let all = outcomes().await;
    for o in &all {
        println!(
            "{:<26} {:<40} calls={} est={:<4} chars/4={:<4} ok={} recall={:.2} prec={:?} signalled={} silent={}",
            o.case,
            o.strategy.label(),
            o.calls,
            o.estimated,
            o.chars4,
            o.satisfied,
            o.recall,
            o.precision,
            o.signalled,
            o.silent_miss
        );
    }
}
