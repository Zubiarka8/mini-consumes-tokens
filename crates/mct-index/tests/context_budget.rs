//! Budgeted selection: ranking, de-duplication, pagination and fitting.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_index::{
    select, Basis, BudgetError, Candidate, ContextBudget, DetailLevel, EstimatedCounter, Evidence,
    SelectedRow, TokenCounter,
};

fn cand(id: i64, path: &str, start: u32, end: u32, evidence: Evidence, score: f64) -> Candidate {
    Candidate {
        symbol_id: id,
        path: path.to_string(),
        start_line: start,
        end_line: end,
        evidence,
        role: String::new(),
        score,
    }
}

fn def(id: i64, path: &str, line: u32, score: f64) -> Candidate {
    cand(id, path, line, line + 3, Evidence::Definition, score)
}

fn budget(max_tokens: Option<usize>, max_results: usize, detail: DetailLevel) -> ContextBudget {
    ContextBudget::new(max_tokens, max_results, detail).unwrap()
}

/// A TOON-like rendering whose size depends on level, path and truncation, so
/// the closure measures the serialized response, not just the payload.
fn render(rows: &[SelectedRow], truncated: bool) -> String {
    let mut out = String::from("rows[N]{id,path,line,level,body}:\n");
    for row in rows {
        let body = match row.level {
            DetailLevel::Overview => "",
            DetailLevel::Summary => "fn signature(a: u8) -> u8",
            DetailLevel::Detailed => {
                "fn signature(a: u8) -> u8 { let long_body = a + 1; long_body * 2 }"
            }
        };
        out.push_str(&format!(
            "  {},{},{},{},{}\n",
            row.candidate.symbol_id,
            row.candidate.path,
            row.candidate.start_line,
            row.level.as_str(),
            body
        ));
    }
    if truncated {
        out.push_str("truncated: more results, use offset\n");
    }
    out
}

fn cost(rows: &[SelectedRow], truncated: bool) -> usize {
    EstimatedCounter.count(&render(rows, truncated)).value
}

fn ids(selection: &mct_index::Selection) -> Vec<i64> {
    selection
        .rows
        .iter()
        .map(|r| r.candidate.symbol_id)
        .collect()
}

#[test]
fn zero_limits_are_rejected_and_huge_ones_clamped() {
    assert_eq!(
        ContextBudget::new(Some(0), 5, DetailLevel::Detailed),
        Err(BudgetError::ZeroMaxTokens)
    );
    assert_eq!(
        ContextBudget::new(None, 0, DetailLevel::Detailed),
        Err(BudgetError::ZeroMaxResults)
    );
    let big = ContextBudget::new(Some(usize::MAX), usize::MAX, DetailLevel::Summary).unwrap();
    assert_eq!(big.max_tokens(), Some(mct_index::MAX_BUDGET_TOKENS));
    assert_eq!(big.max_results(), mct_index::MAX_BUDGET_RESULTS);
}

#[test]
fn detail_levels_parse_order_and_default_to_the_unbudgeted_behavior() {
    assert_eq!("Summary".parse::<DetailLevel>(), Ok(DetailLevel::Summary));
    assert!(matches!(
        "full".parse::<DetailLevel>(),
        Err(BudgetError::UnknownDetail(_))
    ));
    assert!(DetailLevel::Overview < DetailLevel::Summary);
    assert!(DetailLevel::Summary < DetailLevel::Detailed);
    assert_eq!(DetailLevel::default(), DetailLevel::Detailed);
    assert_eq!(DetailLevel::Detailed.lower(), Some(DetailLevel::Summary));
    assert_eq!(DetailLevel::Overview.lower(), None);
    assert_eq!(DetailLevel::Overview.higher(), Some(DetailLevel::Summary));
}

