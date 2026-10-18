# Token twins validation

Validated on 2026-10-15 on macOS / Apple Silicon, on the working-tree improvements
to `feat/207-token-twins` based on `ac525cac1453f2b296588abe621d0e002ac96d66`.
Design and limitations: [ADR-008](../docs/01-architecture/adrs/008-token-twins-comparison-policies.md).

## Fixed quality cases

The real CLI integration suite loads the fixtures in
`crates/mct-cli/tests/fixtures/duplicates/`. These supplement the existing tests
for operators, nesting, visibility, comments/layout, first/last class methods,
thresholds, language separation, staleness and failures.

- `boundaries.rs`: an identical Rust pair, one inside a module on a single line.
  The original branch returned no group; the new byte-boundary regression
  returns one. This was observed failing before implementing the correction.
- `identifiers.py`: three valid two-argument functions; repeated/renamed use one
  argument twice, while distinct uses each argument once. Broad returns all
  three, consistent returns only repeated/renamed, exact returns none because
  names differ.
- `members.py`: the same pair structure for repeated versus distinct attribute
  spellings. Consistent normalization preserves attribute repetition too.
- `interpolation.py`: addition/renamed have the same expression under different
  names/static text; subtraction changes the interpolation operator. Broad
  returns three, consistent only addition/renamed, exact none.
- `scopes.py`: repeated/renamed preserve spelling patterns through a shadowed
  parameter. Distinct renames the inner binding independently and is excluded by
  consistent mode, although binding-aware renaming could prove equivalence.
  This explicitly guards the documented scope-resolution limitation. A token
  threshold excludes the smaller nested functions from this outer-pair test.
- A separate exact-mode test places the same named function in two files: equal
  token text groups; changing only the numeric literal separates the copies.

For the first three Python triplets, the stricter oracle has one expected pair
per triplet. Broad produces 9 candidate pairs: 3 expected, 6 extra relative to
that oracle (precision 3/9, recall 3/3). Consistent produces the 3 expected pairs
and no extras (precision 3/3, recall 3/3). Those extra broad candidates are
intentional broad-policy behavior, not violations of its contract. This tiny,
constructed sample demonstrates the policy difference; it is not an estimate
of real-project precision, recall or semantic equivalence.

## Stratified source review

Scope: only the tracked `crates/` tree at `ac525ca`, extracted into a temporary
project and indexed through mct-cli. Default thresholds, current analyzer:
148 broad groups, 145 consistent groups, 41 exact groups; zero analysis skips.
The index has 517 files / 10,510 symbols and the 16 intentionally malformed
fixtures. `index_coverage: partial` correctly remains separate from
`status: complete`. These totals are for this snapshot, not the full working tree.

Sample selection was fixed by broad-mode rank: first five groups, five starting
at the midpoint (75-79), and last five (144-148). For each group, inspected the
complete source of its first two members through the project's MCP
`build_context_pack` tool, not just report excerpts.

- 1, `push_symbol`: useful shared bookkeeping; bodies match.
- 2, `push_relation`: useful shared empty-name guard and relation construction.
- 3, `open_indexed`: useful test setup differing in parser/count/literals.
- 4, `parse`: useful, low-value parser-test wrapper with different parser types.
- 5, dependency `open_indexed`: useful repeated fixture/registry/index setup.
- 75, exact-file versus hidden-directory scope tests: conventional repeated test
  shape; keep the distinct scenarios visible.
- 76, Bash/PowerShell `visit_children`: useful identical traversal helper.
- 77, C#/Python line-range tests: conventional assertion shape over distinct
  language contracts; shared shape does not justify merging them.
- 78, CSS `rules`: useful identical symbol-filter/range/sort helper.
- 79, Markdown/XAML module-name tests: conventional corpus contract over
  different data; normalization intentionally erases the differing symbols.
- 144, HTML/XAML `registry`: useful, low-value setup with different parser pairs.
- 145, HTML parent-resolution versus Markdown heading-hierarchy tests:
  conventional assertions covering different behaviors.
- 146, PHP versus PowerShell dependency tests: conventional assertions for
  distinct import syntax/resolution scenarios.
- 147, `server_with`: useful identical options-aware server fixture.
- 148, `build_server_at`: useful identical asynchronous server fixture.

Result for the 15 sampled pairs: 10 potentially useful consolidations, 5
conventional similarities, 0 structural mismatches. Utility is a maintainer
judgment; no consolidation was performed. No general false-positive rate or
recall claim follows from this ranked, single-project sample. It supports
keeping broad as the default and offering stricter policies explicitly.

## Scale measurements

The Unix harness [duplicate_analysis.py](duplicate_analysis.py) generates static
Python fixtures and invokes only mct-cli. It never reads SQLite itself or runs
fixture code. `init` is excluded from measurements. Each binary gets a warm-up,
then the median of repeated fresh CLI processes; wait4 provides per-process peak
RSS. It checks group member locations, names, kinds, languages, line/token counts
and order are identical between compared binaries. Added report headers change
output size independently of membership.

```sh
cargo build --release -p mct-cli
python3 benchmarks/duplicate_analysis.py /path/to/before/mct-cli target/release/mct-cli --functions 1000 --runs 5
```

Workloads: 1,000 small files forming ten groups; four large files forming ten
groups; four files with 1,000 unique operator sequences; four files forming one
1,000-member group. File count and operator sequence generation are deterministic.
This is a local comparison, not a cross-machine performance guarantee.

Candidate retrieval now uses one query, versus one symbol query per indexed
file in the original implementation (1,000 versus 1 on the small-file fixture;
4 versus 1 on the other fixtures). This count follows directly from the query
implementation; it is not SQLite tracing, and excludes unchanged hash/diagnostic
queries. Singleton excerpts are never allocated. Existing loaded source is reused
once a group forms; the first cross-file member may require one hash-checked
late read. `--no-excerpts` avoids excerpt work entirely. Output pagination still
computes complete filtered groups and totals.

The baseline is a clean, isolated release build of unmodified `ac525ca` (tracked
workspace manifests, crates and the required vendored Markdown grammar). The
candidate is this working tree's release CLI, built with the same workspace
profile. Raw results: [1,000 functions](token-twins-results.toon) (five runs) and
[10,000 functions](token-twins-scale-results.toon) (three runs). Binary 0 is the
baseline; binary 1 is the candidate.

For 1,000 functions, baseline/candidate medians in seconds:
- Many small files: 0.209318 / 0.208304.
- Few large files: 0.114336 / 0.117568.
- Unique functions: 0.115195 / 0.115245.
- Large duplicate group: 0.113081 / 0.119100.

