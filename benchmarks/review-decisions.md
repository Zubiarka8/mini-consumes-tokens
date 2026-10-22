# Review decisions: overhead benchmark

Measures what shared review decisions cost, in parts that are never added
together:

- **query response**: a default `find_dead_code` call (latency, bytes,
  estimated tokens, and a SHA-256 proving the bytes are unchanged);
- **catalog**: the `find_dead_code` tool entry and the whole `tools/list`
  catalog, paid once per session by clients that load every schema;
- **review data**: the scan, the review keys, reading the store files, parsing
  and resolving them;
- **memory**: peak resident memory of one load, in its own process;
- **CLI**: `duplicates` with the cache and with `--reviewed`.

Design: `docs/superpowers/specs/2026-10-22-shared-review-decisions-design.md`
(§12); the decision is recorded in ADR-019.

## Gates (§12), final build `cccb522`

| gate | measured | result |
|---|---|---|
| `reviewed` omitted: identical bytes | SHA-256 identical for both formats and the catalog, in all 18 runs | met |
| `reviewed` omitted: p50 within 2% of baseline | nine alternating runs: toon 20.06 → 20.02 ms (−0.2%), text 20.08 → 20.07 ms (−0.1%) | met; resolution about ±1% (see Methodology) |
| 1,000 rows: at most +5 ms | keys 1.5 + load 0.39 + resolve 0.08 ≈ +2.0 ms | met |
| 100,000 rows (`c3`): at most +50 ms | load 40.7 + resolve 2.5 ≈ +43.2 ms | met |
| 100,000 rows: at most +64 MiB peak | `c3` 77.4 MB against 23.2 MB with no store: +54.2 MB ≈ +51.7 MiB | met |
| 100,000 rows, all formats | load 142 ms + resolve 6.4 ms; peak +145 MB | not gated; reported |
| false suppression of unknown content | unit test for an empty hash; browser scenario B (below) | met |

Before this pass (`e79001b`), the same 100,000-row load was 51.9 ms and the
peak +98.4 MB, so the two misses were 1.04× and 1.5× the gate; both are now
met. The gates were not relaxed.

## Methodology

- Release build; one machine; the dataset is an untracked copy of the
  repository at the base commit (`e68aec9`), so no developer index is used.
- `review_overhead` runs 5 untimed warmup calls, then 50 timed ones; the
  tables report p50 and p95.
- Default output: nine alternating process runs per binary. The baseline is
  `2b814a9`, which has the same index and dead-code code as `e68aec9` plus
  the benchmark itself; the candidate is `cccb522`.
- Review data and memory: three process runs each; medians are reported.
- Peak memory: `/usr/bin/time -l` on a process that only loads a prepared
  store (`--load`), so the file generation is not counted.
- Tokens are the project's `EstimatedCounter`, not a model tokenizer.
- Latency is in-process (no stdio transport). No CPU isolation: the machine
  had 10 cores and a one-minute load average of 3.5 to 7 during the runs.
  Identical default runs spread from 19.6 to 22.8 ms, about ±7%, so a 2% gate
  cannot be read from three runs. Nine alternating runs give medians whose
  resolution is about ±1%, which is what the gate verdict relies on.

```sh
B=<scratch directory>
git -C <checkout> archive e68aec9 | tar -x -C "$B"
cargo build --release -p mct-cli --bin mct-cli
cargo build --release -p mct-mcp-server --example review_overhead
target/release/mct-cli --root "$B" init
target/release/examples/review_overhead --root "$B" --iterations 50
# memory, one store per process
target/release/examples/review_overhead --root "$B" --prepare c3 100000 <dir>
/usr/bin/time -l target/release/examples/review_overhead --root "$B" --load <dir>
```

## Environment

Apple M4 (10 cores), macOS 27.0, rustc 1.98.1 locally (CI pins 1.96.0).
Dataset: 926 indexed files, 202 dead-code candidates.

## Baseline (P0, base `e68aec9`, one run)

| format | p50 µs | p95 µs | bytes | EstimatedCounter | SHA-256 |
|---|---|---|---|---|---|
| toon | 39204 | 42604 | 4387 | 1714 | `92c7c8d17a5ae9405ec56672dd5e76b96c0ba16a16f3ce80d61d152c52380386` |
| text | 38278 | 41227 | 2508 | 749 | `dcb1bf9f878a417c31371f850147ebafd34046768c673180e7bd58f38e77b0a6` |

