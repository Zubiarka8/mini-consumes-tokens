use std::collections::HashMap;
use std::sync::Arc;

use crate::{LanguageParser, ParseError, ParsedFile, SourceFile};

/// Central lookup from file extension to the [`LanguageParser`] that handles
/// it. This is the only place the indexer and MCP server go to resolve a
/// parser — neither ever matches on language names or extensions directly, so
/// registering a new plugin here is the entire integration surface.
#[derive(Default, Clone)]
pub struct LanguageRegistry {
    by_extension: HashMap<&'static str, Arc<dyn LanguageParser>>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a parser for all of its declared [`LanguageParser::file_extensions`].
    ///
    /// # Panics
    /// Panics if an extension is already claimed by another registered parser —
    /// this is a startup-time configuration error, not something that can occur
    /// from indexing arbitrary repo content.
    pub fn register(&mut self, parser: Arc<dyn LanguageParser>) {
        for ext in parser.file_extensions() {
            let previous = self.by_extension.insert(ext, parser.clone());
            assert!(
                previous.is_none(),
                "extension `.{ext}` already registered to language `{}`, cannot also register `{}`",
                previous.map(|p| p.language_id()).unwrap_or_default(),
                parser.language_id(),
            );
        }
    }

    pub fn for_extension(&self, extension: &str) -> Option<&Arc<dyn LanguageParser>> {
        self.by_extension.get(extension)
    }

    pub fn supported_extensions(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.by_extension.keys().copied()
    }

    pub fn language_ids(&self) -> Vec<&'static str> {
        let mut ids: Vec<_> = self
            .by_extension
            .values()
            .map(|p| p.language_id())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Resolves the parser for `file` by its extension and parses it.
    pub fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let extension = file
            .relative_path
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_string();
        match self.by_extension.get(extension.as_str()) {
            Some(parser) => parser.parse(file),
            None => Err(ParseError::UnsupportedExtension { extension }),
        }
    }
}
