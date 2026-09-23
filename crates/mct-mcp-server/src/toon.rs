//! A minimal TOON (Token-Oriented Object Notation) encoder/decoder for this
//! server's tool responses.
//!
//! The dominant shape across these tools' output is an array of uniform
//! records (symbol hits, references, calls, dead-code candidates — always
//! the same columns, row after row). This server's existing plain-text
//! format (see `crate::format`) already avoids JSON's per-row `{"key":
//! "value", ...}` repetition, but it still spends bytes on labels and
//! decorative punctuation per line. TOON goes one step further for that
//! specific shape: the field names are written once, in a header, and every
//! row after that is just its values — closer to CSV than to JSON, but
//! self-describing (row count + column names up front) the way JSON is and
//! CSV isn't.
//!
//! This module only handles the tabular case. Non-uniform/nested data (a
//! single record, or records that don't share one flat column set) has no
//! natural tabular encoding, so [`format`](crate::format)'s existing text
//! renderers are used for those unchanged — see the doc comment on
//! `OutputFormat` for which tools this applies to.
//!
//! Never used on adversarial input directly (this module encodes/decodes
//! already-indexed symbol data, not raw repo source), but still holds to the
//! workspace-wide no-`unwrap`/`panic` rule on any path processing
//! repo-derived strings (symbol names, paths) since those flow in from
//! parsed source files.

/// Which shape a tool renders its response in. `Text` (the default, and the
/// only option before this module existed) keeps today's response
/// byte-for-byte — no existing MCP consumer breaks by upgrading this server.
/// `Toon` is opt-in per call via each tool's `format` argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Toon,
}

impl OutputFormat {
    /// Parses a tool's optional `format` argument. `None` or an empty/blank
    /// string is `Text` (today's default, unchanged). An unrecognized value
    /// is rejected rather than silently falling back, matching how
    /// `find_symbol`'s `match` argument is validated in `server.rs`.
    pub fn parse(raw: Option<&str>) -> Result<Self, String> {
        match raw.map(str::trim) {
            None => Ok(Self::Text),
            Some("") => Ok(Self::Text),
            Some("text") => Ok(Self::Text),
            Some("toon") => Ok(Self::Toon),
            Some(other) => Err(format!("format must be one of text, toon — got `{other}`")),
        }
    }
}

/// Encodes a uniform array of records as one TOON tabular block:
///
/// ```text
/// name[2]{col_a,col_b}:
///   value_a1,value_b1
///   value_a2,value_b2
/// ```
///
/// `name` is a label for the block (e.g. `symbols`, `references`), `headers`
/// are the column names in the order every row's values appear, and `rows`
/// are the already-stringified cell values — one inner `Vec` per record,
/// same length and order as `headers`. A row shorter than `headers` renders
/// its missing trailing cells as empty fields rather than panicking — this
/// runs over data the caller assembled, and a mismatched row is a caller bug
/// that should show up as an odd-looking empty field, not a crashed tool
/// response.
pub fn encode_table(name: &str, headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = format!("{name}[{}]{{{}}}:\n", rows.len(), headers.join(","));
    for row in rows {
        out.push_str("  ");
        let cells: Vec<String> = (0..headers.len())
            .map(|i| escape_field(row.get(i).map(String::as_str).unwrap_or("")))
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

/// Quotes a field when it contains the comma delimiter, a quote, a
/// backslash, or a line break, backslash-escaping the special characters
/// inside the quotes. Backslash escaping (not CSV's double-quote-doubling)
/// is what lets a literal newline inside a value survive without breaking
/// this format's one-row-per-line invariant — a naive embedded `\n` would
/// otherwise split what looks like one row into two when read back line by
/// line.
fn escape_field(value: &str) -> String {
    let needs_quotes =
        value.contains(',') || value.contains('"') || value.contains('\\') || value.contains(['\n', '\r']);
    if !needs_quotes {
        return value.to_string();
    }
    let mut escaped = String::with_capacity(value.len() + 2);
    for c in value.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            other => escaped.push(other),
        }
    }
    format!("\"{escaped}\"")
}

/// A decoded TOON tabular block, as produced by [`encode_table`]. Exists for
/// round-trip testing the encoder — this server's tools only ever encode,
/// never decode, a real response — so it's kept intentionally simple rather
/// than a general-purpose TOON parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedTable {
    pub name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Parses a block produced by [`encode_table`] back into its name, headers