| catalog entry | bytes | EstimatedCounter | SHA-256 |
|---|---|---|---|
| find_dead_code | 1334 | 551 | `acc66c0689abd225b08b658db5eed24b2c3a4d9d97177df48686705dcd80cb4c` |
| all 22 tools | 34769 | 14490 | `39ca32ff5b81e57304112ffda824803d5571268e043b21d77cff3a76f50529e3` |

The P0 figures come from an earlier session. Comparisons in this document
are always made between binaries run in the same session.

## Default query: nine alternating runs (`2b814a9` against `cccb522`)

| format | baseline median µs | baseline range µs | candidate median µs | candidate range µs | difference |
|---|---|---|---|---|---|
| toon | 20060 | 19742 – 21843 | 20017 | 19765 – 20765 | −0.2% |
| text | 20080 | 19621 – 22798 | 20068 | 19809 – 20475 | −0.1% |

Output bytes are identical in every run: toon 4387 bytes, text 2508 bytes, and
the SHA-256 of both, of `find_dead_code` and of all 22 tools equals the P0
value above. The catalog is unchanged for the default call. The projected
`reviewed` property adds 1334 → 1576 bytes (+94 estimated tokens, +0.65% of the
catalog); it is projected, not published.

## Review data (`cccb522`, medians of three runs, µs)

Review keys for the 202 candidates (file hashes and identities): p50 median
1518 µs (1503 to 1570).

Store load, `Reviews::load` on each store, and `Reviews::resolve` on the 202
candidates:

| store | rows | file bytes | load p50 | load p95 (max of 3) | resolve p50 | hidden |
|---|---|---|---|---|---|---|
| c3 | 0 | 124 | 47 | 49 | 42 | 0 |
| c3 | 1,000 | 342,630 | 394 | 460 | 84 | 100 |
| c3 | 10,000 | 3,423,802 | 3,634 | 4,093 | 325 | 202 |
| c3 | 100,000 | 33,554,388 | 40,698 | 59,332 | 2,542 | 202 |
| a2 | 100,000 | 18,633,760 | 41,323 | 55,840 | 2,366 | 202 |
| b2 | 100,000 | 24,000,331 | 56,818 | 69,241 | 1,359 | 202 |
| all | 0 | 327 | 67 | 74 | 40 | 0 |
| all | 1,000 | 849,445 | 1,093 | 1,185 | 147 | 100 |
| all | 10,000 | 8,487,893 | 13,051 | 21,186 | 875 | 202 |
| all | 100,000 | 76,188,479 | 142,426 | 198,343 | 6,405 | 202 |

Peak memory, one load and resolve per process, three runs:

| store | peak RSS | delta vs no store |
|---|---|---|
| no review files | 23.2 MB | — |
| `c3`, 100,000 rows | 77.4 MB | +54.2 MB (≈ 51.7 MiB) |
| all three formats, 100,000 rows | 168.3 MB | +145.1 MB |

## Before and after, within each session

Each row compares two binaries run alternately under the same conditions.
Medians of three runs unless stated.

| change | commit | 100,000-row `c3` load p50 | peak RSS, one load |
|---|---|---|---|
| one identity copy per row (was two) | `fb3d116` | 51.9 → 48.1 ms | 121.4 → 110.7 MB |
| `memchr` for lines (set 2) | `b167420` (part 1) | 49.7 → 47.7 ms | unchanged |
| `memchr` for fields (set 3) | `b167420` (part 2) | 47.4 → 40.1 ms | unchanged |
| line-by-line reading | `8ed8a08` | 39.6 → 39.5 ms | 110.6 → 77.0–78.3 MB |

The line-by-line reading did not change the 100,000-row time, but it made
the 10,000-row load slower by about 0.4 ms (3.2 → 3.6 ms) and the 1,000-row
load by about 0.04 ms. That cost is measured and accepted: the 1,000-row
total stays near 2 ms.

Start of this pass (`e79001b`) against the current build, the 100,000-row
`c3` load fell from 51.9 ms to 40.7 ms (−22%) and the peak from 121.4 MB to
77.4 MB (−36%). The empty-hash fix (`cccb522`) does not change the timed path
and is not separately timed.

## Where the time went

Method: a scratch harness outside the repository repeats `Reviews::load` on the
100,000-row `c3` store, and `sample` records it for 8 seconds at 1 ms. Sample
counts for the top of the stack, from one profile of the `e79001b` build (the
counts show the ranking, not exact shares):

- searching for newlines and commas in the standard library's byte search
  (`memchr_aligned`, `CharSearcher`): about 1,500 samples, the largest item;
