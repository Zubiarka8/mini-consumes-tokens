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
The fully green check.sh criterion is not met. See the
[full F1.3 report](token-twins-types-report.md) for scope, status and follow-up.

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
