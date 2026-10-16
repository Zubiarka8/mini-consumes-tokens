# F1.3 primitive spelling evidence — implementation report

Branch: `feat/207-token-twins`; base: `ac525cac1453f2b296588abe621d0e002ac96d66`.
All changes remain uncommitted on the inherited dirty tree. Earlier changes are
preserved. No commits, push, PR, dependencies, network calls, schema changes,
LanguageParser changes or MCP signature changes were introduced. Tasks and
project-management records remain coordinator-owned and untouched.

## Result and acceptance

The agreed F1.3 slice reports primitive spelling conflicts as evidence, always
with `compatibility: unknown`. Both Python builtin names and Rust primitive
names can be shadowed. A minimal Rust program with `type u32 = String` compiles;
syntax cannot prove type identity. No group is filtered, split, reordered or
classified by this evidence. Broad, consistent and exact retain their policies.

Implementation, focused tests, release comparison, documentation and the fixed
source evaluation are complete. Native check.sh exits 1 on the three pre-existing
watcher failures (ERR-009); acceptance requiring a fully green check.sh is not
established. The full --no-fail-fast workspace run exits 101: 1,521 passed, 9 failed,
24 ignored. All nine failures reproduce before the increment: three in the
initial native baseline, and six in an isolated reconstruction of the pre-slice
source with dependencies rebuilt. Their underlying cause remains unconfirmed.
The global detector objective is not complete.

## State of every requested area

Statuses of earlier increments are carried forward from the supplied handoff;
this increment changes only F1.3 and its documentation.

| Area | Status |
| --- | --- |
| F1.1 Original information | Finished |
| F1.2 Name comparison | Partial; differences shown, 30% criterion deliberately undefined |
| F1.3 Conservative type evidence | Implemented under agreed spelling-only policy; full-check acceptance blocked by ERR-009; semantic compatibility unknown |
| F1.4 Constants | Finished |
| F1.5 Trivial noise | Finished |
| F1.6 Decorators and outer context | Finished |
| F1.7 Interpolation | Partial; evidence present, broad whole-literal policy unchanged |
| Splat parameter roles | Not started |
| Rust mut pattern roles | Not started |
| Rust path segments and locals | Not started |
| Rust format placeholders | Not started |
| Singleton slot optimization | Not started |
| Large-group evidence summaries | Not started |
| Phase 2 REAL/SUSPICIOUS/FALSE_POSITIVE | Not started |
| Phase 2 recoverable subgroups | Not started |
| Phase 3 budgeted partial blocks | Not started |
| Mixed-group splitting regression | Not started; depends on Phase 2 |
| Partial-block regressions | Not started; depends on Phase 3 |
| Logging/trivial calls versus real logic regression | Finished, unchanged |
| Duplicated body_shapes test helper | Deferred to the user's decision, unchanged |
| Latest baseline verification | Re-run; failed on ERR-009 before detector changes |
| tasks/plan.md and project-management records | Untouched, coordinator-owned |

## Changes by crate

- mct-core: Token carries a primitive_spelling flag. It describes a complete,
  direct annotation's spelling, never a resolved type.
- mct-tree-sitter: generic tables for primitive spellings, annotation sites and
  transparent wrappers. Only type-role identifiers at supported complete sites
  receive the flag. Traversal remains iterative and language-independent.
- mct-lang-python: int, str, float, complex, bool and bytes spellings at direct
  parameter, return and local annotations. Qualified names, unions, Optional,
  generic arguments, parenthesized expressions and forward strings stay unknown.
- mct-lang-rust: listed primitive_type spellings in direct parameter, return and let
  annotations. References, qualified paths, generic arguments and custom names
  stay unknown. A shadowed u32 spelling is still lexical evidence only.
- mct-index: propagate the flag in compact slots; TypeEvidence counts all type
  positions and primitive spelling conflicts, including comparisons between
  nonfirst members. Mixed roles remain unknown. List at most 12 conflict rows
  while counting every position. Add primitive-type-spellings-differ warnings.
  The synthetic test tokenizer explicitly opts out of primitive metadata.
