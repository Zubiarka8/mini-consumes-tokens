//! Astro components (`.astro`): a frontmatter of TypeScript between two
//! `---` fence lines, then an HTML-like template.
//!
//! The frontmatter is parsed by the TypeScript grammar over the file with
//! every other byte blanked ([`frontmatter_only`]), so its symbols, imports
//! and calls keep the file's own lines and columns. The template is not
//! parsed: [`template_components`] scans it for component tags, a name
//! starting with an uppercase letter (`<Card>`) or a member name
//! (`<UI.Card>`), which the parser records as `Renders` relations from the
//! file's module symbol. Lowercase tags are HTML elements. `<script>` and
//! `<style>` bodies and HTML comments are skipped; a tag inside a `{…}`
//! expression is a render too (`{items.map((i) => <Item />)}`).

use mct_core::Location;

/// The byte range of the frontmatter's body: from the line after an
/// opening `---` that is the file's first non-blank line, to the start of
/// the next `---` line. `None` when the file has no closed frontmatter.
fn frontmatter(contents: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    let mut body_start = None;
    for line in contents.split_inclusive('\n') {
        let fence = line.trim_end() == "---";
        match body_start {
            None if fence => body_start = Some(offset + line.len()),
            None if !line.trim().is_empty() => return None,
            Some(start) if fence => return Some((start, offset)),
            _ => {}
        }
        offset += line.len();
    }
    None
}

/// `contents` with everything but the frontmatter body blanked, keeping
/// every line break: the TypeScript a `.astro` file declares.
pub(crate) fn frontmatter_only(contents: &str) -> String {
    let keep: Vec<_> = frontmatter(contents)
        .map(|(start, end)| start..end)
        .into_iter()
        .collect();
    mct_tree_sitter::mask(contents, &keep)
}

/// The whole file, where an Astro file's module symbol is: the file is
/// the component, so a render of it resolves to line 1, not to the
/// frontmatter's first statement.
pub(crate) fn file_location(contents: &str) -> Location {
    let lines = contents.lines().count() as u32;
    Location {
        line: 1,
        column: 1,
        byte_len: contents.len() as u32,
        end_line: (lines > 1).then_some(lines),
    }
}

/// Each component tag of the template, in order: its name and where it is.
pub(crate) fn template_components(contents: &str) -> Vec<(String, Location)> {
    let start = frontmatter(contents).map_or(0, |(_, end)| {
        // Past the closing fence line.
        contents[end..]
            .find('\n')
            .map_or(contents.len(), |n| end + n + 1)
    });
    let bytes = contents.as_bytes();
    let mut found = Vec::new();
    let mut i = start;
    while let Some(next) = contents[i..].find('<') {
        let at = i + next;
        let rest = &contents[at..];
        if rest.starts_with("<!--") {
            i = rest.find("-->").map_or(contents.len(), |n| at + n + 3);
            continue;
        }
        let name_len = rest[1..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '$')))
            .unwrap_or(rest.len() - 1);
        let name = &rest[1..1 + name_len];
        let raw_text = ["script", "style"]
            .into_iter()
            .find(|tag| name.eq_ignore_ascii_case(tag));
        if let Some(tag) = raw_text {
            let close = format!("</{tag}");
            i = contents[at..]
                .find(&close)
                .map_or(contents.len(), |n| at + n + close.len());
            continue;
        }
        let is_component = name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            || (name.contains('.') && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()));
        if is_component && !name.ends_with('.') {
            let line_start = contents[..at].rfind('\n').map_or(0, |n| n + 1);
            let line = bytes[..at].iter().filter(|&&b| b == b'\n').count() as u32 + 1;
            found.push((
                name.to_string(),
                Location {
                    line,
                    column: (at + 1 - line_start) as u32 + 1,
                    byte_len: name.len() as u32,
                    end_line: None,
                },
            ));
        }
        i = at + 1;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "---\nimport Card from './Card.astro';\nconst title = 'Hi';\n---\n\
<!-- <Hidden /> -->\n<Layout title={title}>\n  <h1>{title}</h1>\n  <UI.Button />\n\
  {items.map((i) => <Card item={i} />)}\n</Layout>\n<script>const x = <Nope />;</script>\n";

    #[test]
    fn the_frontmatter_keeps_its_lines_and_the_rest_is_blank() {
        let masked = frontmatter_only(PAGE);
        assert_eq!(masked.len(), PAGE.len());
        let lines: Vec<&str> = masked.lines().collect();
        assert_eq!(lines[0].trim(), "");
        assert_eq!(lines[1], "import Card from './Card.astro';");
        assert_eq!(lines[3].trim(), "");
        assert!(!masked.contains("Layout"));
        // No frontmatter: nothing is TypeScript.
        assert_eq!(frontmatter_only("<p>hi</p>\n").trim(), "");
        // An unclosed fence is not a frontmatter.
        assert_eq!(frontmatter_only("---\nconst a = 1;\n").trim(), "");
    }

    #[test]
    fn component_tags_are_found_with_their_place() {
        let tags: Vec<(String, u32, u32)> = template_components(PAGE)
            .into_iter()
            .map(|(name, loc)| (name, loc.line, loc.column))
            .collect();
        assert_eq!(
            tags,
            [
                ("Layout".to_string(), 6, 2),
                ("UI.Button".to_string(), 8, 4),
                // The `\` continuation drops the line's indent.
                ("Card".to_string(), 9, 20),
            ]
        );
    }
}
