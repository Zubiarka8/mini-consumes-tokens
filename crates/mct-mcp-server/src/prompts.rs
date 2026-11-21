//! MCP prompts: the `mct-*` skill workflows, offered to clients that list
//! prompts instead of reading skill files (issue #560).
//!
//! Each prompt names the same tool calls as the skill of the same name in
//! `crates/mct-cli/skills/`; a test in `mct-cli` keeps the two in step. The
//! prompts are plain data: asking for one runs nothing and touches no index.

use std::collections::HashSet;

use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, Prompt, PromptArgument, PromptMessage, Role,
};
use rmcp::ErrorData as McpError;

/// Shown first in every prompt: with Tool Attention on, `tools/list` hides
/// every tool but `select_tools` until it is called.
const ATTENTION_NOTE: &str = "If tools/list shows only select_tools, call select_tools with this \
request first; every tool named below stays callable by name either way.";

/// One argument a prompt takes.
pub struct Arg {
    pub name: &'static str,
    pub help: &'static str,
    pub required: bool,
}

/// One workflow.
pub struct Workflow {
    pub name: &'static str,
    pub description: &'static str,
    pub arg: Option<Arg>,
    /// The steps; `{arg}` is replaced by the argument's value, and a line
    /// holding `{arg?}` text only appears when the optional argument is given.
    pub steps: &'static str,
}

const fn required(name: &'static str, help: &'static str) -> Option<Arg> {
    Some(Arg {
        name,
        help,
        required: true,
    })
}

const fn optional(name: &'static str, help: &'static str) -> Option<Arg> {
    Some(Arg {
        name,
        help,
        required: false,
    })
}

