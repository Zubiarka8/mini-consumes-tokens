use std::path::{Path, PathBuf};
use std::sync::Arc;

use ccm_core::LanguageRegistry;
use ccm_index::{ExcludeSet, Index};
use clap::{Parser, Subcommand};

/// mini-consumes-tokens: index a repository and inspect the index from the
/// command line. The MCP server (`ccm-mcp-server`) does the same indexing
/// automatically at startup — this CLI is for manual/scripted use (CI, a
/// pre-commit hook, or just checking coverage before wiring up the plugin).
#[derive(Parser, Debug)]
#[command(name = "ccm")]
struct Cli {
    /// Project root to operate on. Defaults to the current working directory.
    #[arg(long, global = true)]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Build the index for the first time. Equivalent to `reindex` — kept as
    /// a separate, discoverable first command for a fresh checkout.
    Init,
    /// Re-scan the project and update the index.
    Reindex {
        /// Re-parse every supported file, even if unchanged since last run.
        #[arg(long)]
        force: bool,
    },
    /// Report index health: coverage per language, last indexed time,
    /// unsupported languages seen, and files that failed to parse.
    Status,
    /// Write (or update) `.mcp.json` at the project root so Claude Code (or
    /// any other client reading that file) can launch `ccm-mcp-server` for
    /// this project. Merges into an existing file instead of overwriting it,
    /// so other servers already configured there are left untouched.
    McpRegister {
        /// Server name (the key under `mcpServers`). Defaults to the root
        /// directory's own name.
        #[arg(long)]
        name: Option<String>,
    },
}

fn build_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(ccm_lang_rust::RustParser));
    registry.register(Arc::new(ccm_lang_python::PythonParser));
    registry.register(Arc::new(ccm_lang_java::JavaParser));
    registry.register(Arc::new(ccm_lang_kotlin::KotlinParser));
    registry.register(Arc::new(ccm_lang_csharp::CSharpParser));
    registry.register(Arc::new(ccm_lang_js_ts::JsTsParser));
    registry.register(Arc::new(ccm_lang_cpp::CppParser));
    registry.register(Arc::new(ccm_lang_go::GoParser));
    registry.register(Arc::new(ccm_lang_html::HtmlParser));
    registry.register(Arc::new(ccm_lang_css::CssParser));
    registry.register(Arc::new(ccm_lang_xml::XmlParser));
    registry.register(Arc::new(ccm_lang_xaml::XamlParser));
    registry.register(Arc::new(ccm_lang_bash::BashParser));
    registry.register(Arc::new(ccm_lang_powershell::PowerShellParser));
    registry.register(Arc::new(ccm_lang_php::PhpParser));
    registry.register(Arc::new(ccm_lang_md::MarkdownParser));
    registry
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let root = match cli.root {
        Some(root) => root,
        None => std::env::current_dir()?,
    };

    // Doesn't touch the index at all, so it's handled before Index::open —
    // running it shouldn't have the side effect of creating .ccm-index/.
    if let Command::McpRegister { name } = cli.command {
        return mcp_register(&root, name);
    }

    let db_path = root.join(".ccm-index").join("index.sqlite3");
    let registry = build_registry();
    let mut index = Index::open(&root, &db_path, ExcludeSet::default())?;

    match cli.command {
        Command::Init => print_reindex(index.reindex(&registry, false)?),
        Command::Reindex { force } => print_reindex(index.reindex(&registry, force)?),
        Command::Status => print_status(index.status()?),
        Command::McpRegister { .. } => unreachable!("returned above"),
    }

    Ok(())
}

/// Inserts (or replaces) this project's `ccm-mcp-server` entry under
/// `mcpServers` in `<root>/.mcp.json`, creating the file if it doesn't exist.
/// Any other server already configured there is left untouched — this only
/// ever writes the one key it owns.
fn mcp_register(root: &Path, name: Option<String>) -> anyhow::Result<()> {
    let root = root.canonicalize()?;
    let server_name = name.unwrap_or_else(|| {
        root.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("ccm-mcp-server")
            .to_string()
    });
    let config_path = root.join(".mcp.json");

    let mut config: serde_json::Value = if config_path.exists() {
        serde_json::from_str(&std::fs::read_to_string(&config_path)?)?
    } else {
        serde_json::json!({})
    };

    let top = config
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{}: top level must be a JSON object", config_path.display()))?;
    let servers = top
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{}: \"mcpServers\" must be a JSON object", config_path.display()))?;

    servers.insert(
        server_name.clone(),
        serde_json::json!({
            "type": "stdio",
            "command": "ccm-mcp-server",
            "args": ["--root", display_path(&root)],
            "env": {}
        }),
    );

    std::fs::write(&config_path, format!("{}\n", serde_json::to_string_pretty(&config)?))?;
    println!("Registered `{server_name}` in {}", display_path(&config_path));
    Ok(())
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

fn print_reindex(report: ccm_index::ReindexReport) {
    println!(
        "Reindex complete: {} parsed, {} unchanged, {} removed, {} symbols written.",
        report.files_parsed, report.files_unchanged, report.files_removed, report.symbols_written
    );
    if !report.issues.is_empty() {
        println!("{} issue(s):", report.issues.len());
        for issue in &report.issues {
            println!("  {} [{:?}]: {}", issue.relative_path, issue.kind, issue.detail);
        }
    }
}

fn print_status(status: ccm_index::IndexStatus) {
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
            println!("  {}: {} files, {} symbols", lang.language, lang.file_count, lang.symbol_count);
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
        let dir = std::env::temp_dir().join(format!("ccm-cli-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[test]
    fn mcp_register_creates_new_config() {
        let dir = temp_project_dir("new");

        mcp_register(&dir, Some("my-server".to_string())).expect("register should succeed");

        let contents = fs::read_to_string(dir.join(".mcp.json")).expect("config should exist");
        let json: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
        assert_eq!(json["mcpServers"]["my-server"]["command"], "ccm-mcp-server");
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

        mcp_register(&dir, Some("mine".to_string())).expect("first register should succeed");
        mcp_register(&dir, Some("mine".to_string())).expect("second register should succeed");

        let contents = fs::read_to_string(dir.join(".mcp.json")).expect("config should exist");
        let json: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
        assert_eq!(json["mcpServers"]["other-tool"]["command"], "other-tool");
        assert_eq!(json["mcpServers"]["mine"]["command"], "ccm-mcp-server");
        assert_eq!(json["mcpServers"].as_object().unwrap().len(), 2);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mcp_register_strips_windows_verbatim_path_prefix() {
        let dir = temp_project_dir("verbatim-path");

        mcp_register(&dir, Some("my-server".to_string())).expect("register should succeed");

        let contents = fs::read_to_string(dir.join(".mcp.json")).expect("config should exist");
        assert!(
            !contents.contains(r"\\?\"),
            "config should not contain the Windows verbatim-path prefix, got: {contents}"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mcp_register_defaults_name_to_root_directory_name() {
        let dir = temp_project_dir("default-name");

        mcp_register(&dir, None).expect("register should succeed");

        let contents = fs::read_to_string(dir.join(".mcp.json")).expect("config should exist");
        let json: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
        let expected_name = dir.canonicalize().unwrap().file_name().unwrap().to_string_lossy().to_string();
        assert!(json["mcpServers"].as_object().unwrap().contains_key(&expected_name));

        fs::remove_dir_all(&dir).ok();
    }
}
