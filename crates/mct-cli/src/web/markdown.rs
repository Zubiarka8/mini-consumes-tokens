//! Small, escaping Markdown renderer for the generated vault notes (ADR-018 D7).
//!
//! Supported: front matter (returned apart), headings, paragraphs, flat lists,
//! block quotes, fenced code, tables, rules, `code`, `**bold**`, `*italic*`,
//! `[text](target)` and `[[wiki|label]]`. Everything is HTML-escaped. A link is
//! emitted only when the caller's resolver maps its target to an internal route;
//! any other target (including `http(s)`) is shown as text, so a note can never
//! make the page navigate anywhere unlisted. Raw HTML in a note is shown as text.

use super::render::escape;

/// `(front matter lines as key/value, body)`. No front matter returns the text unchanged.
pub(super) fn split_front_matter(text: &str) -> (Vec<(String, String)>, &str) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return (Vec::new(), text);
    };
    let Some(end) = rest.find("\n---\n") else {
        return (Vec::new(), text);
    };
    let pairs = rest[..end]
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|inner| inner.strip_suffix('"'))
                .map_or(value.to_string(), |inner| {
                    inner.replace("\\\"", "\"").replace("\\\\", "\\")
                });
            (key.trim().to_string(), value)
        })
        .collect();
    (pairs, &rest[end + 5..])
}

type Resolver<'a> = &'a dyn Fn(&str) -> Option<String>;

fn inline(text: &str, resolve: Resolver<'_>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    let find = |from: usize, needle: &[char]| -> Option<usize> {
        (from..chars.len().saturating_sub(needle.len() - 1))
            .find(|i| chars[*i..].starts_with(needle))
    };
    while at < chars.len() {
        let rest = &chars[at..];
        if rest[0] == '`' {
            if let Some(end) = find(at + 1, &['`']) {
                let code: String = chars[at + 1..end].iter().collect();
                out.push_str(&format!("<code>{}</code>", escape(&code)));
                at = end + 1;
                continue;
            }
        } else if rest.starts_with(&['[', '[']) {
            if let Some(end) = find(at + 2, &[']', ']']) {
                let inner: String = chars[at + 2..end].iter().collect();
                let (target, label) = inner.split_once('|').unwrap_or((&inner, &inner));
                out.push_str(&link(target, label, resolve));
                at = end + 2;
                continue;
            }
        } else if rest[0] == '[' {
            if let Some(close) = find(at + 1, &[']']) {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = find(close + 2, &[')']) {
                        let label: String = chars[at + 1..close].iter().collect();
                        let target: String = chars[close + 2..end].iter().collect();
                        out.push_str(&link(&target, &label, resolve));
                        at = end + 1;
                        continue;
                    }
                }
            }
        } else if rest.starts_with(&['*', '*']) {
            if let Some(end) = find(at + 2, &['*', '*']).filter(|end| *end > at + 2) {
                let inner: String = chars[at + 2..end].iter().collect();
                out.push_str(&format!("<strong>{}</strong>", escape(&inner)));
                at = end + 2;
                continue;
            }
        } else if rest[0] == '*' {
            if let Some(end) = find(at + 1, &['*']).filter(|end| *end > at + 1) {
                let inner: String = chars[at + 1..end].iter().collect();
                out.push_str(&format!("<em>{}</em>", escape(&inner)));
                at = end + 1;
                continue;
            }
        }
        out.push_str(&escape(&rest[0].to_string()));
        at += 1;
    }
    out
}

fn link(target: &str, label: &str, resolve: Resolver<'_>) -> String {
    match resolve(target.trim()) {
        Some(href) => format!("<a href=\"{}\">{}</a>", escape(&href), escape(label)),
        None => format!("<span class=\"unlinked\">{}</span>", escape(label)),
    }
}

fn is_rule(line: &str) -> bool {
    let line = line.trim();
    line.len() >= 3 && line.chars().all(|ch| matches!(ch, '-' | '*' | '_'))
}

fn cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

fn is_table_separator(line: &str) -> bool {
    line.contains('-')
        && line
            .trim()
            .chars()
            .all(|ch| matches!(ch, '|' | '-' | ':' | ' '))
}

