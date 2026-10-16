use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use mct_core::{LanguageRegistry, ParseError, SourceFile};
use mct_index::{ExcludeSet, Index};
use mct_languages::build_registry;

mod duplicates;

/// Index a repository and inspect the index from the command line.
///
/// The MCP server (`mct-mcp-server`) does the same indexing automatically at
/// startup — this CLI is for manual/scripted use (CI, a pre-commit hook, or
/// just checking coverage before wiring up the plugin).
#[derive(Parser, Debug)]
#[command(
    name = "mct-cli",
    version,
    disable_version_flag = true,
    after_help = "\
Invoked here via `cargo run -p mct-cli --`; once installed on PATH (e.g.
`cargo install --path crates/mct-cli`) the binary is `mct-cli` too, so drop
the `cargo run -p mct-cli --` prefix from every example below.

Typical first run:
  cargo run -p mct-cli -- --root . init
  cargo run -p mct-cli -- --root . ignore-init --import-gitignore
  cargo run -p mct-cli -- --root . gitignore-init
  cargo run -p mct-cli -- --root . status

Query without an MCP client, on one or several projects:
  cargo run -p mct-cli -- --root . call --tool get_indexing_status
  cargo run -p mct-cli -- --roots ../a,../b call --tool list_symbols --args '{\"path\":\"src\"}'

