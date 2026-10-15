//! Candidate collection, tokenization, normalization and grouping for
//! [`super::find_duplicate_groups`].
//!
//! Contract placeholder for TASK-207-B: returns an empty, complete report so
//! the CLI can be built against the public types while the analysis is
//! implemented on its own branch.

use mct_core::TokenSourceRegistry;

use super::{DuplicateOptions, DuplicateReport, GeneratedCodeHandling};
use crate::{Index, Result};

pub(super) fn run(
    _index: &Index,
    token_sources: &TokenSourceRegistry,
    options: DuplicateOptions,
) -> Result<DuplicateReport> {
    Ok(DuplicateReport {
        groups: Vec::new(),
        skipped: Vec::new(),
        complete: true,
        options,
        candidates_analyzed: 0,
        below_threshold: 0,
        supported_languages: token_sources
            .language_ids()
            .into_iter()
            .map(str::to_string)
            .collect(),
        unsupported_languages: Vec::new(),
        generated_code: GeneratedCodeHandling::ConfiguredExclusionsOnly,
    })
}
