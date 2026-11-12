//! The score tables of `docs/02-crates/parsers/symbol-coverage.md` are
//! derived from its checklists. A parser PR that ticks a checklist item and
//! forgets the tables leaves the page contradicting itself, so this test
//! recomputes both tables from the checklists, with the page's own rules:
//! `[x]` is 1, `[~]` is 0.5, `[ ]` is 0, `[?]` is left out, and a score
//! rounds to the nearest whole percent with halves rounded up.

use std::collections::BTreeMap;

const PAGE: &str = include_str!("../../../docs/02-crates/parsers/symbol-coverage.md");

/// Checklist section heading → area column, in the area table's order.
const AREAS: [(&str, &str); 7] = [
    ("Functions and methods", "Functions"),
    ("Types and object-oriented programming", "Types / OOP"),
    ("Fields, constants and variables", "Data"),
    ("Relations", "Relations"),
    ("Features: Search and lookup", "Search"),
    ("Features: Code quality", "Quality"),
    ("Features: Structure", "Structure"),
];

/// Areas scored as features; the others are constructs.
const FEATURE_AREAS: [&str; 3] = ["Search", "Quality", "Structure"];

type Areas = BTreeMap<&'static str, Vec<char>>;

/// Each language's checklist marks, grouped by area, in page order.
fn checklists() -> Result<Vec<(String, Areas)>, String> {
    let body = section(PAGE, "## Checklist by language")?;
    let mut out: Vec<(String, Areas)> = Vec::new();
    let mut area = None;
    for line in body.lines() {
        if let Some(name) = line
            .strip_prefix("### ")
            .and_then(|rest| rest.strip_suffix(')'))
            .and_then(|rest| rest.split_once(" checklist ("))
            .map(|(name, _)| name)
        {
            out.push((name.to_string(), BTreeMap::new()));
            area = None;
        } else if let Some(heading) = line.strip_prefix("**").and_then(|l| l.strip_suffix("**")) {
            let column = AREAS
                .iter()
                .find(|(h, _)| *h == heading)
                .map(|(_, column)| *column)
                .ok_or_else(|| format!("unknown checklist heading `{heading}`"))?;
            area = Some(column);
        } else if let Some(mark) = line.strip_prefix("- [").and_then(|l| l.chars().next()) {
            let (_, areas) = out
                .last_mut()
                .ok_or_else(|| format!("checklist item before any language: {line}"))?;
            let area = area.ok_or_else(|| format!("checklist item before any heading: {line}"))?;
            areas.entry(area).or_default().push(mark);
        }
    }
    Ok(out)
}

/// The text of the `## heading` section, up to the next `## ` heading.
/// Matched line by line, so a CRLF checkout (Git for Windows) reads the same.
fn section(page: &'static str, heading: &str) -> Result<&'static str, String> {
    let mut offset = 0;
    let mut start = None;
    for line in page.split_inclusive('\n') {
        let text = line.trim_end_matches(['\r', '\n']);
        match start {
            Some(begin) if text.starts_with("## ") => return Ok(&page[begin..offset]),
            None if text == heading => start = Some(offset + line.len()),
            _ => {}
        }
        offset += line.len();
    }
    start
        .map(|begin| &page[begin..])
        .ok_or_else(|| format!("no `{heading}` section"))
}

#[test]
fn a_section_is_found_with_either_line_ending() -> Result<(), String> {
    for page in [
        "# Page\n## Wanted\nbody\n## Next\nrest\n",
        "# Page\r\n## Wanted\r\nbody\r\n## Next\r\nrest\r\n",
    ] {
        assert_eq!(section(page, "## Wanted")?.trim_end(), "body");
        assert_eq!(section(page, "## Next")?.trim_end(), "rest");
    }
    Ok(())
}

/// `(percent, scored checks)`, or `None` with no scored check.
fn score(marks: &[char]) -> Result<Option<(u64, usize)>, String> {
    // Weights in halves, so a score is exact integer arithmetic.
    let mut halves = 0_u64;
    let mut scored = 0_u64;
    for mark in marks {
        match mark {
            'x' => halves += 2,
            '~' => halves += 1,
            ' ' => {}
            '?' => continue,
            other => return Err(format!("unknown checklist mark `[{other}]`")),
        }
        scored += 1;
    }
    if scored == 0 {
        return Ok(None);
    }
    // round_half_up(halves / 2 / scored * 100) = floor((100 * halves + scored) / (2 * scored))
    let percent = (100 * halves + scored) / (2 * scored);
    Ok(Some((percent, marks.iter().filter(|m| **m != '?').count())))
}

fn marks_of(areas: &Areas, wanted: &[&str]) -> Vec<char> {
    wanted
        .iter()
        .flat_map(|area| areas.get(area).into_iter().flatten().copied())
        .collect()
}

/// The data rows of the first Markdown table after `marker`, split into cells.
fn table_rows(marker: &str) -> Result<Vec<Vec<String>>, String> {
    let start = PAGE
        .find(marker)
        .ok_or_else(|| format!("no table with header `{marker}`"))?;
    Ok(PAGE[start..]
        .lines()
        .take_while(|line| line.starts_with('|'))
        .skip(2)
        .map(|line| {
            line.trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().to_string())
                .collect()
        })
        .collect())
}

fn percent(value: Option<(u64, usize)>) -> String {
    value.map_or_else(|| "—".to_string(), |(p, _)| format!("{p}%"))
}

#[test]
fn the_coverage_score_table_matches_the_checklists() -> Result<(), String> {
    let constructs: Vec<&str> = AREAS
        .iter()
        .map(|(_, column)| *column)
        .filter(|column| !FEATURE_AREAS.contains(column))
        .collect();
    let mut expected = Vec::new();
    for (language, areas) in checklists()? {
        let construct = score(&marks_of(&areas, &constructs))?;
        let feature = score(&marks_of(&areas, &FEATURE_AREAS))?;
        let unverified = areas.values().flatten().filter(|m| **m == '?').count();
        expected.push(vec![
            language,
            percent(construct),
            construct.map_or(0, |(_, n)| n).to_string(),
            percent(feature),
            feature.map_or(0, |(_, n)| n).to_string(),
            unverified.to_string(),
        ]);
    }
    assert_eq!(table_rows("| Language | Constructs |")?, expected);
    Ok(())
}

#[test]
fn the_area_score_table_matches_the_checklists() -> Result<(), String> {
    let mut expected = Vec::new();
    for (language, areas) in checklists()? {
        let mut row = vec![language];
        for (_, column) in AREAS {
            row.push(percent(score(&marks_of(&areas, &[column]))?));
        }
        expected.push(row);
    }
    assert_eq!(table_rows("| Language | Functions |")?, expected);
    Ok(())
}