Run `cargo run -p mct-cli -- <command> --help` for a command's full
description and examples."
)]
struct Cli {
    /// Project root to operate on. Defaults to the current working directory.
    ///
    /// `init` and `call` accept several roots, as a repeated `--root` or a
    /// comma-separated `--roots a,b`; every other command takes exactly one.
    #[arg(
        long,
        visible_alias = "roots",
        global = true,
        value_delimiter = ',',
        action = clap::ArgAction::Append
    )]
    root: Vec<PathBuf>,

    /// Print version.
    #[arg(short = 'v', short_alias = 'V', long = "version", action = clap::ArgAction::Version)]
    version: (),

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Build the index for the first time.
    ///
    /// Equivalent to `reindex` — kept as a separate, discoverable first
    /// command for a fresh checkout. With several roots (`--root a --root b`
    /// or `--roots a,b`) each one is indexed independently and gets its own
    /// summary. A root whose index is already up to date is not re-parsed
    /// unless `--force` is given.
    #[command(
        after_help = "Examples:\n  cargo run -p mct-cli -- --root . init\n  cargo run -p mct-cli -- --roots ../a,../b init\n  cargo run -p mct-cli -- --root . init --force"
    )]
    Init {
        /// Re-parse every supported file, even if unchanged since last run.
        #[arg(long)]
        force: bool,
    },
    /// Re-scan the project and update the index.
    ///
    /// Only re-parses files that changed since the last run, unless
    /// `--force` is given. Safe to run repeatedly (e.g. from a pre-commit
    /// hook or CI step) — a no-op reindex costs one blob-hash comparison
    /// per file.
    #[command(
        after_help = "Examples:\n  cargo run -p mct-cli -- --root . reindex\n  cargo run -p mct-cli -- --root . reindex --force"
    )]
    Reindex {
        /// Re-parse every supported file, even if unchanged since last run.
        #[arg(long)]
        force: bool,
    },
    /// Call one MCP tool once and print its result.
    ///
    /// Runs the same tool code `mct-mcp-server` serves, in this process: no
    /// server to start, no JSON-RPC to write. Each root is brought up to date
    /// first (an incremental reindex, like the server's startup), then the
    /// tool is called with `--args`. With several roots the call runs on each
    /// and every result is labelled `== <root> ==`; a failure on one root is
    /// reported and the rest still run, then the exit status is non-zero.
    /// Tool names: `list_symbols`, `get_file_skeleton`, `get_project_overview`,
    /// `get_file_tree`, `find_symbol`, `search_symbols`, `hybrid_search`,
    /// `find_references`, `find_calls`, `find_callers`, `impact_analysis`,
    /// `build_context_pack`, `find_dead_code`, `batch`, `get_indexing_status`,
    /// `reindex`. Add `"format":"toon"` to `--args` where a tool supports it
    /// for the most compact output.
    #[command(
        after_help = "Examples:\n  cargo run -p mct-cli -- --root . call --tool get_indexing_status\n  cargo run -p mct-cli -- --root . call --tool list_symbols --args '{\"path\":\"src/main.rs\"}'\n  cargo run -p mct-cli -- --root . call --tool find_symbol --args-file query.json --json\n  echo '{\"symbol\":\"main\"}' | cargo run -p mct-cli -- --roots ../a,../b call --tool build_context_pack --args -"
    )]
    Call {
        /// Tool to call (see the list above).
        #[arg(long)]
        tool: String,
        /// The tool's arguments as one JSON object; `-` reads it from stdin.
        /// Defaults to `{}`.
        #[arg(long, conflicts_with = "args_file")]
        args: Option<String>,
        /// Read the arguments JSON object from this file; `-` is stdin.
        #[arg(long)]
        args_file: Option<PathBuf>,
        /// Print the raw MCP result as JSON instead of its text. With several
        /// roots, an array of `{root, result}` / `{root, error}` objects.
        #[arg(long)]
        json: bool,
        /// Give up on a root after this many seconds (indexing included).
        #[arg(long, value_name = "SECONDS")]
        timeout: Option<u64>,
    },
    /// Report index health.
    ///
    /// Prints coverage per language, the last indexed time, any unsupported
    /// languages seen while walking the tree, and files that failed to
    /// parse. Run this after `init`/`reindex` to sanity-check the result,
    /// or on its own to check whether the index looks stale.
    #[command(after_help = "Example:\n  cargo run -p mct-cli -- --root . status")]
    Status,
    /// Write (or update) `.mcp.json` so an MCP client can launch this server.
    ///
    /// Writes (or updates) `.mcp.json` at the project root so Claude Code
    /// (or any other client reading that file) can launch `mct-mcp-server`
    /// for this project. Merges into an existing file instead of
    /// overwriting it, so other servers already configured there are left
    /// untouched.
    #[command(
        after_help = "Examples:\n  cargo run -p mct-cli -- --root . mcp-register\n  cargo run -p mct-cli -- --root . mcp-register --name my-project\n  cargo run -p mct-cli -- --root . mcp-register --reindex-delay 30s\n  cargo run -p mct-cli -- --root . mcp-register --reindex-delay off"
    )]
    McpRegister {
        /// Server name (the key under `mcpServers`). Defaults to the root
        /// directory's own name.
        #[arg(long)]
        name: Option<String>,
        /// Quiet period without file changes before the server refreshes the
        /// index automatically: `15s`, `30s`, `45s`, `1m`, a custom positive
        /// `<N>s` / `<N>m` / `<N>ms`, or `off` to disable automatic updates.
        /// Written to the server entry's args as `--reindex-delay` and kept
        /// by later `mcp-register` runs that omit the flag.
        #[arg(long, value_name = "VALUE", value_parser = parse_reindex_delay)]
        reindex_delay: Option<String>,
    },
    /// Write a starter `.mctignore` for custom indexing exclusions.
    ///
    /// Writes a starter `.mctignore` at the project root, if one doesn't
    /// already exist. Lets a project exclude extra files/directories from
    /// indexing (e.g. `docs/`, `*.md`) on top of the built-in exclusions,
    /// without touching git. Without `--import-gitignore`, an existing file
    /// is left untouched.
    #[command(
        after_help = "Examples:\n  cargo run -p mct-cli -- --root . ignore-init\n  cargo run -p mct-cli -- --root . ignore-init --import-gitignore"
    )]
    IgnoreInit {
        /// Activate `@import-gitignore`, so everything the project's
        /// `.gitignore` excludes is excluded from indexing too. On a fresh
        /// file this is baked into the generated content; on an existing
        /// one, the directive is appended if it isn't already there —
        /// otherwise this is the one case `ignore-init` updates a file
        /// instead of leaving it alone.
        #[arg(long)]
        import_gitignore: bool,
    },
    /// Keep the generated index out of git.
    ///
    /// Adds `.mct-index/` to the project's `.gitignore` (creating it if
    /// needed), so the generated symbol database — fully derived from
    /// source, regenerated by `reindex` — never ends up committed. Leaves
    /// everything else already in the file untouched; a no-op if it's
    /// already covered.
    #[command(after_help = "Example:\n  cargo run -p mct-cli -- --root . gitignore-init")]
    GitignoreInit,
    /// Write a CSV report of dead-code candidates (heuristic: symbols with
    /// zero indexed references) — one row per candidate with its file,
    /// function name, kind, language, and start/end line.
    DeadCode {
        /// File, directory, or crate prefix to scan — same matching
        /// semantics as `list_symbols`. Omit for the whole project.
        #[arg(long)]
        path: Option<String>,
        /// Exact language id to keep (e.g. `rust`, `python`, `go`).
        #[arg(long)]
        language: Option<String>,
        /// Where to write the CSV report. Defaults to
        /// `<root>/.mct-index/dead-code-report.csv`.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Report candidate duplicate functions and methods, as TOON.
    ///
    /// Groups functions and methods of the same language whose token
    /// sequences are identical once identifiers and literals are normalized
    /// (Type-2 clones). Every group is a CANDIDATE for human review: matching
    /// structure does not prove the code is redundant or behaves the same.
    /// Runs on demand against the existing index and the current files; it
    /// never reindexes or migrates the index, so run `init` or `reindex` first.
    /// Candidates that cannot be analyzed are listed as skipped and mark the
    /// report partial. Only languages with a token source are analyzed; the
    /// report names the rest.
    #[command(
        after_help = "Examples:\n  cargo run -p mct-cli -- --root . duplicates\n  cargo run -p mct-cli -- --root . duplicates --min-lines 10 --min-tokens 80"
    )]
    Duplicates {
        #[command(flatten)]
        options: duplicates::Args,
    },
    /// Parse files without indexing them and report, per file, either its
    /// symbol/relation counts or the first syntax error with its line.
    ///
    /// For narrowing down which construct a grammar rejects: no index is
    /// opened or written, and the files need not be under `--root`. Exits
    /// non-zero if any file fails to parse.
    #[command(after_help = "Example:\n  cargo run -p mct-cli -- probe snippet.cpp other.cpp")]
    Probe {
        /// Files to parse; the extension picks the language.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Update mct-cli and mct-mcp-server to the latest GitHub release.
    ///
    /// Runs the official installer (`install.sh`, or `install.ps1` on
    /// Windows) with this binary's own folder as the install directory, so
    /// both programs are replaced where they already are. Needs network
    /// access, plus `curl` and `bash` on macOS/Linux. Release binaries are
    /// built without the `semantic` feature: a source install with
    /// `--features semantic` is replaced by one without it. Close the MCP
    /// client first on Windows, which cannot overwrite a running
    /// `mct-mcp-server.exe`.
    #[command(
        short_flag = 'u',
        long_flag = "update",
        after_help = "Examples:\n  mct-cli update\n  mct-cli -u\n  mct-cli --update"
    )]
    Update,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let roots = if cli.root.is_empty() {
        vec![std::env::current_dir()?]
    } else {
        cli.root
    };

    // The only commands that take several roots; they open their own indexes.
    if let Command::Init { force } = cli.command {
        return init_roots(&roots, force);
    }
    if let Command::Call {
        tool,
        args,
        args_file,
        json,
        timeout,
    } = cli.command
    {
        let args = read_call_args(args.as_deref(), args_file.as_deref())?;
        return call_roots(&roots, &tool, args, json, timeout);
    }
    let root = match roots.as_slice() {
        [root] => root.clone(),
        _ => anyhow::bail!(
            "this command takes exactly one --root; only `init` and `call` accept several"
        ),
    };

    // None of these touch the index at all, so they're handled before
    // Index::open — running them shouldn't have the side effect of creating
    // .mct-index/.
    if let Command::McpRegister {
        name,
        reindex_delay,
    } = cli.command
    {
        return mcp_register(&root, name, reindex_delay);
    }
    if let Command::IgnoreInit { import_gitignore } = cli.command {
        return ignore_init(&root, import_gitignore);
    }
    if let Command::GitignoreInit = cli.command {
        return gitignore_init(&root);
    }
    if let Command::Probe { files } = &cli.command {
        return probe(&build_registry(), files);
    }
    if let Command::Update = cli.command {
        return self_update();
    }
    // Read-only like `status`, but it must not create an empty index for a
    // project that was never indexed, so it checks for the database itself.
    if let Command::Duplicates { options } = cli.command {
        return duplicates::run(&root, options.into());
    }

    let db_path = root.join(".mct-index").join("index.sqlite3");
    let registry = build_registry();
    let exclude = ExcludeSet::new(&mct_index::read_ignore_file(&root));
    let mut index = Index::open(&root, &db_path, exclude)?;

    match cli.command {
        Command::Reindex { force } => print_reindex(index.reindex(&registry, force)?),
        Command::Status => print_status(index.status()?),
        Command::DeadCode {
            path,
            language,
            output,
        } => dead_code_report(&index, path.as_deref(), language.as_deref(), output)?,
        Command::Init { .. }
        | Command::Call { .. }
        | Command::McpRegister { .. }
        | Command::IgnoreInit { .. }
        | Command::GitignoreInit
        | Command::Probe { .. }
        | Command::Duplicates { .. }
        | Command::Update => {
            unreachable!("returned above")
        }
    }

    Ok(())
}

