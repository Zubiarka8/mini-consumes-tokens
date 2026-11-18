"""No tracked Markdown file keeps a merge-conflict marker line."""

import re
import subprocess
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
MARKER = re.compile(r"^(<{7} |>{7} |\|{7,}( [0-9a-f]{7,})?$)")
SEPARATOR = re.compile(r"^={7,}$")


def markers(text: str) -> list[int]:
    """1-based lines that are conflict markers. A `=======` line is one
    only when nothing a setext heading could underline sits above it."""
    lines = text.split("\n")
    hits = []
    for i, line in enumerate(lines):
        if MARKER.match(line):
            hits.append(i + 1)
        elif SEPARATOR.match(line):
            above = lines[i - 1] if i else ""
            if not above.strip() or MARKER.match(above) or SEPARATOR.match(above):
                hits.append(i + 1)
    return hits


class MergeMarkers(unittest.TestCase):
    def test_the_rule(self):
        self.assertEqual(markers("a\n<<<<<<< HEAD\nb\n||||||| 1234567\n=======\n>>>>>>> x"), [2, 4, 5, 6])
        self.assertEqual(markers("||||||||| 2922794a\n========="), [1, 2])
        self.assertEqual(markers("Decision log\n============"), [])  # setext heading
        self.assertEqual(markers("| a | b |\n|---|---|"), [])

    def test_no_tracked_markdown_has_one(self):
        files = subprocess.run(["git", "ls-files", "*.md"], cwd=REPO, check=True,
                               capture_output=True, text=True).stdout.split()
        self.assertTrue(files)
        for name in files:
            path = REPO / name
            if not path.is_file():
                continue
            with self.subTest(file=name):
                self.assertEqual(markers(path.read_text(encoding="utf-8", errors="replace")), [])


if __name__ == "__main__":
    unittest.main()