For 10,000 functions:
- Many small files: 1.391866 / 1.390556.
- Few large files: 0.749771 / 0.752660.
- Unique functions: 0.767813 / 0.738567.
- Large duplicate group: 0.724359 / 0.744688.

Group membership, ordering and line/token counts match in all eight comparisons.
At 10,000 unique functions peak RSS changes from 115,015,680 to 113,803,264 bytes.
Other RSS changes are small; consult raw results. Reducing query count does not
produce a systematic end-to-end speedup on these workloads. Precision metadata
and coverage checks add work; delayed excerpts reduce singleton storage. Retain
these changes for bounded query count, useful controls and correctness, not a
claim that every run is faster. Earlier trials under compiler load varied enough
that no general speedup claim is justified.

## Aligned evidence cost

Same harness and workloads, comparing the pre-evidence release CLI (binary 0,
the I1-I6 candidate measured above, built from the working tree before the
evidence slice) with the release CLI that adds token roles and aligned evidence
(binary 1). Group membership, ordering and counts match in all eight
comparisons. Output grows with the `alignment` blocks. Raw results:
[token-twins-evidence-results.toon](token-twins-evidence-results.toon), with
1,000 functions (five runs) and 10,000 functions (three runs).

Medians in seconds, before / after:
- 1,000 functions: many small 0.182 / 0.194; few large 0.115 / 0.125; unique
  0.116 / 0.121; large group 0.115 / 0.126.
- 10,000 functions: many small 1.446 / 1.317; few large 0.741 / 0.811; unique
  0.744 / 0.784; large group 0.741 / 0.808.

The added work is role resolution during tokenization, plus interning and
retaining identifier/literal slots for every candidate, singletons included.
Peak RSS grows by 1-2.5 MB at 1,000 functions and by 5-8 MB at 10,000, the
largest being many small files (28.5 MB to 36.6 MB). The many-small 10,000 run
was faster after the change; treat that as noise, not a speedup. Dropping
singleton slots is the obvious memory optimization if this cost matters.

## Aligned evidence on this repository

The sample and criterion were fixed before inspecting results. Sample: this
repository's index, default thresholds and `broad`; the first five ranked groups
for `--language python`, for `--language rust`, and, added before inspection
once the whole-repository Rust sample proved to be snapshot copies, for
`--language rust --path crates`. Criterion: whether each listed difference row
names texts and lines that exist in the source, with a role that matches the
documented syntax rules. Structural precision and refactoring usefulness
belong to the future verdict phase and are not measured here.

- Python and whole-repository Rust: all ten groups are exact copies of files
  under an indexed `prompt/.../resolve-pr151-snapshot` directory. Evidence
  reports `identifier_pattern: identical` with no differences, which is correct.
  These copies say nothing about the evidence on differing members.
- Rust under `crates`: two groups are identical copies across language crates
  (`push_symbol`, `push_relation`). Three groups differ only in a parser type
  name and test literals (`open_indexed`, `parse`, depgraph helpers). Three rows
  were checked against the source (`crates/mct-lang-bash/tests/parse.rs:10`,
  `crates/mct-lang-bash/tests/index_integration.rs:28`, two literals) and were
  correct. The other rows were checked for internal consistency only. The
  parser names are unit-struct expressions, so they are reported as `name`, not
  `type`, as documented.

This sample is too small and too homogeneous for a precision figure; none is
claimed. Usability finding: groups of 10-15 members produce difference rows
listing one value per member, which is hard to read. Summarizing distinct
values for large groups is a candidate follow-up.

## Context evidence (decorators and enclosing constructs)

Cost: the same harness and inputs, release binaries. Before = the aligned
evidence slice, after = with callable context. Membership was identical in all
eight comparisons. Seconds, warm median:

| Functions | Scenario | Before | After |
|---|---|---|---|
| 1,000 | many-small (9 runs, twice) | 0.141 / 0.140 | 0.144 / 0.143 |
| 1,000 | few-large | 0.140 | 0.140 |
| 1,000 | unique | 0.132 | 0.143 |
| 1,000 | large-group | 0.131 | 0.138 |
| 10,000 | many-small | 1.956 | 2.004 |
| 10,000 | few-large | 0.850 | 0.862 |
| 10,000 | unique | 0.831 | 0.859 |
| 10,000 | large-group | 0.847 | 0.848 |

A first 5-run many-small measurement at 1,000 functions read 0.208 / 0.292. Two
9-run repeats gave the values above, so it is treated as noise. Peak RSS changed
by at most +0.4 MB at 1,000 functions and grew 1.8–3.2 MB at 10,000, because every candidate
keeps its decorator and enclosing texts.

Real code, sample and criterion fixed before inspection: the first 20 broad
groups of `--path crates --language rust` and the Python groups of the whole
repository (19 in total). The criterion is that every `differs` row and a spot
check of `same` rows match the source.

- Rust: 19 `same`, 1 `differs`. The difference is three
  `crates/mct-mcp-server/src/server.rs` methods
  (`find_references`/`find_calls`/`find_callers`, lines 1973–2083) with
  distinct `#[tool(description = ...)]` attributes inside a `#[tool_router] impl`
  (line 1201). Verified against the source. The comment before each attribute
  is not part of it.
- Enclosing labels in the sample were `(top level)` (14), `impl` (5) and
  `#[tool_router] impl` (1). Two spot checks, `push_symbol` in
  `crates/mct-lang-bash/src/lib.rs:188` (inside `impl Walker`) and
  `open_indexed` in `crates/mct-lang-bash/tests/index_integration.rs:22` (top
  level), were correct.
- Python: 19 `same`, all identical snapshot copies.

These counts describe this sample only, not a precision rate.

## Trivial-call priority

Cost: the same harness and inputs, release binaries. Before = with callable
context, after = with body shapes and priority. Group counts were identical in
all comparisons. Seconds, warm median:

| Functions | Scenario | Before | After |
|---|---|---|---|
| 1,000 | many-small (9 runs) | 0.206 | 0.233 |
| 1,000 | few-large | 0.140 | 0.143 |
| 1,000 | unique | 0.134 | 0.138 |
| 1,000 | large-group | 0.144 | 0.135 |
| 10,000 | many-small (3 runs) | 1.920 | 2.151 |
| 10,000 | few-large | 1.226 | 1.193 |
| 10,000 | unique | 1.165 | 1.176 |
| 10,000 | large-group | 1.191 | 1.193 |