/// The workflows, in the order `prompts/list` returns them.
pub const WORKFLOWS: &[Workflow] = &[
    Workflow {
        name: "overview",
        description: "Orientation in an unfamiliar crate, directory or file. Use first, before reading code.",
        arg: optional("path", "Crate, directory or file; omit for the whole project"),
        steps: "1. Call get_project_overview with path={arg} (omit path for the whole project).\n\
2. If the layout is still unclear, call get_file_tree with path={arg}.\n\
3. Report the modules, the key symbols and the top callers, then name the one file to look at next.\n\
Do not read whole files.",
    },
    Workflow {
        name: "tree",
        description: "Directory and file layout of a path, without symbols.",
        arg: optional("path", "Directory to show; omit for the project root"),
        steps: "1. Call get_file_tree with path={arg} (omit path for the project root). Raise depth only if the tree is too shallow.\n\
2. Report the layout and point at the directories that matter for the task.\n\
Do not read whole files.",
    },
    Workflow {
        name: "skeleton",
        description: "Shape of one file: top-level declarations with bodies collapsed.",
        arg: required("path", "File to outline"),
        steps: "1. Call get_file_skeleton with path={arg}. If you do not know the file, call list_symbols first.\n\
2. Report the declarations in order, with their signatures.\n\
Read the file itself only to edit it.",
    },
    Workflow {
        name: "symbols",
        description: "What a file, directory or crate defines. Use when you do not know a symbol's exact name.",
        arg: required("path", "File, directory or crate"),
        steps: "1. Call list_symbols with path={arg}. A file path matches exactly; a directory or crate path matches as a prefix. Narrow with kind or language.\n\
2. Report the symbols grouped by file and kind.\n\
Do not read whole files.",
    },
    Workflow {
        name: "find",
        description: "Go to the definition of a symbol by its exact name.",
        arg: required("name", "Exact symbol name"),
        steps: "1. Call find_symbol with name={arg}.\n\
2. Report each definition as path:line with its kind and signature. If several match, list all of them and use path or language to narrow.\n\
Do not read whole files.",
    },
    Workflow {
        name: "search",
        description: "Find symbols when the exact name is unknown: words, a partial identifier or a different casing.",
        arg: required("query", "Words or a partial identifier"),
        steps: "1. Call search_symbols_like with query={arg}. An exact name always ranks first.\n\
2. Report the hits with path:line and kind. If none fit, rephrase the words once, then try the meaning prompt.\n\
Do not read whole files.",
    },
    Workflow {
        name: "meaning",
        description: "Search by what the code does, not by its name.",
        arg: required("query", "What the code does, in words"),
        steps: "1. Call hybrid_search_like with query={arg}. Put the phrase in double quotes for an exact phrase match.\n\
2. Report the hits with path:line and a one-line reason each. If the first line says the semantic half is off, say that the ranking is lexical only.\n\
Do not read whole files.",
    },
    Workflow {
        name: "callers",
        description: "Who calls a function directly.",
        arg: required("function", "Function or method name"),
        steps: "1. Call find_callers with function={arg}. Use depth=2 only if the direct callers are not enough.\n\
2. Report callers as path:line, grouped by file. Say so when the name is ambiguous instead of picking one.\n\
Do not read whole files.",
    },
    Workflow {
        name: "calls",
        description: "What a function calls.",
        arg: required("function", "Function or method name"),
        steps: "1. Call find_calls with function={arg}.\n\
2. Report the callees as path:line. List unresolved or ambiguous callees separately and do not guess them.\n\
Do not read whole files.",
    },
    Workflow {
        name: "refs",
        description: "Every use of a symbol: calls, imports, extends, implements and overrides.",
        arg: required("symbol", "Symbol name"),
        steps: "1. Call find_references with symbol={arg}. Page with offset if the list is cut.\n\
2. Report the references grouped by kind and file.\n\
Do not read whole files.",
    },
    Workflow {
        name: "impact",
        description: "Blast radius of changing a symbol: callers, references and covering tests.",
        arg: required("symbol", "Symbol you plan to change or remove"),
        steps: "1. Call impact_analysis with symbol={arg}, depth=2.\n\
2. Call build_context_pack on the same symbol.\n\
3. Report callers by crate, the tests that cover it, and any ambiguous caller without guessing.\n\
Do not read whole files.",
    },
    Workflow {
        name: "context",
        description: "Everything needed before editing a symbol: definition, callers, callees, dependencies and tests.",
        arg: required("symbol", "Symbol you plan to edit"),
        steps: "1. Call build_context_pack with symbol={arg}. Use path or language if the name is ambiguous, and source_lines to see more of the body.\n\
2. Report the definition, then the callers, callees and tests it lists.\n\
Read the file only to edit it.",
    },
    Workflow {
        name: "deps",
        description: "Import graph of a file or directory: what it imports, who imports it and import cycles.",
        arg: required("module", "File or directory"),
        steps: "1. Call get_dependency_graph with module={arg}. Raise depth for more hops, or pass export=mermaid for a diagram.\n\
2. Report the imports, the importers and any cycle. List unresolved and third-party imports separately.\n\
Do not read whole files.",
    },
    Workflow {
        name: "dead-code",
        description: "Candidate dead code in a path, each candidate checked. Never delete from it unasked.",
        arg: optional("path", "Directory or file; omit for the whole project"),
        steps: "1. Call find_dead_code with path={arg}.\n\
2. For each hit, call find_references, and impact_analysis if it looks used, before calling it dead.\n\
3. Report confirmed candidates and the hits you ruled out with the reason. Zero indexed references is a heuristic, not proof.\n\
Do not delete anything unasked.",
    },
    Workflow {
        name: "duplicates",
        description: "Duplicate code in a path, telling true duplicates from look-alikes.",
        arg: optional("path", "Directory or file; omit for the whole project"),
        steps: "1. Call find_duplicate_code with path={arg}.\n\
2. For each group, call build_context_pack on its members to compare what they do.\n\
3. Report true duplicates apart from code that only looks alike, and which copy to keep.\n\
Do not edit unasked.",
    },
    Workflow {
        name: "status",
        description: "Health of the index: parse failures, read failures, dependencies and staleness.",
        arg: None,
        steps: "1. Call get_indexing_status.\n\
2. Report the file and symbol counts, any syntax or read failure with its path, and whether the index looks stale.\n\
3. Suggest the reindex prompt only if it looks stale.",
    },
    Workflow {
        name: "reindex",
        description: "Refresh the index now. Use only when status shows it stale or after large edits.",
        arg: None,
        steps: "1. Call reindex. Pass force=true only if unchanged files must be parsed again, for example after upgrading mct-cli.\n\
2. Report the files parsed, removed and unchanged, and every issue it lists.",
    },
];

/// `prompts/list`.
pub fn list() -> Vec<Prompt> {
    WORKFLOWS
        .iter()
        .map(|w| {
            let arguments = w.arg.as_ref().map(|a| {
                vec![PromptArgument::new(a.name)
                    .with_description(a.help)
                    .with_required(a.required)]
            });
            Prompt::new(w.name, Some(w.description), arguments)
        })
        .collect()
}

/// The prompt text for `w` with its argument filled in.
fn render(w: &Workflow, value: Option<&str>) -> String {
    let steps = match value {
        Some(value) => w.steps.replace("{arg}", value),
        // An omitted optional argument: drop the parameter, keep the call.
        None => w
            .steps
            .replace(" with path={arg}", "")
            .replace(" (omit path for the whole project)", "")
            .replace(" (omit path for the project root)", "")
            .replace("path={arg}", "the whole project"),
    };
    format!("{ATTENTION_NOTE}\n\n{steps}")
}

