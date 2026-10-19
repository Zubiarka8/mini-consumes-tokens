# MCP indexing and concurrent-agent isolation investigation

Status: investigation complete; temporary isolated binaries validated. Shared installation and client reconnection remain unchanged.

## Scope and outcome

- User-confirmed branch: `claude/semantic-model-eval`, base `ac525cac1453f2b296588abe621d0e002ac96d66`, dirty working tree.
- The other worktree, `claude/semantic-model-eval-run`, does not contain the discarded snapshot and is outside this investigation.
- Current-source CLI and MCP server both build successfully with a new, isolated Cargo target directory. No Rust source correction was required to achieve this.
- The freshly built CLI forcibly indexes 1,206 files and writes 19,662 symbols, exit 0. The freshly built server completes MCP initialize and `get_indexing_status`, exit 0. These figures precede this report, which itself may subsequently enter the Markdown index.
- The installed global CLI and server fail before indexing with `Attempt to migrate a database with a migration number that is too high`, exit 1. Their schema support differs from the index they are being asked to open.
- The 28 reported parse errors were not reproduced in this checkout. Its current report contains 18: 16 intentional malformed corpus fixtures and two copies in the discarded snapshot.

## Distinct failures and evidence

### Executable/schema mismatch (ERR-017)

The installed CLI's `status` command and the installed server's smoke test both fail with the same migration diagnostic. Existing local release binaries and newly built isolated binaries open the same project index successfully. No database deletion or manual schema change was needed for those successful operations.

The displayed application version is `0.2.0` for both installed and local CLI executables; that version alone does not identify their source revision or migration support.

### Cargo artifact inconsistency (ERR-018)

With the normal `target/`, `cargo build --locked -p mct-cli` exits 101 with five diagnostics: missing public `BodyShape`, `LiteralCategory`, `TokenRole`, `CallableSpan`, `TokenSource::tokenize_structural`, and the new fields on `Token` (grouped into five compiler errors). Those exports, methods and fields exist in the current sources.

`cargo check` succeeds, and the same source tree builds CLI and server successfully when `CARGO_TARGET_DIR` points to a fresh directory. This establishes that the normal artifacts affect the outcome. The exact cache invalidation mechanism and the process that produced the inconsistent artifacts were not traced. The user reports two agents operating concurrently; attributing a particular overwrite to either agent would exceed the evidence.

The temporary environment keeps its Cargo artifacts, CLI and server outside the global installation. Its launchers pin the source root and reject use after the root changes branches. They do not freeze source contents: parser/server changes still require rebuilding into that isolated target.

### Querying another corpus (ERR-016)

The connected `cargo_deps` MCP's `get_file_tree` shows `smallvec-1.16.1`. Its six files / 293 symbols belong to a dependency corpus, not this project. The earlier missing project-symbol result therefore does not demonstrate a failed project index.

The project `.mcp.json` already names the intended checkout as `--root`, but launches the server by its global executable name. Actual client registration/reconnection was not changed. An absolute isolated executable plus an explicit root is the suitable temporary client configuration.

### Expected syntax rejection and duplicate source (ERR-003)

All 18 current paths are under `tests/corpus/malformed/`. The shared corpus contract explicitly requires syntax rejection (`malformed_verdict`, `crates/mct-corpus/src/lib.rs`). Its focused test passes.

The Lua and XAML files under `prompt/github/discarded/resolve-pr151-snapshot/` have identical SHA-256 hashes to their originals. The snapshot is untracked and not excluded by the current project rules. The indexer walks permitted files on disk; Git tracking is not its source inventory.

The duplicates affect graph quality too: a current-source context pack reports both the real and archived `read_repository_file` definitions as uncertain candidates. This is more consequential than merely increasing the diagnostics count.

`index_file` records a syntax issue for the rejected file and the reindex loop continues. The successful initialization of a corpus containing invalid files confirms that these per-file diagnostics do not abort indexing. Historical script failures in `pr-body.sh` and `pr-status.ps1` do not reproduce on the current files; both pass a direct parser probe. This does not establish that all historical grammar limitations were fixed.

## Reproduced exclusion alternatives

A disposable corpus was constructed from the 18 reported malformed inputs and one valid Rust control. Both the existing local CLI and the newly built CLI reproduce:

| Configuration in the disposable corpus | Syntax diagnostics | Valid control indexed | Reindex exit |
| --- | ---: | --- | ---: |
| No custom exclusions | 18 | yes | 0 |
| Exclude `prompt/github/discarded/resolve-pr151-snapshot/` | 16 | yes | 0 |
| Also exclude `**/tests/corpus/malformed/**` | 0 | yes | 0 |