const INSTALLER_BASE: &str =
    "https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main";

/// Reinstalls both binaries from the latest GitHub release into the folder
/// this executable runs from, by running the same installer the README
/// documents. The installer prints the release it installed.
// ponytail: always reinstalls, even when already on the latest release —
// compare against the release tag first if the download time matters.
fn self_update() -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cannot tell which folder {} is in", exe.display()))?;
    println!(
        "mct-cli {} installed in {}; fetching the latest release…",
        env!("CARGO_PKG_VERSION"),
        dir.display()
    );

    #[cfg(windows)]
    let status = {
        // Windows cannot overwrite a running .exe but can rename it, so this
        // binary steps aside for the installer and is put back if it fails.
        let old = exe.with_extension("old.exe");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old)?;
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"])
            .arg(format!("irm {INSTALLER_BASE}/install.ps1 | iex"))
            .env("MCT_INSTALL_DIR", dir)
            .status();
        if !matches!(&status, Ok(s) if s.success()) && !exe.exists() {
            let _ = std::fs::rename(&old, &exe);
        }
        status?
    };
    #[cfg(not(windows))]
    let status = std::process::Command::new("bash")
        .arg("-c")
        .arg(format!("curl -fsSL {INSTALLER_BASE}/install.sh | bash"))
        .env("INSTALL_DIR", dir)
        .status()?;

    if !status.success() {
        anyhow::bail!("the installer failed ({status}); nothing was updated");
    }
    Ok(())
}

/// Inserts (or replaces) this project's `mct-mcp-server` entry under
/// `mcpServers` in `<root>/.mcp.json`, creating the file if it doesn't exist.
/// Any other server already configured there is left untouched — this only
/// ever writes the one key it owns.
/// Parses each file with the registry and prints one line per file (plus
/// the offending source line on a syntax error). Errs if any file failed.
fn probe(registry: &LanguageRegistry, files: &[PathBuf]) -> anyhow::Result<()> {
    let mut failed = 0;
    for path in files {
        let shown = display_path(path);
        let contents = match mct_index::read_repository_file(path).and_then(|bytes| {
            String::from_utf8(bytes)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        }) {
            Ok(contents) => contents,
            Err(e) => {
                println!("FAIL {shown}: {e}");
                failed += 1;
                continue;
            }
        };
        let file = SourceFile {
            relative_path: shown.clone(),
            contents,
        };
        match registry.parse(&file) {
            Ok(parsed) => println!(
                "ok   {shown}: {} symbols, {} relations",
                parsed.symbols.len(),
                parsed.relations.len()
            ),
            Err(ParseError::Syntax { line, message, .. }) => {
                failed += 1;
                println!("FAIL {shown}:{line}: {message}");
                let text = file
                    .contents
                    .lines()
                    .nth((line as usize).saturating_sub(1))
                    .unwrap_or_default();
                println!("     {line:>5} | {}", text.trim_end());
            }
            Err(e) => {
                failed += 1;
                println!("FAIL {shown}: {e}");
            }
        }
    }
    if failed > 0 {
        anyhow::bail!("{failed} of {} file(s) failed to parse", files.len());
    }
    Ok(())
}

/// Validates and normalises a `--reindex-delay` value (same grammar as the
/// server): `off`, or a positive `<N>ms` / `<N>s` / `<N>m`. Returns the
/// trimmed, lowercase form.
fn parse_reindex_delay(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_ascii_lowercase();
    if value == "off" {
        return Ok(value);
    }
    let invalid = || {
        format!(
            "invalid reindex delay `{raw}`: use `off` or a positive duration such as 15s, 30s, 45s, 1m, 500ms"
        )
    };
    let digits_end = value
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(invalid)?;
    let (digits, unit) = value.split_at(digits_end);
    if !matches!(unit, "ms" | "s" | "m") {
        return Err(invalid());
    }
    match digits.parse::<u64>() {
        Ok(n) if n > 0 => Ok(value),
        _ => Err(invalid()),
    }
}

/// Finds a `--reindex-delay` value in an existing server entry's args.
fn existing_reindex_delay(entry: Option<&serde_json::Value>) -> Option<String> {
    let args = entry?.get("args")?.as_array()?;
    let strs: Vec<&str> = args.iter().filter_map(|a| a.as_str()).collect();
    strs.iter().enumerate().find_map(|(i, a)| {
        if *a == "--reindex-delay" {
            strs.get(i + 1).map(|v| v.to_string())
        } else {
            a.strip_prefix("--reindex-delay=").map(str::to_string)
        }
    })
}

fn mcp_register(
    root: &Path,
    name: Option<String>,
    reindex_delay: Option<String>,
) -> anyhow::Result<()> {
    let reindex_delay = reindex_delay
        .map(|v| parse_reindex_delay(&v).map_err(|e| anyhow::anyhow!(e)))
        .transpose()?;
    let root = root.canonicalize()?;
    let server_name = name.unwrap_or_else(|| {
        root.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("mct-mcp-server")
            .to_string()
    });
    let config_path = root.join(".mcp.json");

    let mut config: serde_json::Value = if config_path.exists() {
        serde_json::from_str(&std::fs::read_to_string(&config_path)?)?
    } else {
        serde_json::json!({})
    };

    let top = config.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("{}: top level must be a JSON object", config_path.display())
    })?;
    let servers = top
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}: \"mcpServers\" must be a JSON object",
                config_path.display()
            )
        })?;

    // An explicit flag wins; otherwise keep what the entry already has so the
    // choice survives re-registering.
    let delay = reindex_delay.or_else(|| existing_reindex_delay(servers.get(&server_name)));
    let mut args = vec!["--root".to_string(), display_path(&root)];
    if let Some(delay) = delay {
        args.push("--reindex-delay".to_string());
        args.push(delay);
    }

    servers.insert(
        server_name.clone(),
        serde_json::json!({
            "type": "stdio",
            "command": "mct-mcp-server",
            "args": args,
            "env": {}
        }),
    );

    std::fs::write(
        &config_path,
        format!("{}\n", serde_json::to_string_pretty(&config)?),
    )?;
    println!(
        "Registered `{server_name}` in {}",
        display_path(&config_path)
    );
    Ok(())
}

