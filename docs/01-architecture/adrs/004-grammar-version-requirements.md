# ADR-004: Grammar version requirements

## Status
Accepted (2026-10-10)

## Context
- `CONTRIBUTING.md` asked for "an exact version" of each `tree-sitter-<name>` grammar, but no crate used `=x.y.z`: 14 wrote a caret `x.y.z`, and `mct-lang-bash`, `mct-lang-php`, `mct-lang-powershell` (and `mct-tree-sitter`'s `tree-sitter-bash`) wrote `x.y`.
- A grammar release can change what parses and what the corpus snapshots contain, so the version a parser was validated against must be reproducible.
- `Cargo.lock` is committed; release builds (`release.yml`) and the documented installs use `--locked`. The crates are meant to be publishable to crates.io (`RELEASING.md`), where an `=` requirement forces every dependent onto one grammar build and blocks its patch releases.
- For a `0.y` grammar, a caret requirement already excludes the next minor (`"0.25.1"` means `>=0.25.1, <0.26.0`).

## Decision
- Write each grammar requirement as the full version the parser and its corpus were validated against, with default caret semantics: `tree-sitter-bash = "0.25.1"`. Not `"0.25"`, not `"=0.25.1"`.
- `Cargo.lock` is the exact pin. Upgrading a grammar is a deliberate `cargo update -p tree-sitter-<name> --precise <v>` that also raises the requirement to `<v>`, with `corpus-report.sh <lang> --bless` and its snapshot diff reviewed in the same PR.
- `scripts/unix/new-language-check.sh` and `scripts/windows/new-language-check.ps1` require the `x.y.z` form.

## Consequences
- The minimum validated version is visible in each crate's `Cargo.toml`; the lockfile decides the build. No dependency changed: the three crates moved to the version already locked (`0.25.1`, `0.24.2`, `0.26.4`).
- A downstream crates.io user may still resolve a newer patch release than the one tested here. That is accepted as the normal semver contract; this repository's own builds are fixed by `Cargo.lock`.
- CI `build-test` (build, test, clippy) and the quality report pass `--locked`, so a requirement change that the lockfile cannot satisfy fails the job instead of updating `Cargo.lock` on the runner. The fuzz crates keep their own lockfiles and are not covered.

#adr
