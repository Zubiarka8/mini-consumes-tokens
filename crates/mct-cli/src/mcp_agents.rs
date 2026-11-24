//! Merge MCP configuration for supported agent clients.

use std::fs;
use std::path::Path;

use crate::skills_register::{atomic_write, regular_or_missing, safe_directories};
use anyhow::{Context, Result};
use serde_json::{json, Value};

fn read_json(path: &Path) -> Result<Value> {
    if regular_or_missing(path)? {
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    } else {
        Ok(json!({}))
    }
}

fn object_at<'a>(
    value: &'a mut Value,
    key: &str,
) -> Result<&'a mut serde_json::Map<String, Value>> {
    value
        .as_object_mut()
        .context("configuration must be an object")?
        .entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .with_context(|| format!("{key} must be an object"))
}

fn retain_delay(args: &[String], old: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut result = args.to_vec();
    if !result.iter().any(|arg| arg == "--reindex-delay") {
        let old: Vec<_> = old.into_iter().collect();
        if let Some(delay) = old.iter().enumerate().find_map(|(i, arg)| {
            if arg == "--reindex-delay" {
                old.get(i + 1).cloned()
            } else {
                arg.strip_prefix("--reindex-delay=").map(str::to_string)
            }
        }) {
            result.extend(["--reindex-delay".to_string(), delay]);
        }
    }
    result
}

fn merge_json(
    path: &Path,
    section: &str,
    name: &str,
    args: &[String],
    dry_run: bool,
) -> Result<()> {
    safe_directories(path.parent().context("config has no parent")?)?;
    let mut config = read_json(path)?;
    let entry = if section == "opencode" {
        // OpenCode reads `mcp.<name>` (verified with opencode v2.0.26:
        // `opencode mcp list` shows nothing for `mcp.servers.<name>`).
        let server = object_at(&mut config, "mcp")?
            .entry(name)
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .context("server must be an object")?;
        let old = server
            .get("command")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string);
        let mut command = vec!["mct-mcp-server".to_string()];
        command.extend(retain_delay(args, old));
        server.insert("type".into(), json!("local"));
        server.insert("command".into(), json!(command));
        "OpenCode"
    } else {
        let server = object_at(&mut config, "mcpServers")?
            .entry(name)
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .context("server must be an object")?;
        let old = server
            .get("args")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string);
        let args = retain_delay(args, old);
        server.insert("type".into(), json!("stdio"));
        server.insert("command".into(), json!("mct-mcp-server"));
        server.insert("args".into(), json!(args));
        server.entry("env").or_insert_with(|| json!({}));
        section
    };
    println!(
        "{} {entry}: {}",
        if dry_run { "Would update" } else { "Updating" },
        path.display()
    );
    if !dry_run {
        fs::create_dir_all(path.parent().context("config has no parent")?)?;
        atomic_write(
            path,
            &format!("{}\n", serde_json::to_string_pretty(&config)?),
        )?;
    }
    Ok(())
}

fn merge_codex(path: &Path, name: &str, args: &[String], dry_run: bool) -> Result<()> {
    safe_directories(path.parent().context("config has no parent")?)?;
    let text = if regular_or_missing(path)? {
        fs::read_to_string(path)?
    } else {
        String::new()
    };
    let mut doc = text.parse::<toml_edit::DocumentMut>()?;
    let tables = doc
        .as_table_mut()
        .entry("mcp_servers")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let tables = tables
        .as_table_mut()
        .context("mcp_servers must be a TOML table")?;
    let server = tables
        .entry(name)
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let server = server
        .as_table_mut()
        .context("MCP server must be a TOML table")?;
    server.insert("command", toml_edit::value("mct-mcp-server"));
    let old = server
        .get("args")
        .and_then(toml_edit::Item::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml_edit::Value::as_str)
        .map(str::to_string);
    let args = retain_delay(args, old);
    let mut array = toml_edit::Array::new();
    for arg in &args {
        array.push(arg.as_str());
    }
    server.insert("args", toml_edit::value(array));
    println!(
        "{} Codex: {}",
        if dry_run { "Would update" } else { "Updating" },
        path.display()
    );
    if !dry_run {
        fs::create_dir_all(path.parent().context("config has no parent")?)?;
        atomic_write(path, &doc.to_string())?;
    }
    Ok(())
}