- comparing keys in the tree map (`memcmp` and the insert path): about 1,500;
- allocating and freeing the small strings: about 730;
- validating the keys (`valid_key`): 440;
- the row parser itself: 592;
- reading the file: 341;
- UTF-8 validation of the whole file: 138.

Two of these were addressed: the newline and comma search (`memchr`) and the
second copy of each identity (one allocation and 64 bytes per row). Streaming
the file removed the whole-file buffer, which was 33.5 MB of the peak.

## Changes made and rejected

Made, each with its own tests:

1. One identity per row: the key of the map is the only copy. `Row` no longer
   stores it (`fb3d116`).
2. `memchr` for lines and fields, with the same split rules as
   `str::lines` and `str::split(',')`. `memchr` was already in the lock file as
   a transitive dependency (`b167420`).
3. Line-by-line reading: the parsers accept any `BufRead`, a file is never held
   whole while it is parsed, and it is read at most 32 MiB + 1 byte. The save
   path still reads the canonical file whole to back it up (`8ed8a08`).

Not made:

- **Binary 32-byte keys.** The memory gate now passes without them, and they
  would change every identity type in the crate. The key comparisons remain
  the largest cost after the changes above; this is the next candidate if the
  100,000-row time needs more margin.
- **Building the tree map from a sorted vector.** It would make the comparisons
  disappear during the build, but it keeps a second copy of the rows while it
  runs, which raises peak memory. This is reasoned from the standard library's
  implementation; it was not measured.
- **A parse cache for the MCP server.** It would need an invalidation rule
  keyed on the store's bytes, and no such rule is designed.
- **Changing the 32 MiB bound.** Capacity stays near 190,000 rows in total.

## Browser validation

Driven through the Chrome DevTools Protocol in headless Brave, against
scratch projects only. The driver scripts are kept outside the repository.

Scenario B: a finding without a content hash is never hidden. The index was
created by a binary from before the literal-search migration, which clears
every stored hash until the next reindex. A file unreadable during that
reindex keeps an empty hash. A false-positive decision was then saved in the
page.

| build | decision after save | `dead-code --reviewed hide` |
|---|---|---|
| before the fix (`8ed8a08`) | `false-positive`, shown as hidden | 1 hidden of 2 candidates |
| after the fix (`cccb522`) | `needs-reevaluation`, not hidden | 0 hidden of 3 candidates |

The pre-fix build hid a finding whose content was unknown. The fix is covered
by the unit test `an_empty_content_hash_is_unknown_content`.

Scenario A: legacy decisions. A `reviews.toon` (version 2) decision was
written by the B2 binary through its own web form, then read by the fixed
build:

- the detail page shows the chip "Falso positivo", the list gives state
  `false-positive`, and the default filter hides it; the CLI hides 1 of 1;
- after the function's body changed and the store was reindexed, the same
  decision shows `needs-reevaluation` and is listed; the CLI hides nothing.

Duplicate-group decisions in `duplicate-reviews.toon` were not exercised in
the browser in this pass. They are covered over HTTP by the compatibility
tests with the 6a426df binary (`CM0-1`) and by the unit tests.

## Windows locking

- The whole `mct-index` crate cannot be checked for `x86_64-pc-windows-msvc`
  on this macOS host: its C dependencies (`libz-sys`, `libsqlite3-sys`) need an
  MSVC C compiler to run their build scripts.
- The standard-library calls that the lock uses (`File::try_lock`,
  `TryLockError`) and the Windows-only retry in `open` were type-checked for
  that target in a separate crate without C dependencies: the check passes.
- Not validated: runtime behaviour on Windows (contention between two writers,
  release when a writer exits, the persistent empty `.lock` file on NTFS) and
  network filesystems. CI has a `windows-latest` job, but this branch has no
  pushed CI run, so none of this is claimed as tested.

## Locking and temporary files

Loading is untouched by the locking change, so the load and default-query
figures above apply. Save latency was not measured after the change; a save
adds one lock call and one directory scan. The crash test
(`crates/mct-index/tests/review_crash.rs`) passes in the full suite; its last
summary was 150 kills, 304 acknowledged saves, 149 lock files left behind, and
none of them blocked a later save.

## Corrections to earlier sections

The earlier P1 and P2/P4 figures came from different runs, one of them without
warm-up, and were not reproducible together. The figures that were in this file
before this pass (load about 101 ms, peak +98.4 MB, default +5.9% and +3.3%) came
from an earlier session. In this session the same `c3` store loaded in about
52 ms with the same binary, so those figures are superseded by the tables above.
