//! Installation of the embedded agent skill templates.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

const MANIFEST: &str = ".mct-skills-managed";

fn digest(text: &str) -> Result<String> {
    Ok(git2::Oid::hash_object(git2::ObjectType::Blob, text.as_bytes())?.to_string())
}

pub(crate) fn regular_or_missing(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(true),
        Ok(_) => anyhow::bail!("not a regular file: {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    let parent = path.parent().context("file has no parent")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(contents.as_bytes())?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub(crate) fn safe_directories(path: &Path) -> Result<()> {
    // Check the destination and its agent-owned parent directories. System
    // ancestors such as macOS /var can legitimately be symlinks.
    for ancestor in path.ancestors().take(3) {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if !meta.file_type().is_dir() => {
                anyhow::bail!("not a directory: {}", ancestor.display())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn install(dir: &Path, dry_run: bool, force: bool) -> Result<()> {
    safe_directories(dir)?;
    let manifest_path = dir.join(MANIFEST);
    let mut managed = BTreeMap::new();
    if regular_or_missing(&manifest_path)? {
        for line in fs::read_to_string(&manifest_path)?.lines() {
            let (name, hash) = line.split_once('\t').context("invalid skill manifest")?;
            if !name.starts_with("mct-")
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                || git2::Oid::from_str(hash).is_err()
            {
                anyhow::bail!("invalid skill manifest entry");
            }
            managed.insert(name.to_string(), hash.to_string());
        }
    }
    for skill in crate::skills::SKILLS {
        let path = dir.join(skill.name).join("SKILL.md");
        safe_directories(path.parent().context("skill has no parent")?)?;
        let exists = regular_or_missing(&path)?;
        let wanted = digest(skill.text)?;
        let current = if exists {
            Some(digest(&fs::read_to_string(&path)?)?)
        } else {
            None
        };
        if current
            .as_ref()
            .is_some_and(|hash| hash != &wanted && managed.get(skill.name) != Some(hash))
            && !force
        {
            println!("Preserved edited skill: {}", path.display());
            continue;
        }
        if current.as_ref() != Some(&wanted) {
            println!(
                "{} {}",
                if dry_run { "Would write" } else { "Writing" },
                path.display()
            );
            if !dry_run {
                fs::create_dir_all(path.parent().context("skill has no parent")?)?;
                atomic_write(&path, skill.text)?;
            }
        }
        managed.insert(skill.name.to_string(), wanted);
    }
    let retired: Vec<_> = managed
        .keys()
        .filter(|name| {
            !crate::skills::SKILLS
                .iter()
                .any(|s| s.name == name.as_str())
        })
        .cloned()
        .collect();
    for name in retired {
        let folder = dir.join(&name);
        safe_directories(&folder)?;
        let path = folder.join("SKILL.md");
        if !regular_or_missing(&path)? {
            managed.remove(&name);
        } else if managed.get(&name) == Some(&digest(&fs::read_to_string(&path)?)?) {
            println!(
                "{} {}",
                if dry_run { "Would remove" } else { "Removing" },
                path.display()
            );
            if !dry_run {
                fs::remove_file(&path)?;
                if fs::read_dir(&folder)?.next().is_none() {
                    fs::remove_dir(folder)?;
                }
            }
            managed.remove(&name);
        }
    }
    if !dry_run {
        fs::create_dir_all(dir)?;
        let contents = managed
            .iter()
            .map(|(name, hash)| format!("{name}\t{hash}\n"))
            .collect::<String>();
        atomic_write(&manifest_path, &contents)?;
    }
    Ok(())
}

pub(crate) const AGENTS: &[&str] = &["claude", "codex", "cursor", "copilot", "opencode"];
const START: &str = "<!-- mct:skills -->";
const END: &str = "<!-- /mct:skills -->";
const BLOCK: &str = "<!-- mct:skills -->\nFor code lookup, impact, dead code, duplicates or the index, use the mct-* skills.\n<!-- /mct:skills -->";

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            dir.join(program).is_file()
                || (cfg!(windows) && dir.join(format!("{program}.exe")).is_file())
        })
    })
}

pub(crate) fn detected(root: &Path, home: &Path) -> Vec<&'static str> {
    detected_with(root, home, on_path)
}

