//! Failed tool calls, recorded in the project's local incident log
//! (`<root>/.mct-index/errors/`, see [`mct_index::log_incident`]).
//!
//! Every function is best effort and returns nothing: the caller keeps the
//! error or the refusal it already has and sends it to the client unchanged.
//! Only the tool name and the diagnostic are recorded, never the call's
//! arguments.

use std::path::Path;

use mct_index::{log_incident, Incident};
use rmcp::{
    model::{CallToolResult, ErrorCode},
    ErrorData as McpError,
};

use crate::content::result_text;

/// Longest diagnostic kept, so a message that echoes input stays small.
const MAX_DETAIL: usize = 400;

/// `invalid-params` for a caller's mistake, `internal-error` for a server fault.
pub(crate) fn error_kind(err: &McpError) -> &'static str {
    if err.code == ErrorCode::INVALID_PARAMS {
        "invalid-params"
    } else {
        "internal-error"
    }
}

/// Records a tool call that ended in `err`.
pub(crate) fn record_error(root: &Path, operation: &str, tool: &str, err: &McpError) {
    record(root, operation, error_kind(err), tool, &err.message);
}

/// The kind of an `is_error` result a tool call returned: rmcp refuses
/// arguments it cannot deserialize with a result, not a protocol error.
pub(crate) fn result_error_kind(result: &CallToolResult) -> &'static str {
    if result_text(result).starts_with("failed to deserialize parameters") {
        "invalid-params"
    } else {
        "tool-error"
    }
}

/// Records a call answered with an `is_error` result.
pub(crate) fn record_refusal(
    root: &Path,
    operation: &str,
    tool: &str,
    kind: &str,
    result: &CallToolResult,
) {
    record(root, operation, kind, tool, &result_text(result));
}

/// Records a failure outside any tool call: the startup index, the watcher's
/// reindex or the maintainer's catch-up. `detail` is a diagnostic the caller
/// already logs; it carries no arguments.
pub fn record_failure(root: &Path, operation: &str, kind: &str, detail: &str) {
    let detail = truncate(detail, MAX_DETAIL);
    let _ = log_incident(
        root,
        &Incident {
            operation,
            kind,
            path: None,
            line: None,
            detail: &detail,
        },
    );
}

/// Records a failed `mct-cli` command. Quoted values in `message` are
/// replaced, as for a tool call, because the message may echo an argument.
pub fn record_cli_error(root: &Path, operation: &str, message: &str) {
    record_failure(root, operation, "cli-error", &scrub_values(message));
}

fn record(root: &Path, operation: &str, kind: &str, tool: &str, message: &str) {
    let detail = format!("{tool}: {}", scrub_values(message));
    let detail = truncate(&detail, MAX_DETAIL);
    let _ = log_incident(
        root,
        &Incident {
            operation,
            kind,
            path: None,
            line: None,
            detail: &detail,
        },
    );
}

/// `serde` reports a bad argument as ``invalid type: string "abc", expected …``
/// and ``unknown variant `abc` ``; the value the caller sent is not needed to
/// understand the failure, so it is replaced.
fn scrub_values(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(start) = rest.find(['"', '`']) {
        let quote = rest[start..].chars().next().unwrap_or('"');
        let after = start + quote.len_utf8();
        let Some(len) = rest[after..].find(quote) else {
            break;
        };
        out.push_str(&rest[..after]);
        out.push_str("<value>");
        out.push(quote);
        rest = &rest[after + len + quote.len_utf8()..];
    }
    out.push_str(rest);
    out
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn quoted_values_are_replaced() {
        assert_eq!(
            scrub_values(r#"invalid type: string "abc", expected u32"#),
            r#"invalid type: string "<value>", expected u32"#
        );
        assert_eq!(
            scrub_values("unknown field `limitt`, expected one of `limit`"),
            "unknown field `<value>`, expected one of `<value>`"
        );
        assert_eq!(scrub_values("plain message"), "plain message");
        assert_eq!(scrub_values("dangling \" quote"), "dangling \" quote");
    }

    #[test]
    fn kinds_follow_the_error_code() {
        assert_eq!(
            error_kind(&McpError::invalid_params("bad", None)),
            "invalid-params"
        );
        assert_eq!(
            error_kind(&McpError::internal_error("oops", None)),
            "internal-error"
        );
    }
}
