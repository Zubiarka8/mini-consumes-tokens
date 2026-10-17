"""
Prompt analysis check modules.

These are tangible pattern-based checks that can be automated.
Other aspects (role, examples, clarity, etc.) require Claude's judgment
and are handled through the checklist, not scripts.

Each module provides:
- detect(text) -> dict with 'found', 'details', 'locations'
- NAME: Check identifier
- DESCRIPTION: What this check looks for
- RECOMMENDATION: When to add/improve this element
"""

from . import xml_tags
from . import variables

ALL_CHECKS = [
    xml_tags,
    variables,
]
