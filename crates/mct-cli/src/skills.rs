//! The `mct-*` agent skills shipped inside the binary (issue #560).
//!
//! Each skill is a short `SKILL.md` under `crates/mct-cli/skills/<name>/`,
//! compiled in with `include_str!`, so the text works offline and always
//! matches the version of the tools it names. Editing a skill is editing that
//! Markdown file. `skills-register` installs them for the AI agents a project
//! uses; this module holds the templates and tests that keep them honest.

/// One skill template.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Skill {
    /// The directory name and the `name` in the frontmatter, such as `mct-impact`.
    pub(crate) name: &'static str,
    /// The whole `SKILL.md`: frontmatter, then the steps.
    pub(crate) text: &'static str,
}

/// Every shipped skill, in name order.
pub(crate) const SKILLS: &[Skill] = &[
    Skill {
        name: "mct-callers",
        text: include_str!("../skills/mct-callers/SKILL.md"),
    },
    Skill {
        name: "mct-calls",
        text: include_str!("../skills/mct-calls/SKILL.md"),
    },
    Skill {
        name: "mct-context",
        text: include_str!("../skills/mct-context/SKILL.md"),
    },
    Skill {
        name: "mct-dead-code",
        text: include_str!("../skills/mct-dead-code/SKILL.md"),
    },
    Skill {
        name: "mct-deps",
        text: include_str!("../skills/mct-deps/SKILL.md"),
    },
    Skill {
        name: "mct-duplicates",
        text: include_str!("../skills/mct-duplicates/SKILL.md"),
    },
    Skill {
        name: "mct-find",
        text: include_str!("../skills/mct-find/SKILL.md"),
    },
    Skill {
        name: "mct-impact",
        text: include_str!("../skills/mct-impact/SKILL.md"),
    },
    Skill {
        name: "mct-meaning",
        text: include_str!("../skills/mct-meaning/SKILL.md"),
    },
    Skill {
        name: "mct-obsidian",
        text: include_str!("../skills/mct-obsidian/SKILL.md"),
    },
    Skill {
        name: "mct-overview",
        text: include_str!("../skills/mct-overview/SKILL.md"),
    },
    Skill {
        name: "mct-probe",
        text: include_str!("../skills/mct-probe/SKILL.md"),
    },
    Skill {
        name: "mct-refs",
        text: include_str!("../skills/mct-refs/SKILL.md"),
    },
    Skill {
        name: "mct-reindex",
        text: include_str!("../skills/mct-reindex/SKILL.md"),
    },
    Skill {
        name: "mct-search",
        text: include_str!("../skills/mct-search/SKILL.md"),
    },
    Skill {
        name: "mct-setup",
        text: include_str!("../skills/mct-setup/SKILL.md"),
    },
    Skill {
        name: "mct-skeleton",
        text: include_str!("../skills/mct-skeleton/SKILL.md"),
    },
    Skill {
        name: "mct-status",
        text: include_str!("../skills/mct-status/SKILL.md"),
    },
    Skill {
        name: "mct-symbols",
        text: include_str!("../skills/mct-symbols/SKILL.md"),
    },
    Skill {
        name: "mct-tree",
        text: include_str!("../skills/mct-tree/SKILL.md"),
    },
    Skill {
        name: "mct-update",
        text: include_str!("../skills/mct-update/SKILL.md"),
    },
    Skill {
        name: "mct-web",
        text: include_str!("../skills/mct-web/SKILL.md"),
    },
];

#[cfg(test)]
mod tests {
    // Test code: an unwrap()/expect() here means a broken test precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::collections::BTreeSet;

    use super::*;