/// and rows. Returns `None` on anything that doesn't match that exact
/// shape — this is a round-trip check for the encoder, not a lenient parser
/// for hand-written TOON.
pub fn decode_table(input: &str) -> Option<DecodedTable> {
    let mut lines = input.lines();
    let header_line = lines.next()?;

    let bracket_start = header_line.find('[')?;
    let bracket_end = header_line.find(']')?;
    if bracket_end < bracket_start {
        return None;
    }
    let name = header_line[..bracket_start].to_string();
    let _count: usize = header_line[bracket_start + 1..bracket_end].parse().ok()?;

    let brace_start = header_line.find('{')?;
    let brace_end = header_line.find('}')?;
    if brace_end < brace_start {
        return None;
    }
    let header_body = &header_line[brace_start + 1..brace_end];
    let headers: Vec<String> = if header_body.is_empty() {
        Vec::new()
    } else {
        header_body.split(',').map(str::to_string).collect()
    };

    let mut rows = Vec::new();
    for line in lines {
        let trimmed = line.strip_prefix("  ").unwrap_or(line);
        if trimmed.is_empty() {
            continue;
        }
        rows.push(parse_csv_line(trimmed));
    }

    Some(DecodedTable { name, headers, rows })
}

/// Splits one row line on unquoted commas, undoing [`escape_field`]'s
/// backslash escaping (`\\`, `\"`, `\n`, `\r`) inside quoted fields.
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '\\' => match chars.next() {
                    Some('n') => current.push('\n'),
                    Some('r') => current.push('\r'),
                    Some('"') => current.push('"'),
                    Some('\\') => current.push('\\'),
                    // An unrecognized escape: keep both characters as-is
                    // rather than dropping the backslash silently.
                    Some(other) => {
                        current.push('\\');
                        current.push(other);
                    }
                    None => current.push('\\'),
                },
                '"' => in_quotes = false,
                _ => current.push(c),
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => fields.push(std::mem::take(&mut current)),
                _ => current.push(c),
            }
        }
    }
    fields.push(current);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_format_parses_the_documented_values() {
        assert_eq!(OutputFormat::parse(None), Ok(OutputFormat::Text));
        assert_eq!(OutputFormat::parse(Some("")), Ok(OutputFormat::Text));
        assert_eq!(OutputFormat::parse(Some("  ")), Ok(OutputFormat::Text));
        assert_eq!(OutputFormat::parse(Some("text")), Ok(OutputFormat::Text));
        assert_eq!(OutputFormat::parse(Some("toon")), Ok(OutputFormat::Toon));
        assert!(OutputFormat::parse(Some("xml")).is_err());
    }

    #[test]
    fn round_trips_a_simple_table() {
        let headers = ["name", "kind", "line"];
        let rows = vec![
            vec!["compute".to_string(), "function".to_string(), "10".to_string()],
            vec!["helper".to_string(), "function".to_string(), "20".to_string()],
        ];
        let encoded = encode_table("symbols", &headers, &rows);
        assert_eq!(
            encoded,
            "symbols[2]{name,kind,line}:\n  compute,function,10\n  helper,function,20\n"
        );
        let decoded = decode_table(&encoded).expect("must decode what we just encoded");
        assert_eq!(decoded.name, "symbols");
        assert_eq!(decoded.headers, vec!["name", "kind", "line"]);
        assert_eq!(decoded.rows, rows);
    }

    #[test]
    fn empty_array_still_renders_a_valid_header_with_zero_rows() {
        let encoded = encode_table("symbols", &["name", "kind"], &[]);
        assert_eq!(encoded, "symbols[0]{name,kind}:\n");
        let decoded = decode_table(&encoded).expect("must decode an empty table");
        assert_eq!(decoded.rows, Vec::<Vec<String>>::new());
        assert_eq!(decoded.headers, vec!["name", "kind"]);
    }

    #[test]
    fn fields_with_commas_quotes_and_newlines_round_trip() {
        let headers = ["name", "detail"];
        let rows = vec![vec![
            "weird, name".to_string(),
            "has \"quotes\" and\na newline".to_string(),
        ]];
        let encoded = encode_table("symbols", &headers, &rows);
        assert!(encoded.contains("\"weird, name\""), "got: {encoded}");
        assert!(encoded.contains("\\\"quotes\\\""), "got: {encoded}");
        // The embedded literal newline must not survive as a raw `\n` byte —
        // that would split one logical row across two lines.
        assert_eq!(encoded.lines().count(), 2, "must still be one line per row: {encoded:?}");
        let decoded = decode_table(&encoded).expect("must decode escaped fields");
        assert_eq!(decoded.rows, rows);
    }

    #[test]
    fn a_short_row_pads_missing_trailing_cells_as_empty_rather_than_panicking() {
        let headers = ["name", "kind", "parent"];
        let rows = vec![vec!["compute".to_string(), "function".to_string()]];
        let encoded = encode_table("symbols", &headers, &rows);
        assert_eq!(encoded, "symbols[1]{name,kind,parent}:\n  compute,function,\n");
    }

    #[test]
    fn decode_rejects_input_that_does_not_look_like_an_encoded_table() {
        assert!(decode_table("not a table at all").is_none());
        assert!(decode_table("name(no brackets){a,b}:\n").is_none());
    }
}
