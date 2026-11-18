"""Supply-chain rules for every workflow file."""

import re
import unittest
from pathlib import Path

WORKFLOWS = sorted((Path(__file__).resolve().parents[3] / ".github/workflows").glob("*.yml"))
USES = re.compile(r"^\s*(?:- )?uses:\s*(\S+)(.*)$", re.M)


class PinnedActions(unittest.TestCase):
    def test_every_remote_action_is_pinned_to_a_commit_with_a_version_comment(self):
        self.assertTrue(WORKFLOWS)
        for path in WORKFLOWS:
            for ref, rest in USES.findall(path.read_text(encoding="utf-8")):
                if ref.startswith("./"):
                    continue  # a workflow of this repository, same commit
                with self.subTest(file=path.name, uses=ref):
                    self.assertRegex(ref, r"^[\w.-]+/[\w.-]+(/[\w./-]+)?@[0-9a-f]{40}$")
                    self.assertRegex(rest, r"#\s*\S+", "say which version the SHA is")

    def test_one_action_one_commit(self):
        """Two pins of the same action must not drift apart."""
        seen: dict[str, set[str]] = {}
        for path in WORKFLOWS:
            for ref, _ in USES.findall(path.read_text(encoding="utf-8")):
                if "@" in ref and not ref.startswith("./"):
                    name, sha = ref.split("@", 1)
                    seen.setdefault(name, set()).add(sha)
        for name, shas in seen.items():
            self.assertEqual(len(shas), 1, f"{name} pinned to {sorted(shas)}")


if __name__ == "__main__":
    unittest.main()