    /// The tools each skill must call, from the table in issue #560.
    const USES: &[(&str, &[&str])] = &[
        ("mct-overview", &["get_project_overview", "get_file_tree"]),
        ("mct-tree", &["get_file_tree"]),
        ("mct-skeleton", &["get_file_skeleton"]),
        ("mct-symbols", &["list_symbols"]),
        ("mct-find", &["find_symbol"]),
        ("mct-search", &["search_symbols_like"]),
        ("mct-meaning", &["hybrid_search_like"]),
        ("mct-callers", &["find_callers"]),
        ("mct-calls", &["find_calls"]),
        ("mct-refs", &["find_references"]),
        ("mct-impact", &["impact_analysis", "build_context_pack"]),
        ("mct-context", &["build_context_pack"]),
        ("mct-deps", &["get_dependency_graph"]),
        (
            "mct-dead-code",
            &["find_dead_code", "find_references", "impact_analysis"],
        ),
        (
            "mct-duplicates",
            &["find_duplicate_code", "build_context_pack"],
        ),
        ("mct-status", &["get_indexing_status"]),
        ("mct-reindex", &["reindex"]),
        ("mct-setup", &[]),
        ("mct-probe", &[]),
        ("mct-web", &[]),
        ("mct-obsidian", &[]),
        ("mct-update", &[]),
    ];

    /// Tool names, and every parameter name those tools take, from the live
    /// tool router: the names a skill is allowed to use.
    fn tool_vocabulary() -> (BTreeSet<String>, BTreeSet<String>) {
        let root = std::env::temp_dir();
        let index =
            mct_index::Index::open_in_memory(&root, mct_index::ExcludeSet::default()).unwrap();
        let server = mct_mcp_server::server::MctServer::new(index, mct_languages::build_registry());
        let mut tools = BTreeSet::new();
        let mut parameters = BTreeSet::new();
        for tool in server.tool_catalog() {
            tools.insert(tool.name.to_string());
            if let Some(properties) = tool
                .input_schema
                .get("properties")
                .and_then(|p| p.as_object())
            {
                parameters.extend(properties.keys().cloned());
            }
        }
        (tools, parameters)
    }

    fn cli_subcommands() -> BTreeSet<String> {
        use clap::CommandFactory;
        crate::Cli::command()
            .get_subcommands()
            .map(|c| c.get_name().to_string())
            .collect()
    }

    /// Lowercase words, `snake_case` kept whole.
    fn words(text: &str) -> BTreeSet<String> {
        text.split(|c: char| !(c.is_ascii_lowercase() || c == '_'))
            .filter(|w| !w.is_empty() && !w.starts_with('_') && !w.ends_with('_'))
            .map(str::to_string)
            .collect()
    }

    /// Words shaped like `snake_case`: how most tools and parameters are spelled.
    fn snake_words(text: &str) -> BTreeSet<String> {
        words(text)
            .into_iter()
            .filter(|w| w.contains('_'))
            .collect()
    }

    fn body(skill: &Skill) -> &str {
        skill
            .text
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .map(|(_, body)| body)
            .unwrap()
    }

    #[test]
    fn there_are_twenty_two_skills_in_name_order() {
        assert_eq!(SKILLS.len(), 22);
        let names: Vec<&str> = SKILLS.iter().map(|s| s.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted, "names are unique and sorted");
        let table: BTreeSet<&str> = USES.iter().map(|(name, _)| *name).collect();
        assert_eq!(table, names.iter().copied().collect::<BTreeSet<_>>());
    }

    #[test]
    fn every_template_has_the_standard_frontmatter_and_stays_short() {
        for skill in SKILLS {
            assert!(skill.name.starts_with("mct-"), "{}", skill.name);
            let lines: Vec<&str> = skill.text.lines().collect();
            assert_eq!(lines[0], "---", "{}", skill.name);
            assert_eq!(lines[1], format!("name: {}", skill.name));
            let description = lines[2]
                .strip_prefix("description: \"")
                .and_then(|d| d.strip_suffix('"'))
                .unwrap_or_else(|| panic!("{}: description must be one quoted line", skill.name));
            assert!(
                description.len() > 30 && description.contains("Use "),
                "{}: say what it does and when to use it",
                skill.name
            );
            assert_eq!(
                lines[3], "---",
                "only name and description in the frontmatter"
            );
            let steps = lines.len() - 4;
            assert!((2..=10).contains(&steps), "{}: {steps} lines", skill.name);
            assert!(skill.text.is_ascii(), "{}: plain ASCII", skill.name);
        }
    }