/// `prompts/get`: an unknown name, a missing required argument or an
/// argument the prompt does not take is `invalid_params`.
pub fn get(request: &GetPromptRequestParams) -> Result<GetPromptResult, McpError> {
    let Some(workflow) = WORKFLOWS.iter().find(|w| w.name == request.name) else {
        return Err(McpError::invalid_params(
            format!(
                "unknown prompt `{}`; available: {}",
                request.name,
                WORKFLOWS
                    .iter()
                    .map(|w| w.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None,
        ));
    };
    let given = request.arguments.as_ref();
    let accepted: HashSet<&str> = workflow.arg.iter().map(|a| a.name).collect();
    if let Some(given) = given {
        if let Some(extra) = given.keys().find(|k| !accepted.contains(k.as_str())) {
            return Err(McpError::invalid_params(
                format!("prompt `{}` has no argument `{extra}`", workflow.name),
                None,
            ));
        }
    }
    let value = match (&workflow.arg, given) {
        (Some(arg), given) => {
            let value = given.and_then(|g| g.get(arg.name));
            match value {
                Some(serde_json::Value::String(text)) if !text.trim().is_empty() => {
                    Some(text.trim())
                }
                Some(_) | None if arg.required => {
                    return Err(McpError::invalid_params(
                        format!(
                            "prompt `{}` needs the argument `{}` ({})",
                            workflow.name, arg.name, arg.help
                        ),
                        None,
                    ));
                }
                _ => None,
            }
        }
        (None, _) => None,
    };
    Ok(GetPromptResult::new(vec![PromptMessage::new_text(
        Role::User,
        render(workflow, value),
    )])
    .with_description(workflow.description))
}

#[cfg(test)]
mod tests {
    // Test code: an unwrap()/expect() here means a broken test precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use serde_json::json;

    fn request(name: &str, arguments: Option<serde_json::Value>) -> GetPromptRequestParams {
        let mut request = GetPromptRequestParams::default();
        request.name = name.to_string();
        request.arguments = arguments.and_then(|v| v.as_object().cloned());
        request
    }

    fn text(result: &GetPromptResult) -> String {
        serde_json::to_value(&result.messages[0].content).unwrap()["text"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn every_tool_a_prompt_calls_exists() {
        for w in WORKFLOWS {
            let calls: Vec<&str> = w.steps.split("Call ").skip(1).collect();
            assert!(!calls.is_empty(), "{} calls no tool", w.name);
            for part in calls {
                let tool: String = part
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                assert!(
                    crate::ttc::KNOWN_TOOL_NAMES.contains(&tool.as_str()),
                    "{}: `{tool}` is not a tool",
                    w.name
                );
            }
        }
    }

    #[test]
    fn the_list_has_every_workflow_with_its_argument() {
        let prompts = list();
        assert_eq!(prompts.len(), WORKFLOWS.len());
        let impact = prompts.iter().find(|p| p.name == "impact").unwrap();
        let args = impact.arguments.as_ref().unwrap();
        assert_eq!(args[0].name, "symbol");
        assert_eq!(args[0].required, Some(true));
        let status = prompts.iter().find(|p| p.name == "status").unwrap();
        assert!(status.arguments.is_none());
        let names: HashSet<_> = prompts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names.len(), prompts.len(), "names are unique");
    }

    #[test]
    fn a_prompt_fills_in_its_argument_and_mentions_tool_attention() {
        let result = get(&request("impact", Some(json!({"symbol": "  parse_file "})))).unwrap();
        let body = text(&result);
        assert!(
            body.starts_with("If tools/list shows only select_tools"),
            "{body}"
        );
        assert!(
            body.contains("impact_analysis with symbol=parse_file, depth=2"),
            "{body}"
        );
        assert!(!body.contains("{arg}"));
    }

    #[test]
    fn a_missing_required_argument_is_invalid_params() {
        for arguments in [
            None,
            Some(json!({})),
            Some(json!({"symbol": ""})),
            Some(json!({"symbol": 3})),
        ] {
            let error = get(&request("impact", arguments)).unwrap_err();
            assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
            assert!(error.message.contains("symbol"), "{}", error.message);
        }
        let error = get(&request("nope", None)).unwrap_err();
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        let error = get(&request("status", Some(json!({"x": "1"})))).unwrap_err();
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }

    #[test]
    fn an_optional_argument_can_be_left_out() {
        let body = text(&get(&request("overview", None)).unwrap());
        assert!(body.contains("Call get_project_overview"), "{body}");
        assert!(!body.contains("{arg}") && !body.contains("path="), "{body}");
        let body = text(&get(&request("dead-code", Some(json!({"path": "crates/x"})))).unwrap());
        assert!(body.contains("find_dead_code with path=crates/x"), "{body}");
    }
}
