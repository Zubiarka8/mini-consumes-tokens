import unittest

from helpers import GitRepo

from detect_changes import ZERO_SHA, Change, RangeError, changed_files, parse_name_status


class ParseNameStatus(unittest.TestCase):
    def test_add_modify_delete(self):
        raw = "A\0new.rs\0M\0mod.rs\0D\0gone.rs\0"
        self.assertEqual(
            parse_name_status(raw),
            [Change("A", "new.rs"), Change("M", "mod.rs"), Change("D", "gone.rs")],
        )

    def test_rename_keeps_both_paths(self):
        change = parse_name_status("R087\0old/a.rs\0new/a.rs\0")[0]
        self.assertEqual(change, Change("R", "new/a.rs", "old/a.rs"))
        self.assertEqual(change.paths(), ("old/a.rs", "new/a.rs"))

    def test_truncated_record_is_an_error_not_a_guess(self):
        with self.assertRaises(RangeError):
            parse_name_status("R100\0only-old\0")
        with self.assertRaises(RangeError):
            parse_name_status("M\0")

    def test_empty_diff(self):
        self.assertEqual(parse_name_status(""), [])


class ChangedFiles(unittest.TestCase):
    def setUp(self):
        self.repo = GitRepo()
        self.repo.write("crates/a/src/lib.rs", "fn a() {}\n" * 20)
        self.repo.write("crates/b/src/lib.rs")
        self.repo.write("docs/x.md")
        self.base = self.repo.commit("base")

    def tearDown(self):
        self.repo.close()

    def test_docs_only(self):
        self.repo.write("docs/x.md", "changed\n")
        head = self.repo.commit()
        self.assertEqual(changed_files(self.base, head, self.repo.path), [Change("M", "docs/x.md")])

    def test_deleted_file_is_reported(self):
        self.repo.git("rm", "-q", "crates/b/src/lib.rs")
        head = self.repo.commit()
        self.assertEqual(changed_files(self.base, head, self.repo.path),
                         [Change("D", "crates/b/src/lib.rs")])

    def test_renamed_file_reports_old_and_new_path(self):
        self.repo.git("mv", "crates/a/src/lib.rs", "crates/b/src/moved.rs")
        head = self.repo.commit()
        [change] = changed_files(self.base, head, self.repo.path)
        self.assertEqual(change.status, "R")
        self.assertEqual(change.paths(), ("crates/a/src/lib.rs", "crates/b/src/moved.rs"))

    def test_several_crates(self):
        self.repo.write("crates/a/src/lib.rs", "fn z() {}\n")
        self.repo.write("crates/b/src/lib.rs", "y\n")
        head = self.repo.commit()
        paths = {c.path for c in changed_files(self.base, head, self.repo.path)}
        self.assertEqual(paths, {"crates/a/src/lib.rs", "crates/b/src/lib.rs"})

    def test_diff_is_against_the_merge_base_not_the_moving_main(self):
        self.repo.git("checkout", "-q", "-b", "feature")
        self.repo.write("crates/a/src/feature.rs")
        head = self.repo.commit("feature")
        self.repo.git("checkout", "-q", "main")
        self.repo.write("crates/b/src/on-main.rs")
        main_tip = self.repo.commit("main moves on")
        paths = {c.path for c in changed_files(main_tip, head, self.repo.path)}
        self.assertEqual(paths, {"crates/a/src/feature.rs"})

    def test_missing_base_commit_is_an_error(self):
        with self.assertRaises(RangeError):
            changed_files("1" * 40, "HEAD", self.repo.path)

    def test_zero_sha_of_a_new_branch_is_an_error(self):
        with self.assertRaises(RangeError):
            changed_files(ZERO_SHA, "HEAD", self.repo.path)

    def test_shallow_history_without_the_base_is_an_error(self):
        self.repo.write("docs/y.md")
        self.repo.commit()
        shallow = GitRepo()
        try:
            shallow.git("fetch", "-q", "--depth", "1", f"file://{self.repo.path}", "main")
            shallow.git("checkout", "-q", "FETCH_HEAD")
            with self.assertRaises(RangeError):
                changed_files(self.base, "HEAD", shallow.path)
        finally:
            shallow.close()


if __name__ == "__main__":
    unittest.main()
