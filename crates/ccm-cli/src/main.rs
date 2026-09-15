use std::path::PathBuf;
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
}

fn build_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(ccm_lang_rust::RustParser));
    registry.register(Arc::new(ccm_lang_python::PythonParser));
    registry.register(Arc::new(ccm_lang_java::JavaParser));
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
    let db_path = root.join(".claude-index").join("index.sqlite3");
    let registry = build_registry();
    let mut index = Index::open(&root, &db_path, ExcludeSet::default())?;

    match cli.command {
        Command::Init => print_reindex(index.reindex(&registry, false)?),
        Command::Reindex { force } => print_reindex(index.reindex(&registry, force)?),
        Command::Status => print_status(index.status()?),
    }

    Ok(())
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