The many-small difference did not reproduce. Two repeats with the binary order
swapped read 1.377 / 1.351 and 1.344 / 1.355 at 10,000 functions (5 runs), and
0.145 / 0.146 and 0.144 / 0.148 at 1,000 (9 runs), before / after. Peak RSS
grew by at most 0.2 MB at 1,000 functions and 2.3 MB at 10,000.

Real code, sample and criterion fixed before inspection: every broad group with
default thresholds of `--path crates --language rust` (154) and of
`--language python` (20). The structural criterion is that every demoted
group's members contain only single-call statements with no nested call, as
the source shows. The first five normal groups of each sample must each contain
another statement. Usefulness was noted, not scored.

- Rust: 4 demoted, 150 `other`. One is the trivial-call fixture. The other
  three are one-statement delegating wrappers:
  `impact_analysis`/`impact_analysis_toon` and
  `relation_hits`/`relation_hits_toon` in `crates/mct-mcp-server/src/format.rs`,
  and `find_callers_of_symbols`/`find_references_of_symbols` in
  `crates/mct-index/src/lib.rs`. All match the source. The first five normal
  groups have `let`, `assert` or a method chain. `parse` in
  `crates/mct-lang-*/tests/parse.rs` is a one-statement wrapper too, but
  `.parse(..).expect(..)` is a nested call, so it is not demoted (a
  conservative miss, by design).
- Python: 1 demoted (the fixture), 19 `other`, all real logic.

These counts describe this sample only, not a precision rate.

## Verification and remaining failures

- Real CLI duplicate suite: 24 passed, including the quality corpus, output
  controls, exact literals and shadowing limitation.
- Final mct-index package check: 297 passed, 2 ignored; fmt and strict CI Clippy
  passed. After the final interner selection optimization, the 29 duplicate
  analysis regressions and strict index Clippy passed again.
- CLI renderer tests: 10 passed. Python token suite (including structural string
  cases) and Rust token suite passed. Release CLI/server builds passed.
- Full `scripts/unix/check.sh`: fmt, workspace strict Clippy and evaluation passed
  (accuracy 1.000); the workspace test command stopped at the background-watcher
  binary with 1,287 passed, 3 failed, 23 ignored. This is not an all-green suite.
- The same three watcher assertions fail in a native focused rerun and in an
  isolated release build of unmodified `ac525ca` (8 passed, 3 failed, 20.18 s).
  They concern file edits, deletion/rename and ignore-rule updates. Their cause
  is unconfirmed and outside this feature; tracked as ERR-009 in errors/REGISTRY.md.
- A stale local MCP release binary initially rejected the temporary index's
  schema. Rebuilding resolved initialization and source queries (ERR-015). The
  primary index was not rebuilt or modified for this evaluation.

No commits, remote publication or production duplicate consolidation were
performed. The result remains a local candidate for maintainer review.

## Conservative type evidence (F1.3, 2026-10-15)

Predeclared evaluation protocol (before examining type results): use the current
repository index with default thresholds, broad mode, all Rust groups under
`crates` and all Python groups under `crates`. Review every reported primitive
incompatibility against each member's source and inspect the first three groups
with unknown type positions per language. Check whether the reported position is
a complete, direct annotation and the differing Rust primitive spellings exist
at the reported lines. Count observations only; do not infer precision or recall.
The index may omit unindexed files or retain stale locations; record skips.

Performance protocol: retain a release binary of the initial dirty tree before
implementation; use `duplicate_analysis.py` unchanged, all four scenarios at
1,000 (five runs) and 10,000 (three runs), identical generated inputs, warm-up
and per-process peak RSS. Repeat apparent regressions with binary order reversed.
These unannotated Python workloads measure overhead, not type coverage.

### F1.3 measurements and source evaluation

Raw paired observations: [type evidence results](token-twins-types-results.toon).
The initial order is before/after, the repeat after/before. All eight comparisons
and their reversed repeats retain group membership, member order, line counts
and token counts. The table keeps before/after column order even for repeats.
Times are seconds; RSS is the median of per-process peaks in MiB on the repeat.

| Functions | Scenario | Initial time before / after | Reversed time before / after | Reversed RSS before / after | Groups |
| --- | --- | --- | --- | --- | --- |
| 1,000 | many-small | 0.225 / 0.225 | 0.163 / 0.161 | 14.91 / 14.92 | 10 |
| 1,000 | few-large | 0.137 / 0.136 | 0.097 / 0.098 | 18.61 / 18.48 | 10 |
| 1,000 | unique | 0.131 / 0.129 | 0.094 / 0.097 | 19.77 / 19.81 | 0 |
| 1,000 | large-group | 0.134 / 0.145 | 0.099 / 0.098 | 19.19 / 19.22 | 1 |
| 10,000 | many-small | 2.142 / 2.231 | 1.524 / 1.482 | 38.16 / 38.14 | 10 |
| 10,000 | few-large | 1.046 / 1.021 | 1.126 / 0.873 | 95.27 / 105.72 | 10 |
| 10,000 | unique | 0.835 / 0.834 | 0.834 / 0.829 | 105.72 / 117.83 | 0 |
| 10,000 | large-group | 0.902 / 0.942 | 0.879 / 0.864 | 106.91 / 106.91 | 1 |

At 1,000 functions the initial large-group increase did not reproduce with
reversed order. At 10,000, timings change substantially between rounds; these
observations do not establish a general speedup or a fixed overhead percentage.
Peak RSS differences reach +12.11 MiB on a reversed observation. Few-large has
higher RSS after in both rounds (about +10.45 to +11.67 MiB); large-group and
unique differences vary with the round. The cause of these RSS changes is
unconfirmed. A local size diagnostic gives unchanged Token (56 bytes) and Slot
(12 bytes) sizes; this does not establish equal allocation or resident memory.
Do not claim memory neutrality. The unchanged generator has no annotations, so
these measurements cover the added metadata/report path, not annotation-heavy
workloads. Output adds 150 bytes per unannotated reported group.

The predeclared source sample has 153 Rust groups (3,479 candidates, 887 below
threshold) and one Python group (370 candidates, 276 below threshold). Both
analyses report complete candidate processing and zero skips, with partial
whole-index coverage. Before/after member rows and order are identical. There
are zero primitive spelling conflicts; 97 Rust groups have type-role positions
and unknown compatibility. No Python group has type-role positions. The Python
group is the pre-existing trivial-call fixture, not application-code evidence.