#[test]
fn the_estimator_is_labelled_an_estimate_and_handles_unicode() {
    let counter = EstimatedCounter;
    let ascii = counter.count("fn parse_request(x)");
    assert_eq!(ascii.basis, Basis::Estimated);
    assert!(!ascii.basis.is_exact());
    assert!(Basis::Measured("o200k").is_exact());
    assert!(Basis::Billed.is_exact());
    assert_eq!(counter.count("").value, 0);
    assert_eq!(counter.count("   \n").value, 0);
    // One token per non-ASCII char: a multibyte string is never cheaper per char.
    assert_eq!(counter.count("日本語🙂").value, 4);
    assert!(counter.count(&"é".repeat(100)).value >= 100);
}

#[test]
fn without_a_token_budget_everything_up_to_max_results_is_returned() {
    let cands = (0..5).map(|i| def(i, "a.rs", i as u32 * 10, 1.0)).collect();
    let mut calls = 0;
    let sel = select(cands, 0, &budget(None, 3, DetailLevel::Detailed), |_, _| {
        calls += 1;
        0
    });
    assert_eq!(calls, 0, "cost is not consulted without max_tokens");
    assert_eq!(sel.rows.len(), 3);
    assert_eq!(sel.total, 5);
    assert_eq!(sel.next_offset, Some(3));
    assert!(sel.truncated());
    assert_eq!(sel.degraded, 0);
    assert!(sel.rows.iter().all(|r| r.level == DetailLevel::Detailed));
}

#[test]
fn empty_candidates_give_an_empty_untruncated_selection() {
    let sel = select(vec![], 0, &budget(Some(10), 5, DetailLevel::Detailed), cost);
    assert!(sel.rows.is_empty());
    assert_eq!(sel.next_offset, None);
    assert!(!sel.truncated());
    assert!(!sel.minimal());
    assert_eq!(sel.needed_min_tokens, None);
}

#[test]
fn a_roomy_budget_changes_nothing() {
    let cands = vec![def(1, "a.rs", 1, 3.0), def(2, "b.rs", 1, 2.0)];
    let sel = select(
        cands,
        0,
        &budget(Some(10_000), 10, DetailLevel::Detailed),
        cost,
    );
    assert_eq!(ids(&sel), vec![1, 2]);
    assert_eq!((sel.degraded, sel.dropped), (0, 0));
    assert!(sel.spent <= 10_000);
    assert!(!sel.truncated() && !sel.over_budget);
}

#[test]
fn an_over_budget_page_degrades_the_tail_first_then_drops_rows() {
    let cands: Vec<_> = (0..6)
        .map(|i| def(i, "src/lib.rs", i as u32 * 10, 10.0 - i as f64))
        .collect();
    let full = cost(
        &select(
            cands.clone(),
            0,
            &budget(None, 6, DetailLevel::Detailed),
            cost,
        )
        .rows,
        false,
    );
    // Allow roughly 60% of the full cost: degradation alone can satisfy it.
    let limit = full * 6 / 10;
    let sel = select(
        cands.clone(),
        0,
        &budget(Some(limit), 6, DetailLevel::Detailed),
        cost,
    );
    assert!(sel.spent <= limit, "{} > {limit}", sel.spent);
    assert!(sel.degraded > 0);
    // The most relevant row is the last to lose detail.
    let levels: Vec<_> = sel.rows.iter().map(|r| r.level).collect();
    assert!(levels.windows(2).all(|w| w[0] >= w[1]), "{levels:?}");
    assert_eq!(sel.rows[0].level, DetailLevel::Detailed);
    assert_eq!(
        sel.dropped, 0,
        "rows are only dropped once all are Overview"
    );

    // Tighter still: rows must be dropped, all survivors at Overview.
    let tight = cost(
        &select(
            cands.clone(),
            0,
            &budget(None, 2, DetailLevel::Overview),
            cost,
        )
        .rows,
        true,
    );
    let sel = select(
        cands,
        0,
        &budget(Some(tight), 6, DetailLevel::Detailed),
        cost,
    );
    assert!(sel.spent <= tight);
    assert!(sel.dropped > 0);
    assert!(sel.rows.iter().all(|r| r.level == DetailLevel::Overview));
    assert!(sel.rows.iter().all(|r| r.degraded()));
    assert_eq!(sel.next_offset, Some(sel.rows.len()));
}

