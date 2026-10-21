// Example/eval code: a panic means a broken evaluation precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Calibration of the budget's token estimate against the model's own count.
//!
//! ```sh
//! cargo run -p mct-mcp-server --example token_calibration --release
//! ANTHROPIC_API_KEY=... cargo run -p mct-mcp-server --example token_calibration --release
//! ```
//!
//! Corpus: every response of the labelled retrieval cases, in `text` and in
//! `toon`. Three different things are reported and never merged:
//! - `EstimatedCounter` (the budget's estimate) and `chars/4`: heuristics;
//! - the count from `POST /v1/messages/count_tokens`: what the model's
//!   tokenizer reports for that text sent as one user message. This is a
//!   measured count, not the billed usage of any session;
//! - billed tokens: never measured here.
//!
//! No tokenizer for Claude models is published, so without `ANTHROPIC_API_KEY`
//! (or without network) the model column is "not measured" and only the two
//! heuristics are printed. The request goes through `curl`, so the workspace
//! gains no HTTP dependency; the key is read from the environment and handed
//! to curl in a private temporary header file, never on its command line.
//! `MCT_CALIBRATION_URL` overrides the endpoint (used to test the plumbing
//! against a local stub; a stub says nothing about the real API).
//! The fixed per-request framing tokens are estimated with a one-character
//! message and subtracted, and that overhead is printed.

#[path = "../tests/common/retrieval.rs"]
mod retrieval;

use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Stdio};

use mct_index::{EstimatedCounter, TokenCounter};
use retrieval::{run_all_in, server_at, write_project};

fn count_tokens(model: &str, headers: &std::path::Path, text: &str) -> Result<u64, String> {
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": text}],
    });
    let mut child = Command::new("curl")
        .args(["-sS", "--max-time", "30", "-X", "POST", "-H"])
        .arg(format!("@{}", headers.display()))
        .args(["--data-binary", "@-"])
        .arg(
            std::env::var("MCT_CALIBRATION_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages/count_tokens".into()),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(body.to_string().as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let reply: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("not JSON: {e}"))?;
    reply
        .get("input_tokens")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("no input_tokens in {reply}"))
}

#[tokio::main]
async fn main() {
    let root = write_project();
    let server = server_at(&root).await;
    let mut corpus: BTreeSet<(String, String)> = BTreeSet::new();
    for format in ["text", "toon"] {
        for outcome in run_all_in(&server, format).await {
            for (i, answer) in outcome.answers.iter().enumerate() {
                corpus.insert((
                    format!("{format}/{}/{}/{i}", outcome.case, outcome.strategy.label()),
                    answer.clone(),
                ));
            }
        }
    }
    std::fs::remove_dir_all(root).ok();

    let key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.is_empty());
    let model =
        std::env::var("MCT_CALIBRATION_MODEL").unwrap_or_else(|_| "claude-sonnet-5-5".into());
    let headers = std::env::temp_dir().join(format!("mct-calibration-{}.hdr", std::process::id()));
    let mut overhead = 0u64;
    let mut measured = key.is_some();
    if let Some(key) = &key {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        // Owner-only on Unix; Windows has no mode bits and uses the profile ACL.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&headers).unwrap();
        write!(
            file,
            "x-api-key: {key}\nanthropic-version: 2023-06-01\ncontent-type: application/json\n"
        )
        .unwrap();
        match count_tokens(&model, &headers, ".") {
            Ok(n) => overhead = n.saturating_sub(1),
            Err(e) => {
                eprintln!("count_tokens unavailable ({e}); the model column is not measured");
                measured = false;
            }
        }
    } else {
        eprintln!("ANTHROPIC_API_KEY not set: the model column is not measured");
    }

    println!("| text | chars | EstimatedCounter | chars/4 | model count (net of framing) | abs err (est) | rel err (est) | rel err (chars/4) |");
    println!("|---|---|---|---|---|---|---|---|");
    let mut rows: Vec<(String, f64, f64)> = Vec::new();
    let (mut abs_est, mut rel_est, mut rel_c4, mut signed_est, mut n) =
        (0f64, 0f64, 0f64, 0f64, 0f64);
    for (label, text) in &corpus {
        let est = EstimatedCounter.count(text).value as f64;
        let c4 = text.chars().count().div_ceil(4) as f64;
        let real = if measured {
            count_tokens(&model, &headers, text)
                .ok()
                .map(|t| t.saturating_sub(overhead) as f64)
        } else {
            None
        };
        match real {
            Some(real) if real > 0.0 => {
                let e = est - real;
                abs_est += e.abs();
                rel_est += e.abs() / real;
                rel_c4 += (c4 - real).abs() / real;
                signed_est += e / real;
                n += 1.0;
                rows.push((label.clone(), e / real, real));
                println!(
                    "| {label} | {} | {est} | {c4} | {real} | {:.0} | {:+.1}% | {:+.1}% |",
                    text.chars().count(),
                    e.abs(),
                    100.0 * e / real,
                    100.0 * (c4 - real) / real
                );
            }
            _ => println!(
                "| {label} | {} | {est} | {c4} | not measured | - | - | - |",
                text.chars().count()
            ),
        }
    }
    std::fs::remove_file(&headers).ok();
    println!();
    if n > 0.0 {
        println!("texts measured: {n}, framing overhead subtracted: {overhead} token(s)");
        println!("EstimatedCounter: mean abs error {:.1} tokens, mean abs rel error {:.1}%, mean signed rel error {:+.1}%", abs_est / n, 100.0 * rel_est / n, 100.0 * signed_est / n);
        println!("chars/4: mean abs rel error {:.1}%", 100.0 * rel_c4 / n);
        rows.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
        println!("largest EstimatedCounter deviations:");
        for (label, rel, real) in rows.iter().take(5) {
            println!("- {label}: {:+.1}% (model count {real})", 100.0 * rel);
        }
    } else {
        println!(
            "Model count: not measured ({} texts, heuristics only).",
            corpus.len()
        );
    }
}