The first three Rust groups with type positions are push_symbol (15 members,
7 positions), push_relation (14 members, 5 positions), and open_indexed (10
members, 1 position). All 39 member signatures were checked through exact-path
CLI-backed MCP context packs. Representative source:
- crates/mct-lang-bash/src/lib.rs:188: String, SymbolKind, Location,
  Option<String>, SymbolId; the body also names SymbolRecord. All remain unknown.
- crates/mct-lang-bash/src/lib.rs:208: SymbolId, RelationKind, String, Location;
  the body names SymbolRelation. All remain unknown.
- crates/mct-lang-bash/tests/index_integration.rs:22: return type Index, unknown.

The sample contains no positive primitive-conflict case. Positive detection,
shadowed spellings, unknown members, compound annotations and all three policies
are covered by the real CLI fixtures and focused tests; that is regression
coverage, not a measured real-code precision or recall rate. Semantic
compatibility remains unknown by the maintainer's explicit policy decision.

F1.3 native check.sh: formatting and strict workspace Clippy pass; mct-eval
passes without regressions (accuracy 1.000). Tests stop at the three pre-existing
ERR-009 watcher failures: 1,312 passed, 3 failed, 23 ignored. Before implementation
the native baseline had the same failures (1,306 passed, 3 failed, 23 ignored).
The fully green check.sh criterion is not met. The F1.3 implementation report
(changes by crate, test list) was removed as superseded by this section; it is
kept in history as `benchmarks/token-twins-types-report.md` at `90a3817`.

Supplementary performance protocol, declared before running: reuse the unchanged
`few-large` generator at 1,000 and 10,000 functions, then add complete direct
parameter/return int or str annotations. Alternate the annotation by groups of
ten function indices so each structural group contains both spellings. Execute
no fixture code. After workspace checks finish, warm both release CLIs and take
five samples per binary in alternating before/after and after/before order.
Check identical ordered member rows; report times, RSS, group count and output
size. This measures the active Python annotation/conflict path as a supplement
to the unchanged unannotated benchmark suite, not a real-code precision sample.

Supplementary annotated results (before / after), five alternating paired runs:

| Functions | Seconds | Peak RSS MiB | Groups before / after | Conflict groups after |
| --- | --- | --- | --- | --- |
| 1,000 | 0.1004 / 0.1022 | 18.91 / 19.05 | 10 / 10 | 10 |
| 10,000 | 0.9157 / 0.9603 | 110.06 / 111.11 | 10 / 10 | 10 |

Ordered member rows, line counts and token counts match in every sample. All ten
groups expose two primitive spelling-conflict positions while compatibility
remains unknown. At 10,000 functions this observation is about +4.9% wall time
and +1.05 MiB peak RSS; it does not establish a fixed overhead. Output grows
from 2,807,827 to 3,068,345 bytes because conflict rows list each member's values
and lines. Rust-specific annotation performance remains unmeasured.

Full native workspace follow-up (--offline --workspace --no-fail-fast):
1,521 passed, 9 failed, 24 ignored (exit 101). The six additional failures are
one reindex_delay_e2e case and all five reindex_delay_timing cases. A rebuilt
isolated pre-increment source workspace reproduces those six failures (3 pass,
6 fail); the three background_watcher cases already failed in the native
baseline before the increment. All nine predate F1.3. No watcher code or tests
were changed; underlying filesystem/event-delivery cause remains unconfirmed.

## Python splat parameter roles (2026-10-15)

Predeclared evaluation protocol (before examining results): the current
repository index, broad mode, every Python group with `--min-lines 3
--min-tokens 20` (default thresholds give a single Python group), and every Rust
group under `crates` at default thresholds. Compare the F1.3 release binary with
the new one. Group membership and order must match; every changed difference
row is checked against the source and must be a declared `*` or `**`
parameter. Count observations only; no precision or recall is inferred.

Results: Python 38 groups (792 candidates, complete). Membership and order are
unchanged. The only changed group is the `splat.py` regression fixture, which
the index had already picked up: `args / items` and `kwargs / options` move from
merged `name` rows to one `parameter` row each at the declaration (lines 6 / 13)
plus `name` rows for their later reads; the `*rest` / `*tail` unpacking target
stays `name`. No application group in the sample has a differing splat
parameter, so this sample shows no real-code effect. Rust: 153 groups,
byte-identical reports, as expected with no Rust wrapper.

Performance: `duplicate_analysis.py` unchanged, all four scenarios at 1,000
(five runs) and 10,000 functions (three runs), in before/after and then
after/before order. Raw observations: [splat results](token-twins-splat-results.toon).
Before / after, seconds:

| Functions | Scenario | Initial order | Reversed order | Groups |
| --- | --- | --- | --- | --- |
| 1,000 | many-small | 0.114 / 0.112 | 0.073 / 0.074 | 10 |
| 1,000 | few-large | 0.074 / 0.075 | 0.048 / 0.049 | 10 |
| 1,000 | unique | 0.078 / 0.060 | 0.046 / 0.047 | 0 |
| 1,000 | large-group | 0.051 / 0.048 | 0.048 / 0.048 | 1 |
| 10,000 | many-small | 0.717 / 0.701 | 0.694 / 0.717 | 10 |
| 10,000 | few-large | 0.432 / 0.437 | 0.434 / 0.437 | 10 |
| 10,000 | unique | 0.421 / 0.423 | 0.423 / 0.426 | 0 |
| 10,000 | large-group | 0.437 / 0.446 | 0.448 / 0.476 | 1 |

Group counts and output bytes are identical in every comparison. Time
differences change sign between orders and are not attributed to the change.
Peak RSS differs by at most 0.94 MiB (1,000-function `unique`, initial order
only) except in 10,000-function `unique`, where the
before binary peaks at about 117.8 MiB and the after binary at about 105.8 MiB
in both orders. The change adds one table lookup per identifier and allocates
nothing, so a 12 MiB reduction is not attributed to it. This is the same
scenario and size as the unexplained F1.3 RSS increase; the evidence points
to binary or allocator behavior rather than the algorithm, but the cause is
unconfirmed. The generated workloads have no splat parameters, so the active
wrapper path is exercised only by the tests and the source evaluation.

## Explainable verdicts

Design and rule table: [ADR-010](../docs/01-architecture/adrs/010-duplicate-group-verdicts.md).

Evaluation protocol, fixed before inspecting any verdict:
- Index a copy of this worktree's tracked and unignored files (no `prompt/`
  snapshots) with the release CLI.
- Python: `--language python --min-lines 3 --min-tokens 20`, broad. Rust:
  `--language rust --path crates`, default thresholds.