#[test]
fn a_budget_below_the_minimal_metadata_gives_a_valid_empty_response() {
    let cands = vec![
        def(1, "crates/very/long/path/name.rs", 1, 1.0),
        def(2, "b.rs", 1, 0.5),
    ];
    let sel = select(cands, 0, &budget(Some(1), 5, DetailLevel::Detailed), cost);
    assert!(sel.rows.is_empty());
    assert!(sel.minimal());
    assert!(sel.over_budget);
    assert_eq!(
        sel.next_offset,
        Some(0),
        "no progress, caller must raise the budget"
    );
    let needed = sel.needed_min_tokens.unwrap();
    assert!(needed > 1);
    // Raising the budget to the reported minimum fits at least one row.
    let again = select(
        vec![
            def(1, "crates/very/long/path/name.rs", 1, 1.0),
            def(2, "b.rs", 1, 0.5),
        ],
        0,
        &budget(Some(needed), 5, DetailLevel::Detailed),
        cost,
    );
    assert!(!again.rows.is_empty());
    assert!(!again.over_budget && !again.minimal());
}

#[test]
fn a_budget_that_fits_the_empty_response_but_no_row_is_not_over_budget() {
    let cands = vec![def(1, "a/b/c/d/e/f/g/h.rs", 1, 1.0)];
    let empty = cost(&[], true);
    let sel = select(
        cands,
        0,
        &budget(Some(empty), 5, DetailLevel::Detailed),
        cost,
    );
    assert!(sel.rows.is_empty() && sel.minimal());
    assert!(!sel.over_budget);
}

#[test]
fn reduction_terminates_even_when_cost_never_shrinks() {
    let cands: Vec<_> = (0..4).map(|i| def(i, "a.rs", i as u32, 1.0)).collect();
    let mut calls = 0;
    let sel = select(
        cands,
        0,
        &budget(Some(5), 4, DetailLevel::Detailed),
        |_, _| {
            calls += 1;
            1_000
        },
    );
    assert!(sel.rows.is_empty() && sel.over_budget);
    // 4 rows * 2 degradations + 4 drops + the first call + the final probe.
    assert!(calls <= 4 * 3 + 2, "{calls}");
}

#[test]
fn exact_duplicates_collapse_but_same_name_in_other_files_do_not() {
    let cands = vec![
        def(1, "a.rs", 10, 2.0),
        def(1, "a.rs", 10, 2.0),
        def(2, "b.rs", 10, 2.0), // same line numbers, different symbol and file
        def(3, "a.rs", 10, 2.0), // same file/lines, different symbol id
    ];
    let sel = select(cands, 0, &budget(None, 10, DetailLevel::Overview), |_, _| 0);
    assert_eq!(sel.total, 3);
    assert_eq!(ids(&sel), vec![1, 3, 2]);
}

#[test]
fn contained_fragments_of_one_symbol_collapse_but_distinct_evidence_stays() {
    let mut caller = cand(1, "a.rs", 12, 14, Evidence::Relation, 1.0);
    caller.role = "caller".into();
    let mut callee = caller.clone();
    callee.role = "callee".into();
    let cands = vec![
        cand(1, "a.rs", 10, 30, Evidence::Fragment, 3.0),
        cand(1, "a.rs", 12, 14, Evidence::Fragment, 2.0), // contained: redundant
        cand(1, "a.rs", 25, 40, Evidence::Fragment, 1.5), // overlaps, not contained: kept
        cand(2, "a.rs", 12, 14, Evidence::Fragment, 1.0), // other symbol, overlapping span: kept
        caller,
        callee, // different relation on the same symbol and span: kept
    ];
    let sel = select(cands, 0, &budget(None, 10, DetailLevel::Summary), |_, _| 0);
    assert_eq!(sel.total, 5);
    let span = |r: &SelectedRow| {
        (
            r.candidate.symbol_id,
            r.candidate.start_line,
            r.candidate.end_line,
        )
    };
    let fragments: Vec<_> = sel
        .rows
        .iter()
        .filter(|r| r.candidate.evidence == Evidence::Fragment)
        .map(span)
        .collect();
    assert_eq!(fragments, vec![(1, 10, 30), (1, 25, 40), (2, 12, 14)]);
    let relations = sel
        .rows
        .iter()
        .filter(|r| r.candidate.evidence == Evidence::Relation)
        .count();
    assert_eq!(relations, 2);
}