/// Writes a starter `.mctignore` at `root` if one doesn't already exist.
/// Never overwrites an existing file's own patterns — same non-destructive
/// stance as `mcp_register`, just without a merge target here since there's
/// nothing to merge into a plain-text ignore file.
///
/// `import_gitignore` lets the user opt into `@import-gitignore` instead of
/// hand-editing the file: on a fresh file it's baked into the generated
/// content (uncommenting the template's own example line for it); on an
/// existing one, [`activate_gitignore_import`] appends it if it isn't
/// already there — the one case this function updates rather than leaves
/// alone, gated entirely behind the flag so a plain `ignore-init` on an
/// existing file still never touches it.
fn ignore_init(root: &Path, import_gitignore: bool) -> anyhow::Result<()> {
    let path = root.join(mct_index::IGNORE_FILE_NAME);
    if path.exists() {
        if import_gitignore {
            return activate_gitignore_import(&path);
        }
        println!("{} already exists, leaving it as-is.", display_path(&path));
        return Ok(());
    }

    let contents = if import_gitignore {
        let commented = format!("# {}\n", mct_index::GITIGNORE_IMPORT_DIRECTIVE);
        let active = format!("{}\n", mct_index::GITIGNORE_IMPORT_DIRECTIVE);
        mct_index::IGNORE_FILE_TEMPLATE.replacen(&commented, &active, 1)
    } else {
        mct_index::IGNORE_FILE_TEMPLATE.to_string()
    };
    std::fs::write(&path, &contents)?;
    println!(
        "Wrote starter {}{}",
        display_path(&path),
        if import_gitignore {
            ", importing .gitignore"
        } else {
            ""
        }
    );
    Ok(())
}

/// Appends [`mct_index::GITIGNORE_IMPORT_DIRECTIVE`] to an existing
/// `.mctignore` at `path`, if it isn't already active on some line of its
/// own — idempotent, and never touches anything else already in the file.
fn activate_gitignore_import(path: &Path) -> anyhow::Result<()> {
    let contents = std::fs::read_to_string(path)?;
    if contents
        .lines()
        .any(|line| line.trim() == mct_index::GITIGNORE_IMPORT_DIRECTIVE)
    {
        println!("{} already imports .gitignore.", display_path(path));
        return Ok(());
    }

    let mut updated = contents;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(mct_index::GITIGNORE_IMPORT_DIRECTIVE);
    updated.push('\n');

    std::fs::write(path, &updated)?;
    println!(
        "Added `{}` to {}",
        mct_index::GITIGNORE_IMPORT_DIRECTIVE,
        display_path(path)
    );
    Ok(())
}

/// `.mct-index/`'s entry, appended to `.gitignore` by [`gitignore_init`].
const GITIGNORE_INDEX_ENTRY: &str = ".mct-index/";

/// Ensures `<root>/.gitignore` excludes `.mct-index/` — the generated
/// SQLite database is fully derived from source (`reindex` regenerates it
/// from nothing) and changes on every run, so committing it just bloats
/// history with a binary file nobody needs to review or merge.
///
/// Additive and idempotent, same non-destructive stance as `mcp_register`/
/// `ignore_init`: creates `.gitignore` if it doesn't exist, appends the
/// entry if no existing line already covers it, and never touches anything
/// else already there. The existing-coverage check is a plain trimmed-line
/// match, not full gitignore pattern matching — good enough to avoid
/// duplicating the exact line this function itself writes on a second run,
/// without reimplementing gitignore semantics for a single well-known path.
fn gitignore_init(root: &Path) -> anyhow::Result<()> {
    let path = root.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    if existing
        .lines()
        .any(|line| line.trim() == GITIGNORE_INDEX_ENTRY || line.trim() == ".mct-index")
    {
        println!("{} already excludes `.mct-index/`.", display_path(&path));
        return Ok(());
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    if !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(
        "# mini-consumes-tokens: local symbol index, regenerated by `mct-cli reindex` — never worth committing.\n",
    );
    updated.push_str(GITIGNORE_INDEX_ENTRY);
    updated.push('\n');

    std::fs::write(&path, &updated)?;
    println!("Added `.mct-index/` to {}", display_path(&path));
    Ok(())
}
/// Writes `mct_index::find_dead_code_candidates`'s result to a CSV file: one
/// row per candidate with the project root, its file, name, kind, language,
/// and start/end line — the same fields stored in `SymbolListEntry`, the
/// table this heuristic scan filters down.
fn dead_code_report(
    index: &Index,
    path: Option<&str>,
    language: Option<&str>,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    let candidates = mct_index::find_dead_code_candidates(index, path, language)?;
    let output =
        output.unwrap_or_else(|| index.root().join(".mct-index").join("dead-code-report.csv"));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let root = display_path(index.root());
    let mut csv = String::from("project_root,file,function,kind,language,start_line,end_line\n");
    for candidate in &candidates {
        csv.push_str(&csv_field(&root));
        csv.push(',');
        csv.push_str(&csv_field(&candidate.relative_path));
        csv.push(',');
        csv.push_str(&csv_field(&candidate.name));
        csv.push(',');
        csv.push_str(&csv_field(&candidate.kind));
        csv.push(',');
        csv.push_str(&csv_field(&candidate.language));
        csv.push(',');
        csv.push_str(&candidate.line.to_string());
        csv.push(',');
        if let Some(end_line) = candidate.end_line {
            csv.push_str(&end_line.to_string());
        }
        csv.push('\n');
    }

    std::fs::write(&output, csv)?;
    println!(
        "Wrote {} dead-code candidate(s) to {}",
        candidates.len(),
        display_path(&output)
    );
    Ok(())
}

/// Quotes a CSV field only when it contains a comma, quote, or newline —
/// doubling any embedded quote, per RFC 4180. Every field this module writes
/// is either a project-relative path or an identifier, so this is defensive
/// rather than expected to trigger in practice.
fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// `Path::canonicalize` returns Windows' `\\?\`-prefixed "verbatim" form —
/// a valid Win32 path, but not what should land in a human-edited config
/// file or get passed as a plain CLI argument to a subprocess that may not
/// expect it. Strips it for display/config purposes; a no-op on any other
/// platform. `\\?\UNC\server\share` (the verbatim form of a network path)
/// needs the `UNC` segment folded back into a leading `\\`, not just the
/// `\\?\` marker dropped, or the result wouldn't be a valid UNC path.
fn display_path(path: &Path) -> String {
    let s = path.to_string_lossy().into_owned();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    }
}