/// [`detected`] with the `PATH` lookup injected, so tests do not depend on
/// which agents the machine running them has installed.
fn detected_with(root: &Path, home: &Path, on_path: impl Fn(&str) -> bool) -> Vec<&'static str> {
    type AgentSignals = (
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
        Option<&'static str>,
    );
    let signals: &[AgentSignals] = &[
        (
            "claude",
            &[".claude", "CLAUDE.md"],
            &[".claude"],
            Some("claude"),
        ),
        (
            "codex",
            &[".codex", "AGENTS.md"],
            &[".codex"],
            Some("codex"),
        ),
        ("cursor", &[".cursor", ".cursorrules"], &[".cursor"], None),
        (
            "copilot",
            &[".github/copilot-instructions.md", ".github/skills"],
            &[".copilot"],
            None,
        ),
        (
            "opencode",
            &[".opencode", "opencode.json"],
            &[".config/opencode"],
            Some("opencode"),
        ),
    ];
    signals
        .iter()
        .filter(|(_, project, user, command)| {
            project.iter().any(|p| root.join(p).exists())
                || user.iter().any(|p| home.join(p).exists())
                || command.is_some_and(&on_path)
        })
        .map(|(name, _, _, _)| *name)
        .collect()
}

fn pointer(path: &Path, dry_run: bool) -> Result<()> {
    let text = if regular_or_missing(path)? {
        fs::read_to_string(path)?
    } else {
        String::new()
    };
    let updated = match (text.find(START), text.find(END)) {
        (None, None) => format!(
            "{}{}{}\n",
            text,
            if text.is_empty() || text.ends_with('\n') {
                ""
            } else {
                "\n"
            },
            BLOCK
        ),
        (Some(start), Some(end))
            if end >= start + START.len()
                && text.matches(START).count() == 1
                && text.matches(END).count() == 1 =>
        {
            format!("{}{}{}", &text[..start], BLOCK, &text[end + END.len()..])
        }
        _ => anyhow::bail!("invalid skill markers in {}", path.display()),
    };
    if updated != text {
        println!(
            "{} {}",
            if dry_run { "Would update" } else { "Updating" },
            path.display()
        );
        if !dry_run {
            atomic_write(path, &updated)?;
        }
    }
    Ok(())
}

pub(crate) fn run(
    root: &Path,
    agents: &[String],
    global: bool,
    dry_run: bool,
    force: bool,
) -> Result<()> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(std::path::PathBuf::from)
        .context("home directory is unavailable")?;
    let chosen: Vec<&str> = if agents.is_empty() {
        detected(root, &home)
    } else {
        agents.iter().map(String::as_str).collect()
    };
    install_for(root, &home, &chosen, global, dry_run, force)
}

