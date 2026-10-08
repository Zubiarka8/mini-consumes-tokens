"""Real Git regression fixtures; no network, credentials or existing checkout writes."""

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('pr_conflicts', Path(__file__).resolve().parents[2] / 'pr-conflicts.py')
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ConflictTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'repo with spaces'
        self.root.mkdir()
        self.git('init', '-b', 'main')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.git('config', 'user.name', 'Fixture')
        self.file = self.root / 'file with spaces.txt'
        self.commit('initial\n')
        self.initial = self.git('rev-parse', 'HEAD')
        self.git('branch', 'feature')

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, text=True, stderr=subprocess.DEVNULL).strip()

    def commit(self, text):
        self.file.write_text(text)
        self.git('add', '.')
        self.git('commit', '-m', 'fixture')
        return self.git('rev-parse', 'HEAD')

    def pr(self, head):
        base = self.git('rev-parse', 'main')
        self.git('update-ref', 'refs/remotes/origin/main', base)
        self.git('update-ref', 'refs/remotes/origin/feature', head)
        return {'number': 7, 'baseRefName': 'main', 'headRefName': 'feature', 'headRefOid': head, 'url': 'https://example.invalid/pull/7'}

    def test_conflict_paths_and_original_checkout_are_preserved(self):
        self.commit('main\n')
        self.git('switch', 'feature')
        head = self.commit('feature\n')
        before = self.git('status', '--porcelain')
        report = MODULE.inspect_pr(self.root, self.pr(head))
        self.assertEqual(report['state'], 'conflicting')
        self.assertEqual(report['conflicts'], [self.file.name])
        self.assertEqual(self.file.read_text(), 'feature\n')
        self.assertEqual(self.git('status', '--porcelain'), before)

    def test_same_patch_with_different_history_is_redundant(self):
        self.commit('same patch\n')
        self.git('switch', 'feature')
        head = self.commit('same patch\n')
        self.git('commit', '--allow-empty', '-m', 'different history')
        head = self.git('rev-parse', 'HEAD')
        report = MODULE.inspect_pr(self.root, self.pr(head))
        self.assertEqual(report['state'], 'redundant')
        self.assertEqual(report['conflicts'], [])

    def test_stale_remote_is_an_error(self):
        pr = self.pr(self.initial)
        pr['headRefOid'] = 'a' * 40
        with self.assertRaisesRegex(RuntimeError, 'differs from GitHub'):
            MODULE.inspect_pr(self.root, pr)

    def test_prepare_conflict_only_writes_a_new_worktree(self):
        self.commit('main\n')
        self.git('switch', 'feature')
        head = self.commit('feature\n')
        report = MODULE.inspect_pr(self.root, self.pr(head))
        path = Path(self.temp.name) / 'isolated review'
        MODULE.prepare(self.root, report, path)
        self.assertEqual(self.file.read_text(), 'feature\n')
        self.assertEqual(self.git('branch', '--show-current'), 'feature')
        self.assertEqual(self.git('status', '--porcelain'), '')
        self.assertTrue((path / self.file.name).read_text().startswith('<<<<<<<'))
        with self.assertRaisesRegex(RuntimeError, 'already exists'):
            MODULE.prepare(self.root, report, path)

    def test_git_failure_is_not_reported_as_mergeable(self):
        with self.assertRaises(RuntimeError):
            MODULE.merge_conflicts(self.root, 'missing-base', self.initial)

    def test_redundant_pr_is_not_prepared(self):
        report = MODULE.inspect_pr(self.root, self.pr(self.initial))
        with self.assertRaisesRegex(RuntimeError, 'adds nothing'):
            MODULE.prepare(self.root, report, Path(self.temp.name) / 'unused')
        self.assertFalse((Path(self.temp.name) / 'unused').exists())

    def test_checks_keep_failures_and_pending_distinct(self):
        self.assertEqual(MODULE.checks_summary([
            {'conclusion': 'SUCCESS'}, {'state': 'FAILURE'},
            {'conclusion': '', 'status': 'IN_PROGRESS'}, {'conclusion': 'SKIPPED'},
        ]), {'passed': 2, 'failed': 1, 'pending': 1})

    def test_origin_identifies_the_repository_explicitly(self):
        for url in ['https://github.com/owner/repo.git', 'git@github.com:owner/repo.git']:
            self.assertEqual(MODULE.repository_name(url), 'github.com/owner/repo')
        with self.assertRaises(RuntimeError):
            MODULE.repository_name('/local/repo')


if __name__ == '__main__':
    unittest.main()