The real project's ignore files and original fixtures were not modified. These are measurements on the deliberately constructed corpus, not claims that the real project was reconfigured. Do not make malformed fixtures valid: that would invalidate the negative tests.

## Options and recommendation

| Option | Benefit | Cost / limitation | Decision |
| --- | --- | --- | --- |
| Separate Cargo target and absolute executable per agent | Avoids overwriting global executables and reusing the inconsistent target | Extra build storage; rebuild after parser/server edits | Implemented and validated for this session |
| Separate worktree and index per agent | Separates source edits, schema versions and derived database state | More disk usage; reconcile changes through Git | Recommended when agents work on different changes |
| Register the isolated server with its absolute path and explicit project root | Selects the intended executable and corpus | Client must reconnect; pointing a config at a temporary path requires managing its lifetime | Ready to use; client configuration unchanged |
| Reserve global installation for one integration/release owner | Keeps the shared installed tools coherent | Requires coordination between sessions | Recommended shared policy, not enforced here |
| Exclude only the discarded snapshot from the primary corpus | Removes archived definitions and two duplicate syntax diagnostics | Archived content then needs a separate index if it is intentionally queried | Narrow, tested alternative; not applied to project configuration |
| Separate expected-fixture diagnostics from unexpected parse failures in reports | Preserves negative tests and makes health reports useful | Requires a precise classification policy and reporting tests | Recommended product improvement |
| Exclude only intentional malformed fixtures from a production-focused index | Removes the remaining 16 expected diagnostics | That index cannot answer source queries about these fixtures | Tested alternative; do not exclude all tests or hide errors in valid source |
| Support partial extraction with explicit diagnostics and provenance | Could retain valid declarations in genuinely incomplete source | Cross-parser and possibly public-contract/schema design; risk of wrong relations | Future design only, not needed to resolve current startup failure |
| Consider polling when native watcher delivery is proven unreliable | Potential fallback for missing native events | I/O overhead; the present watcher root cause remains unconfirmed | Separate investigation; not a claimed fix |

Recommended order: use the isolated tools now; ensure agents have separate worktrees/indexes; reconnect the intended project server by absolute executable path; keep global installation under one owner; then exclude the archival snapshot if it is outside the intended corpus. Preserve the negative parser tests. No broad parser rewrite is justified by the 18 observed fixture diagnostics.

## Validation

All commands below refer to this user-confirmed source tree. Network access was disabled for Cargo checks/builds. Paths to disposable build/corpus directories are session-local; the command roles and actual outcomes are preserved here.

| Operation | Exit | Result |
| --- | ---: | --- |
| `cargo check --locked -p mct-cli` | 0 | Check metadata succeeds with the normal target |
| `cargo build --locked -p mct-cli`, normal target | 101 | Five errors referring to absent older core APIs |
| `cargo build --locked -p mct-cli`, fresh isolated target | 0 | Current-source CLI built |
| `cargo build --locked -p mct-mcp-server`, same isolated target | 0 | Current-source server built |
| Installed `mct-cli status` | 1 | Unsupported index migration version |
| `scripts/unix/mcp-smoke.sh` | 1 | Installed server fails with the migration diagnostic |
| Fresh server invoked directly: MCP initialize and status | 0 | 1,206 files / 19,662 symbols |
| Fresh CLI `reindex --force` on the project | 0 | 1,206 parsed, 18 expected fixture diagnostics |
| CLI `probe` on 18 invalid fixtures and a valid Rust control | 1 | Expected negative outcome: fixtures rejected, control accepted |
| `cargo test --locked -p mct-corpus --lib a_malformed_file_must_end_in_a_syntax_error` | 0 | 1 passed |
| Fresh CLI exclusion comparison on disposable corpus | 0 | 18 → 16 → 0, control preserved |
| `bash -n` temporary launchers and activation file | 0 | Syntax accepted |
| Temporary agent CLI launcher: status | 0 | Correct checkout index queried |

Changing `PATH` did not retarget `mcp-smoke.sh`: its output still selected the installed global executable. The successful fresh-server check therefore invoked the exact isolated binary directly rather than treating the PATH experiment as a new-server test.

No full workspace green claim is made. Watcher/reindex-delay test failures from ERR-009 remain a separate unresolved issue. No Rust sources, parser policies, project exclusions, global binaries, client configuration or Git history were changed by this investigation.

## Primary references

- [Cargo build cache and target directories](https://doc.rust-lang.org/cargo/reference/build-cache.html).
- [Tree-sitter ERROR and MISSING nodes](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html): error recovery exists at the tree level; accepting partial semantic records is a separate application contract.
- [notify 8.2.0 known problems and PollWatcher](https://docs.rs/notify/8.2.0/notify/): documents possible native-event limitations; it does not prove the cause of ERR-009 here.
