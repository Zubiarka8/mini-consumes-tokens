//! Read-only duplicate discovery over MCP. The detector and the review domain
//! stay in `mct-index`; this module owns the input bounds and the compact,
//! budgeted response.
//!
//! Review decisions come from the shared TOON store (`mct_index::reviews`),
//! the same one the dashboard and the CLI read, so a group means the same
//! thing in all three. Only a decision that still matches the group's exact
//! code hides it; see ADR-022.
mod group;
mod presentation;

use mct_index::reviews::FindingKeys;

use mct_index::reviews::{duplicate_findings, Resolution, Reviews, Visibility};
use mct_index::{DuplicateGroup, DuplicateOptions, Index};
use rmcp::ErrorData as McpError;

const DEFAULT_LIMIT: usize = 10;
const DEFAULT_MAX_TOKENS: usize = 4000;

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindDuplicateCodeArgs {
    /// Root-relative file or directory.
    pub path: Option<String>,
    /// Indexed language id; default all.
    pub language: Option<String>,
    /// broad (default), consistent or exact.
    pub normalization: Option<String>,
    /// Minimum token-bearing lines (default 6).
    pub min_lines: Option<usize>,
    /// Minimum tokens (default 40).
    pub min_tokens: Option<usize>,
    /// Groups per page, 1..100 (default 10).
    pub limit: Option<usize>,
    /// Groups to skip after review filtering.
    pub offset: Option<usize>,
    /// Also list groups hidden by a review decision (default false).
    pub include_reviewed: Option<bool>,
    /// Estimated response budget, 512..16000 (default 4000).
    pub max_tokens: Option<usize>,
    /// Focus one eligible group by id to continue an oversized member list.
    pub group_id: Option<String>,
    /// Members to skip within group_id (default 0); independent of group offset.
    pub member_offset: Option<usize>,
    /// Snapshot from the previous response; reject changed inputs or results.
    pub expected_snapshot: Option<String>,
}

fn invalid(message: &str) -> McpError {
    McpError::invalid_params(message.to_string(), None)
}

impl FindDuplicateCodeArgs {
    pub(crate) fn validate(&self) -> Result<(), McpError> {
        for token in [&self.group_id, &self.expected_snapshot]
            .into_iter()
            .flatten()
        {
            if token.len() != 64
                || !token
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid(
                    "group_id and expected_snapshot must be 64 lowercase hex characters",
                ));
            }
        }
        if self.member_offset.is_some() && self.group_id.is_none() {
            return Err(invalid("member_offset requires group_id"));
        }
        if self.group_id.is_some() && self.offset.unwrap_or(0) != 0 {
            return Err(invalid(
                "group_id cannot be combined with a nonzero group offset",
            ));
        }
        if !(1..=100).contains(&self.limit.unwrap_or(DEFAULT_LIMIT)) {
            return Err(invalid("limit must be in 1..100"));
        }
        if !(512..=16000).contains(&self.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS)) {
            return Err(invalid("max_tokens must be in 512..16000 (estimated)"));
        }
        if self.min_lines == Some(0) || self.min_tokens == Some(0) {
            return Err(invalid("min_lines and min_tokens must be positive"));
        }
        if let Some(path) = &self.path {
            // Reject both platform separators consistently on every host.
            if path.len() > 4096
                || path.contains(['\\', ':', '\0'])
                || path.starts_with('/')
                || path.split('/').any(|part| part == "..")
                || path.chars().any(char::is_control)
            {
                return Err(invalid(
                    "path must be a root-relative file or directory without traversal",
                ));
            }
        }
        if self.language.as_ref().is_some_and(|language| {
            language.is_empty()
                || language.len() > 64
                || !language
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        }) {
            return Err(invalid(
                "language must be an indexed language id (1..64 ASCII characters)",
            ));
        }
        if self
            .normalization
            .as_deref()
            .is_some_and(|n| !matches!(n, "broad" | "consistent" | "exact"))
        {
            return Err(invalid("normalization must be broad, consistent, or exact"));
        }
        Ok(())
    }
}

/// One group with its review outcome.
struct Row<'a> {
    group: &'a DuplicateGroup,
    finding: &'a FindingKeys,
    resolution: &'a Resolution,
}

impl Row<'_> {
    fn actionable(&self) -> bool {
        self.resolution.visibility() == Visibility::Actionable
    }
}

/// The caller holds the server's index mutex and read snapshot.
pub(crate) fn run(index: &Index, args: FindDuplicateCodeArgs) -> Result<String, McpError> {
    args.validate()?;
    let options = DuplicateOptions {
        path: args
            .path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(|path| path.trim_end_matches('/').to_string()),
        language: args.language.clone(),
        min_lines: args.min_lines.unwrap_or(mct_index::DEFAULT_MIN_LINES),
        min_tokens: args.min_tokens.unwrap_or(mct_index::DEFAULT_MIN_TOKENS),
        normalization: args
            .normalization
            .as_deref()
            .unwrap_or("broad")
            .parse()
            .map_err(|_| invalid("invalid normalization"))?,
        excerpts: false,
        ..DuplicateOptions::default()
    };
    // Review filtering happens before pagination, so the detector reports
    // every group and `limit`/`offset` apply to what is left.
    let report =
        mct_index::find_duplicate_groups(index, &mct_languages::build_token_sources(), options)
            .map_err(|_| McpError::internal_error("cannot analyze duplicate candidates", None))?;
    let file_hashes = index
        .file_hashes()
        .map_err(|_| McpError::internal_error("cannot read indexed file hashes", None))?;
    let findings = duplicate_findings(&report.groups, report.options.normalization, &file_hashes);
    // Never fails: an unreadable review file leaves every group actionable.
    let reviews = Reviews::load(index.root());
    let resolutions = reviews.resolve(&findings);
    let rows: Vec<Row> = report
        .groups
        .iter()
        .zip(&findings)
        .zip(&resolutions)
        .map(|((group, finding), resolution): (_, &Resolution)| Row {
            group,
            finding,
            resolution,
        })
        .collect();
    let generation = index
        .published_generation()
        .map_err(|_| McpError::internal_error("cannot read index generation", None))?;
    presentation::render(
        index.root(),
        generation,
        &report,
        &rows,
        reviews.is_degraded(),
        &args,
    )
}