#[test]
fn equal_scores_are_ordered_deterministically_whatever_the_input_order() {
    let base = vec![
        def(9, "z.rs", 1, 1.0),
        def(3, "a.rs", 7, 1.0),
        def(5, "a.rs", 2, 1.0),
        def(4, "a.rs", 2, 1.0),
        def(1, "m.rs", 1, f64::NAN),
        def(2, "n.rs", 1, 1.0 + 1e-9), // equal after quantization
    ];
    let expected = {
        let sel = select(
            base.clone(),
            0,
            &budget(None, 10, DetailLevel::Overview),
            |_, _| 0,
        );
        ids(&sel)
    };
    assert_eq!(expected.last(), Some(&1), "NaN ranks last");
    for rotation in 0..base.len() {
        let mut shuffled = base.clone();
        shuffled.rotate_left(rotation);
        shuffled.reverse();
        let sel = select(
            shuffled,
            0,
            &budget(None, 10, DetailLevel::Overview),
            |_, _| 0,
        );
        assert_eq!(ids(&sel), expected);
    }
}

#[test]
fn pagination_after_dedup_neither_skips_nor_repeats() {
    let mut cands: Vec<_> = (0..23)
        .map(|i| def(i, "a.rs", i as u32 * 5, 100.0 - i as f64))
        .collect();
    cands.extend(cands.clone()); // every candidate twice
    let mut seen = Vec::new();
    let mut offset = 0;
    loop {
        let sel = select(
            cands.clone(),
            offset,
            &budget(Some(400), 4, DetailLevel::Summary),
            cost,
        );
        assert_eq!(sel.total, 23);
        seen.extend(ids(&sel));
        match sel.next_offset {
            Some(next) => {
                assert!(next > offset, "paging must make progress");
                offset = next;
            }
            None => break,
        }
    }
    assert_eq!(seen, (0..23).collect::<Vec<_>>());
}

#[test]
fn pagination_resumes_exactly_where_budget_dropped_rows() {
    let cands: Vec<_> = (0..10)
        .map(|i| def(i, "a.rs", i as u32, 10.0 - i as f64))
        .collect();
    let tight = cost(
        &select(
            cands.clone(),
            0,
            &budget(None, 3, DetailLevel::Overview),
            cost,
        )
        .rows,
        true,
    );
    let first = select(
        cands.clone(),
        0,
        &budget(Some(tight), 10, DetailLevel::Overview),
        cost,
    );
    let next = first.next_offset.unwrap();
    assert_eq!(next, first.rows.len());
    let second = select(
        cands,
        next,
        &budget(Some(tight), 10, DetailLevel::Overview),
        cost,
    );
    assert_eq!(second.rows[0].candidate.symbol_id, next as i64);
}

#[test]
fn an_offset_past_the_end_is_empty_not_an_error() {
    let cands = vec![def(1, "a.rs", 1, 1.0)];
    let sel = select(cands, 99, &budget(Some(50), 5, DetailLevel::Detailed), cost);
    assert!(sel.rows.is_empty());
    assert_eq!(sel.next_offset, None);
    assert!(!sel.minimal());
}

#[test]
fn long_paths_and_unicode_count_toward_the_cost() {
    let ascii = vec![def(1, "a.rs", 1, 1.0)];
    let long = vec![def(
        1,
        &format!("docs/{}/注記.md", "ディレクトリ/".repeat(40)),
        1,
        1.0,
    )];
    let b = budget(Some(10_000), 5, DetailLevel::Overview);
    let short_cost = select(ascii, 0, &b, cost).spent;
    let long_cost = select(long, 0, &b, cost).spent;
    assert!(long_cost > short_cost + 100, "{long_cost} vs {short_cost}");
}
