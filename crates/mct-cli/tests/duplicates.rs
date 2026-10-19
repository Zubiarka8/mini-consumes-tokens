//! End-to-end tests of `mct-cli duplicates`, running the real binary against
//! throwaway projects: the report's shape, the command's error handling, and
//! grouping through the shipped Rust and Python token sources.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A small Rust project in a fresh temp dir, removed on drop.
struct Project(PathBuf);

impl Project {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("mct-cli-duplicates-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("src/main.rs"),
            "fn alpha() {}\n\nfn main() {\n    alpha();\n}\n",
        )
        .unwrap();
        Self(dir)
    }

    /// A new project holding `files` as `(relative path, source)` pairs, then
    /// indexed. A `src/main.rs` entry replaces the default one.
    fn indexed(tag: &str, files: &[(&str, &str)]) -> Self {
        let project = Self::new(tag);
        for (path, source) in files {
            let path = project.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
        project.init();
        project
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn init(&self) {
        let output = mct(&[&"--root", &self.0, &"init"]);
        assert!(output.status.success(), "init failed: {}", stderr(&output));
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mct(args: &[&dyn AsRef<std::ffi::OsStr>]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mct-cli"))
        .args(args.iter().map(|arg| arg.as_ref()))
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The report of `duplicates` run on `project` with `args`; the run must succeed.
fn duplicates(project: &Project, args: &[&str]) -> String {
    let mut command: Vec<&dyn AsRef<std::ffi::OsStr>> = vec![&"--root", &project.0, &"duplicates"];
    for arg in args {
        command.push(arg);
    }
    let output = mct(&command);
    assert!(
        output.status.success(),
        "duplicates failed: {}",
        stderr(&output)
    );
    stdout(&output)
}

#[test]
fn exact_callable_boundaries_ignore_neighboring_syntax() {
    let project = Project::indexed(
        "byte-boundaries",
        &[(
            "src/main.rs",
            include_str!("fixtures/duplicates/boundaries.rs"),
        )],
    );
    let text = duplicates(&project, &["--min-lines", "1", "--min-tokens", "1"]);
    assert!(text.contains("total_groups: 1\n"), "{text}");
}

#[test]
fn duplicate_report_cache_is_reused_and_invalidated_by_source_changes() {
    let project = Project::new("report-cache");
    project.init();
    let first = mct(&[&"--root", &project.path(), &"duplicates"]);
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(!stderr(&first).contains("reused cached report"));

    let second = mct(&[&"--root", &project.path(), &"duplicates"]);
    assert!(second.status.success(), "{}", stderr(&second));
    assert!(stderr(&second).contains("reused cached report"));
    assert_eq!(stdout(&first), stdout(&second));

    fs::write(
        project.path().join("src/main.rs"),
        "fn alpha() { let changed = 1; }\n\nfn main() { alpha(); }\n",
    )
    .unwrap();
    let changed = mct(&[&"--root", &project.path(), &"duplicates"]);
    assert!(changed.status.success(), "{}", stderr(&changed));
    assert!(!stderr(&changed).contains("reused cached report"));

    let output_file = project.path().join("duplicates.toon");
    let saved = mct(&[
        &"--root",
        &project.path(),
        &"duplicates",
        &"--output",
        &output_file,
    ]);
    assert!(saved.status.success(), "{}", stderr(&saved));
    assert_eq!(fs::read_to_string(output_file).unwrap(), stdout(&saved));
}

#[test]
fn normalization_quality_corpus_preserves_identifier_and_interpolation_structure() {
    for (tag, source) in [
        (
            "identifiers",
            include_str!("fixtures/duplicates/identifiers.py"),
        ),
        (
            "interpolation",
            include_str!("fixtures/duplicates/interpolation.py"),
        ),
        ("members", include_str!("fixtures/duplicates/members.py")),
    ] {
        let project = Project::indexed(tag, &[("sample.py", source)]);
        for (policy, members, groups) in [("broad", 3, 1), ("consistent", 2, 1), ("exact", 0, 0)] {
            let text = duplicates(
                &project,
                &[
                    "--min-lines",
                    "1",
                    "--min-tokens",
                    "1",
                    "--normalization",
                    policy,
                    "--language",
                    "python",
                ],
            );
            assert!(
                text.contains(&format!("total_groups: {groups}\n")),
                "{tag}/{policy}: {text}"
            );
            if members > 0 {
                assert!(
                    text.contains(&format!("member_count: {members}\n")),
                    "{tag}/{policy}: {text}"
                );
            }
            if policy == "consistent" {
                let excluded = if tag == "interpolation" {
                    "subtraction"
                } else {
                    "distinct"
                };
                assert!(
                    !text.contains(&format!(",{excluded},function,")),
                    "{tag}: {text}"
                );
                assert!(text.contains(",renamed,function,"), "{tag}: {text}");
            }
        }
    }
}

/// The report text of the group whose members include `symbol`.
fn group_of<'a>(report: &'a str, symbol: &str) -> &'a str {
    let row = report
        .find(&format!(",{symbol},function,"))
        .unwrap_or_else(|| panic!("no member {symbol}: {report}"));
    let start = report[..row].rfind("  - index: ").unwrap();
    let end = report[row..]
        .find("  - index: ")
        .or_else(|| report[row..].find("skipped["))
        .map_or(report.len(), |offset| row + offset);
    &report[start..end]
}

/// Every function counts (thresholds of 1), excerpts omitted, then `extra`.
fn trivial_report(project: &Project, extra: &[&str]) -> String {
    let base = ["--min-lines", "1", "--min-tokens", "1", "--no-excerpts"];
    duplicates(project, &[&base[..], extra].concat())
}

fn evidence_report(project: &Project, policy: &str) -> String {
    trivial_report(project, &["--normalization", policy])
}

#[test]
fn splat_parameters_are_reported_apart_from_unpacking_targets_and_reads() {
    let project = Project::indexed(
        "splat",
        &[("splat.py", include_str!("fixtures/duplicates/splat.py"))],
    );
    for policy in ["broad", "consistent"] {
        let report = evidence_report(&project, policy);
        assert!(report.contains("total_groups: 1\n"), "{report}");
        let group = group_of(&report, "forward_a");
        for expected in [
            "identifier_pattern: one-to-one\n",
            "        parameter: 2\n",
            // The declared `*args` and `**kwargs` are parameters...
            "        parameter,1,args / items,6 / 13\n",
            "        parameter,1,kwargs / options,6 / 13\n",
            // ...their later reads are not...
            "        name,1,args / items,7 / 14\n",
            "        name,2,kwargs / options,9 / 16\n",
            // ...and neither is a `*rest` unpacking target.
            "        name,3,rest / tail,7 / 14\n",
            "      warnings[0]{code,detail}:\n",
        ] {
            assert!(
                group.contains(expected),
                "{policy}: {expected:?} in {group}"
            );
        }
    }
}

/// The `subgroups` block of one group's report text.
fn subgroups_of(group: &str) -> &str {
    let start = group
        .find("    subgroups:\n")
        .unwrap_or_else(|| panic!("no subgroups block: {group}"));
    let end = group[start..]
        .find("    members[")
        .map_or(group.len(), |offset| start + offset);
    &group[start..end]
}

#[test]
fn mixed_groups_split_into_evidence_subgroups_without_changing_membership() {
    let project = Project::indexed(
        "mixed",
        &[
            ("mixed.py", include_str!("fixtures/duplicates/mixed.py")),
            ("src/main.rs", include_str!("fixtures/duplicates/mixed.rs")),
        ],
    );
    for policy in ["broad", "consistent"] {
        let report = evidence_report(&project, policy);
        assert!(report.contains("subgroups_scope: "), "{report}");
        let (groups, splits) = if policy == "broad" { (5, 3) } else { (5, 2) };
        assert!(
            report.contains(&format!("total_groups: {groups}\n")),
            "{policy}: {report}"
        );
        assert!(
            report.contains(&format!("split_groups: {splits}\n")),
            "{policy}: {report}"
        );

        // A different data flow (`value * value`) separates by identifier
        // pattern under broad; consistent already keeps it out of the group.
        let flow = subgroups_of(group_of(&report, "scale_all"));
        if policy == "broad" {
            for expected in [
                "      basis[1]: identifier-pattern\n",
                "        1,2,1 2,",
                ",identifier-pattern=#1\n",
                "        2,1,\"3\",",
                ",identifier-pattern=#2\n",
            ] {
                assert!(flow.contains(expected), "{expected:?}: {flow}");
            }
        } else {
            assert!(flow.contains("      status: single\n"), "{flow}");
            assert!(!report.contains(",square_all,function,"), "{report}");
        }

        // One structural group, two families: the group keeps all four
        // members in path order and the subgroups only add a view.
        let python = group_of(&report, "user_label");
        assert!(python.contains("member_count: 4\n"), "{policy}: {python}");
        let rows = python.find(",user_label,function,").unwrap();
        for (symbol, previous) in [
            ("order_label", "user_label"),
            ("owner_label", "order_label"),
            ("invoice_label", "owner_label"),
        ] {
            let at = python.find(&format!(",{symbol},function,")).unwrap();
            let before = python.find(&format!(",{previous},function,")).unwrap();
            assert!(rows <= before && before < at, "{policy}: {python}");
        }
        let expected = "    subgroups:
      status: split
      basis[2]: member,callee
      count: 2
      separating_items: 3
      all_distinct: false
      omitted_subgroups: 0
      rows[2]{id,size,members,locations,values}:
        1,2,1 3,\"mixed.py:7 / mixed.py:23\",member=user_id; callee=load_user; member=display_name
        2,2,2 4,\"mixed.py:15 / mixed.py:31\",member=order_id; callee=load_order; member=total
";
        assert_eq!(subgroups_of(python), expected, "{policy}");

        // Rust: the same split with an odd-sized family; a singleton
        // subgroup is listed like any other.
        let rust = group_of(&report, "user_title");
        assert!(rust.contains("member_count: 3\n"), "{policy}: {rust}");
        for expected in [
            "      basis[2]: member,callee\n",
            "      count: 2\n",
            "      all_distinct: false\n",
            "        1,2,1 3,\"src/main.rs:5 / src/main.rs:17\",member=user_id; callee=load_user; \
             member=display_name\n",
            "        2,1,\"2\",\"src/main.rs:11\",member=order_id; callee=load_order; member=total\n",
        ] {
            assert!(
                subgroups_of(rust).contains(expected),
                "{policy} {expected:?}: {rust}"
            );
        }

        // Renaming only (a called parameter, a recursive call, renamed
        // locals) never separates members.
        for symbol in ["apply_each", "countdown"] {
            let single = subgroups_of(group_of(&report, symbol));
            for expected in [
                "      status: single\n",
                "      basis[0]:\n",
                "      count: 1\n",
                "      omitted_subgroups: 0\n",
                "      rows[0]{id,size,members,locations,values}:\n",
            ] {
                assert!(single.contains(expected), "{policy} {symbol}: {single}");
            }
        }
    }

    // `split_groups` counts every group, like `total_groups`, not the page.
    let page = trivial_report(&project, &["--limit", "1", "--offset", "2"]);
    for expected in ["total_groups: 5\n", "split_groups: 3\n", "groups[1]:\n"] {
        assert!(page.contains(expected), "{expected:?}: {page}");
    }
}

#[test]
fn aligned_evidence_reports_located_differences_without_changing_groups() {
    let project = Project::indexed(
        "evidence",
        &[
            (
                "evidence.py",
                include_str!("fixtures/duplicates/evidence.py"),
            ),
            (
                "src/main.rs",
                include_str!("fixtures/duplicates/evidence.rs"),
            ),
        ],
    );
    let broad = evidence_report(&project, "broad");
    assert!(
        broad.contains("alignment_not_evaluated[6]: "),
        "the report must name what it does not evaluate: {broad}"
    );

    // Consistent renaming: renamed parameters and locals, nothing to warn about.
    for symbol in ["renamed_a", "same_types_a", "untyped_a"] {
        let group = group_of(&broad, symbol);
        assert!(
            group.contains("identifier_pattern: one-to-one\n"),
            "{group}"
        );
        assert!(group.contains("warnings[0]{code,detail}:\n"), "{group}");
    }
    let renamed = group_of(&broad, "renamed_a");
    assert!(
        renamed.contains("        parameter,1,count / size,5 / 10\n"),
        "{renamed}"
    );

    // Broad groups `a*a` with `x*y`; the pattern check says why that is weak.
    let pattern = group_of(&broad, "pattern_a");
    assert!(
        pattern.contains("identifier_pattern: not-one-to-one\n"),
        "{pattern}"
    );
    assert!(
        pattern.contains("identifier-pattern-differs,\"identifier spellings are not a consistent renaming: `a` in member 1 aligns with both `x` and `y` in member 2 (line 22)"),
        "{pattern}"
    );

    // Different attributes, methods and call targets are reported apart from
    // renamed locals, with their texts and lines, and no inferred domain.
    let members = group_of(&broad, "members_a");
    for expected in [
        "        member: 2\n",
        "        callee: 1\n",
        "        member,1,email / path,28 / 34\n",
        "        member,1,send / write,29 / 35\n",
        "        callee,1,load / save,30 / 36\n",
        "        name,1,user / config,28 / 34\n",
        "member-names-differ,",
        "callee-names-differ,",
    ] {
        assert!(members.contains(expected), "{expected}: {members}");
    }

    // Different explicit types are a warning, not a claimed incompatibility;
    // equal or absent annotations produce none.
    let typed = group_of(&broad, "typed_a");
    assert!(
        typed.contains("        type,2,int / str,39 / 45\n"),
        "{typed}"
    );
    assert!(typed.contains("compatibility is not evaluated"), "{typed}");
    assert!(
        typed.contains("        literal-category,1,\"1 (number) / \\\"x\\\" (string)\",41 / 47\n"),
        "{typed}"
    );
    for symbol in ["same_types_a", "untyped_a"] {
        assert!(
            !group_of(&broad, symbol).contains("type-names-differ"),
            "{symbol}"
        );
    }

    // Same category, different value: a value change with both originals.
    let retries = group_of(&broad, "retries_a");
    assert!(
        retries.contains("        literal-value,1,3 / 5,74 / 79\n"),
        "{retries}"
    );
    assert!(retries.contains("literal-values-differ,"), "{retries}");

    // Broad keeps f-strings whole but no longer hides that they differ.
    let interpolated = group_of(&broad, "fmt_a");
    assert!(
        interpolated.contains("interpolated-strings-differ,"),
        "{interpolated}"
    );
    assert!(
        interpolated.contains("f\\\"{order.total * order.rate + 1}\\\" / f\\\"{order.total}\\\""),
        "{interpolated}"
    );

    // A docstring difference is listed but is not a warning.
    let documented = group_of(&broad, "documented_a");
    assert!(
        documented.contains("        docstring: 1\n"),
        "{documented}"
    );
    assert!(
        documented.contains("warnings[0]{code,detail}:\n"),
        "{documented}"
    );

    // Spelling, not binding: reusing `x` for two loops where the copy uses
    // `y` and `z` is reported as not one-to-one (documented limitation).
    let shadow = group_of(&broad, "shadow_a");
    assert!(
        shadow.contains("`x` in member 1 aligns with both `y` and `z` in member 2"),
        "{shadow}"
    );

    // Rust roles: fields and methods are members, the bare call is a callee,
    // `i64`/`String` are types.
    let rust = group_of(&broad, "order_total");
    for expected in [
        "        member,1,total / label,10 / 16\n",
        "        member,1,saturating_mul / repeat,10 / 16\n",
        "        callee,1,audit / trace,11 / 17\n",
        "        type,2,i64 / String,9 / 15\n",
        "literal-categories-differ,",
    ] {
        assert!(rust.contains(expected), "{expected}: {rust}");
    }

    // Evidence never changes grouping: consistent mode still splits the
    // pairs its policy separates, and still reports the evidence it keeps.
    let consistent = evidence_report(&project, "consistent");
    for split in ["pattern_a", "fmt_a", "shadow_a"] {
        assert!(
            !consistent.contains(&format!(",{split},function,")),
            "{split}: {consistent}"
        );
    }
    assert!(
        group_of(&consistent, "members_a").contains("member-names-differ,"),
        "{consistent}"
    );
}

#[test]
fn context_evidence_reports_decorators_and_enclosing_constructs_without_changing_groups() {
    let project = Project::indexed(
        "contexts",
        &[
            (
                "contexts.py",
                include_str!("fixtures/duplicates/contexts.py"),
            ),
            (
                "src/main.rs",
                include_str!("fixtures/duplicates/contexts.rs"),
            ),
        ],
    );
    // `exact` keeps the differing names, so these pairs only form under the
    // two normalizing policies.
    for policy in ["broad", "consistent"] {
        let report = evidence_report(&project, policy);
        assert!(report.contains("context_scope: "), "{report}");

        // The decorator and the enclosing `try`/`except` are warnings on a
        // group that still forms; they never split it.
        let cached = group_of(&report, "cached_total");
        for expected in [
            ",plain_total,function,",
            "      status: differs\n",
            "@functools.cache / (none)",
            "        enclosing,false,(top level)\n",
            "decorators-differ,",
        ] {
            assert!(cached.contains(expected), "{policy} {expected}: {cached}");
        }
        assert!(!cached.contains("enclosing-context-differs"), "{cached}");

        let guarded = group_of(&report, "guarded_rate");
        for expected in [
            ",fallback_rate,function,",
            "        enclosing,true,try / try > except\n",
            "        decorators,false,(none)\n",
            "enclosing-context-differs,",
            "does not by itself make the repetition false",
        ] {
            assert!(guarded.contains(expected), "{policy} {expected}: {guarded}");
        }

        // Shared decorators are shown once and raise nothing.
        let same = group_of(&report, "same_context_a");
        assert!(same.contains("      status: same\n"), "{same}");
        assert!(same.contains("        decorators,false,"), "{same}");
        assert!(same.contains("warnings[0]{code,detail}:\n"), "{same}");

        // Rust: a test module and an outer attribute are context, not body.
        let module = group_of(&report, "production_sum");
        assert!(module.contains(",helper_sum,function,"), "{module}");
        assert!(
            module.contains("(top level) / #[cfg(test)] mod"),
            "{module}"
        );
        let inline = group_of(&report, "fast_max");
        assert!(inline.contains(",slow_max,function,"), "{inline}");
        assert!(inline.contains("#[inline] / (none)"), "{inline}");
    }
}

#[test]
fn call_only_groups_are_demoted_by_shape_not_by_callee_name_and_never_removed() {
    let project = Project::indexed(
        "trivial",
        &[
            ("trivial.py", include_str!("fixtures/duplicates/trivial.py")),
            (
                "src/main.rs",
                include_str!("fixtures/duplicates/trivial.rs"),
            ),
        ],
    );
    // `exact` keeps the differing callee names, so these pairs only form
    // under the two normalizing policies.
    for policy in ["broad", "consistent"] {
        let report = trivial_report(&project, &["--normalization", policy]);
        assert!(report.contains("  trivial_calls: demote\n"), "{report}");
        assert!(report.contains("priority_scope: "), "{report}");
        assert!(report.contains("demoted_groups: 2\n"), "{policy}: {report}");

        // Only flat calls: demoted with its reason, whatever the callees are.
        for (symbol, twin) in [("log_order", "audit_order"), ("trace_order", "emit_order")] {
            let group = group_of(&report, symbol);
            for expected in [
                format!(",{twin},function,"),
                "      level: demoted\n".to_string(),
                "      call_shape: call-only\n".to_string(),
                "(5 statements".to_string(),
                "callee names are not considered".to_string(),
            ] {
                assert!(group.contains(&expected), "{policy} {expected}: {group}");
            }
        }
        // Real logic around `print`/`log` calls is not demoted.
        let logic = group_of(&report, "print_report");
        assert!(logic.contains(",log_summary,function,"), "{logic}");
        assert!(logic.contains("      level: normal\n"), "{logic}");
        assert!(logic.contains("      call_shape: other\n"), "{logic}");
        // Calls whose arguments hold another call are not flat.
        let nested = group_of(&report, "store_order");
        assert!(nested.contains(",cache_order,function,"), "{nested}");
        assert!(nested.contains("      call_shape: other\n"), "{nested}");

        // Demoted groups come after every normal group.
        let first_demoted = report.find("      level: demoted\n").unwrap();
        let last_normal = report.rfind("      level: normal\n").unwrap();
        assert!(last_normal < first_demoted, "{policy}: {report}");
    }

    // Python alone: the call-only pair outweighs the logic pair (7 lines
    // against 6), so `keep` restores it to the top and demotes nothing.
    let python = ["--language", "python"];
    let demoted = trivial_report(&project, &python);
    let kept = trivial_report(
        &project,
        &[&python[..], &["--trivial-calls", "keep"]].concat(),
    );
    let order = |report: &str| {
        let log = report.find(",log_order,function,").unwrap();
        let logic = report.find(",print_report,function,").unwrap();
        log < logic
    };
    assert!(!order(&demoted), "{demoted}");
    assert!(order(&kept), "{kept}");
    assert!(kept.contains("  trivial_calls: keep\n"), "{kept}");
    assert!(kept.contains("demoted_groups: 0\n"), "{kept}");
    let group = group_of(&kept, "log_order");
    assert!(group.contains("      level: normal\n"), "{group}");
    assert!(group.contains("      call_shape: call-only\n"), "{group}");
    assert!(
        group.contains("kept in rank order by the keep policy"),
        "{group}"
    );
    // Both policies report the same groups.
    assert_eq!(
        demoted.matches("  - index: ").count(),
        kept.matches("  - index: ").count()
    );

    let output = mct(&[
        &"--root",
        &project.path(),
        &"duplicates",
        &"--trivial-calls",
        &"hide",
    ]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("expected demote or keep"),
        "{}",
        stderr(&output)
    );
}

/// `(level, reasons line)` of the group whose members include `symbol`.
fn verdict_of(report: &str, symbol: &str) -> (String, String) {
    let group = group_of(report, symbol);
    let block = &group[group.find("    verdict:\n").unwrap()..];
    let mut lines = block.lines().skip(1);
    let level = lines.next().unwrap().trim().to_string();
    let reasons = lines.next().unwrap().trim().to_string();
    (level, reasons)
}

/// The header's verdict counts must match the levels of the listed groups.
fn assert_counts_match_listed_groups(report: &str) {
    let count = |level: &str| report.matches(&format!("      level: {level}\n")).count();
    let expected = format!(
        "verdict_counts:\n  real: {}\n  suspicious: {}\n  false_positive: {}\n",
        count("REAL"),
        count("SUSPICIOUS"),
        count("FALSE_POSITIVE"),
    );
    assert!(report.contains(&expected), "{expected} in {report}");
    assert_eq!(
        count("REAL") + count("SUSPICIOUS") + count("FALSE_POSITIVE"),
        report.matches("  - index: ").count(),
        "{report}"
    );
}

#[test]
fn verdicts_explain_each_group_from_its_evidence_without_changing_groups() {
    let project = Project::indexed(
        "verdicts",
        &[
            (
                "evidence.py",
                include_str!("fixtures/duplicates/evidence.py"),
            ),
            (
                "contexts.py",
                include_str!("fixtures/duplicates/contexts.py"),
            ),
            ("trivial.py", include_str!("fixtures/duplicates/trivial.py")),
            ("splat.py", include_str!("fixtures/duplicates/splat.py")),
            (
                "src/main.rs",
                include_str!("fixtures/duplicates/evidence.rs"),
            ),
        ],
    );

    let report = evidence_report(&project, "broad");
    assert!(report.contains("verdict_scope: "), "{report}");
    assert!(
        report.contains(
            "behavioral equivalence (verdicts are heuristic explanations and prove nothing)"
        ),
        "{report}"
    );
    assert!(report.contains("  false_positive: 0\n"), "{report}");
    assert_counts_match_listed_groups(&report);
    for (symbol, level, reasons) in [
        // Only renamed parameters and locals in the same context.
        (
            "renamed_a",
            "REAL",
            "reasons[2]: consistent-renaming,same-context",
        ),
        (
            "same_types_a",
            "REAL",
            "reasons[2]: consistent-renaming,same-context",
        ),
        (
            "same_context_a",
            "REAL",
            "reasons[2]: consistent-renaming,same-context",
        ),
        (
            "documented_a",
            "REAL",
            "reasons[3]: consistent-renaming,same-context,docstrings-differ",
        ),
        // Differences the detector cannot judge. Renamed locals are not
        // declared like parameters, so they cannot be told apart from a
        // reference to a different outside item (a conservative miss).
        (
            "forward_a",
            "SUSPICIOUS",
            "reasons[1]: undeclared-names-differ",
        ),
        (
            "pattern_a",
            "SUSPICIOUS",
            "reasons[1]: identifier-pattern-differs",
        ),
        (
            "members_a",
            "SUSPICIOUS",
            "reasons[2]: member-names-differ,callee-names-differ",
        ),
        (
            "typed_a",
            "SUSPICIOUS",
            "reasons[3]: type-names-differ,primitive-type-spellings-differ,literal-categories-differ",
        ),
        (
            "retries_a",
            "SUSPICIOUS",
            "reasons[1]: literal-values-differ",
        ),
        (
            "fmt_a",
            "SUSPICIOUS",
            "reasons[1]: interpolated-strings-differ",
        ),
        ("cached_total", "SUSPICIOUS", "reasons[1]: decorators-differ"),
        (
            "guarded_rate",
            "SUSPICIOUS",
            "reasons[1]: enclosing-context-differs",
        ),
        (
            "log_order",
            "SUSPICIOUS",
            "reasons[3]: undeclared-names-differ,member-names-differ,call-only-bodies",
        ),
        (
            "order_total",
            "SUSPICIOUS",
            "reasons[4]: member-names-differ,callee-names-differ,type-names-differ,literal-categories-differ",
        ),
    ] {
        assert_eq!(
            verdict_of(&report, symbol),
            (format!("level: {level}"), reasons.to_string()),
            "{symbol}"
        );
    }

    // A verdict never removes a group: both ranking policies list the same
    // members, and the call-only group stays suspicious when kept in rank order.
    let kept = trivial_report(&project, &["--trivial-calls", "keep"]);
    assert_counts_match_listed_groups(&kept);
    assert_eq!(
        verdict_of(&kept, "log_order").1,
        "reasons[3]: undeclared-names-differ,member-names-differ,call-only-bodies"
    );
    let members = |text: &str| -> Vec<String> {
        let mut rows: Vec<String> = text
            .lines()
            .filter(|line| line.contains(",function,"))
            .map(str::to_string)
            .collect();
        rows.sort();
        rows
    };
    assert_eq!(members(&report), members(&kept));

    // Counts follow the reported page, not the whole total.
    let page = trivial_report(&project, &["--limit", "2"]);
    assert!(page.contains("omitted_groups: "), "{page}");
    assert_counts_match_listed_groups(&page);

    // Consistent mode splits the pairs its policy separates; what remains
    // keeps its verdict.
    let consistent = evidence_report(&project, "consistent");
    assert_counts_match_listed_groups(&consistent);
    assert!(!consistent.contains(",pattern_a,function,"), "{consistent}");
    assert_eq!(verdict_of(&consistent, "renamed_a").0, "level: REAL");
}

#[test]
fn identical_copies_are_real_with_identical_tokens_in_every_policy() {
    let source = "def copy(items):\n    total = 0\n    for item in items:\n        total += item * 2\n    return total\n";
    let project = Project::indexed("verdict-identical", &[("a.py", source), ("b.py", source)]);
    for policy in ["broad", "consistent", "exact"] {
        let report = evidence_report(&project, policy);
        assert!(report.contains("total_groups: 1\n"), "{policy}: {report}");
        assert_eq!(
            verdict_of(&report, "copy"),
            (
                "level: REAL".to_string(),
                "reasons[2]: identical-tokens,same-context".to_string()
            ),
            "{policy}"
        );
        assert!(
            report.contains("verdict_counts:\n  real: 1\n  suspicious: 0\n  false_positive: 0\n"),
            "{report}"
        );
    }
}

#[test]
fn coverage_and_pagination_are_independent_of_analysis_completeness() {
    let project = Project::indexed(
        "coverage-page",
        &[
            (
                "sample.py",
                include_str!("fixtures/duplicates/identifiers.py"),
            ),
            ("broken.py", "def broken(:\n"),
        ],
    );
    let text = trivial_report(&project, &["--language", "python", "--limit", "0"]);
    for expected in [
        "status: complete\n",
        "index_coverage: partial\n",
        "index_syntax_errors: 1\n",
        "total_groups: 1\n",
        "omitted_groups: 1\n",
        "groups[0]:\n",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
}

#[test]
fn path_filter_runs_before_reading_stale_files_and_respects_directory_boundaries() {
    let project = Project::indexed(
        "path-filter",
        &[(
            "src2/twins.py",
            include_str!("fixtures/duplicates/identifiers.py"),
        )],
    );
    fs::remove_file(project.path().join("src2/twins.py")).unwrap();
    let text = duplicates(&project, &["--path", "src"]);
    assert!(text.contains("total_skipped: 0\n"), "{text}");
}

#[test]
fn report_controls_preserve_totals_rank_positions_and_omit_excerpts() {
    let project = Project::indexed("report-controls", &[("sample.py", "def a(x):\n    return x + x\ndef b(y):\n    return y + y\ndef c(x):\n    return x - x\ndef d(y):\n    return y - y\n")]);
    let text = trivial_report(
        &project,
        &[
            "--language",
            "python",
            "--path",
            "./sample.py",
            "--normalization",
            "consistent",
            "--ranking",
            "tokens",
            "--limit",
            "1",
            "--offset",
            "1",
        ],
    );
    for expected in [
        "total_groups: 2\n",
        "omitted_groups: 1\n",
        "groups[1]:\n",
        "  - index: 2\n",
        "consistent identifier spelling",
        "excerpts: omitted\n",
    ] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
    assert!(!text.contains("def c"), "{text}");
    let root_filter = duplicates(
        &project,
        &["--path", ".", "--min-lines", "1", "--min-tokens", "1"],
    );
    assert!(root_filter.contains("total_groups: 2\n"), "{root_filter}");
}

#[test]
fn exact_mode_matches_equal_names_across_files_and_preserves_literal_text() {
    for literal in ["1", "2"] {
        let project = Project::indexed(
            &format!("exact-literal-{literal}"),
            &[
                ("a.py", "def copy(x):\n    return x + 1\n"),
                ("b.py", &format!("def copy(x):\n    return x + {literal}\n")),
            ],
        );
        let text = duplicates(
            &project,
            &[
                "--min-lines",
                "1",
                "--min-tokens",
                "1",
                "--language",
                "python",
                "--normalization",
                "exact",
            ],
        );
        let groups = if literal == "1" { 1 } else { 0 };
        assert!(
            text.contains(&format!("total_groups: {groups}\n")),
            "{text}"
        );
    }
}

#[test]
fn consistent_mode_compares_spellings_without_claiming_scope_resolution() {
    let project = Project::indexed(
        "identifier-scopes",
        &[("scopes.py", include_str!("fixtures/duplicates/scopes.py"))],
    );
    let text = duplicates(
        &project,
        &[
            "--min-lines",
            "1",
            "--min-tokens",
            "20",
            "--normalization",
            "consistent",
            "--language",
            "python",
        ],
    );
    assert!(
        text.contains("total_groups: 1\n") && text.contains("member_count: 2\n"),
        "{text}"
    );
    assert!(!text.contains(",distinct,function,"), "{text}");
    // A shadowed parameter with another spelling is intentionally distinct,
    // even though a binding-aware alpha-renamer could prove equivalence.
}

#[test]
fn duplicate_analysis_does_not_modify_the_database() {
    let project = Project::new("read-only");
    project.init();
    let database = project.path().join(".mct-index/index.sqlite3");
    let before = fs::read(&database).unwrap();
    let modified = fs::metadata(&database).unwrap().modified().unwrap();
    duplicates(&project, &[]);
    assert_eq!(fs::read(&database).unwrap(), before);
    assert_eq!(
        fs::metadata(&database).unwrap().modified().unwrap(),
        modified
    );
}

/// Asserts the fixed header and the empty tail of a report with no groups.
fn assert_zero_group_report(text: &str, min_lines: &str, min_tokens: &str) {
    assert!(text.starts_with("notice: "), "{text}");
    assert!(text.contains("CANDIDATE duplicates"), "{text}");
    assert!(text.contains("human review"), "{text}");
    assert!(text.contains("does not prove"), "{text}");
    assert!(text.contains("\nstatus: complete\n"), "{text}");
    assert!(
        text.contains(&format!(
            "\noptions:\n  min_lines: {min_lines}\n  min_tokens: {min_tokens}\n"
        )),
        "{text}"
    );
    assert!(text.contains("\nsupported_languages["), "{text}");
    assert!(text.contains("\nunsupported_languages["), "{text}");
    assert!(text.contains("\ngenerated_code: "), "{text}");
    assert!(text.contains("\ncandidates_analyzed: "), "{text}");
    assert!(text.contains("\nbelow_threshold: "), "{text}");
    assert!(text.contains("\ntotal_groups: 0\n"), "{text}");
    assert!(text.contains("\ntotal_skipped: 0\n"), "{text}");
    assert!(
        text.contains(
            "\nskipped_by_reason:\n  read-error: 0\n  parse-error: 0\n  stale-location: 0\n  invalid-range: 0\n"
        ),
        "{text}"
    );
    assert!(text.contains("\ngroups[0]:\n"), "{text}");
    assert!(
        text.ends_with("\nskipped[0]{path,range,symbol,language,reason,detail}:\n"),
        "{text}"
    );
}

#[test]
fn duplicates_prints_a_valid_complete_report_with_default_thresholds() {
    let project = Project::new("defaults");
    project.init();

    assert_zero_group_report(&duplicates(&project, &[]), "6", "40");
}

#[test]
fn custom_thresholds_are_echoed_in_the_report() {
    let project = Project::new("custom");
    project.init();

    let text = duplicates(&project, &["--min-lines", "12", "--min-tokens", "90"]);
    assert_zero_group_report(&text, "12", "90");
}

#[test]
fn the_output_is_identical_between_runs() {
    let project = Project::new("repeat");
    project.init();

    assert_eq!(duplicates(&project, &[]), duplicates(&project, &[]));
}

#[test]
fn a_project_without_an_index_fails_clearly_and_creates_none() {
    let project = Project::new("no-index");

    let output = mct(&[&"--root", &project.path(), &"duplicates"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("no index found"), "{err}");
    assert!(err.contains("init"), "{err}");
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(
        !project.path().join(".mct-index").exists(),
        "duplicates must not create an index"
    );
}

#[test]
fn a_missing_root_fails_clearly() {
    let missing =
        std::env::temp_dir().join(format!("mct-cli-duplicates-missing-{}", std::process::id()));
    let _ = fs::remove_dir_all(&missing);

    let output = mct(&[&"--root", &missing, &"duplicates"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("is not a directory"), "{err}");
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn a_corrupt_index_fails_clearly() {
    let project = Project::new("corrupt");
    fs::create_dir_all(project.path().join(".mct-index")).unwrap();
    fs::write(
        project.path().join(".mct-index/index.sqlite3"),
        "this is not a sqlite database, just text that is long enough to be read as a header",
    )
    .unwrap();

    let output = mct(&[&"--root", &project.path(), &"duplicates"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(
        err.contains("cannot open the index") || err.contains("cannot query the index"),
        "{err}"
    );
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn invalid_thresholds_are_rejected() {
    let project = Project::new("invalid");
    project.init();

    for arg in [
        "--min-lines=0",
        "--min-lines=abc",
        "--min-lines=-4",
        "--min-tokens=0",
        "--min-tokens=1.5",
        "--min-tokens=",
    ] {
        let output = mct(&[&"--root", &project.path(), &"duplicates", &arg]);
        assert!(!output.status.success(), "`{arg}` should be rejected");
        let flag = arg.split('=').next().unwrap();
        assert!(
            stderr(&output).contains(flag),
            "error should name {flag}: {}",
            stderr(&output)
        );
        assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    }
}

#[test]
fn several_roots_are_rejected() {
    let a = Project::new("multi-a");
    let b = Project::new("multi-b");
    let output = mct(&[&"--root", &a.path(), &"--root", &b.path(), &"duplicates"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("exactly one --root"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn help_lists_the_command_and_its_thresholds() {
    let output = mct(&[&"duplicates", &"--help"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("CANDIDATE"), "{text}");
    assert!(text.contains("--min-lines"), "{text}");
    assert!(text.contains("--min-tokens"), "{text}");
    assert!(text.contains("[default: 6]"), "{text}");
    assert!(text.contains("[default: 40]"), "{text}");
    assert!(text.contains("Examples:"), "{text}");
}

/// Two Rust functions with the same structure under different names and
/// literal values, in different files, plus one with a different operator.
const RUST_TWIN_A: &str = "pub fn total(items: &[u32]) -> u32 {
    // Sum the items above the limit.
    let mut sum = 0;
    for item in items {
        if *item > 10 {
            sum += item;
        }
    }
    sum
}
";
const RUST_TWIN_B: &str = "pub fn weight(values: &[u32]) -> u32 {
    let mut acc = 1;
    for value in values {
        if *value > 99 {
            acc += value;
        }
    }
    acc
}

pub fn other(values: &[u32]) -> u32 {
    let mut acc = 1;
    for value in values {
        if *value < 99 {
            acc += value;
        }
    }
    acc
}
";
const PYTHON_TWINS: &str = "def scale(rows, factor):
    out = []
    for row in rows:
        if row > 0:
            out.append(row * factor)
    return out


def shift(cells, delta):
    \"\"\"Shift every positive cell.\"\"\"
    result = []
    for cell in cells:
        if cell > 5:
            result.append(cell * delta)
    return result
";

fn grouping_report(project: &Project) -> String {
    duplicates(project, &["--min-lines", "4", "--min-tokens", "20"])
}

#[test]
fn rust_and_python_twins_are_grouped_per_language_by_the_real_token_sources() {
    let project = Project::indexed(
        "twins",
        &[
            ("src/a.rs", RUST_TWIN_A),
            ("src/b.rs", RUST_TWIN_B),
            ("tools.py", PYTHON_TWINS),
        ],
    );

    let text = grouping_report(&project);
    assert!(text.contains("\nstatus: complete\n"), "{text}");
    assert!(
        text.contains("\nsupported_languages[2]: python,rust\n"),
        "{text}"
    );
    assert!(text.contains("\ntotal_groups: 1\n"), "{text}");
    // Same structure, other names and literals; `other` differs by one
    // operator and stays out, and the comment in `total` changes nothing. In
    // Python, `shift`'s docstring is a literal statement `scale` lacks, so
    // the two do not group here (see the next test for the matching pair).
    assert!(
        text.contains("src/a.rs,1-10,total,function,rust,"),
        "{text}"
    );
    assert!(
        text.contains("src/b.rs,1-9,weight,function,rust,"),
        "{text}"
    );
    assert!(!text.contains(",other,"), "{text}");
    assert!(
        text.contains("inspect src/a.rs:1-10, src/b.rs:1-9"),
        "{text}"
    );
    assert_eq!(text.matches("\n    language: rust\n").count(), 1, "{text}");
    assert_eq!(
        text.matches("\n    language: python\n").count(),
        0,
        "{text}"
    );
}

#[test]
fn python_twins_form_one_group_and_never_join_a_rust_group() {
    let python = PYTHON_TWINS.replace("    \"\"\"Shift every positive cell.\"\"\"\n", "");
    let project = Project::indexed(
        "python-twins",
        &[("tools.py", &python), ("src/a.rs", RUST_TWIN_A)],
    );

    let text = grouping_report(&project);
    assert!(text.contains("\ntotal_groups: 1\n"), "{text}");
    assert!(text.contains("\n    language: python\n"), "{text}");
    assert!(
        text.contains("tools.py,1-6,scale,function,python,"),
        "{text}"
    );
    assert!(
        text.contains("tools.py,9-14,shift,function,python,"),
        "{text}"
    );
    assert!(!text.contains("src/a.rs,"), "{text}");
}

/// Same tokens in the same order; only `total = total * row` moves out of
/// the `if`. In Python that indentation is syntax and changes behavior.
const PYTHON_INDENTATION_ONLY: &str = "def inside(flag, rows):
    total = 0
    for row in rows:
        if flag:
            total = total + row
            total = total * row
    return total


def outside(flag, rows):
    total = 0
    for row in rows:
        if flag:
            total = total + row
        total = total * row
    return total
";

#[test]
fn python_functions_differing_only_in_block_nesting_do_not_group() {
    let project = Project::indexed(
        "python-indentation",
        &[("tools.py", PYTHON_INDENTATION_ONLY)],
    );

    let text = grouping_report(&project);
    assert!(text.contains("\nstatus: complete\n"), "{text}");
    assert!(text.contains("\ntotal_groups: 0\n"), "{text}");
    // Both Python functions pass the thresholds; only `src/main.rs`'s two
    // tiny Rust functions fall below them.
    assert!(text.contains("\ncandidates_analyzed: 4\n"), "{text}");
    assert!(text.contains("\nbelow_threshold: 2\n"), "{text}");
}

#[test]
fn python_functions_with_the_same_nesting_but_other_layout_still_group() {
    // A one-line suite and an indented block are the same structure.
    let one_line = PYTHON_INDENTATION_ONLY
        .replace(
            "        if flag:\n            total = total + row\n        total",
            "        if flag: total = total + row\n        total",
        )
        .replace("def inside", "def unused_inside")
        .replace("def outside", "def compact");
    let source = format!("{PYTHON_INDENTATION_ONLY}\n\n{one_line}");
    let project = Project::indexed("python-layout", &[("tools.py", &source)]);

    let text = grouping_report(&project);
    assert!(text.contains("\ntotal_groups: 2\n"), "{text}");
    assert!(text.contains(",outside,function,python,"), "{text}");
    assert!(text.contains(",compact,function,python,"), "{text}");
    assert!(
        text.contains("inspect tools.py:10-16, tools.py:28-33"),
        "{text}"
    );
}

#[test]
fn the_first_and_last_methods_of_a_class_group_like_any_other() {
    // The class body starts on `first`'s first line and ends on `last`'s last
    // line; that enclosing block must not leak into either method.
    let method = |name: &str| {
        format!(
            "    def {name}(self, rows):
        total = 0
        for row in rows:
            if row:
                total = total + row
        return total
"
        )
    };
    let source = format!("class Box:\n{}\n{}", method("first"), method("last"));
    let project = Project::indexed("python-class-edges", &[("tools.py", &source)]);

    let text = grouping_report(&project);
    assert!(text.contains("\ntotal_groups: 1\n"), "{text}");
    assert!(
        text.contains("inspect tools.py:2-7, tools.py:9-14"),
        "{text}"
    );
}

#[test]
fn rust_twins_that_differ_only_in_visibility_group() {
    // A private function copied elsewhere as `pub(super)` is still a copy.
    let private = RUST_TWIN_A.replace("pub fn total", "fn total");
    let restricted = RUST_TWIN_B.replace("pub fn weight", "pub(super) fn weight");
    let project = Project::indexed(
        "rust-visibility",
        &[("src/a.rs", &private), ("src/b.rs", &restricted)],
    );

    let text = grouping_report(&project);
    assert!(text.contains("\nstatus: complete\n"), "{text}");
    assert!(text.contains("\ntotal_groups: 1\n"), "{text}");
    assert!(
        text.contains("inspect src/a.rs:1-10, src/b.rs:1-9"),
        "{text}"
    );
    assert!(!text.contains(",other,"), "{text}");
}

#[test]
fn a_file_edited_after_indexing_is_skipped_as_stale_not_grouped() {
    let project = Project::indexed(
        "stale",
        &[("src/a.rs", RUST_TWIN_A), ("src/b.rs", RUST_TWIN_B)],
    );
    fs::write(
        project.path().join("src/b.rs"),
        format!("// edited\n{RUST_TWIN_B}"),
    )
    .unwrap();

    let text = grouping_report(&project);
    assert!(text.contains("\nstatus: partial\n"), "{text}");
    assert!(text.contains("\ntotal_groups: 0\n"), "{text}");
    assert!(text.contains("\n  stale-location: 2\n"), "{text}");
    assert!(
        text.contains("src/b.rs,1-9,weight,rust,stale-location,"),
        "{text}"
    );
}

#[test]
fn primitive_spelling_conflicts_keep_compatibility_unknown_and_preserve_groups() {
    for (tag, path, source) in [
        (
            "type-rust",
            "src/main.rs",
            include_str!("fixtures/duplicates/types.rs"),
        ),
        (
            "type-python",
            "src/main.py",
            include_str!("fixtures/duplicates/types.py"),
        ),
    ] {
        let project = Project::indexed(tag, &[(path, source)]);
        let report = |mode: &str| {
            duplicates(
                &project,
                &[
                    "--min-lines",
                    "1",
                    "--min-tokens",
                    "1",
                    "--normalization",
                    mode,
                ],
            )
        };
        for mode in ["broad", "consistent"] {
            let text = report(mode);
            assert!(text.contains("total_groups: 2\n"), "{text}");
            assert_eq!(text.matches("compatibility: unknown").count(), 2, "{text}");
            assert_eq!(
                text.matches("primitive_spelling_conflicts: 2").count(),
                1,
                "{text}"
            );
            assert_eq!(
                text.matches("primitive_spelling_conflicts: 0").count(),
                1,
                "{text}"
            );
            assert!(text.contains("primitive-type-spellings-differ"), "{text}");
            let conflict_row = if tag == "type-rust" {
                "u32 / u64 / Alias (unknown),4 / 5 / 6"
            } else {
                "int / str / Custom (unknown),4 / 8 / 12"
            };
            assert!(text.contains(conflict_row), "{text}");
            assert!(text.contains("Custom") || text.contains("Alias"), "{text}");
            assert!(!text.contains("compatibility: incompatible"), "{text}");
        }
        let exact = report("exact");
        assert!(exact.contains("total_groups: 0\n"), "{exact}");
    }
}

/// A project holding the partial-block corpus: two functions per language
/// that share one run of statements but differ elsewhere, and one pair of
/// whole-function copies per language.
fn blocks_project(tag: &str) -> Project {
    Project::indexed(
        tag,
        &[
            (
                "src/blocks.rs",
                include_str!("fixtures/duplicates/blocks.rs"),
            ),
            ("blocks.py", include_str!("fixtures/duplicates/blocks.py")),
        ],
    )
}

fn blocks_report(project: &Project, extra: &[&str]) -> String {
    duplicates(project, &[&["--no-excerpts"][..], extra].concat())
}

#[test]
fn partial_blocks_report_a_shared_run_in_different_functions_but_not_whole_copies() {
    let project = blocks_project("partial-blocks");
    for policy in ["broad", "consistent"] {
        let text = blocks_report(&project, &["--partial-blocks", "--normalization", policy]);
        // The whole-function copies are groups; the shared runs are not.
        assert!(text.contains("total_groups: 2\n"), "{policy}: {text}");
        assert!(
            text.contains("partial_block_count: 2\n"),
            "{policy}: {text}"
        );
        let section = &text[text.find("partial_blocks[2]:\n").unwrap()..];
        let expected = "\
partial_blocks[2]:
  - id: 1
    language: rust
    tokens: 68
    lines: 11
    occurrences[2]{path,start_line,end_line,function}:
      src/blocks.rs,12,22,summarize_orders
      src/blocks.rs,28,38,summarize_refunds
  - id: 2
    language: python
    tokens: 47
    lines: 9
    occurrences[2]{path,start_line,end_line,function}:
      blocks.py,14,22,load_orders
      blocks.py,29,37,load_invoices
skipped[0]";
        assert!(section.starts_with(expected), "{policy}: {section}");
        for twin in ["copy_left", "twin_left"] {
            assert!(!section.contains(twin), "{policy}: {section}");
        }
        assert!(
            text.contains("partial_blocks_budget:\n  min_tokens: 40\n  min_lines: 6\n"),
            "{text}"
        );
        assert!(text.contains("  budget_exhausted: false\n"), "{text}");
        assert!(text.contains("partial_blocks_warning: "), "{text}");
        assert!(
            text.contains("extraction feasibility (variables in/out, control flow, side effects) is not evaluated"),
            "{text}"
        );
        // Repetition is now evaluated; only extractability is not.
        assert!(!text.contains("partial-block repetition"), "{text}");
        assert!(
            text.contains("\"extractability of repeated partial blocks (variables in/out, control flow, side effects)\""),
            "{text}"
        );
        assert!(!text.contains("extractable"), "{text}");
    }
    // Exact mode keeps names, so the copies are no longer groups and their
    // bodies become partial blocks; the renamed loop variable shortens the
    // Rust block and drops the Python one below the token minimum.
    let text = blocks_report(&project, &["--partial-blocks", "--normalization", "exact"]);
    assert!(text.contains("total_groups: 0\n"), "{text}");
    assert!(text.contains("partial_block_count: 3\n"), "{text}");
    assert!(
        text.contains("      src/blocks.rs,14,21,summarize_orders\n      src/blocks.rs,30,37,summarize_refunds\n"),
        "{text}"
    );
    assert!(!text.contains(",load_orders\n"), "{text}");
}

#[test]
fn partial_blocks_are_off_by_default_and_leave_the_report_unchanged() {
    let project = blocks_project("partial-blocks-off");
    let text = blocks_report(&project, &[]);
    assert!(!text.contains("partial_block"), "{text}");
    assert!(
        text.lines()
            .any(|line| line.starts_with("alignment_not_evaluated[6]: ")
                && line
                    .split(',')
                    .any(|item| item == "partial-block repetition")),
        "{text}"
    );
    assert!(text.contains("total_groups: 2\n"), "{text}");
    let on = blocks_report(&project, &["--partial-blocks"]);
    // Groups are untouched by the search: the group section is identical.
    let groups = |report: &str| {
        let start = report.find("groups[").unwrap();
        let end = report
            .find("partial_blocks[")
            .unwrap_or(report.find("skipped[").unwrap());
        report[start..end].to_string()
    };
    assert_eq!(groups(&text), groups(&on));
}

#[test]
fn exhausted_partial_block_budgets_are_named_in_the_header() {
    let project = blocks_project("partial-blocks-budgets");
    for (flag, value, exhausted, count) in [
        ("--block-max-comparisons", "1", "max-comparisons", 0),
        ("--block-max-blocks", "1", "max-blocks", 1),
        ("--block-max-windows", "60", "max-windows", 0),
    ] {
        let text = blocks_report(&project, &["--partial-blocks", flag, value]);
        assert!(
            text.contains(&format!("  budget_exhausted: {exhausted}\n")),
            "{flag}: {text}"
        );
        assert!(
            text.contains(&format!("partial_block_count: {count}\n")),
            "{flag}: {text}"
        );
    }
    let first = blocks_report(&project, &["--partial-blocks", "--block-max-blocks", "1"]);
    assert!(
        first.contains("src/blocks.rs,12,22,summarize_orders"),
        "{first}"
    );
}

#[test]
fn partial_block_thresholds_apply_and_output_is_deterministic() {
    let project = blocks_project("partial-blocks-thresholds");
    let strict = blocks_report(&project, &["--partial-blocks", "--block-min-tokens", "60"]);
    assert!(strict.contains("partial_block_count: 1\n"), "{strict}");
    assert!(strict.contains("  min_tokens: 60\n"), "{strict}");
    let tall = blocks_report(&project, &["--partial-blocks", "--block-min-lines", "10"]);
    assert!(tall.contains("partial_block_count: 1\n"), "{tall}");
    let again = blocks_report(&project, &["--partial-blocks"]);
    assert_eq!(again, blocks_report(&project, &["--partial-blocks"]));
}

#[test]
fn block_budget_flags_require_partial_blocks() {
    let project = Project::new("partial-blocks-requires");
    project.init();
    let output = mct(&[
        &"--root",
        &project.path(),
        &"duplicates",
        &"--block-min-tokens",
        &"10",
    ]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("--partial-blocks"),
        "{}",
        stderr(&output)
    );
    let output = mct(&[
        &"--root",
        &project.path(),
        &"duplicates",
        &"--partial-blocks",
        &"--block-max-windows",
        &"0",
    ]);
    assert!(!output.status.success());
    let help = stdout(&mct(&[&"duplicates", &"--help"]));
    for flag in [
        "--partial-blocks",
        "--block-min-tokens",
        "--block-min-lines",
        "--block-max-windows",
        "--block-max-comparisons",
        "--block-max-blocks",
    ] {
        assert!(help.contains(flag), "{flag}: {help}");
    }
}
