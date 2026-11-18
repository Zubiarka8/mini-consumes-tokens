import random
import unittest

from helpers import POLICY, changes, graph

import policy as pol
from classify_risk import classify
from detect_changes import Change


def plan_for(*paths):
    return classify(changes(*paths), graph(), POLICY)


class Levels(unittest.TestCase):
    def test_docs_only_is_r0_and_runs_only_the_docs_consumers(self):
        plan = plan_for("docs/guide.md", "README.md")
        self.assertEqual(plan.level, "R0")
        self.assertFalse(plan.full)
        self.assertEqual(plan.test_crates, set(POLICY["docs_consumer_crates"]))

    def test_parser_change_selects_the_parser_and_its_dependents(self):
        plan = plan_for("crates/mct-lang-a/src/lib.rs")
        self.assertEqual(plan.level, "R2")
        self.assertFalse(plan.full)
        self.assertEqual(plan.test_crates,
                         {"mct-lang-a", "mct-languages", "mct-cli", "mct-mcp-server", "mct-eval"})

    def test_a_parser_test_only_change_stays_in_the_parser(self):
        plan = plan_for("crates/mct-lang-a/tests/parse.rs")
        self.assertEqual((plan.level, plan.test_crates), ("R1", {"mct-lang-a"}))

    def test_a_crate_manifest_reaches_dependents_even_in_tests_dir_logic(self):
        plan = plan_for("crates/mct-lang-a/Cargo.toml")
        self.assertEqual(plan.level, "R2")

    def test_a_leaf_crate_is_r1(self):
        plan = plan_for("crates/mct-leaf-user/src/lib.rs")
        self.assertEqual((plan.level, plan.test_crates), ("R1", {"mct-leaf-user"}))

    def test_core_change_is_full(self):
        plan = plan_for("crates/mct-core/src/lib.rs")
        self.assertTrue(plan.full)
        self.assertEqual(plan.level, "R3")
        self.assertEqual(plan.test_crates, set(graph().dirs))

    def test_cargo_lock_is_r4_full(self):
        plan = plan_for("Cargo.lock")
        self.assertEqual((plan.level, plan.full), ("R4", True))

    def test_ci_change_is_r4_full(self):
        for path in (".github/workflows/ci.yml", "scripts/ci/ci_policy.toml",
                     "scripts/ci/detect_changes.py", "scripts/ci/tests/test_x.py"):
            with self.subTest(path=path):
                plan = plan_for(path)
                self.assertEqual((plan.level, plan.full), ("R4", True))

    def test_unknown_path_is_full(self):
        plan = plan_for("some-new-top-level-file.bin")
        self.assertTrue(plan.full)
        self.assertEqual(plan.level, POLICY["unknown_level"])

    def test_a_file_of_a_deleted_crate_is_full(self):
        plan = classify([Change("D", "crates/mct-removed/src/lib.rs")], graph(), POLICY)
        self.assertTrue(plan.full)

    def test_rename_counts_both_sides(self):
        plan = classify([Change("R", "crates/mct-leaf-user/src/x.rs", "crates/mct-core/src/x.rs")],
                        graph(), POLICY)
        self.assertTrue(plan.full)

    def test_vendored_grammar_belongs_to_its_crate(self):
        plan = plan_for("vendor/tree-sitter-md/src/parser.c")
        self.assertIn("mct-lang-md", plan.touched_crates)
        self.assertFalse(plan.full)

    def test_fuzz_harness_is_full(self):
        self.assertTrue(plan_for("crates/mct-lang-a/fuzz/fuzz_targets/parse.rs").full)

    def test_a_markdown_file_inside_a_crate_is_not_docs(self):
        plan = plan_for("crates/mct-lang-a/README.md")
        self.assertIn("mct-lang-a", plan.test_crates)

    def test_wide_impact_becomes_full(self):
        plan = plan_for("crates/mct-tree-sitter/src/lib.rs")
        self.assertTrue(plan.full)

    def test_no_changes(self):
        plan = classify([], graph(), POLICY)
        self.assertEqual((plan.level, plan.test_crates), ("R0", set()))

    def test_every_path_has_a_reason(self):
        plan = plan_for("docs/a.md", "crates/mct-lang-b/src/x.rs", "Cargo.lock")
        self.assertEqual({r.path for r in plan.reasons} - {"*"},
                         {"docs/a.md", "crates/mct-lang-b/src/x.rs", "Cargo.lock"})


class Properties(unittest.TestCase):
    """Randomized: more changed files never lower the level or shrink the selection."""

    POOL = [
        "docs/a.md", "README.md", "website/index.html", "errors/REGISTRY.md",
        "crates/mct-lang-a/src/lib.rs", "crates/mct-lang-b/tests/t.rs",
        "crates/mct-leaf/src/lib.rs", "crates/mct-cli/src/main.rs",
        "crates/mct-core/src/lib.rs", "Cargo.lock", "Cargo.toml",
        ".github/workflows/ci.yml", "scripts/unix/check.sh", "weird/path",
        "crates/mct-gone/src/lib.rs", "vendor/tree-sitter-md/grammar.js",
    ]

    def test_monotonic_and_deterministic(self):
        rng = random.Random(7)
        for _ in range(500):
            small = rng.sample(self.POOL, rng.randint(0, 5))
            big = small + rng.sample(self.POOL, rng.randint(0, 5))
            a, b = plan_for(*small), plan_for(*big)
            self.assertLessEqual(pol.level_index(a.level), pol.level_index(b.level))
            self.assertTrue(a.test_crates <= b.test_crates)
            self.assertTrue(b.full or not a.full)
            again = plan_for(*reversed(small))
            self.assertEqual((again.level, again.full, again.test_crates),
                             (a.level, a.full, a.test_crates))
            self.assertEqual([r.path for r in again.reasons], [r.path for r in a.reasons])

    def test_a_critical_path_is_never_docs(self):
        rng = random.Random(11)
        critical = ["Cargo.lock", ".github/workflows/x.yml", "crates/mct-core/src/a.rs",
                    "crates/mct-index/src/db.rs", "scripts/ci/main.py"]
        docs = ["docs/a.md", "README.md", "website/x.css"]
        for _ in range(200):
            paths = rng.sample(docs, rng.randint(0, 3)) + [rng.choice(critical)]
            plan = plan_for(*paths)
            self.assertTrue(plan.full)
            self.assertGreaterEqual(pol.level_index(plan.level), pol.level_index("R3"))


if __name__ == "__main__":
    unittest.main()