    #[test]
    fn a_skill_names_only_tools_and_parameters_that_exist() {
        let (tools, parameters) = tool_vocabulary();
        for skill in SKILLS {
            for word in snake_words(body(skill)) {
                assert!(
                    tools.contains(&word) || parameters.contains(&word),
                    "{} names `{word}`, which is neither a tool nor a parameter",
                    skill.name
                );
            }
        }
    }

    #[test]
    fn the_word_after_call_is_a_tool() {
        let (tools, _) = tool_vocabulary();
        for skill in SKILLS {
            for line in body(skill).lines() {
                for part in line.split("Call ").skip(1) {
                    let word: String = part
                        .chars()
                        .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                        .collect();
                    assert!(
                        tools.contains(&word),
                        "{} says `Call {word}`, which is not a tool",
                        skill.name
                    );
                }
            }
        }
    }

    #[test]
    fn a_skill_calls_the_tools_it_is_for() {
        let (tools, _) = tool_vocabulary();
        for (name, expected) in USES {
            let skill = SKILLS.iter().find(|s| s.name == *name).unwrap();
            let used = words(body(skill));
            for tool in *expected {
                assert!(tools.contains(*tool), "{tool} is not a tool");
                assert!(used.contains(*tool), "{name} never calls {tool}");
            }
        }
    }

    #[test]
    fn a_skill_runs_only_subcommands_that_exist() {
        let subcommands = cli_subcommands();
        for skill in SKILLS {
            for span in body(skill).split('`').skip(1).step_by(2) {
                let Some(rest) = span.strip_prefix("mct-cli ") else {
                    continue;
                };
                let mut words = rest.split_whitespace();
                let mut word = words.next();
                // `--root <path>` comes before the subcommand.
                if word == Some("--root") {
                    words.next();
                    word = words.next();
                }
                let word = word.unwrap_or_default();
                assert!(
                    subcommands.contains(word),
                    "{} runs `mct-cli {word}`, which is not a subcommand",
                    skill.name
                );
            }
        }
    }

    #[test]
    fn a_skill_points_only_at_skills_that_exist() {
        let names: BTreeSet<&str> = SKILLS.iter().map(|s| s.name).collect();
        for skill in SKILLS {
            // A leading dot is a directory (`.mct-index`); a trailing one ends a sentence.
            let separators = |c: char| !(c.is_ascii_lowercase() || c == '-' || c == '.');
            for word in body(skill).split(separators) {
                let word = word.trim_end_matches('.');
                if word.starts_with("mct-") && !["mct-cli", "mct-mcp-server"].contains(&word) {
                    assert!(
                        names.contains(word),
                        "{} points at missing {word}",
                        skill.name
                    );
                }
            }
        }
    }

    /// The MCP prompts and the skills are the same workflows: each prompt has
    /// the skill of its name, with the same tool calls and the same wording.
    #[test]
    fn each_mcp_prompt_matches_its_skill() {
        let skill_steps = |name: &str| -> Vec<String> {
            let skill = SKILLS
                .iter()
                .find(|s| s.name == format!("mct-{name}"))
                .unwrap_or_else(|| panic!("prompt `{name}` has no skill `mct-{name}`"));
            let body = skill.text.split("---").nth(2).unwrap_or_default();
            body.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        };
        // Wording that legitimately differs: a skill points at a sibling skill,
        // a prompt at the sibling prompt, and `<arg>` is `{arg}`.
        let normalise = |line: &str| {
            line.replace("<arg>", "{arg}")
                .replace("mct-reindex", "the reindex prompt")
                .replace("mct-meaning", "the meaning prompt")
                .replace("mct-status", "status")
        };
        for w in mct_mcp_server::prompts::WORKFLOWS {
            let prompt: Vec<String> = w.steps.lines().map(|l| normalise(l.trim())).collect();
            let skill: Vec<String> = skill_steps(w.name).iter().map(|l| normalise(l)).collect();
            assert_eq!(prompt, skill, "prompt `{}` drifted from its skill", w.name);
        }
    }
}