- mct-cli: render alignment.types with unknown compatibility, counts, bounded
  per-member values and lines. Add real Rust/Python fixtures. No new options.
- Documentation: ADR-009, COMMANDS.md, CHANGELOG.md, benchmark notes and raw TOON
  measurements. ERR-009/016 observations are retained in errors records.

## Tests

Seven new tests:
- Generic walker: recognition requires a type role and the correct annotation
  field; arbitrary references and other roles are not recognized.
- Python and Rust adapters: complete direct annotations versus compound,
  qualified and custom annotations; Python broad/structural token streams.
- Three pure index tests: an unknown first member cannot hide two later
  primitive spellings; equal/unknown/mixed roles never establish compatibility;
  bounded listings retain total counts and source lines.
- Real CLI regression: both language fixtures in broad and consistent retain
  two groups, exact retains zero, both direct type positions report conflicts,
  compound types do not, values/lines agree with the source, and compatibility
  is unknown. The test failed before implementation and passed afterward.

Focused results: 11 evidence tests, 10 renderer tests, both adapter tests, generic
walker test and CLI regression pass. Renderer snapshot updated. No tests were
skipped or weakened to hide a failure.

## Full check result

Baseline before code changes, both sandboxed and native: exit 1, formatting OK,
1,306 passed / 3 failed / 23 ignored, strict workspace Clippy OK, mct-eval OK
(accuracy 1.000, catalog approximately 9,021 tokens). The same three watcher
failures are recorded as ERR-009.

Final native scripts/unix/check.sh: exit 1.

| Step | Result |
| --- | --- |
| fmt | OK, no formatting changes |
| tests | 1,312 passed, 3 failed, 23 ignored |
| clippy | OK, no warnings; strict workspace/all-targets/all-features invocation |
| eval | OK, no regressions; accuracy 1.000, catalog approximately 9,021 tokens |

The failing assertions are watcher_reindexes_after_a_file_change_settles,
watcher_applies_ignore_file_edits_without_a_restart, and
watcher_drops_a_deleted_file_and_follows_a_rename_incrementally. The test command
stops at this binary, so these totals are not a full workspace test census.
The evaluation accuracy belongs to mct-eval's existing tool suite, not to duplicate
detection. Full workspace follow-up (--offline --workspace --no-fail-fast): exit 101,
1,521 passed, 9 failed, 24 ignored. Besides the three failures above, one
reindex_delay_e2e test and five reindex_delay_timing tests fail. A native isolated pre-increment source rebuild reproduces all six additional
failures (3 passed / 6 failed across those two targets). The three original
watcher failures also reproduce in the initial native baseline. All nine
failures therefore predate F1.3; their underlying cause remains unconfirmed.
No watcher test or production watcher code was changed.

## Performance

Release binaries from the same local dirty base before and after the slice;
unchanged duplicate_analysis.py generator, identical inputs, warm-up, init
excluded, fresh CLI processes. Initial runs: 5 at 1,000 functions, 3 at 10,000;
reversed repeats: 7 and 5. RSS is median per-process peak MiB on the repeat.
Column order is always before / after, even in the reversed repeat.

| Functions | Scenario | Initial seconds before / after | Reversed seconds before / after | Reversed RSS MiB before / after | Groups |
| --- | --- | --- | --- | --- | --- |
| 1,000 | many-small | 0.225 / 0.225 | 0.163 / 0.161 | 14.91 / 14.92 | 10 |
| 1,000 | few-large | 0.137 / 0.136 | 0.097 / 0.098 | 18.61 / 18.48 | 10 |
| 1,000 | unique | 0.131 / 0.129 | 0.094 / 0.097 | 19.77 / 19.81 | 0 |
| 1,000 | large-group | 0.134 / 0.145 | 0.099 / 0.098 | 19.19 / 19.22 | 1 |
| 10,000 | many-small | 2.142 / 2.231 | 1.524 / 1.482 | 38.16 / 38.14 | 10 |
| 10,000 | few-large | 1.046 / 1.021 | 1.126 / 0.873 | 95.27 / 105.72 | 10 |
| 10,000 | unique | 0.835 / 0.834 | 0.834 / 0.829 | 105.72 / 117.83 | 0 |
| 10,000 | large-group | 0.902 / 0.942 | 0.879 / 0.864 | 106.91 / 106.91 | 1 |

