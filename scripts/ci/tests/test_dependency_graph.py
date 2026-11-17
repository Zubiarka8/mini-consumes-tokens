import random
import shutil
import unittest
from pathlib import Path

from helpers import POLICY, graph, metadata

from dependency_graph import CrateGraph, MetadataError, from_metadata, load

REPO = Path(__file__).resolve().parents[3]


class Graph(unittest.TestCase):
    def test_from_metadata_keeps_workspace_edges_of_every_kind(self):
        g = from_metadata(metadata())
        self.assertIn("mct-corpus", g.deps["mct-lang-a"])  # dev-dependency
        self.assertNotIn("serde", g.deps["mct-lang-a"])  # registry crate
        self.assertEqual(g.dirs["mct-cli"], "crates/mct-cli")

    def test_windows_paths_are_normalized(self):
        meta = metadata("C:\\w")
        for p in meta["packages"]:
            p["manifest_path"] = p["manifest_path"].replace("/", "\\")
        self.assertEqual(from_metadata(meta).dirs["mct-core"], "crates/mct-core")

    def test_unknown_dependency_is_an_error(self):
        with self.assertRaises(MetadataError):
            CrateGraph({"a": "crates/a"}, {"a": {"ghost"}})

    def test_owner_is_the_longest_prefix(self):
        g = CrateGraph({"outer": "crates/x", "inner": "crates/x/fuzz"}, {"outer": set(), "inner": set()})
        self.assertEqual(g.owner_of("crates/x/fuzz/src/a.rs"), "inner")
        self.assertEqual(g.owner_of("crates/x/src/a.rs"), "outer")
        self.assertIsNone(g.owner_of("crates/xy/src/a.rs"))

    def test_transitive_dependents(self):
        self.assertEqual(
            graph().dependents({"mct-tree-sitter"}),
            {"mct-tree-sitter", "mct-lang-a", "mct-lang-b", "mct-lang-md",
             "mct-languages", "mct-cli", "mct-mcp-server", "mct-eval"},
        )

    def test_a_dev_dependency_change_reaches_its_users(self):
        self.assertIn("mct-lang-a", graph().dependents({"mct-corpus"}))

    def test_transitive_dependencies(self):
        self.assertEqual(graph().dependencies({"mct-lang-b"}),
                         {"mct-lang-b", "mct-core", "mct-tree-sitter"})


@unittest.skipUnless(shutil.which("cargo"), "cargo not installed")
class RealWorkspace(unittest.TestCase):
    """The policy's assumptions hold on this repository's actual graph."""

    @classmethod
    def setUpClass(cls):
        cls.graph = load(str(REPO))

    def test_every_crate_dir_exists(self):
        for name, directory in self.graph.dirs.items():
            self.assertTrue((REPO / directory / "Cargo.toml").is_file(), name)

    def test_policy_crates_exist(self):
        for key in ("critical_crates", "platform_sensitive_crates", "docs_consumer_crates"):
            for crate in POLICY[key]:
                self.assertIn(crate, self.graph.dirs, f"{key}: {crate}")
        for crate in POLICY["path_owners"].values():
            self.assertIn(crate, self.graph.dirs)

    def test_index_and_corpus_reach_every_language_crate(self):
        langs = {c for c in self.graph.dirs if c.startswith("mct-lang-")}
        self.assertTrue(langs <= self.graph.dependents({"mct-index"}))
        self.assertTrue(langs <= self.graph.dependents({"mct-corpus"}))

    def test_a_language_crate_reaches_the_binaries(self):
        users = self.graph.dependents({"mct-lang-go"})
        self.assertTrue({"mct-languages", "mct-cli", "mct-mcp-server", "mct-eval"} <= users)


class GraphProperties(unittest.TestCase):
    """Random DAGs: dependents() is the exact reverse of dependencies()."""

    def random_graph(self, rng: random.Random) -> CrateGraph:
        n = rng.randint(1, 25)
        names = [f"c{i}" for i in range(n)]
        deps = {name: {names[j] for j in range(i) if rng.random() < 0.2}
                for i, name in enumerate(names)}
        return CrateGraph({name: f"crates/{name}" for name in names}, deps)

    def test_reverse_closure_matches_forward_closure(self):
        rng = random.Random(1234)
        for _ in range(300):
            g = self.random_graph(rng)
            for crate in g.dirs:
                users = g.dependents({crate})
                expected = {c for c in g.dirs if crate in g.dependencies({c})}
                self.assertEqual(users, expected)

    def test_closure_is_monotonic_and_idempotent(self):
        rng = random.Random(99)
        for _ in range(300):
            g = self.random_graph(rng)
            seeds = set(rng.sample(sorted(g.dirs), rng.randint(1, len(g.dirs))))
            closed = g.dependents(seeds)
            self.assertTrue(seeds <= closed)
            self.assertEqual(g.dependents(closed), closed)


if __name__ == "__main__":
    unittest.main()