- Run the same commands with the pre-verdict release binary. Reports must be
  identical once verdict lines are removed.
- Report the distribution and the reason frequencies, then inspect Rust ranks
  1-5 and Python ranks 1-5 against the source.
- Criterion: every reason names a difference that exists. A `REAL` group shows
  nothing beyond the declaration name, renamed parameters and docstrings.
  `SUSPICIOUS` names something a reviewer must actually judge.

Both rounds left membership, order, counts and every pre-existing line
identical: 153 Rust groups (3,505 candidates) and 7 Python groups (430
candidates), both complete with zero skips.

Round 1 (first rule draft, any one-to-one renaming of `name` positions):
- Rust: 42 `REAL`, 111 `SUSPICIOUS`. Python: 1 `REAL`, 6 `SUSPICIOUS`.
- The sample looked right. The riskiest class was `REAL` through
  `consistent-renaming`, so all 12 such Rust groups were inspected as well.
  This was an addition made after seeing the results.
- Six of the 12 were wrong. Their members reach different items through
  renamed paths, enum variants, unit structs or grammar functions:
  - groups 24 and 66, `traversal::find_calls_bfs` and
    `queries::find_references_scoped` wrappers in
    `crates/mct-index/src/lib.rs`;
  - 86, `SymbolMatchMode::Prefix` / `Fuzzy` tests;
  - 89 and 145, parser unit structs;
  - 126, `tree_sitter_python` / `tree_sitter_rust`.
- This led to the parameter-only `undeclared-names-differ` rule (ADR-010).

Round 2 (final rules; not blind, because the rule change came from round 1):

| Language | Groups | REAL | SUSPICIOUS | FALSE_POSITIVE |
|---|---|---|---|---|
| Rust (`crates`) | 153 | 36 (30 `identical-tokens`, 6 `consistent-renaming`) | 117 | 0 |
| Python | 7 | 0 | 7 | 0 |

Rust reason frequencies:

| Reason | Groups |
|---|---|
| `literal-values-differ` | 98 |
| `undeclared-names-differ` | 49 |
| `member-names-differ` | 13 |
| `identifier-pattern-differs` | 7 |
| `callee-names-differ` | 5 |
| `type-names-differ` | 4 |
| `call-only-bodies` | 4 |
| `literal-categories-differ` | 2 |
| `enclosing-context-differs` | 2 |
| `decorators-differ` | 1 |

Python: `undeclared-names-differ` 4, `member-names-differ`,
`identifier-pattern-differs` and `callee-names-differ` 3 each,
`call-only-bodies` 1. All 7 Python groups are test fixtures under
`crates/mct-cli/tests/fixtures/duplicates`.

Sample, read against the source through MCP context packs (Rust) and the
fixtures (Python):
- Rust 1 and 2, `push_symbol` (15 members) and `push_relation` (14): `REAL`
  `identical-tokens`. Correct; the copies are identical.
- Rust 3, 4 and 5, `open_indexed`, `parse` and depgraph `open_indexed`:
  `SUSPICIOUS`, `undeclared-names-differ` plus `literal-values-differ`.
  Correct: `BashParser` versus `CppParser` and the expected counts and
  fixture paths differ. These are useful consolidations, but only with a
  parameter.
- Python 1, `scopes.py`: `identifier-pattern-differs` and
  `callee-names-differ`. Correct for `distinct`. The `repeated`/`renamed` pair
  in the same group is equivalent, so this is a conservative over-flag from
  shadowing.
- Python 2, `print_report`/`log_summary`: member, callee and undeclared
  differences (`print` / `log`). Correct.
- Python 3, `shadow_a`/`shadow_b`: `identifier-pattern-differs` and
  `undeclared-names-differ`. These are equivalent loops with shadowed names,
  so this is a conservative miss.
- Python 4, `splat.py`: `undeclared-names-differ` from renamed locals
  (`head`/`first`). A consistent rename, so this is a conservative miss.
- Python 5, `pattern_a`/`pattern_b` (`a*a` versus `x*y`):
  `identifier-pattern-differs`. Correct; the data flow differs.

Of 10 sampled verdicts, none is a confident wrong label. Seven are accurate
and three are conservative `SUSPICIOUS` on equivalent code.

Of the 12 groups inspected after round 1:
- The 6 still `REAL` were checked: 33 `build_server`, 48 HTML/XAML corpus
  tests, 94 `text`, 96 `search_args`, 119 `to_relative_slash_path` and 144
  `parent`. Each differs only in its name and renamed parameters. Group 119's
  two copies, for example, differ only in `entry_path` versus `canonical`.
- The other 6 are now `SUSPICIOUS` with `undeclared-names-differ`.

These counts describe this snapshot and this sample. They are not a precision
or recall rate.

Performance: `duplicate_analysis.py` unchanged. The pre-verdict and verdict
release binaries were run at 1,000 functions (5 runs) and 10,000 (3 runs), in
both binary orders. Raw results:
[token-twins-verdicts-results.toon](token-twins-verdicts-results.toon).

Three other agents were building concurrently, and the load average was
37-72. Timings are therefore dominated by noise. For example, 10,000-function
`unique` read 0.89 / 4.74 s before/after in one order and 4.78 / 3.17 s in the
reversed order. No timing difference is attributed to the change. Medians at
1,000 functions, before / after, in seconds:

| Scenario | Initial order | Reversed order |
|---|---|---|
| many-small | 0.160 / 0.202 | 0.165 / 0.248 |
| few-large | 0.130 / 0.116 | 0.102 / 0.100 |
| unique | 0.136 / 0.117 | 0.092 / 0.091 |
| large-group | 0.128 / 0.132 | 0.096 / 0.140 |

Other observations:
- Peak RSS differs by at most about 1.4 MB, except one reversed-order
  10,000-function `few-large` observation, where the after-binary peaked 9 MB
  lower. That difference did not reproduce in the initial order and is not
  attributed to the change.
- Group counts are identical in every comparison.
- Output grows by 562 bytes on empty reports (header) and by about 173 bytes
  per reported group.
- The added work is one parameter set per member and one scan of the evidence
  per reported group, so a measurable cost is not expected. Under this load it
  could not be confirmed either way.
## Evidence subgroups (2026-10-15)

Predeclared evaluation protocol (fixed before inspecting results): a fresh
index of the feature worktree, built by the new release CLI and read by both
binaries; broad mode; every Python group with `--language python --min-lines 3
--min-tokens 20`, and every Rust group with `--language rust --path crates` at
default thresholds. Before = release CLI of the tree with aligned evidence, type
evidence and splat roles; after = the same tree plus subgroups. Member rows and
order must be identical. Every split group is inspected against its difference
rows and, where unclear, the source, and judged meaningful when the separating
items name a real difference in what the members reach, call, declare, which
literal type they use, or how values flow. Counts only; no precision.

