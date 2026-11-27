//! `find_files` (issue #629): the path catalog, rendered once for the MCP
//! tool and for `mct-cli files`, so both print the same rows in the same
//! order from the same index query ([`mct_index::Index::find_files`]).

use mct_index::{FilePage, FileQueryInput, Index, IndexError};
use rmcp::ErrorData as McpError;

use crate::toon::{encode_table, OutputFormat};

/// `find_files` arguments. Every filter is optional; they combine with AND.
#[derive(Debug, Default, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct FindFilesArgs {
    /// Words that must all occur in the repo-relative path or file name,
    /// case-insensitive (`logo`, `demo intro`).
    #[serde(default)]
    pub query: Option<String>,
    /// File or directory, repo-relative with `/`: the path itself and
    /// everything under it (`demo/`, `assets/fonts`).
    #[serde(default)]
    pub path: Option<String>,
    /// image, vector, video, audio, subtitle, document, spreadsheet,
    /// presentation, ebook, font, design, model3d, data, database,
    /// geospatial, archive, binary, email, source or other.
    #[serde(default)]
    pub category: Option<String>,
    /// File format id or extension alias, case-insensitive: `pdf`, `jpg`
    /// (= jpeg), `tgz` (= tar.gz), `xlsx`; a language id for source files.
    #[serde(default)]
    pub format: Option<String>,
    /// Page size, 1..200. Defaults to 50.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Rows to skip: pass the previous page's `next_offset`. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
    /// Response shape: `toon` (default) or `text`.
    #[serde(default)]
    pub output_format: Option<String>,
}

/// Runs the catalog query for `args` and renders the page.
pub fn run(index: &Index, args: &FindFilesArgs) -> Result<String, McpError> {
    let output = OutputFormat::parse(args.output_format.as_deref())
        .map_err(|msg| McpError::invalid_params(msg.replace("format", "output_format"), None))?;
    let input = FileQueryInput {
        query: args.query.as_deref(),
        path: args.path.as_deref(),
        category: args.category.as_deref(),
        format: args.format.as_deref(),
        limit: args.limit,
        offset: args.offset,
    };
    let page = index.find_files(&input).map_err(|err| match err {
        IndexError::InvalidFileQuery(_) => McpError::invalid_params(err.to_string(), None),
        other => McpError::internal_error(other.to_string(), None),
    })?;
    Ok(render(args, &page, output))
}

/// The filters the caller set, echoed so a page reads on its own.
fn filters(args: &FindFilesArgs) -> String {
    [
        ("query", &args.query),
        ("path", &args.path),
        ("category", &args.category),
        ("format", &args.format),
    ]
    .iter()
    .filter_map(|(name, value)| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| format!("{name}={v}"))
    })
    .collect::<Vec<_>>()
    .join(" ")
}

fn render(args: &FindFilesArgs, page: &FilePage, output: OutputFormat) -> String {
    let next = page
        .next_offset
        .map_or_else(|| "none".to_string(), |n| n.to_string());
    let mut out = String::new();
    if page.catalogued == 0 {
        out.push_str("The path catalog is empty: reindex the project first.\n");
    } else if page.total == 0 {
        out.push_str("No catalogued file matches.\n");
    }
    let filters = filters(args);
    let with_note = page.entries.iter().any(|e| e.note.is_some());
    let note = |e: &mct_index::CatalogEntry| {
        e.note
            .as_deref()
            .map(|n| format!("{}: {n}", e.evidence))
            .unwrap_or_default()
    };
    match output {
        OutputFormat::Toon => {
            if !filters.is_empty() {
                out.push_str(&format!("filters: {filters}\n"));
            }
            out.push_str(&format!(
                "total: {}\noffset: {}\nreturned: {}\nnext_offset: {next}\n",
                page.total,
                page.offset,
                page.entries.len()
            ));
            let mut columns = vec!["path", "category", "format", "bytes"];
            if with_note {
                columns.push("note");
            }
            let rows: Vec<Vec<String>> = page
                .entries
                .iter()
                .map(|e| {
                    let mut row = vec![
                        e.relative_path.clone(),
                        e.category.clone(),
                        e.format.clone(),
                        e.size_bytes.map(|n| n.to_string()).unwrap_or_default(),
                    ];
                    if with_note {
                        row.push(note(e));
                    }
                    row
                })
                .collect();
            if !rows.is_empty() {
                out.push_str(&encode_table("files", &columns, &rows));
            }
        }
        OutputFormat::Text => {
            out.push_str(&format!(
                "{} file(s) match{}; showing {} from offset {}, next_offset {next}\n",
                page.total,
                if filters.is_empty() {
                    String::new()
                } else {
                    format!(" ({filters})")
                },
                page.entries.len(),
                page.offset
            ));
            for e in &page.entries {
                let format = if e.format.is_empty() { "?" } else { &e.format };
                let size = e.size_bytes.map(|n| format!("  {n} B")).unwrap_or_default();
                let note = note(e);
                let note = if note.is_empty() {
                    note
                } else {
                    format!("  ({note})")
                };
                out.push_str(&format!(
                    "{}  {}/{format}{size}{note}\n",
                    e.relative_path, e.category
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use mct_index::CatalogEntry;

    use super::*;

    fn entry(path: &str, note: Option<&str>) -> CatalogEntry {
        CatalogEntry {
            relative_path: path.to_string(),
            category: "vector".into(),
            format: "svg".into(),
            evidence: if note.is_some() {
                "ambiguous"
            } else {
                "extension"
            }
            .into(),
            note: note.map(str::to_string),
            size_bytes: Some(62),
        }
    }

    fn page(entries: Vec<CatalogEntry>, total: usize, next: Option<usize>) -> FilePage {
        FilePage {
            entries,
            total,
            offset: 0,
            next_offset: next,
            catalogued: 10,
        }
    }

    #[test]
    fn a_toon_page_lists_rows_and_where_the_next_one_starts() {
        let args = FindFilesArgs {
            query: Some("logo".into()),
            ..Default::default()
        };
        let out = render(
            &args,
            &page(vec![entry("brand/logo.svg", None)], 2, Some(1)),
            OutputFormat::Toon,
        );
        assert_eq!(
            out,
            "filters: query=logo\ntotal: 2\noffset: 0\nreturned: 1\nnext_offset: 1\n\
             files[1]{path,category,format,bytes}:\n  brand/logo.svg,vector,svg,62\n"
        );
    }

    #[test]
    fn the_note_column_appears_only_for_a_doubtful_row() {
        let out = render(
            &FindFilesArgs::default(),
            &page(
                vec![entry("a.svg", None), entry("b.svg", Some("why"))],
                2,
                None,
            ),
            OutputFormat::Toon,
        );
        assert!(out.contains("{path,category,format,bytes,note}"), "{out}");
        assert!(out.contains("b.svg,vector,svg,62,ambiguous: why"), "{out}");
    }

    #[test]
    fn an_empty_catalog_and_an_empty_match_say_which_it_is() {
        let mut empty = page(vec![], 0, None);
        empty.catalogued = 0;
        let out = render(&FindFilesArgs::default(), &empty, OutputFormat::Text);
        assert!(out.starts_with("The path catalog is empty"), "{out}");
        let out = render(
            &FindFilesArgs::default(),
            &page(vec![], 0, None),
            OutputFormat::Toon,
        );
        assert!(out.starts_with("No catalogued file matches."), "{out}");
    }
}