/// Installs the skills and pointer blocks for the `chosen` agents (none
/// chosen: the shared `.agents/skills/` and `AGENTS.md` only).
fn install_for(
    root: &Path,
    home: &Path,
    chosen: &[&str],
    global: bool,
    dry_run: bool,
    force: bool,
) -> Result<()> {
    for agent in chosen {
        if !AGENTS.contains(agent) {
            anyhow::bail!("unknown agent: {agent}");
        }
    }
    let base = if global { home } else { root };
    safe_directories(base)?;
    println!(
        "Detected: {}",
        if chosen.is_empty() {
            "none".to_string()
        } else {
            chosen.join(", ")
        }
    );
    if chosen.is_empty() || chosen.iter().any(|a| *a != "claude") {
        install(&base.join(".agents/skills"), dry_run, force)?;
    }
    if chosen.contains(&"claude") {
        install(&base.join(".claude/skills"), dry_run, force)?;
    }
    if chosen.contains(&"copilot") {
        install(
            &base.join(if global {
                ".copilot/skills"
            } else {
                ".github/skills"
            }),
            dry_run,
            force,
        )?;
    }
    if !global {
        if chosen.is_empty() || chosen.iter().any(|a| *a != "claude") {
            pointer(&base.join("AGENTS.md"), dry_run)?;
        }
        if chosen.contains(&"claude") {
            pointer(&base.join("CLAUDE.md"), dry_run)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn edited_skills_are_preserved_unless_forced() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), false, false).unwrap();
        let path = dir.path().join("mct-find/SKILL.md");
        std::fs::write(&path, "custom").unwrap();
        install(dir.path(), false, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "custom");
        install(dir.path(), false, true).unwrap();
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("name: mct-find"));
    }

    #[test]
    fn dry_run_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("skills");
        install(&target, true, false).unwrap();
        assert!(!target.exists());
    }

    #[test]
    fn pointer_preserves_existing_text_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("AGENTS.md");
        fs::write(&path, "Keep this instruction.\n").unwrap();
        pointer(&path, false).unwrap();
        let first = fs::read_to_string(&path).unwrap();
        pointer(&path, false).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), first);
        assert!(first.starts_with("Keep this instruction.\n"));
        assert_eq!(first.matches(START).count(), 1);
    }

    #[test]
    fn explicit_agents_get_their_documented_skill_directories() {
        let dir = tempfile::tempdir().unwrap();
        let agents = vec!["claude".to_string(), "copilot".to_string()];
        run(dir.path(), &agents, false, false, false).unwrap();
        for folder in [".claude/skills", ".agents/skills", ".github/skills"] {
            assert!(dir.path().join(folder).join("mct-find/SKILL.md").is_file());
        }
        assert!(dir.path().join("AGENTS.md").is_file());
        assert!(dir.path().join("CLAUDE.md").is_file());
    }

    #[test]
    fn each_project_or_user_signal_detects_its_agent_and_none_detects_nothing() {
        let cases: &[(&str, bool, &str)] = &[
            (".claude", false, "claude"),
            ("CLAUDE.md", false, "claude"),
            (".codex", false, "codex"),
            ("AGENTS.md", false, "codex"),
            (".cursor", false, "cursor"),
            (".cursorrules", false, "cursor"),
            (".github/copilot-instructions.md", false, "copilot"),
            (".copilot", true, "copilot"),
            ("opencode.json", false, "opencode"),
            (".config/opencode", true, "opencode"),
        ];
        for (signal, in_home, agent) in cases {
            let (root, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
            let at = if *in_home { home.path() } else { root.path() }.join(signal);
            fs::create_dir_all(at.parent().unwrap()).unwrap();
            fs::write(&at, "").unwrap();
            assert_eq!(
                detected_with(root.path(), home.path(), |_| false),
                [*agent],
                "signal {signal}"
            );
        }
        let (root, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        assert!(detected_with(root.path(), home.path(), |_| false).is_empty());
        assert_eq!(
            detected_with(root.path(), home.path(), |program| program == "codex"),
            ["codex"]
        );
    }

    #[test]
    fn nothing_detected_installs_only_the_shared_directory_and_agents_md() {
        let (root, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        install_for(root.path(), home.path(), &[], false, false, false).unwrap();
        assert!(root
            .path()
            .join(".agents/skills/mct-find/SKILL.md")
            .is_file());
        assert!(root.path().join("AGENTS.md").is_file());
        for absent in [".claude", ".github", "CLAUDE.md"] {
            assert!(!root.path().join(absent).exists(), "{absent}");
        }
    }

    #[test]
    fn retired_skills_are_removed_only_if_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let untouched = dir.path().join("mct-retired/SKILL.md");
        let edited = dir.path().join("mct-edited/SKILL.md");
        fs::create_dir_all(untouched.parent().unwrap()).unwrap();
        fs::create_dir_all(edited.parent().unwrap()).unwrap();
        fs::write(&untouched, "original").unwrap();
        fs::write(&edited, "custom").unwrap();
        let hash = digest("original").unwrap();
        fs::write(
            dir.path().join(MANIFEST),
            format!("mct-retired\t{hash}\nmct-edited\t{hash}\n"),
        )
        .unwrap();
        install(dir.path(), false, false).unwrap();
        assert!(!untouched.exists());
        assert_eq!(fs::read_to_string(edited).unwrap(), "custom");
    }

    #[cfg(unix)]
    #[test]
    fn installation_rejects_a_symlinked_skill_directory() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(other.path(), dir.path().join("mct-find")).unwrap();
        assert!(install(dir.path(), false, true).is_err());
        assert!(!other.path().join("SKILL.md").exists());
    }
}