The rule changed once during the work. The first version also separated by
decorators and enclosing text and had no identifier-pattern item. Its Rust run
(22 of 154 split) had two context-only splits of identical helpers (top level
against `#[cfg(test)] mod`). The sibling labeled corpus
(`score_duplicate_labels.py`, `--min-lines 5 --min-tokens 30`) then scored it
10 of 14 exact partitions: three context cases labeled one family were split,
and a not-one-to-one data-flow family was not. Context was removed and the
identifier pattern added; the same scorer now reports 14 of 14. Because the
change followed that score, the corpus figure is not independent validation.

Results for the final rule. Membership and order are identical in both
samples (member rows compared line by line).

Python: 10 groups, 8 split. All are regression fixtures (the worktree has no
application Python above the thresholds): `mixed.py` twice (two families; the
`value * value` data flow), `scopes.py` (one-to-one against not), `trivial.py`
twice, `evidence.py` three times (members and callees, a not-one-to-one
pattern, and a shadowing pair, which splits by spelling as documented). Each
matches its fixture's intent.

Rust: 154 groups, 25 split, 129 single, 0 not evaluated.

- Different targets, meaningful (17): the three MCP tool handlers
  (`find_references` / `find_calls` / `find_callers`, server.rs:1973-2083) and
  five test groups split by which of those tools they call; the three `*_bfs`
  wrappers in mct-index; `children` against `named_children` in five
  `visit_children` walkers (3 / 2); one `content_of` variant built with
  `iter().filter_map()` against seven built with `first()` (1 / 7); `as_str`
  against `clone` (2 / 2); `fixture_root` against `services_fixture_root`
  (4 / 1); `symbols` against `relations`; `read` against `write` lock guards;
  `search` against `hybrid`; the `*_impl` against `*_toon_impl` delegates; and
  the `insert` / `put` fixture.
- Test data, correct but weak for consolidation (6): four identifier-pattern
  splits where a test repeats one expected value in one member and uses two in
  the other (`SymbolKind::Field` twice against `Method` and `Field`;
  `Extends`/`Implements` against `Imports` twice; `Struct` against
  `Module`/`Method`; `find_symbol` twice against `find_symbol` and
  `find_references`), and two literal-category splits (string against
  character, number against string).
- Spurious (2): six `new` constructors split three ways. `mct-lang-xaml`
  initialises `import_targets` where the others have `relation_targets` (real),
  but `mct-lang-html` lists the same fields in another order, so its singleton
  reflects field order. And `parse_at` in `mct-lang-md` splits from three
  `parse` helpers only because their declaration name equals the `.parse()`
  method they call, which changes the spelling pattern.

No group was split by renaming alone. These counts describe this sample only.

Performance: `duplicate_analysis.py` unchanged. Raw observations for both rule
versions: [subgroup results](token-twins-subgroups-results.toon). The host load
average was 17-66 while other agents built the workspace, and timings swing by
up to a factor of four between rounds (10,000 many-small: 1.04-9.23 s), so no
time or RSS difference is attributed to the change. Final rule, 10,000
functions, before / after then after / before: many-small 1.49 / 1.24 and
1.04 / 1.06; few-large 0.64 / 0.74 and 0.82 / 0.83; unique 0.75 / 0.73 and
0.68 / 0.91; large-group 0.63 / 0.64 and 2.43 / 1.04. Group counts are identical
in every comparison. Output grows by 345 bytes in the header and about 195 bytes
per reported group. The workloads differ only in names, so every group is
`single`; the added work is one pass over each group's aligned slots (already
kept for every candidate) before pagination. A quiet-host remeasurement is a
follow-up.
## Repeated partial blocks

Design: [ADR-012](../docs/01-architecture/adrs/012-repeated-partial-blocks.md).
`--partial-blocks` is opt-in; every number below that says "off" is the
default report.

