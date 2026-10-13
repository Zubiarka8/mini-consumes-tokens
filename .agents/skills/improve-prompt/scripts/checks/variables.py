#!/usr/bin/env python3
"""Check for template variables in prompts."""

import json
import re
import sys

NAME = "variables"
DESCRIPTION = "Detects template variables/placeholders for dynamic content"
RECOMMENDATION = """Add variables when:
- Prompt will be reused with different inputs
- There are placeholders like [INSERT X HERE] or similar
- Dynamic content needs to be injected programmatically
Common formats: {{variable}}, {variable}, ${variable}, [VARIABLE]"""

# Patterns for common variable formats
VARIABLE_PATTERNS = [
    # Double curly braces: {{variable}}
    (re.compile(r'\{\{([^}]+)\}\}'), 'double_curly'),
    # Single curly braces: {variable} (but not JSON-like patterns)
    (re.compile(r'(?<!\{)\{([a-zA-Z_][a-zA-Z0-9_]*)\}(?!\})'), 'single_curly'),
    # Dollar sign: ${variable}
    (re.compile(r'\$\{([^}]+)\}'), 'dollar_curly'),
    # Dollar sign simple: $variable
    (re.compile(r'\$([a-zA-Z_][a-zA-Z0-9_]*)'), 'dollar'),
    # Square brackets placeholder: [VARIABLE] or [INSERT X]
    (re.compile(r'\[([A-Z][A-Z0-9_\s]+)\]'), 'bracket'),
    # Angle brackets: <variable_name> (distinct from XML by underscore/lowercase)
    (re.compile(r'<([a-z][a-z0-9_]+)>'), 'angle'),
]


def detect(text: str) -> dict:
    """
    Detect template variables in the prompt text.

    Returns:
        dict with:
        - found: bool, whether any variables were found
        - count: int, number of unique variables
        - variables: list of unique variable names
        - locations: list of match details with type
    """
    variables_found = set()
    locations = []

    for pattern, var_type in VARIABLE_PATTERNS:
        for match in pattern.finditer(text):
            var_name = match.group(1).strip()
            variables_found.add(var_name)
            locations.append({
                "variable": var_name,
                "type": var_type,
                "match": match.group(0),
                "start": match.start(),
                "end": match.end()
            })

    return {
        "found": len(variables_found) > 0,
        "count": len(variables_found),
        "variables": sorted(list(variables_found)),
        "locations": locations
    }


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python variables.py <prompt_file>")
        sys.exit(1)

    with open(sys.argv[1]) as f:
        text = f.read()

    result = detect(text)
    print(json.dumps(result, indent=2))