/// Fails fast, before any index is touched, on a root that isn't a directory.
fn check_roots(roots: &[PathBuf]) -> anyhow::Result<()> {
    for root in roots {
        anyhow::ensure!(
            root.is_dir(),
            "root `{}` does not exist or is not a directory",
            display_path(root)
        );
    }
    Ok(())
}

/// `init` over one or more roots. A single root prints exactly what it always
/// did; several print a labelled summary each (this run's work, then the
/// index's totals), and keep going past a root that fails.
fn init_roots(roots: &[PathBuf], force: bool) -> anyhow::Result<()> {
    check_roots(roots)?;
    let registry = build_registry();
    let mut failed = 0;
    for root in roots {
        let result = (|| -> anyhow::Result<()> {
            let db_path = root.join(".mct-index").join("index.sqlite3");
            let exclude = ExcludeSet::new(&mct_index::read_ignore_file(root));
            let mut index = Index::open(root, &db_path, exclude)?;
            if roots.len() > 1 {
                println!("== {} ==", display_path(root));
            }
            print_reindex(index.reindex(&registry, force)?);
            if roots.len() > 1 {
                let status = index.status()?;
                println!(
                    "Index: {} files, {} symbols.",
                    status.total_files, status.total_symbols
                );
            }
            Ok(())
        })();
        if let Err(err) = result {
            failed += 1;
            eprintln!("error: {}: {err:#}", display_path(root));
        }
    }
    match (failed, roots.len()) {
        (0, _) => Ok(()),
        (_, 1) => anyhow::bail!("init failed"),
        (failed, total) => anyhow::bail!("init failed on {failed} of {total} roots"),
    }
}

/// Reads `call`'s JSON argument object from `--args`, `--args-file` or stdin
/// (`-`); no argument at all means `{}`.
fn read_call_args(
    args: Option<&str>,
    args_file: Option<&Path>,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    use std::io::Read;
    let read_stdin = || -> anyhow::Result<String> {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        Ok(text)
    };
    let text = match (args, args_file) {
        (Some("-"), _) => read_stdin()?,
        (Some(text), _) => text.to_string(),
        (None, Some(file)) if file == Path::new("-") => read_stdin()?,
        (None, Some(file)) => std::fs::read_to_string(file).map_err(|err| {
            anyhow::anyhow!("cannot read --args-file `{}`: {err}", display_path(file))
        })?,
        (None, None) => return Ok(serde_json::Map::new()),
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(serde_json::Value::Object(map)) => Ok(map),
        Ok(_) => anyhow::bail!("tool arguments must be a JSON object, e.g. '{{\"path\":\"src\"}}'"),
        Err(err) => anyhow::bail!("tool arguments are not valid JSON: {err}"),
    }
}

/// Brings one root's index up to date, then calls `tool` on it in this
/// process, returning the serialized MCP result.
async fn call_one(
    root: &Path,
    tool: &str,
    args: serde_json::Map<String, serde_json::Value>,
) -> anyhow::Result<serde_json::Value> {
    let registry = build_registry();
    let db_path = root.join(".mct-index").join("index.sqlite3");
    let mut index = Index::open(root, &db_path, ExcludeSet::for_project(root))?;
    index.reindex(&registry, false)?;
    let server = mct_mcp_server::server::MctServer::new(index, registry);
    let result = server
        .call_tool_by_name(tool, Some(args))
        .await
        .map_err(|err| anyhow::anyhow!("{}", err.message))?;
    let value = serde_json::to_value(&result)?;
    if value.get("isError").and_then(serde_json::Value::as_bool) == Some(true) {
        anyhow::bail!("{}", result_text(&value));
    }
    Ok(value)
}

