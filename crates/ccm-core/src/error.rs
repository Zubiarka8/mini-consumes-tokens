/// Failure to parse a single file. Kept separate from I/O or database errors
/// upstream, and split into two variants because they have distinct causes a
/// caller needs to distinguish: a language the indexer has no plugin for at
/// all, versus a file in a supported language that failed to parse (invalid
/// syntax, or a tree-sitter grammar edge case).
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("no LanguageParser registered for extension `.{extension}`")]
    UnsupportedExtension { extension: String },

    #[error("syntax error in {path} at line {line}: {message}")]
    Syntax {
        path: String,
        line: u32,
        message: String,
    },
}