pub(super) fn render(body: &str, resolve: Resolver<'_>) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut out = String::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            at += 1;
        } else if let Some(fence) = trimmed.strip_prefix("```") {
            let language: String = fence
                .chars()
                .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '+'))
                .collect();
            let mut code = Vec::new();
            at += 1;
            while at < lines.len() && !lines[at].trim_start().starts_with("```") {
                code.push(lines[at]);
                at += 1;
            }
            at += 1;
            out.push_str(&format!(
                "<pre><code class=\"language-{language}\">{}</code></pre>",
                escape(&code.join("\n"))
            ));
        } else if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|ch| *ch == '#').count();
            if (1..=6).contains(&level) && trimmed[level..].starts_with(' ') {
                out.push_str(&format!(
                    "<h{level}>{}</h{level}>",
                    inline(trimmed[level..].trim(), resolve)
                ));
                at += 1;
            } else {
                at = paragraph(&lines, at, &mut out, resolve);
            }
        } else if is_rule(trimmed) {
            out.push_str("<hr>");
            at += 1;
        } else if trimmed.starts_with('>') {
            let mut quote = Vec::new();
            while at < lines.len() && lines[at].trim_start().starts_with('>') {
                quote.push(lines[at].trim_start().trim_start_matches('>').trim());
                at += 1;
            }
            out.push_str(&format!(
                "<blockquote><p>{}</p></blockquote>",
                inline(&quote.join(" "), resolve)
            ));
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            out.push_str("<ul>");
            while at < lines.len() {
                let item = lines[at].trim_start();
                let Some(text) = item.strip_prefix("- ").or_else(|| item.strip_prefix("* ")) else {
                    break;
                };
                out.push_str(&format!("<li>{}</li>", inline(text, resolve)));
                at += 1;
            }
            out.push_str("</ul>");
        } else if trimmed.starts_with(|ch: char| ch.is_ascii_digit())
            && trimmed
                .split_once(". ")
                .is_some_and(|(n, _)| n.chars().all(|ch| ch.is_ascii_digit()))
        {
            out.push_str("<ol>");
            while at < lines.len() {
                let Some((n, text)) = lines[at].trim_start().split_once(". ") else {
                    break;
                };
                if !n.chars().all(|ch| ch.is_ascii_digit()) {
                    break;
                }
                out.push_str(&format!("<li>{}</li>", inline(text, resolve)));
                at += 1;
            }
            out.push_str("</ol>");
        } else if trimmed.contains('|')
            && lines
                .get(at + 1)
                .is_some_and(|next| is_table_separator(next))
        {
            out.push_str("<div class=\"table-wrap\"><table><thead><tr>");
            for cell in cells(trimmed) {
                out.push_str(&format!("<th>{}</th>", inline(cell, resolve)));
            }
            out.push_str("</tr></thead><tbody>");
            at += 2;
            while at < lines.len() && lines[at].contains('|') {
                out.push_str("<tr>");
                for cell in cells(lines[at]) {
                    out.push_str(&format!("<td>{}</td>", inline(cell, resolve)));
                }
                out.push_str("</tr>");
                at += 1;
            }
            out.push_str("</tbody></table></div>");
        } else {
            at = paragraph(&lines, at, &mut out, resolve);
        }
    }
    out
}

fn paragraph(lines: &[&str], mut at: usize, out: &mut String, resolve: Resolver<'_>) -> usize {
    let mut text = Vec::new();
    while at < lines.len() {
        let line = lines[at].trim();
        if line.is_empty()
            || line.starts_with("```")
            || line.starts_with("- ")
            || line.starts_with("> ")
        {
            break;
        }
        text.push(line);
        at += 1;
    }
    if text.is_empty() {
        // A line that no block rule took: show it rather than loop.
        text.push(lines[at].trim());
        at += 1;
    }
    out.push_str(&format!("<p>{}</p>", inline(&text.join(" "), resolve)));
    at
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn html(text: &str) -> String {
        render(text, &|target| {
            (target == "ok.md").then(|| "nota?n=ok.md".to_string())
        })
    }

    #[test]
    fn raw_html_and_scripts_are_text_and_unlisted_links_are_not_links() {
        let out = html("<script>alert(1)</script>\n\n[bad](javascript:alert(1)) [web](https://example.com) [[Other]]");
        assert!(!out.contains("<script>") && out.contains("&lt;script&gt;"));
        assert!(!out.contains("href"), "{out}");
        assert!(out.contains("unlinked"));
    }

    #[test]
    fn listed_links_and_blocks_render() {
        let out = html("# Title\n\ntext with `code`, **bold** and *it* [go](ok.md) [[ok.md|wiki]]\n\n- a\n- b\n\n1. one\n2. two\n\n> quote\n\n```rust\nlet a = 1 < 2;\n```\n\n| h1 | h2 |\n|---|---|\n| a | b |\n\n---");
        for needle in [
            "<h1>Title</h1>",
            "<code>code</code>",
            "<strong>bold</strong>",
            "<em>it</em>",
            "<a href=\"nota?n=ok.md\">go</a>",
            "<a href=\"nota?n=ok.md\">wiki</a>",
            "<ul><li>a</li><li>b</li></ul>",
            "<ol><li>one</li><li>two</li></ol>",
            "<blockquote>",
            "let a = 1 &lt; 2;",
            "<th>h1</th>",
            "<td>b</td>",
            "<hr>",
        ] {
            assert!(out.contains(needle), "{needle} in {out}");
        }
    }

    #[test]
    fn unterminated_markup_and_odd_lines_terminate() {
        for text in [
            "`open",
            "[open](",
            "[[open",
            "**open",
            "*",
            "```\nno end",
            "#nospace",
            "|",
            "1.",
            "> ",
        ] {
            let _ = html(text);
        }
    }

    #[test]
    fn front_matter_is_split() {
        let (pairs, body) =
            split_front_matter("---\ntitle: \"A \\\"b\\\"\"\npath: \"x/y.rs\"\n---\n# Body\n");
        assert_eq!(pairs[0], ("title".to_string(), "A \"b\"".to_string()));
        assert_eq!(pairs[1].1, "x/y.rs");
        assert_eq!(body, "# Body\n");
        assert_eq!(split_front_matter("no front").1, "no front");
    }
}