/// The text blocks of a serialized MCP result, joined.
fn result_text(result: &serde_json::Value) -> String {
    result
        .get("content")
        .and_then(serde_json::Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// `call` over one or more roots; see [`Command::Call`].
fn call_roots(
    roots: &[PathBuf],
    tool: &str,
    args: serde_json::Map<String, serde_json::Value>,
    json: bool,
    timeout: Option<u64>,
) -> anyhow::Result<()> {
    check_roots(roots)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let mut outcomes = Vec::new();
    for root in roots {
        let call = call_one(root, tool, args.clone());
        let outcome = runtime.block_on(async {
            match timeout {
                Some(secs) => tokio::time::timeout(std::time::Duration::from_secs(secs), call)
                    .await
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("timed out after {secs}s"))),
                None => call.await,
            }
        });
        outcomes.push((root, outcome));
    }

    let failed = outcomes
        .iter()
        .filter(|(_, outcome)| outcome.is_err())
        .count();
    let labelled = roots.len() > 1;
    if json && labelled {
        let entries: Vec<_> = outcomes
            .iter()
            .map(|(root, outcome)| match outcome {
                Ok(result) => serde_json::json!({ "root": display_path(root), "result": result }),
                Err(err) => {
                    serde_json::json!({ "root": display_path(root), "error": format!("{err:#}") })
                }
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        for (root, outcome) in &outcomes {
            if labelled {
                println!("== {} ==", display_path(root));
            }
            match outcome {
                Ok(result) if json => println!("{}", serde_json::to_string_pretty(result)?),
                Ok(result) => println!("{}", result_text(result)),
                Err(err) if labelled => eprintln!("error: {}: {err:#}", display_path(root)),
                Err(err) => eprintln!("error: {err:#}"),
            }
        }
    }
    match (failed, roots.len()) {
        (0, _) => Ok(()),
        (_, 1) => anyhow::bail!("call `{tool}` failed"),
        (failed, total) => anyhow::bail!("call `{tool}` failed on {failed} of {total} roots"),
    }
}

fn print_reindex(report: mct_index::ReindexReport) {
    println!(
        "Reindex complete: {} parsed, {} unchanged, {} removed, {} symbols written.",
        report.files_parsed, report.files_unchanged, report.files_removed, report.symbols_written
    );
    if !report.issues.is_empty() {
        println!("{} issue(s):", report.issues.len());
        for issue in &report.issues {
            println!(
                "  {} [{:?}]: {}",
                issue.relative_path, issue.kind, issue.detail
            );
        }
    }
}

fn print_status(status: mct_index::IndexStatus) {
    println!(
        "{} files indexed, {} symbols total.",
        status.total_files, status.total_symbols
    );
    match status.last_indexed_at {
        Some(ts) => println!("Last indexed at unix timestamp {ts}."),
        None => println!("Never indexed yet."),
    }
    if !status.languages.is_empty() {
        println!("Coverage by language:");
        for lang in &status.languages {
            println!(
                "  {}: {} files, {} symbols",
                lang.language, lang.file_count, lang.symbol_count
            );
        }
    }
    if !status.unsupported_languages.is_empty() {
        println!(
            "Languages seen but not yet supported: {}",
            status.unsupported_languages.join(", ")
        );
    }
    if !status.syntax_errors.is_empty() {
        println!("{} file(s) failed to parse:", status.syntax_errors.len());
        for err in &status.syntax_errors {
            println!("  {}: {}", err.relative_path, err.detail);
        }
    }
    if !status.read_failures.is_empty() {
        println!(
            "{} path(s) could not be read; last-good index data may be stale:",
            status.read_failures.len()
        );
        for error in &status.read_failures {
            println!("  {}: {}", error.relative_path, error.detail);
        }
    }
    if !status.dependencies.is_empty() {
        println!("Dependencies detected:");
        for manifest in &status.dependencies {
            println!(
                "  {} ({}, {} dep(s)):",
                manifest.manifest_path,
                manifest.language,
                manifest.dependencies.len()
            );
            for dep in &manifest.dependencies {
                match &dep.version {
                    Some(version) => println!("    {} {}", dep.name, version),
                    None => println!("    {}", dep.name),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Test code: an unwrap()/expect() here means a broken test precondition,
    // and panicking is the correct behavior — this module only touches
    // temp-dir fixtures this test creates itself, never repo-input content.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use std::fs;

    fn temp_project_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mct-cli-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[test]
    fn a_polyglot_project_indexes_end_to_end_through_the_cli_registry() {
        let dir = temp_project_dir("polyglot-index");
        fs::write(dir.join("lib.rs"), "pub fn add(a: i32) -> i32 { a }\n").unwrap();
        fs::write(dir.join("app.py"), "def run():\n    return add(1)\n").unwrap();
        fs::write(
            dir.join("main.go"),
            "package main\n\nfunc Run() int {\n\treturn 1\n}\n",
        )
        .unwrap();
        fs::write(dir.join("notes.md"), "# Title\n\nbody\n").unwrap();

        let db_path = dir.join(".mct-index").join("index.sqlite3");
        let registry = build_registry();
        let mut index = Index::open(&dir, &db_path, ExcludeSet::default()).unwrap();

        let report = index.reindex(&registry, false).unwrap();
        assert_eq!(report.files_parsed, 4, "{:?}", report.issues);
        assert!(report.issues.is_empty(), "{:?}", report.issues);

        let status = index.status().unwrap();
        let mut languages: Vec<&str> = status
            .languages
            .iter()
            .map(|l| l.language.as_str())
            .collect();
        languages.sort_unstable();
        assert_eq!(languages, vec!["go", "markdown", "python", "rust"]);
        assert_eq!(index.find_symbol("add").unwrap().len(), 1);

        // A second run over an untouched tree re-parses nothing, and the
        // on-disk database survives being reopened.
        drop(index);
        let mut index = Index::open(&dir, &db_path, ExcludeSet::default()).unwrap();
        let second = index.reindex(&registry, false).unwrap();
        assert_eq!(second.files_parsed, 0);
        assert_eq!(second.files_unchanged, 4);
        assert_eq!(index.status().unwrap().total_symbols, status.total_symbols);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_lua_file_is_probed_indexed_and_queryable_through_the_cli_registry() {
        let dir = temp_project_dir("lua-index");
        let file = dir.join("hello.lua");
        fs::write(&file, "function greet()\n  print(\"hello\")\nend\n").unwrap();
        let registry = build_registry();

        assert!(probe(&registry, std::slice::from_ref(&file)).is_ok());

        let db_path = dir.join(".mct-index").join("index.sqlite3");
        let mut index = Index::open(&dir, &db_path, ExcludeSet::default()).unwrap();
        let report = index.reindex(&registry, false).unwrap();
        assert_eq!(report.files_parsed, 1, "{:?}", report.issues);
        assert!(report.issues.is_empty(), "{:?}", report.issues);

        let status = index.status().unwrap();
        let lua = status
            .languages
            .iter()
            .find(|l| l.language == "lua")
            .unwrap_or_else(|| panic!("no lua row: {:?}", status.languages));
        assert_eq!(lua.file_count, 1);
        assert!(status.unsupported_languages.is_empty());

        let hits = index.find_symbol("greet").unwrap();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].language, "lua");
        assert_eq!(hits[0].relative_path, "hello.lua");
        assert_eq!((hits[0].line, hits[0].end_line), (1, Some(3)));

        let calls = index.find_calls("greet").unwrap();
        assert!(calls.iter().any(|c| c.to_name == "print"), "{calls:?}");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_and_version_accept_their_short_and_long_spellings() {
        for args in [
            ["mct-cli", "update"],
            ["mct-cli", "-u"],
            ["mct-cli", "--update"],
        ] {
            let cli = Cli::try_parse_from(args).map_err(|e| e.to_string());
            assert!(
                matches!(
                    cli,
                    Ok(Cli {
                        command: Command::Update,
                        ..
                    })
                ),
                "{args:?} → {cli:?}"
            );
        }
        for flag in ["-v", "-V", "--version"] {
            let kind = Cli::try_parse_from(["mct-cli", flag])
                .map(|_| ())
                .map_err(|e| e.kind());
            assert_eq!(kind, Err(clap::error::ErrorKind::DisplayVersion), "{flag}");
        }
    }

    #[test]
    fn mcp_register_creates_new_config() {
        let dir = temp_project_dir("new");

        mcp_register(&dir, Some("my-server".to_string()), None).expect("register should succeed");

        let contents = fs::read_to_string(dir.join(".mcp.json")).expect("config should exist");
        let json: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
        assert_eq!(json["mcpServers"]["my-server"]["command"], "mct-mcp-server");
        assert_eq!(json["mcpServers"]["my-server"]["type"], "stdio");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mcp_register_preserves_other_servers_and_is_idempotent() {
        let dir = temp_project_dir("merge");
        fs::write(
            dir.join(".mcp.json"),
            r#"{"mcpServers":{"other-tool":{"type":"stdio","command":"other-tool"}}}"#,
        )
        .expect("seed existing config");

        mcp_register(&dir, Some("mine".to_string()), None).expect("first register should succeed");
        mcp_register(&dir, Some("mine".to_string()), None).expect("second register should succeed");

        let contents = fs::read_to_string(dir.join(".mcp.json")).expect("config should exist");
        let json: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
        assert_eq!(json["mcpServers"]["other-tool"]["command"], "other-tool");
        assert_eq!(json["mcpServers"]["mine"]["command"], "mct-mcp-server");
        assert_eq!(json["mcpServers"].as_object().unwrap().len(), 2);

        fs::remove_dir_all(&dir).ok();
    }

    fn registered_args(dir: &Path, name: &str) -> Vec<String> {
        let contents = fs::read_to_string(dir.join(".mcp.json")).expect("config should exist");
        let json: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
        json["mcpServers"][name]["args"]
            .as_array()
            .expect("args array")
            .iter()
            .map(|v| v.as_str().expect("string arg").to_string())
            .collect()
    }

    #[test]
    fn parse_reindex_delay_accepts_and_normalises() {
        for (raw, want) in [
            ("15s", "15s"),
            (" 1M ", "1m"),
            ("45S", "45s"),
            ("250ms", "250ms"),
            ("90s", "90s"),
            ("OFF", "off"),
        ] {
            assert_eq!(parse_reindex_delay(raw).as_deref(), Ok(want), "{raw}");
        }
    }

    #[test]
    fn parse_reindex_delay_rejects_invalid() {
        for raw in [
            "",
            "0",
            "0s",
            "-5s",
            "10",
            "s",
            "1h",
            "abc",
            "1.5s",
            "99999999999999999999s",
        ] {
            assert!(parse_reindex_delay(raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn mcp_register_writes_reindex_delay() {
        let dir = temp_project_dir("delay-write");
        mcp_register(&dir, Some("s".to_string()), Some("30S".to_string())).expect("register");
        let args = registered_args(&dir, "s");
        assert_eq!(&args[2..], ["--reindex-delay", "30s"]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mcp_register_preserves_and_replaces_reindex_delay() {
        let dir = temp_project_dir("delay-persist");
        let reg = |d: Option<&str>| {
            mcp_register(&dir, Some("s".to_string()), d.map(str::to_string)).expect("register")
        };
        reg(Some("1m"));
        reg(None);
        assert_eq!(&registered_args(&dir, "s")[2..], ["--reindex-delay", "1m"]);
        reg(Some("off"));
        reg(None);
        assert_eq!(&registered_args(&dir, "s")[2..], ["--reindex-delay", "off"]);
        reg(Some("15s"));
        let args = registered_args(&dir, "s");
        assert_eq!(args.iter().filter(|a| *a == "--reindex-delay").count(), 1);
        assert_eq!(&args[2..], ["--reindex-delay", "15s"]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mcp_register_invalid_delay_writes_nothing() {
        let dir = temp_project_dir("delay-invalid");
        let err = mcp_register(&dir, None, Some("0s".to_string()));
        assert!(err.is_err());
        assert!(!dir.join(".mcp.json").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mcp_register_without_flag_has_no_delay_args() {
        let dir = temp_project_dir("delay-none");
        mcp_register(&dir, Some("s".to_string()), None).expect("register");
        assert_eq!(registered_args(&dir, "s").len(), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mcp_register_strips_windows_verbatim_path_prefix() {
        let dir = temp_project_dir("verbatim-path");

        mcp_register(&dir, Some("my-server".to_string()), None).expect("register should succeed");

        let contents = fs::read_to_string(dir.join(".mcp.json")).expect("config should exist");
        assert!(
            !contents.contains(r"\\?\"),
            "config should not contain the Windows verbatim-path prefix, got: {contents}"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ignore_init_creates_starter_file() {
        let dir = temp_project_dir("ignore-init-new");

        ignore_init(&dir, false).expect("ignore_init should succeed");

        let contents =
            fs::read_to_string(dir.join(mct_index::IGNORE_FILE_NAME)).expect("file should exist");
        assert_eq!(contents, mct_index::IGNORE_FILE_TEMPLATE);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ignore_init_does_not_overwrite_an_existing_file() {
        let dir = temp_project_dir("ignore-init-existing");
        fs::write(dir.join(mct_index::IGNORE_FILE_NAME), "*.md\n")
            .expect("seed existing ignore file");

        ignore_init(&dir, false).expect("ignore_init should succeed");

        let contents =
            fs::read_to_string(dir.join(mct_index::IGNORE_FILE_NAME)).expect("file should exist");
        assert_eq!(contents, "*.md\n");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ignore_init_with_import_gitignore_activates_the_directive() {
        let dir = temp_project_dir("ignore-init-import-gitignore");

        ignore_init(&dir, true).expect("ignore_init should succeed");

        let contents =
            fs::read_to_string(dir.join(mct_index::IGNORE_FILE_NAME)).expect("file should exist");
        assert!(
            contents
                .lines()
                .any(|line| line == mct_index::GITIGNORE_IMPORT_DIRECTIVE),
            "expected an active (uncommented) directive line, got: {contents}"
        );
        assert_eq!(
            mct_index::read_ignore_file(&dir),
            Vec::<String>::new(),
            "the directive alone (no other patterns, no .gitignore yet) should add nothing"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ignore_init_without_the_flag_never_touches_an_existing_file() {
        let dir = temp_project_dir("ignore-init-no-flag-existing");
        fs::write(dir.join(mct_index::IGNORE_FILE_NAME), "*.md\n")
            .expect("seed existing ignore file");

        ignore_init(&dir, false).expect("ignore_init should succeed");

        let contents =
            fs::read_to_string(dir.join(mct_index::IGNORE_FILE_NAME)).expect("file should exist");
        assert_eq!(contents, "*.md\n");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ignore_init_import_gitignore_flag_activates_the_directive_on_an_existing_file() {
        let dir = temp_project_dir("ignore-init-import-gitignore-existing");
        fs::write(dir.join(mct_index::IGNORE_FILE_NAME), "*.md\n")
            .expect("seed existing ignore file");

        ignore_init(&dir, true).expect("ignore_init should succeed");

        let contents =
            fs::read_to_string(dir.join(mct_index::IGNORE_FILE_NAME)).expect("file should exist");
        assert_eq!(contents, "*.md\n@import-gitignore\n");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ignore_init_import_gitignore_flag_is_idempotent_on_an_existing_file() {
        let dir = temp_project_dir("ignore-init-import-gitignore-idempotent");
        fs::write(dir.join(mct_index::IGNORE_FILE_NAME), "*.md\n")
            .expect("seed existing ignore file");

        ignore_init(&dir, true).expect("first ignore_init should succeed");
        let first =
            fs::read_to_string(dir.join(mct_index::IGNORE_FILE_NAME)).expect("file should exist");
        ignore_init(&dir, true).expect("second ignore_init should succeed");
        let second =
            fs::read_to_string(dir.join(mct_index::IGNORE_FILE_NAME)).expect("file should exist");

        assert_eq!(
            first, second,
            "a second run should not duplicate the directive"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn gitignore_init_creates_a_new_gitignore_excluding_the_index() {
        let dir = temp_project_dir("gitignore-init-new");

        gitignore_init(&dir).expect("gitignore_init should succeed");

        let contents = fs::read_to_string(dir.join(".gitignore")).expect("file should exist");
        assert!(
            contents.lines().any(|line| line.trim() == ".mct-index/"),
            "expected `.mct-index/` in: {contents}"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn gitignore_init_appends_to_an_existing_gitignore_without_disturbing_it() {
        let dir = temp_project_dir("gitignore-init-append");
        fs::write(dir.join(".gitignore"), "node_modules/\n").expect("seed existing gitignore");

        gitignore_init(&dir).expect("gitignore_init should succeed");

        let contents = fs::read_to_string(dir.join(".gitignore")).expect("file should exist");
        assert!(
            contents.contains("node_modules/"),
            "existing entry should survive: {contents}"
        );
        assert!(
            contents.lines().any(|line| line.trim() == ".mct-index/"),
            "expected `.mct-index/` appended, got: {contents}"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn gitignore_init_is_idempotent() {
        let dir = temp_project_dir("gitignore-init-idempotent");

        gitignore_init(&dir).expect("first gitignore_init should succeed");
        let first = fs::read_to_string(dir.join(".gitignore")).expect("file should exist");
        gitignore_init(&dir).expect("second gitignore_init should succeed");
        let second = fs::read_to_string(dir.join(".gitignore")).expect("file should exist");

        assert_eq!(first, second, "a second run should not duplicate the entry");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn gitignore_init_recognizes_an_already_present_entry() {
        let dir = temp_project_dir("gitignore-init-already-present");
        fs::write(dir.join(".gitignore"), "target/\n.mct-index/\n")
            .expect("seed existing gitignore");

        gitignore_init(&dir).expect("gitignore_init should succeed");

        let contents = fs::read_to_string(dir.join(".gitignore")).expect("file should exist");
        assert_eq!(
            contents, "target/\n.mct-index/\n",
            "should not add a duplicate entry"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_mctignore_pattern_keeps_matching_files_out_of_the_index() {
        let dir = temp_project_dir("mctignore-excludes");
        fs::write(dir.join("lib.rs"), "pub fn add(a: i32) -> i32 { a }\n").unwrap();
        fs::write(dir.join("NOTES.md"), "# notes\n").unwrap();
        fs::write(dir.join(mct_index::IGNORE_FILE_NAME), "*.md\n").unwrap();

        let db_path = dir.join(".mct-index").join("index.sqlite3");
        let registry = build_registry();
        let exclude = ExcludeSet::new(&mct_index::read_ignore_file(&dir));
        let mut index = Index::open(&dir, &db_path, exclude).unwrap();

        let report = index.reindex(&registry, false).unwrap();
        assert_eq!(report.files_parsed, 1, "{:?}", report.issues);

        let status = index.status().unwrap();
        let languages: Vec<&str> = status
            .languages
            .iter()
            .map(|l| l.language.as_str())
            .collect();
        assert_eq!(languages, vec!["rust"]);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dead_code_report_writes_a_csv_row_for_an_unreferenced_function() {
        let dir = temp_project_dir("dead-code-report");
        fs::write(
            dir.join("lib.rs"),
            "pub fn used(a: i32) -> i32 { a }\npub fn unused(a: i32) -> i32 { used(a) }\n",
        )
        .unwrap();

        let db_path = dir.join(".mct-index").join("index.sqlite3");
        let registry = build_registry();
        let mut index = Index::open(&dir, &db_path, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();

        let output = dir.join("report.csv");
        dead_code_report(&index, None, None, Some(output.clone())).expect("report should succeed");

        let contents = fs::read_to_string(&output).expect("report should exist");
        let mut lines = contents.lines();
        assert_eq!(
            lines.next().unwrap(),
            "project_root,file,function,kind,language,start_line,end_line"
        );
        let rows: Vec<&str> = lines.collect();
        assert_eq!(
            rows.len(),
            1,
            "expected exactly one candidate, got: {contents}"
        );
        assert!(
            rows[0].contains(",lib.rs,unused,function,rust,"),
            "{}",
            rows[0]
        );
        assert!(
            !contents.contains(",used,"),
            "`used` should not be flagged: {contents}"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn probe_accepts_valid_files_and_reports_syntax_errors() {
        let dir = temp_project_dir("probe");
        let good = dir.join("good.py");
        let bad = dir.join("bad.py");
        fs::write(&good, "def f():\n    return g()\n").expect("write good.py");
        fs::write(&bad, "def f(:\n    pass\n").expect("write bad.py");
        let registry = build_registry();

        assert!(probe(&registry, std::slice::from_ref(&good)).is_ok());
        let err = probe(&registry, &[good, bad]).expect_err("bad.py fails");
        assert!(err.to_string().contains("1 of 2"), "{err}");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mcp_register_defaults_name_to_root_directory_name() {
        let dir = temp_project_dir("default-name");

        mcp_register(&dir, None, None).expect("register should succeed");

        let contents = fs::read_to_string(dir.join(".mcp.json")).expect("config should exist");
        let json: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
        let expected_name = dir
            .canonicalize()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(json["mcpServers"]
            .as_object()
            .unwrap()
            .contains_key(&expected_name));

        fs::remove_dir_all(&dir).ok();
    }
}
