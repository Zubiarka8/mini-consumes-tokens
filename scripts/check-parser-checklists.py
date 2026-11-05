#!/usr/bin/env python3
"""Check that parser feature and language checklist totals agree."""

from __future__ import annotations

import re
import sys
from collections import Counter, defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKLISTS = ROOT / "docs/02-crates/parsers/checklists"
FEATURE_FILE = CHECKLISTS / "by-feature.md"
LANGUAGE_FILE = CHECKLISTS / "by-language.md"
DEFINITION_FILE = ROOT / "docs/02-crates/parsers/features.md"
LANGUAGES = (
    "Rust", "Python", "JS/TS", "Java", "Kotlin", "C#", "C++", "Go", "PHP",
    "Lua", "Bash", "PowerShell", "HTML", "CSS", "XML", "XAML", "Markdown",
)
STATUS_KEYS = ("x", "~", " ", "?")
STATUS_LINE = re.compile(r"^- \[(x|~| |\?)\] ([^:]+): (.+)$")
SUMMARY = re.compile(
    r"Done (\d+) · Partial (\d+) · Not done (\d+) · Not verified (\d+) · Not applicable (\d+)"
)
FEATURE_HEADING = re.compile(r"^### ([A-Z]{2}\d{2}) ")


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def read_feature_matrix(errors: list[str]):
    text = FEATURE_FILE.read_text()
    matrix = defaultdict(Counter)
    features = {}
    current_id = None
    current_lines: list[str] = []

    def finish_feature(feature_id: str | None, lines: list[str]) -> None:
        if feature_id is None:
            return
        if feature_id in features:
            fail(errors, f"by-feature.md: duplicate feature {feature_id}")
        counts = Counter()
        entries = {}
        for line in lines:
            match = STATUS_LINE.match(line)
            if match and match.group(2) in LANGUAGES:
                status, language = match.group(1, 2)
            elif line.startswith("- — "):
                language, _, _ = line[4:].partition(":")
                status = "na"
                if language not in LANGUAGES:
                    continue
            else:
                continue
            if language in entries:
                fail(errors, f"{feature_id}: duplicate row for {language}")
            entries[language] = status
            counts[status] += 1
            matrix[language][feature_id] = status

        if len(entries) != len(LANGUAGES):
            fail(errors, f"{feature_id}: expected {len(LANGUAGES)} language rows, found {len(entries)}")
        features[feature_id] = counts
        block = "\n".join(lines)
        stated = SUMMARY.search(block)
        actual = (counts["x"], counts["~"], counts[" "], counts["?"], counts["na"])
        if not stated or tuple(map(int, stated.groups())) != actual:
            fail(errors, f"{feature_id}: totals say {stated.groups() if stated else 'missing'}, rows count {actual}")

    for line in text.splitlines():
        heading = FEATURE_HEADING.match(line)
        if heading:
            finish_feature(current_id, current_lines)
            current_id = heading.group(1)
            current_lines = []
        elif current_id is not None:
            current_lines.append(line)
    finish_feature(current_id, current_lines)

    definitions = set(re.findall(r"^\| ([A-Z]{2}\d{2}) \|", DEFINITION_FILE.read_text(), re.M))
    if definitions != set(features):
        fail(errors, f"feature definitions and checklists differ: definitions-only {sorted(definitions - set(features))}, checklist-only {sorted(set(features) - definitions)}")
    return matrix, features


def read_language_sections(errors: list[str]):
    lines = LANGUAGE_FILE.read_text().splitlines()
    sections = {}
    current = None
    for line in lines:
        if line.startswith(("## ", "### ")):
            heading = line.lstrip("# ")
            language = next((name for name in LANGUAGES if heading.startswith(name + " (")), None)
            current = language
            if language:
                if language in sections:
                    fail(errors, f"by-language.md: duplicate section for {language}")
                sections[language] = []
        elif current:
            sections[current].append(line)

    summaries = {}
    for language, section_lines in sections.items():
        section = "\n".join(section_lines)
        stated = SUMMARY.search(section)
        if not stated:
            fail(errors, f"by-language.md: missing totals for {language}")
            continue
        summaries[language] = tuple(map(int, stated.groups()))

    summary_table = LANGUAGE_FILE.read_text().split("## Summary", 1)[1].split("## Rust", 1)[0]
    table_rows = re.findall(r"^\| (.+?) \| (\d+) \| (\d+) \| (\d+) \| (\d+) \| (\d+) \|$", summary_table, re.M)
    table = {name: tuple(map(int, values)) for name, *values in table_rows}
    if len(table_rows) != len(LANGUAGES) or set(table) != set(LANGUAGES):
        fail(errors, "by-language.md: summary table does not contain each language exactly once")
    for language in LANGUAGES:
        if table.get(language) != summaries.get(language):
            fail(errors, f"by-language.md: summary table disagrees with {language} section")
    return sections


def compare_views(errors: list[str], matrix, sections) -> None:
    for language in LANGUAGES:
        is_markup = language in {"HTML", "CSS", "XML", "XAML", "Markdown"}
        relevant_ids = ("MK",) if is_markup else ("FN", "TY", "DA", "RE", "SE", "QU", "ST")
        expected = Counter()
        expected_na_ids = set()
        for feature_id, status in matrix[language].items():
            if feature_id.startswith(relevant_ids):
                expected[status] += 1
                if status == "na":
                    expected_na_ids.add(feature_id)

        section = sections.get(language, [])
        if is_markup:
            section = section[: next((i for i, line in enumerate(section) if line == "Constructs:"), len(section))]

        actual = Counter()
        actual_na_ids = set()
        for line in section:
            match = STATUS_LINE.match(line)
            if match:
                actual[match.group(1)] += 1
            elif re.match(r"^- — [A-Z]{2}\d{2} ", line):
                actual["na"] += 1
                actual_na_ids.add(line[4:8])

        keys = (*STATUS_KEYS, "na")
        if tuple(expected[key] for key in keys) != tuple(actual[key] for key in keys):
            fail(errors, f"checklist views disagree for {language}: feature matrix {dict(expected)}, language list {dict(actual)}")
        if expected_na_ids != actual_na_ids:
            fail(errors, f"not-applicable rows disagree for {language}: matrix {sorted(expected_na_ids)}, language list {sorted(actual_na_ids)}")


def main() -> int:
    errors: list[str] = []
    matrix, _ = read_feature_matrix(errors)
    sections = read_language_sections(errors)
    compare_views(errors, matrix, sections)

    if errors:
        print("Parser checklist validation failed:")
        for error in errors:
            print(f"- {error}")
        return 1

    print(f"Parser checklists agree: {len(LANGUAGES)} languages and {len(matrix[LANGUAGES[0]])} features checked.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