Protocol, fixed before running: corpus is a copy of the worktree's `crates/`
directory (tracked and untracked files, `target` excluded, so it includes the
new `blocks.{py,rs}` fixtures), indexed with `mct-cli init`. (1) Feature off:
the pre-feature release CLI and the new one must give byte-identical reports in
`broad`, `consistent` and `exact`, and with `--min-lines 1 --min-tokens 1
--ranking tokens --limit 5`. (2) Feature on, default budgets: report block
counts, work counters and `budget_exhausted` per policy. (3) Manual review of
the first 10 blocks of the default `broad` report: read the source of at least
two occurrences (through the project's MCP `build_context_pack`) and classify
each as useful (repeated logic worth a look), conventional (repetition by
convention or test data) or artifact (the boundaries or the match are wrong).
(4) Cost: `duplicate_analysis.py` at 1,000 (five runs) and 10,000 functions
(three runs), feature off before/after (10,000 also in reversed order), feature
on with defaults, and feature on with `--block-min-lines 1
--block-min-tokens 20`; plus three timed runs on the repository corpus.

### Results on this repository

- Feature off: all four comparisons byte-identical (`broad` 372,989 bytes).
- Feature on, default budgets, `broad`: 2,733 functions searched, 289,097
  windows and 172,117 comparisons (1.4% and 0.9% of their budgets), and
  `budget_exhausted: max-blocks`: 425 maximal blocks exist, the first 200 are
  listed. The block budget is the one hit on this repository; windows and
  comparisons are far from theirs. With `--block-max-blocks 100000`:
  `broad` 425 blocks, `consistent` 388, `exact` 102, no budget exhausted.
- All 425 `broad` blocks are Rust. Python under `crates` yields one block, the
  `blocks.py` fixture: the remaining Python is test corpora whose functions do
  not share runs of 40 tokens and 6 lines.
- Occurrences per block (`broad`, before the overlap fix below): 280 blocks
  with 2, 91 with 3, 48 with 4, the rest 5-13.

Manual review, first 10 blocks of the default report:

| # | Tokens / lines | Occurrences | Class and evidence |
|---|---|---|---|
| 1 | 252 / 29 | `parse` in cpp, python, rust `src/lib.rs` | Useful: parser setup, `first_error` check and module symbol, line-aligned from `let mut parser` to `push_symbol`. |
| 2 | 234 / 28 | same plus css | Useful, but the same repetition as 1 minus its last line, listed because css adds a location. |
| 3 | 223 / 26 | html, php, xaml `parse` | Useful: same setup, other tail. |
| 4 | 205 / 25 | java, lua `parse` | Useful: same setup. |
| 5 | 204 / 17 | three `a_second_reindex_skips_unchanged_files...` tests | Conventional: one test per language crate. |
| 6 | 199 / 23 | js-ts, php `parse` | Useful: same setup. |
| 7 | 190 / 19 | Python and Rust corpus tests | Conventional: tables of `(file, name, kind)` rows. |
| 8 | 184 / 23 | java, lua, xml `parse` | Useful: same setup. |
| 9 | 184 / 35 | two `omni_tools.rs` offset tests | Useful, low value: request, assert, paged request, assert; a helper could take the tool. `broad` equates `find_calls`/`find_references` and their argument structs. |
| 10 | 182 / 35 | three `omni_tools.rs` depth tests | Useful, low value: same shape. |

Every sampled boundary started and ended on a whole statement line in every
occurrence; no artifact was found. Totals: 8 useful (2 of low value), 2
conventional, 0 artifacts. Usability finding: six of the ten blocks are one
repetition, the `parse` setup of the language crates, split by which crates
share how much of the tail. A block that adds one location repeats the rest.
Summarizing such families is a candidate follow-up.

A first look at this sample, before the final rule, showed the corpus table of
block 7 twice, shifted by one row (Python lines 37-55 against Rust 27-45 and
28-46). Redundancy was then changed from "contained in a larger block" to
"overlaps a larger block" in every occurrence; a unit regression reproduces the
shift. That change removed 65 of 490 blocks. The table above is from the final
binary. This is a ranked single-project sample, not a precision rate.

### Cost

Raw results: [token-twins-blocks-results.toon](token-twins-blocks-results.toon).
Other agents were compiling throughout (load average 32-58), so the timings
below are noisy; peak RSS is steadier.

| Run | Seconds | Peak RSS MiB |
|---|---|---|
| Repository, off: before / after (3 runs each) | 0.79-0.96 / 0.79-0.93 | 34.0-35.6 / 34.2-35.7 |
| Repository, `--partial-blocks` | 0.90-1.17 | 55.5-57.1 |
| 1,000 functions, off before / after, many-small | 0.216 / 0.213 | 14.95 / 15.05 |
| 10,000 functions, off before / after, few-large (two orders) | 0.988 / 1.157, 1.139 / 1.041 | 107.5 / 107.6, 107.9 / 106.9 |
| 10,000, `--partial-blocks`, four scenarios | 1.46-3.89 | 42.2-108.4 |
| 10,000, `--partial-blocks --block-min-lines 1 --block-min-tokens 20` | 1.56-3.93 | 96.6-174.5 |

- Off: group membership, order and output bytes are identical in all
  comparisons. Time differences change sign with order and are attributed to
  load. RSS differs by at most about 1 MiB, except one 10,000 `unique` pair
  (2.2 MiB, 0.7 MiB in the reversed order), within the unexplained variation
  recorded in the sections above.
- On, defaults: the generated functions have two-line bodies, below
  `--block-min-lines 6`, so no stream is kept and peak RSS matches the off run.
  Times vary from 1.46 to 3.89 s for the same work and are not attributed to
  the change. A first round, before ineligible streams were dropped, kept every
  function's stream and peaked 31 MiB higher at 10,000 many-small.
- On, low thresholds: every function is searched. Peak RSS grows by about
  57 MiB at 10,000 functions (streams of about 32 bytes per token plus the
  window table). No block is reported: each group's members are whole-function
  copies (excluded), and the `unique` functions differ within their only body
  line, so no whole line repeats. These workloads therefore measure the search
  cost, not block quality, and no budget is reached at their sizes.
- Budget exhaustion is exercised by the unit and CLI tests (each budget) and by
  this repository's default report (`max-blocks`).

### Labeled corpus score

The sibling evaluation task's scorer (`score_duplicate_labels.py`, labeled
corpus of 31 cases, 3 labeled partial blocks, outside this worktree) was run
once against the final release CLI with `--flags "--min-lines 5 --min-tokens 30"
--partial-blocks-flag=--partial-blocks` (block thresholds at their defaults,
40 tokens and 6 lines). Result: 3 reported, 3 true positives, precision 1.000,
recall 1.000, occurrence recall 1.000, no false positive of any kind
(below-minimum, shape-only, covered-by-group, unlabeled). The algorithm was not
tuned to the labels: this was the first run against them, after the design
and the repository review above. Three labeled blocks are too few for a
precision or recall estimate.
## Labeled corpus for verdicts, subgroups and partial blocks

Before this corpus there was no ground truth for the later phases: group
verdicts, subgroup splits and partial-block repetition. The corpus and its
scorer are independent of any implementation and target the shared report
contract, so each phase can be scored as it lands.

### Corpus

[`token-twins-labels/`](token-twins-labels/) holds 15 small Python and Rust
files written to look like application code (importers, billing, handlers,
loaders, retry loops), not one-line toys. Each case is marked by a `# Case` or
`// Case` comment next to its members. Nothing is compiled or executed. The
labels are in [`labels.toon`](token-twins-labels/labels.toon).

| Category | Cases | Labels |
| --- | --- | --- |
| Consistent renaming (incl. methods and 3-member groups) | 6 | REAL |
| Literal or type differences (retry policies, bounds, integer widths) | 4 | SUSPICIOUS |
| Mixed families that should split | 3 | SUSPICIOUS, partition `split` |
| Different members, callees or data flow | 5 | FALSE_POSITIVE |
| Call-only bodies (logging, metrics, tracing) | 5 | 2 REAL, 2 FALSE_POSITIVE, 1 ambiguous |
| Decorator, attribute or enclosing-context differences | 6 | 3 REAL, 1 SUSPICIOUS, 1 FALSE_POSITIVE, 1 ambiguous |
| API-convention builders and data-only tests | 2 | ambiguous |

The totals are 31 group cases: 11 REAL, 8
SUSPICIOUS, 8 FALSE_POSITIVE and 4 ambiguous, with 69 members. All 11 REAL cases
carry the partition `single` and the 3 family cases carry `split`. The other 17
are `unscored` for partitions. There are 3 expected partial blocks (two Python,
one of them repeated three times across two files, and one Rust) with 7
occurrences. There are 3 negative regions: two below-minimum timing snippets and
one shape-only overlap between call-only groups. A further rule covers blocks
inside a reported whole-function group.

### Labeling protocol

