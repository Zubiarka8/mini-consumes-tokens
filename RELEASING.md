# Releasing

This is a manual checklist, not automated — crates.io publishing in
particular is deliberately not wired into CI (see
`.github/workflows/release.yml`'s header comment).

## 1. Pre-flight

- [ ] `CHANGELOG.md` has a dated `[x.y.z]` section for this release (not just `[Unreleased]`).
- [ ] `[workspace.package] version` in the root `Cargo.toml` matches that version.
- [ ] `cargo test --workspace` and `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic` are clean locally.
- [ ] CI is green on the commit you're about to tag.

## 2. Publish to crates.io

One-time setup: `cargo login <your-crates.io-api-token>` (get the token from
https://crates.io/settings/tokens — this repo has no `CRATES_IO_TOKEN`
secret, so this always runs from your machine, never from CI).

Publish in dependency order — `mct-core` first, then everything that only
depends on `mct-core`, then the two binaries that depend on all of it.
crates.io's index needs a short moment to propagate after each publish
before the next crate's build can resolve it as a registry dependency
(`cargo publish` waits for this automatically; if a later step still can't
find a just-published crate, wait ~30s and retry that step alone):

```sh
cargo publish -p mct-core

# Any order among these — they only depend on mct-core:
cargo publish -p mct-index
cargo publish -p mct-lang-rust
cargo publish -p mct-lang-python
cargo publish -p mct-lang-java
cargo publish -p mct-lang-csharp
cargo publish -p mct-lang-kotlin
cargo publish -p mct-lang-js-ts
cargo publish -p mct-lang-cpp
cargo publish -p mct-lang-go
cargo publish -p mct-lang-html
cargo publish -p mct-lang-css
cargo publish -p mct-lang-xml
cargo publish -p mct-lang-xaml
cargo publish -p mct-lang-bash
cargo publish -p mct-lang-powershell
cargo publish -p mct-lang-php
cargo publish -p mct-lang-md
cargo publish -p mct-lang-lua

# Depend on everything above:
cargo publish -p mct-mcp-server
cargo publish -p mct-cli
```

Sanity check from a clean machine (or `cargo uninstall` first): `cargo
install mct-cli` and `cargo install mct-mcp-server` should both succeed and
produce working binaries.

## 3. Tag and push (triggers the binary-release workflow)

```sh
git tag v0.1.0
git push origin v0.1.0
```

This runs `.github/workflows/release.yml`: builds `mct-cli` +
`mct-mcp-server` for linux-x86_64, linux-arm64, macos-x86_64, macos-arm64,
and windows-x86_64, and attaches the archives to a new GitHub Release at
that tag. Watch the Actions run; if any platform job fails, fix and push a
new tag (`v0.1.0` itself should not be force-moved once pushed).

## 4. After the release

- [ ] Point the Claude Code plugin's installer/MCP-server entry at the
      released `mct-mcp-server` binary (or `cargo install mct-mcp-server`
      once published), not at an unversioned source checkout.
- [ ] Update `README.md`'s installation section if the recommended install
      path changed (e.g. from `cargo install --path ...` to `cargo install
      mct-cli` now that it's on crates.io).
