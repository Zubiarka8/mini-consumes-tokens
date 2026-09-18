# Contributing

## Adding a new language

This is the extension point most contributions will touch, so it gets its own checklist:

1. **New crate.** `crates/mct-lang-<name>`, depending on `mct-core` (path dependency) and `tree-sitter-<name>` (pin an exact version).
2. **Implement `mct_core::LanguageParser`:**
   - `language_id()` — a stable lowercase identifier (`"go"`, `"kotlin"`), stored in the index's `language` column.
   - `file_extensions()` — extensions this parser owns, without the leading dot. Two parsers can never claim the same extension (`LanguageRegistry::register` panics on conflict — a startup-time configuration error, not something repo content can trigger).
   - `parse()` — must never execute, `eval`, or otherwise run any part of the input; parsing is purely AST-based. A syntax error in the input is a `ParseError::Syntax` return value, never a panic — this method runs over arbitrary third-party source.
3. **No `unwrap()`/`panic!`** on any path that processes file content from the indexed repo. `.unwrap_or_default()` / early-return on `Option`/`Result` instead.
4. **Register it** — three places, nowhere else changes:
   - `mct-mcp-server/src/registry.rs::build_registry`
   - `mct-cli/src/main.rs::build_registry`
   - if you're also adding a `fuzz/` harness for it (see step 5a below), the `matrix.crate` list in `.github/workflows/ci.yml`'s `fuzz-smoke` job — otherwise CI silently never fuzzes it.
5. **Tests** in `crates/mct-lang-<name>/tests/parse.rs`:
   - A function-and-call extraction test.
   - At least one test covering the language's distinctive idiomatic syntax (generics for C#/Java, templates for C++, decorators for Python — whatever the equivalent is for your language).
   - A syntax-error case asserting `ParseError::Syntax`, not a panic.
5a. **Add a `cargo-fuzz` harness** (`crates/mct-lang-<name>/fuzz/`) — copy the structure of an existing one (e.g. `mct-lang-lua/fuzz/`): a standalone-workspace `Cargo.toml` (`[workspace]` empty table, so it stays out of the root workspace's members/lockfile) and one `fuzz_targets/parse_<name>.rs` calling `<Name>Parser::parse` on arbitrary bytes. Remember the CI matrix entry from step 4.
6. **Update docs:** the language table in `README.md` and the "Cobertura de lenguajes" section in `internal/checklist.md`.
7. **Run the workspace test suite** (`cargo test --workspace`) and fix any regressions before opening a PR.

## Windows build prerequisites

Building this workspace on Windows needs nothing beyond the standard Rust-on-Windows setup — no manual system dependency to install for `git2` or any tree-sitter grammar specifically:

- **`git2`** is a dependency with `default-features = false` (see `mct-index/Cargo.toml`) — it skips the `ssh`/`https` transports and their system OpenSSL/libssh2 requirement, since this project only reads local repo state (blob hashing), never clones/fetches/pushes. No OpenSSL install needed on any OS for this reason.
- **tree-sitter grammars** (and `rusqlite`'s bundled SQLite) are plain C, compiled via the `cc` crate at build time. On Windows that means the MSVC linker/toolset (`link.exe`, `cl.exe`) must be available — in practice, the **Desktop development with C++** workload from Visual Studio Build Tools (or full Visual Studio), which `rustup`'s own Windows installer already prompts for as a prerequisite of the `x86_64-pc-windows-msvc` toolchain. If that's installed (required to use Rust on Windows at all), building this workspace needs nothing further.
- GitHub's `windows-latest` Actions runner ships Visual Studio 2022 with the C++ toolset preinstalled, so CI needs no extra setup step for this either — confirmed by `.github/workflows/ci.yml`'s `build-test` job actually building and testing green on `windows-latest`.

If `cargo build` fails on Windows with a `link.exe`/`cl.exe`-not-found error, the fix is installing the C++ Build Tools workload, not a project-specific dependency.

## Continuous integration

`.github/workflows/ci.yml` has three jobs:

- **`build-test`** — matrix over Linux/macOS/Windows: `cargo build --workspace --all-targets`, `cargo test --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`. Runs on every push/PR, on all three OS, no exceptions.
- **`cargo-audit`** — Linux only (one run is enough for a dependency audit). Two passes: a full report of every severity (informational, never fails the job — low/medium findings stay visible in the log), then a second pass with `severity_threshold = "high"` in a generated `.cargo/audit.toml`, whose exit code is what actually gates the job. Advisories with no CVSS score bypass the severity filter and always fail this step (fail-safe for unscored issues).
- **`fuzz-smoke`** — matrix over `{ubuntu-latest, macos-latest} × {every crate listed in matrix.crate}` (8 as of C++/Go — check the workflow file for the current, authoritative list rather than this count, which will go stale again), short (30s) `cargo-fuzz` campaigns confirming each harness builds and runs without crashing. **Deliberately excludes Windows** — see the long comment above that job in the workflow file for exactly why (an ASan runtime DLL PATH issue and a separate MSVC linker limitation with sancov instrumentation, both confirmed locally before this decision was made). The regular `build-test` job still covers Windows fully; only fuzzing is Linux/macOS-only.

## Changing the `LanguageParser` trait, the SQLite schema, or an already-published MCP tool signature

These are cross-cutting: the trait is implemented by every language crate, the schema is shared by every language's data, and a published tool signature is part of the contract Claude Code agents rely on. Open an issue describing the change before sending a PR — these need explicit sign-off, not a drive-by PR.

## Code style

- No `unwrap()`/`panic!` on repo-input processing paths, in any crate.
- Single responsibility per crate: `mct-core` doesn't know about any specific language or about SQLite; a `mct-lang-*` crate doesn't know about SQLite or MCP.
- Comment the *why*, not the *what* — non-obvious constraints, not a restatement of the code.

## Commit messages

`type(scope): short description` — types: `feat` / `fix` / `refactor` / `docs` / `test` / `chore` / `perf`. Mark breaking changes explicitly.
