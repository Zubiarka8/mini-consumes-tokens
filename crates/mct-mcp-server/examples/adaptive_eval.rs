// Example/eval code: a panic means a broken evaluation precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Extended labelled evaluation and the offline adaptive-detail simulation.
//!
//! ```sh
//! cargo run -p mct-mcp-server --example adaptive_eval --release [-- dev|eval|all]
//! ```
//!
//! Cases and strategies live in `tests/common/{extended,retrieval}.rs`. Tokens
//! are `EstimatedCounter` and `chars/4`, both heuristics, over `text` output
//! (the evidence check needs `path:Lline`); billed tokens are not measured.
//! Latency is the median of three runs in one process.

#[path = "../tests/common/extended.rs"]
mod extended;
#[path = "../tests/common/retrieval.rs"]
mod retrieval;

// `extended.rs` reaches `retrieval` through `super`.
use retrieval::{run_cases, server_at, write_files, Outcome, Split, Strategy};

fn p(sorted: &[u128], q: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

#[tokio::main]
async fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let files = extended::corpus();
    let cases = extended::cases(&files);
    let root = write_files(&files);
    let server = server_at(&root).await;
    let selected: Vec<_> = cases
        .iter()
        .filter(|c| match which.as_str() {
            "dev" => c.split == Split::Dev,
            "eval" => c.split == Split::Eval,
            _ => true,
        })
        .collect();
    println!(
        "corpus: {} files; cases: {} ({} dev, {} eval); showing: {which}\n",
        files.len(),
        cases.len(),
        cases.iter().filter(|c| c.split == Split::Dev).count(),
        cases.iter().filter(|c| c.split == Split::Eval).count(),
    );
    let mut outcomes: Vec<Outcome> = Vec::new();
    for case in &selected {
        let one = std::slice::from_ref(*case);
        outcomes.extend(run_cases(&server, one, &Strategy::EXTENDED, "text", 3).await);
    }
    let id_split = |id: &str| cases.iter().find(|c| c.id == id).map(|c| c.split).unwrap();

    let by = |f: &dyn Fn(&retrieval::Case) -> bool, title: &str| {
        println!("{title}\n");
        println!("| strategy | tasks | satisfied | silent misses | mean recall | calls | calls/task | ~tokens (Est) | ~tokens/satisfied | ~tokens (chars/4) | p50 µs | p95 µs |");
        println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
        for strategy in Strategy::EXTENDED {
            let rows: Vec<&Outcome> = outcomes
                .iter()
                .filter(|o| o.strategy == strategy)
                .filter(|o| cases.iter().find(|c| c.id == o.case).is_some_and(f))
                .collect();
            if rows.is_empty() {
                continue;
            }
            let n = rows.len();
            let ok = rows.iter().filter(|o| o.satisfied).count();
            let est: usize = rows.iter().map(|o| o.estimated).sum();
            let calls: usize = rows.iter().map(|o| o.calls).sum();
            let mut micros: Vec<u128> = rows.iter().map(|o| o.micros).collect();
            micros.sort_unstable();
            println!(
                "| {} | {n} | {ok} | {} | {:.2} | {calls} | {:.2} | {est} | {} | {} | {} | {} |",
                strategy.label(),
                rows.iter().filter(|o| o.silent_miss).count(),
                rows.iter().map(|o| o.recall).sum::<f64>() / n as f64,
                calls as f64 / n as f64,
                est.checked_div(ok).unwrap_or(0),
                rows.iter().map(|o| o.chars4).sum::<usize>(),
                p(&micros, 0.5),
                p(&micros, 0.95),
            );
        }
        println!();
    };
    by(&|_| true, "All tasks");
    by(
        &|c| c.task_budget.is_none(),
        "Tasks without a budget of their own (the unchanged tools can take part)",
    );
    by(&|c| c.split == Split::Dev, "Dev tasks");
    by(&|c| c.split == Split::Eval, "Eval tasks");

    // Where a level policy wins or loses: packs whose task needs source text
    // against packs that need only locations or relations.
    println!("Break-even for packs (tasks without a budget): the share of source-needing packs above which a strategy costs more than the unchanged tool.\n");
    println!("| strategy | saved per location/relations pack | extra per source/section pack | break-even share of source-needing packs | share in this set |");
    println!("|---|---|---|---|---|");
    let packs: Vec<&retrieval::Case> = cases
        .iter()
        .filter(|c| {
            c.kind == retrieval::Kind::Pack
                && c.task_budget.is_none()
                && c.intent != retrieval::Intent::Nothing
        })
        .collect();
    let needs_text = |c: &retrieval::Case| {
        matches!(
            c.intent,
            retrieval::Intent::Source | retrieval::Intent::DocText
        )
    };
    let cost = |s: Strategy, id: &str| {
        outcomes
            .iter()
            .find(|o| o.strategy == s && o.case == id)
            .map_or(0, |o| o.estimated as i64)
    };
    for strategy in [
        Strategy::OverviewEscalate,
        Strategy::AutoRules,
        Strategy::IntentRules,
    ] {
        let (mut save, mut ns, mut waste, mut nw) = (0i64, 0i64, 0i64, 0i64);
        for c in &packs {
            let d = cost(Strategy::Default, c.id) - cost(strategy, c.id);
            if needs_text(c) {
                waste -= d;
                nw += 1;
            } else {
                save += d;
                ns += 1;
            }
        }
        let (save, waste) = (
            save as f64 / ns.max(1) as f64,
            waste as f64 / nw.max(1) as f64,
        );
        let be = if waste > 0.0 {
            format!("{:.0}%", 100.0 * save / (save + waste))
        } else {
            "never loses".to_string()
        };
        println!(
            "| {} | {save:.0} | {waste:+.0} | {be} | {:.0}% |",
            strategy.label(),
            100.0 * nw as f64 / (ns + nw).max(1) as f64
        );
    }
    println!();
    println!("\nPer category (satisfied / tasks), by strategy:\n");
    let mut cats: Vec<&str> = selected.iter().map(|c| c.category).collect();
    cats.sort_unstable();
    cats.dedup();
    print!("| category |");
    for s in Strategy::EXTENDED {
        print!(" {} |", s.label());
    }
    println!();
    print!("|---|");
    for _ in Strategy::EXTENDED {
        print!("---|");
    }
    println!();
    for cat in cats {
        print!("| {cat} |");
        for strategy in Strategy::EXTENDED {
            let ids: Vec<&str> = selected
                .iter()
                .filter(|c| c.category == cat)
                .map(|c| c.id)
                .collect();
            let rows: Vec<_> = outcomes
                .iter()
                .filter(|o| o.strategy == strategy && ids.contains(&o.case))
                .collect();
            let tokens: usize = rows.iter().map(|o| o.estimated).sum();
            print!(
                " {}/{} ({tokens}) |",
                rows.iter().filter(|o| o.satisfied).count(),
                rows.len()
            );
        }
        println!();
    }

    println!("\nCases not satisfied (strategy: calls, tokens, signalled):\n");
    for case in &selected {
        let failed: Vec<String> = outcomes
            .iter()
            .filter(|o| o.case == case.id && !o.satisfied)
            .map(|o| {
                format!(
                    "{} [{} calls, ~{}, {}{}]",
                    o.strategy.label(),
                    o.calls,
                    o.estimated,
                    if o.signalled { "signalled" } else { "SILENT" },
                    ""
                )
            })
            .collect();
        if !failed.is_empty() {
            println!(
                "- {} ({:?}, {:?}): {}",
                case.id,
                id_split(case.id),
                case.intent,
                failed.join("; ")
            );
        }
    }
    std::fs::remove_dir_all(root).ok();
}