pub(crate) fn run(
    root: &Path,
    name: Option<String>,
    delay: Option<String>,
    agents: &[String],
    global: bool,
    dry_run: bool,
) -> Result<()> {
    let root = root.canonicalize()?;
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(std::path::PathBuf::from)
        .context("home directory is unavailable")?;
    let chosen: Vec<&str> = if agents.is_empty() {
        crate::skills_register::detected(&root, &home)
    } else {
        agents.iter().map(String::as_str).collect()
    };
    for agent in &chosen {
        if !crate::skills_register::AGENTS.contains(agent) {
            anyhow::bail!("unknown agent: {agent}");
        }
    }
    let name = name.unwrap_or_else(|| {
        root.file_name()
            .and_then(|part| part.to_str())
            .unwrap_or("mct-mcp-server")
            .to_string()
    });
    let mut args = vec!["--root".to_string(), super::display_path(&root)];
    if let Some(delay) = delay {
        args.extend(["--reindex-delay".to_string(), delay]);
    }
    if agents.is_empty() || chosen.contains(&"claude") || chosen.contains(&"copilot") {
        merge_json(&root.join(".mcp.json"), "mcpServers", &name, &args, dry_run)?;
    }
    if chosen.contains(&"cursor") {
        merge_json(
            &root.join(".cursor/mcp.json"),
            "mcpServers",
            &name,
            &args,
            dry_run,
        )?;
    }
    if chosen.contains(&"opencode") {
        merge_json(
            &root.join("opencode.json"),
            "opencode",
            &name,
            &args,
            dry_run,
        )?;
    }
    if chosen.contains(&"codex") {
        if global {
            merge_codex(&home.join(".codex/config.toml"), &name, &args, dry_run)?;
        } else {
            println!("Codex registration requires --global");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    /// With no other agent detected, `mcp-register` runs the legacy writer
    /// and then this merge on the same `.mcp.json`: the merge must leave
    /// the legacy output byte for byte, with or without a reindex delay.
    #[test]
    fn the_merge_leaves_the_legacy_mcp_json_unchanged() {
        for delay in [None, Some("2s"), Some("750ms")] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            fs::write(
                root.join(".mcp.json"),
                r#"{"mcpServers":{"other":{"command":"x"}}}"#,
            )
            .unwrap();
            crate::mcp_register(&root, Some("srv".into()), delay.map(String::from)).unwrap();
            let legacy = fs::read_to_string(root.join(".mcp.json")).unwrap();
            let mut args = vec!["--root".to_string(), crate::display_path(&root)];
            if let Some(delay) = delay {
                args.extend(["--reindex-delay".to_string(), delay.to_string()]);
            }
            merge_json(&root.join(".mcp.json"), "mcpServers", "srv", &args, false).unwrap();
            assert_eq!(
                fs::read_to_string(root.join(".mcp.json")).unwrap(),
                legacy,
                "delay {delay:?}"
            );
        }
    }

    #[test]
    fn codex_preserves_comments_and_other_servers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "# Keep me\n[mcp_servers.other]\ncommand = 'other'\n").unwrap();
        merge_codex(&path, "mct", &["--root".into(), "/repo".into()], false).unwrap();
        let result = fs::read_to_string(path).unwrap();
        assert!(result.contains("# Keep me"));
        assert!(result.contains("[mcp_servers.other]"));
        assert!(result.contains("[mcp_servers.mct]"));
    }

    #[test]
    fn opencode_preserves_other_servers_and_supports_dry_run() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.json");
        fs::write(&path, r#"{"mcp":{"other":{"type":"local"}}}"#).unwrap();
        let before = fs::read_to_string(&path).unwrap();
        merge_json(
            &path,
            "opencode",
            "mct",
            &["--root".into(), "/repo".into()],
            true,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        merge_json(
            &path,
            "opencode",
            "mct",
            &["--root".into(), "/repo".into()],
            false,
        )
        .unwrap();
        let result: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(result["mcp"]["other"]["type"], "local");
        assert_eq!(result["mcp"]["mct"]["command"][0], "mct-mcp-server");
        assert!(result["mcp"].get("servers").is_none(), "{result}");
    }

    /// The shape `opencode mcp list` (v2.0.26) reads: `mcp.<name>` with
    /// `type: local` and the command as one array.
    #[test]
    fn a_new_opencode_config_puts_the_server_directly_under_mcp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.json");
        merge_json(
            &path,
            "opencode",
            "mct",
            &["--root".into(), "/repo".into()],
            false,
        )
        .unwrap();
        let result: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(
            result,
            json!({"mcp": {"mct": {"type": "local", "command": ["mct-mcp-server", "--root", "/repo"]}}})
        );
    }

    #[test]
    fn reregistering_keeps_delay_and_environment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        fs::write(&path, r#"{"mcpServers":{"mct":{"env":{"CUSTOM":"kept"},"args":["--reindex-delay","30s"]},"other":{"command":"other"}}}"#).unwrap();
        merge_json(
            &path,
            "mcpServers",
            "mct",
            &["--root".into(), "/repo".into()],
            false,
        )
        .unwrap();
        let result: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(result["mcpServers"]["mct"]["env"]["CUSTOM"], "kept");
        assert_eq!(
            result["mcpServers"]["mct"]["args"],
            json!(["--root", "/repo", "--reindex-delay", "30s"])
        );
        assert_eq!(result["mcpServers"]["other"]["command"], "other");
    }

    #[test]
    fn selected_project_agents_use_their_config_and_preserve_legacy_opencode() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("opencode.json"),
            r#"{"mcp":{"other":{"type":"local"}}}"#,
        )
        .unwrap();
        run(
            root,
            Some("mct".into()),
            None,
            &["cursor".into(), "opencode".into()],
            false,
            false,
        )
        .unwrap();
        let cursor: Value =
            serde_json::from_str(&fs::read_to_string(root.join(".cursor/mcp.json")).unwrap())
                .unwrap();
        let opencode: Value =
            serde_json::from_str(&fs::read_to_string(root.join("opencode.json")).unwrap()).unwrap();
        assert_eq!(cursor["mcpServers"]["mct"]["command"], "mct-mcp-server");
        assert_eq!(opencode["mcp"]["other"]["type"], "local");
        assert_eq!(opencode["mcp"]["mct"]["command"][0], "mct-mcp-server");
        assert!(!root.join(".mcp.json").exists());
    }
}