- Labels were written from reading the source, before any verdict, subgroup or
  partial-block implementation was scored. The detector's evidence was not
  consulted to choose a label.
- `REAL`: one rule. Consolidation needs no parameter beyond names, or only wraps
  a decorator or attribute around one shared body.
- `SUSPICIOUS`: plausibly one rule, but merging needs parameters for differing
  members, callees, types, literals or registration, or the group mixes
  families.
- `FALSE_POSITIVE`: the shape coincides, but the rules or effects differ.
- `ambiguous`: reasonable reviewers disagree. Four cases carry this label:
  API-convention builders, data-only tests, same-logger field logging and a
  test oracle that re-implements production code. They are not forced into a
  level.
- Each case has a one-line rationale in `labels.toon`. Each member row gives
  its path, function, inclusive lines, family letter and whether the `broad`,
  `consistent` and `exact` policies are expected to group it.
- Verdict labels describe the labeled member set under the reference policy
  (`broad`) and analysis flags (`--min-lines 5 --min-tokens 30`). The corpus
  uses lower thresholds than the defaults so that realistic short functions
  qualify.

### Matching rules

- **Members**: same repository-relative path and start line. A case is
  matched to the reported group that shares the most of its members. It is
  `exact` only when the member sets are equal.
- **Grouping baseline**: `exact_recall` is the number of labeled groups
  reported with exactly their members, divided by the number of labeled groups
  expected under that policy. The output also lists unexpected groups and
  groups that the labels say must not form under that policy (`absent_violated`).
- **Verdicts**: `verdict.level` per group. The confusion matrix has the
  expected levels as rows and the predicted level as columns, plus `missing`
  (no level), `not-grouped` and `membership-differs`. Ambiguous cases are
  excluded from the matrix and tallied separately. `unsafe_real` counts
  SUSPICIOUS or FALSE_POSITIVE cases called REAL.
- **Subgroups**: the rows of the `subgroups` block are resolved to members. A
  reference can be a 1-based member index (0-based if any reference is 0),
  `path:start[-end]`, or a unique symbol name. A `subgroup` column in the
  members table also works. `single` without rows means one subgroup. The
  score is the exact-partition rate over the 14 scored cases. The result
  notes a `status` that contradicts the resolved references.
- **Partial blocks**: two occurrences match when they have the same path and a
  line-range Jaccard of at least 0.5 (shared lines divided by union lines).
  Each expected occurrence can be matched once. A reported block is a true
  positive when all of its occurrences, at least two, match occurrences of
  one labeled block.
  - Precision is true positives divided by reported blocks.
  - A labeled block is recalled when its matched occurrences cover all of its
    occurrences. Occurrence recall is also reported.
  - Every other block is classified as `below-minimum` or `shape-only` (each
    occurrence overlaps that negative region), `covered-by-group` (each
    occurrence lies inside members of one labeled case), or `unlabeled`.

When a report has none of an aspect's fields, that aspect is
`not-implemented`. The scorer reads TOON tolerantly. It accepts the contract's
shapes and close variants: a scalar `verdict`, `partial_blocks` as a list or as
an object holding the list, and occurrences given as `start_line`/`end_line`,
`range` or `lines`. An aspect that fails to parse becomes `scorer-error` and
the other aspects are still scored.

### Running the scorer

```sh
cargo build --release -p mct-cli
python3 benchmarks/score_duplicate_labels.py --binary target/release/mct-cli
# options: --normalization consistent, --flags "--min-lines 5 --min-tokens 30",
#          --partial-blocks-flag --partial-blocks (empty string: pass none),
#          --grouping-modes broad,consistent, --report saved.toon (no binary run)
```

The scorer copies only the corpus `.py` and `.rs` files into a temporary
project, then runs `init` and `duplicates --no-excerpts` once per grouping
policy. The policy under evaluation also gets the partial-block flag. If the
binary rejects that flag, it is rerun without it and the flag status is
`rejected`. The scorer never writes inside the repository. Its output is TOON.

### Baseline

Raw output: [token-twins-labels-baseline.toon](token-twins-labels-baseline.toon).
It was produced by the release CLI built from this branch before any Phase 2
or Phase 3 work: the `ac525ca` base plus the uncommitted evidence slices
described above.

- Grouping: `broad` reports 31 groups, and all 31 labeled groups are exact
  (recall 1.000), with no unexpected group.
- `consistent` reports 29 groups, all exact (recall 1.000). The two
  not-one-to-one FALSE_POSITIVE pairs are correctly absent, and
  `skew_points` leaves its family case.
- `exact` reports no groups, as labeled: every member has a different
  declaration name.
- Verdicts and subgroups are `not-implemented`.
- Partial blocks are `not-implemented`: the binary rejects `--partial-blocks`.

### Limitations

- The corpus is small and constructed. Rates measure agreement with these
  labels, not real-project precision or recall.
- The labels are one maintainer-style judgment. The ambiguous set and the
  rationales make that judgment reviewable; they do not remove it.
- The partial-block minimum is assumed to equal the analysis thresholds. Every
  expected block has at least 7 lines and 60 tokens, and every below-minimum
  negative shares at most 3 lines. An implementation with a much larger
  minimum would miss the expected blocks by design.
- The line-overlap rule tolerates boundary drift of about a statement. It does
  not distinguish token-level boundaries inside a line.
- Verdict labels hold for `broad`. Under other policies, a case whose group
  changes membership is reported as `membership-differs` instead of being
  scored.
- The corpus is indexed by the repository's own index like any other source.
  A repository-wide `duplicates` run lists these groups among the fixtures.


## Integrated candidate evaluation (2026-10-16)

The combined verdict/subgroup/block implementation was scored with the unchanged
31-case labeled corpus using the debug CLI (quality validation, not a timing
benchmark). [Raw result](token-twins-labels-integrated.toon). Grouping matches
31/31 broad cases and 29/29 consistent cases; all 31 expected exact absences
hold. Verdict accuracy is 0.296 on 27 determinate cases: every case is
SUSPICIOUS, including all 11 labeled REAL cases. No unsafe REAL is emitted,
but the verdicts have limited discriminatory value. The known absence of local
binding evidence remains a limitation; integration does not change its rules.
Partial blocks match 3/3 labeled blocks with no labeled false positives; three
examples cannot establish general precision. Subgroup results are not an
independent holdout, because ADR-011 records tuning after the corpus was seen.
The scorer now reads budget_exhausted from partial_blocks_budget as ADR-012
specifies; two focused Python tests cover present and absent budget output.
No performance benchmark was rerun in this integration.
