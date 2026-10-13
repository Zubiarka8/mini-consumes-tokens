#!/usr/bin/env python3
"""Check for XML tag usage in prompts."""

import json
import re
import sys

NAME = "xml_tags"
DESCRIPTION = "Detects XML-style tags used to structure the prompt"
RECOMMENDATION = """Add XML tags when:
- Prompt has multiple distinct sections (instructions, context, examples)
- There are inputs that need clear boundaries
- Output format needs to be specified
- Prompt is longer than a few sentences
Common tags: <instructions>, <context>, <examples>, <input>, <output>, <constraints>"""

# Pattern matches XML-style tags (opening, closing, or self-closing)
TAG_PATTERN = re.compile(r'</?([a-zA-Z][a-zA-Z0-9_-]*)(?:\s[^>]*)?\s*/?>')


def detect(text: str) -> dict:
    """
    Detect XML tags in the prompt text.

    Returns:
        dict with:
        - found: bool, whether any tags were found
        - count: int, number of unique tags
        - tags: list of unique tag names found
        - locations: list of (tag, start_pos, end_pos) tuples
    """
    matches = list(TAG_PATTERN.finditer(text))

    tags_found = set()
    locations = []

    for match in matches:
        tag_name = match.group(1).lower()
        tags_found.add(tag_name)
        locations.append({
            "tag": tag_name,
            "match": match.group(0),
            "start": match.start(),
            "end": match.end()
        })

    return {
        "found": len(tags_found) > 0,
        "count": len(tags_found),
        "tags": sorted(list(tags_found)),
        "locations": locations
    }


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python xml_tags.py <prompt_file>")
        sys.exit(1)

    with open(sys.argv[1]) as f:
        text = f.read()

    result = detect(text)
    print(json.dumps(result, indent=2))