Membership, order, token counts and line counts match in every initial and
reversed comparison. The initial 1,000-member large-group time increase does not
reproduce. Timings vary between rounds, so no general speedup is claimed. Peak
RSS grows by up to 12.11 MiB in one repeated observation; few-large has higher RSS
after in both rounds. Other RSS differences vary with the round; the cause is
unconfirmed. Token and Slot sizes remain 56 and 12 bytes in a local diagnostic,
which does not prove equal allocation or RSS. Memory neutrality is not claimed.
These unannotated Python workloads do not measure annotation-heavy performance.
See [raw TOON](token-twins-types-results.toon) and
[protocol/details](token-twins.md#conservative-type-evidence-f13-2026-10-15).


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

## Fixed source evaluation

Sample and criterion were recorded before inspecting results: all broad groups
at default thresholds under crates, separately Rust and Python. Review every
primitive conflict and the first three groups with unknown type positions per
language. Check actual primitive spellings and complete annotation sites. No
precision or recall rate is inferred.

Results: Rust 153 groups, 3,479 candidates (887 below threshold); Python one
group, 370 candidates (276 below threshold). Both have complete candidate
processing, zero skips, partial whole-index coverage, and unchanged member rows
and order versus the baseline binary. Zero primitive spelling conflicts;
97 Rust groups have type-role positions. The Python group has no annotations
and is the existing trivial-call fixture; it supplies no application-code type
evidence. There are no positive real-code cases to evaluate in this sample.

All 39 signatures in the first three Rust groups were checked through exact-path
CLI-backed MCP context packs. Examples:
- crates/mct-lang-bash/src/lib.rs:188, push_symbol: String, SymbolKind,
  Location, Option<String>, SymbolId; body also names SymbolRecord. Seven type
  positions, all unknown, across 15 members.
- crates/mct-lang-bash/src/lib.rs:208, push_relation: SymbolId, RelationKind,
  String, Location, plus SymbolRelation in the body. Five positions, all unknown,
  across 14 members. The representative full body was also checked.
- crates/mct-lang-bash/tests/index_integration.rs:22, open_indexed: Index return
  type; one position, unknown, across 10 members.

Positive spelling conflicts, including shadowed names, are exercised by
crates/mct-cli/tests/fixtures/duplicates/types.rs:4 and types.py:4. Fixtures are
regression evidence, not a real-code precision sample.

The connected MCP index only exposed six files and could not locate
GroupEvidence (ERR-016). Source review used this project's CLI call command with
an explicit checkout root. No external SQLite access was used.

## Limitations and next increment

Semantic compatibility remains unknown by explicit decision. Primitive spelling
recognition is restricted to the supported complete annotation sites. Unknown
or compound annotations do not imply compatibility; a spelling conflict does
not prove incompatibility, semantic non-equivalence or safe extraction. No
verdicts, subgrouping or partial blocks are implemented. Existing role gaps and
large-group output limits remain. Rust-specific annotation performance is unmeasured;
the observed RSS increase is unresolved.

Next functional increment: Python splat parameter roles with adapter and CLI
regressions, preserving all mode policies. Resolve ERR-009 in a separately
owned task before claiming a green full workspace gate. Before expanding the
evidence phase, follow up on the variable RSS observations and add a Rust
annotation workload if memory cost is a release concern. Keep body_shapes helper sharing deferred to the user.
