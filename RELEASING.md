# Releasing

This is the checklist. `scripts/unix/release.sh` runs its scriptable steps
(`bump`, `dry-run`, `publish`); the CHANGELOG entry and the review stay
manual. A release has two independent channels:

1. **GitHub Release with prebuilt binaries** — automated by
   `.github/workflows/release.yml` when a `v*` tag is pushed. This is what
   `install.sh` / `install.ps1` download.
2. **crates.io** — manual, from a maintainer's machine, never from CI.
   **Currently blocked** (see § 4). Do not run `cargo publish` for any crate
   until that section says otherwise.

A tag on its own is not a release: `v0.1.0` exists, but its workflow runs
were cancelled and no GitHub Release (and no downloadable archive) exists for
it. Check https://github.com/Zubiarka8/mini-consumes-tokens/releases, not the
tag list.

## 1. Pre-flight

- [ ] `CHANGELOG.md` has a dated `[x.y.z]` section for this release (not just
      `[Unreleased]`), including upgrade steps for breaking changes.
- [ ] `[workspace.package] version` and every internal `version = "x.y.z"` in
      the root `Cargo.toml` `[workspace.dependencies]` match that version;
      `Cargo.lock` and the per-crate `crates/*/fuzz/Cargo.lock` files were
      refreshed with `cargo update --workspace` (only workspace packages
      should change).
- [ ] The README version badge matches.
- [ ] `scripts/unix/check.sh` (or `scripts\windows\check.ps1`) is clean
      locally: fmt, tests, CI-equivalent clippy, `mct-eval`.
- [ ] `python3 scripts/unix/tests/test_install.py` passes.
- [ ] CI is green on the exact commit you are about to tag.
- [ ] A dry run of the Release workflow succeeded on that commit (§ 2).

## 2. Dry-run the binary build (no release, no tag)

From the Actions tab run **Release → Run workflow** on the release branch, or:

```sh
gh workflow run release.yml --ref <branch>
gh run watch
```

A manual run builds all five targets with `--locked`, smoke-tests the four
native ones (`linux-arm64` is cross-compiled and only built and packaged),
checks each archive's layout, and uploads the archives as workflow artifacts
labelled `v<version>-dryrun-<sha>`. Its `release` job is skipped: it cannot
create a tag or a release. Download an artifact and run the installer test or
a manual install against it if you want to check one end to end.

## 3. Tag and push (publishes the GitHub Release)

Only after the owner has approved the candidate commit:

```sh
git tag -a v0.2.0 -m "v0.2.0"
git push origin v0.2.0
```

The workflow refuses a tag that does not equal `v` + the workspace version.
It builds `mct-cli` + `mct-mcp-server` for linux-x86_64, linux-arm64,
macos-x86_64 (`macos-26-intel` runner), macos-arm64, and windows-x86_64,
packages each as `mini-consumes-tokens-<tag>-<platform>.tar.gz` (`.zip` on
Windows) with `README.md`, `LICENSE`, `THIRD_PARTY_NOTICES.md` and
`CHANGELOG.md`, and attaches all five to a new GitHub Release at that tag.
The binaries are built without the optional `semantic` feature. They are
built from this repository, so they include the patched Markdown parser in
`vendor/tree-sitter-md`.

Watch the run. If any platform job fails, nothing is published; fix it and
push a new patch tag (`v0.2.1`) rather than force-moving a pushed tag.

After it finishes, confirm the release has five archives and that
`curl -sSL https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.sh | bash`
installs the new tag on at least one machine.

## 4. crates.io — blocked

The root `Cargo.toml` patches `tree-sitter-md` with `vendor/tree-sitter-md`:
the latest upstream release (0.5.3) has two memory-safety bugs in its block
scanner that ordinary Markdown can reach (upstream issue #243, buffer
overflow in `serialize`; and `isdigit` called with full code points).
`cargo publish` ignores `[patch]`, so a published `mct-lang-md` — and
therefore the published `mct-cli` and `mct-mcp-server`, which depend on it —
would build against the unpatched upstream crate.

Until a `tree-sitter-md` release on crates.io contains both fixes (or the
owner chooses another publishable, reviewed dependency), do **not** publish
`mct-lang-md`, `mct-cli` or `mct-mcp-server`, and do not use `--no-verify` or
remove the patch to get around it. Publishing only the crates that do not
depend on `mct-lang-md` is possible but gives users nothing installable, so
the whole crates.io channel waits.

When the block is lifted:

1. Replace the patch with the fixed upstream version, delete
   `vendor/tree-sitter-md` and the matching `[patch.crates-io]` entries in
   every `crates/*/fuzz/Cargo.toml`, and rerun the full checks and the
   Markdown fuzz target.
2. Log in once with `cargo login` (token from https://crates.io/settings/tokens;
   this repository has no crates.io secret, so publishing stays local).
3. `cargo publish --dry-run -p <crate>` for each crate, then publish in
   dependency order — `cargo publish` waits for each crate to appear in the
   index before returning:

   ```sh
   cargo publish -p mct-core
   cargo publish -p mct-tree-sitter
   # Any order — they depend only on mct-core (and mct-tree-sitter):
   cargo publish -p mct-index
   cargo publish -p mct-lang-rust      # … and every other mct-lang-* crate
   cargo publish -p mct-languages      # after every mct-lang-* crate
   # Depend on everything above:
   cargo publish -p mct-mcp-server
   cargo publish -p mct-cli
   ```

   `mct-eval` and `mct-corpus` are `publish = false`.
4. From a clean machine: `cargo install mct-cli mct-mcp-server`, then update
   the README and installer messages that currently say crates.io is not
   available.

## 5. After the release

- [ ] Start a fresh `[Unreleased]` section if it was emptied.
- [ ] If an MCP client configuration or plugin points at a source checkout,
      point it at the released `mct-mcp-server` binary instead.
