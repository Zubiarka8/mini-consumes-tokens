// Example/eval code: a panic means a broken evaluation precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Labelled retrieval evaluation of `detail` / `max_tokens`.
//!
//! ```sh
//! cargo run -p mct-mcp-server --example retrieval_eval --release
//! ```
//!
//! Prints, per strategy, recall, silent misses, calls, tokens (two separate
//! heuristics) and latency over the cases in `tests/common/retrieval.rs`, then
//! one row per case x strategy. Billed tokens are not measured.

#[path = "../tests/common/retrieval.rs"]
mod retrieval;

use retrieval::{cases, run_all, server_at, write_project, Strategy};

#[tokio::main]
async fn main() {
    let root = write_project();
    let server = server_at(&root).await;
    let outcomes = run_all(&server).await;

    println!("| strategy | cases | satisfied | mean recall | silent misses | calls | ~tokens (EstimatedCounter) | ~tokens (chars/4) | mean latency µs |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for strategy in Strategy::ALL {
        let rows: Vec<_> = outcomes.iter().filter(|o| o.strategy == strategy).collect();
        if rows.is_empty() {
            continue;
        }
        let n = rows.len();
        println!(
            "| {} | {n} | {} | {:.2} | {} | {} | {} | {} | {} |",
            strategy.label(),
            rows.iter().filter(|o| o.satisfied).count(),
            rows.iter().map(|o| o.recall).sum::<f64>() / n as f64,
            rows.iter().filter(|o| o.silent_miss).count(),
            rows.iter().map(|o| o.calls).sum::<usize>(),
            rows.iter().map(|o| o.estimated).sum::<usize>(),
            rows.iter().map(|o| o.chars4).sum::<usize>(),
            rows.iter().map(|o| o.micros).sum::<u128>() / n as u128,
        );
    }

    println!();
    println!("| case | strategy | calls | ~tokens (EstimatedCounter) | ~tokens (chars/4) | satisfied | recall | precision | signalled | silent miss |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for case in cases() {
        for o in outcomes.iter().filter(|o| o.case == case.id) {
            println!(
                "| {} | {} | {} | {} | {} | {} | {:.2} | {} | {} | {} |",
                o.case,
                o.strategy.label(),
                o.calls,
                o.estimated,
                o.chars4,
                if o.satisfied { "yes" } else { "NO" },
                o.recall,
                o.precision.map_or("-".to_string(), |p| format!("{p:.2}")),
                if o.signalled { "yes" } else { "no" },
                if o.silent_miss { "YES" } else { "no" },
            );
        }
    }
    std::fs::remove_dir_all(root).ok();
}
