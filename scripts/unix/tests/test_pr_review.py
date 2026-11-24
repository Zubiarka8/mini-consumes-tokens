#!/usr/bin/env python3
"""Exercise the PR review workflow against disposable local repositories."""
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[2] / 'pr-review.py'


class ReviewWorkflow(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='pr review ')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'checkout'
        self.remote = Path(self.temp.name) / 'origin.git'
        self.path = Path(self.temp.name) / 'work tree'
        self.git('init', '--bare', str(self.remote), cwd=self.temp.name)
        self.git('init', str(self.root), cwd=self.temp.name)
        self.git('config', 'user.email', 'test@example.com')
        self.git('config', 'user.name', 'Test')
        (self.root / 'file').write_text('base\n')
        self.git('add', 'file')
        self.git('commit', '-m', 'base')
        self.base = self.git('rev-parse', 'HEAD')
        self.git('branch', '-M', 'main')
        self.git('remote', 'add', 'origin', str(self.remote))
        self.git('push', 'origin', 'main')
        (self.root / 'file').write_text('head\n')
        self.git('commit', '-am', 'head')
        self.head = self.git('rev-parse', 'HEAD')
        self.git('push', 'origin', 'HEAD:refs/pull/42/head', 'HEAD:refs/heads/feature')
        self.git('config', f'url.{self.remote}.insteadOf', 'https://github.com/owner/repo.git')
        self.git('reset', '--hard', self.base)
        bindir = Path(self.temp.name) / 'bin'
        bindir.mkdir()
        fake = bindir / 'gh'
        fake.write_text('#!/usr/bin/env python3\nimport os,sys\n'
                        'a=sys.argv[1:]\n'
                        'print(os.environ.get("CURRENT_BASE", os.environ["BASE"]) if a[:1]==["api"] else '
                        '"owner/repo" if a[:2]==["repo","view"] else '
                        '"OPEN\\nmain\\n"+os.environ["BASE"]+"\\nfeature\\n"+'
                        'os.environ["HEAD_OID"]+"\\nowner/repo" if "headRefOid" in " ".join(a) '
                        'else "merge: CLEAN; checks: 0")\n')
        fake.chmod(0o755)
        self.env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ['PATH'],
                        BASE=self.base, HEAD_OID=self.head)

    def git(self, *args, cwd=None):
        return subprocess.run(['git', *args], cwd=cwd or self.root, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True).stdout.strip()

    def workflow(self, *args, expected=0):
        p = subprocess.run(['python3', str(SCRIPT), *args, '--worktree', str(self.path)], cwd=self.root,
                           env=self.env, text=True, capture_output=True)
        self.assertEqual(p.returncode, expected, p.stdout + p.stderr)
        return p.stdout + p.stderr

    def test_start_status_and_clean_cleanup(self):
        self.workflow('start', '42', '--dry-run')
        self.assertFalse(self.path.exists())
        self.workflow('start', '42')
        self.assertEqual(self.git('rev-parse', 'HEAD', cwd=self.path), self.head)
        self.assertIn('local HEAD', self.workflow('status', '42'))
        (self.path / 'untracked').write_text('keep')
        self.assertIn('untracked changes', self.workflow('cleanup', '42', expected=2))
        self.assertTrue((self.path / 'untracked').exists())
        (self.path / 'untracked').unlink()
        self.git('config', 'core.excludesFile', str(self.root / 'ignore'))
        (self.root / 'ignore').write_text('artifact\n')
        (self.path / 'artifact').write_text('keep')
        self.assertIn('ignored files', self.workflow('cleanup', '42', expected=2))
        (self.path / 'artifact').unlink()
        self.workflow('cleanup', '42', '--dry-run')
        self.assertTrue(self.path.exists())
        self.workflow('cleanup', '42')
        self.assertFalse(self.path.exists())

    def test_unpublished_and_stale_work_are_not_removed_or_pushed(self):
        self.workflow('start', '42')
        (self.path / 'file').write_text('review\n')
        self.git('commit', '-am', 'review', cwd=self.path)
        self.assertIn('differs', self.workflow('cleanup', '42', expected=2))
        self.assertIn('fast-forward only', self.workflow('push', '42', '--dry-run'))
        self.env['HEAD_OID'] = self.base
        self.assertIn('differs', self.workflow('cleanup', '42', expected=2))
        self.env['HEAD_OID'] = 'f' * 40
        self.assertIn('failed', self.workflow('push', '42', '--dry-run', expected=2))
        self.assertTrue(self.path.exists())

    def test_pushes_only_explicitly_to_the_pr_head(self):
        self.workflow('start', '42')
        (self.path / 'file').write_text('review\n')
        self.git('commit', '-am', 'review', cwd=self.path)
        new_head = self.git('rev-parse', 'HEAD', cwd=self.path)
        self.workflow('push', '42', '--dry-run')
        self.assertEqual(self.git('--git-dir', str(self.remote), 'rev-parse', 'refs/heads/feature'), self.head)
        self.workflow('push', '42')
        self.assertEqual(self.git('--git-dir', str(self.remote), 'rev-parse', 'refs/heads/feature'), new_head)
        self.env['HEAD_OID'] = new_head
        self.workflow('cleanup', '42')

    def test_rejects_unrelated_worktree(self):
        self.git('worktree', 'add', '--detach', str(self.path), self.head)
        self.assertIn('not created by pr-review', self.workflow('cleanup', '42', expected=2))
        self.assertTrue(self.path.exists())

    def test_rejects_changed_head_before_creation(self):
        self.env['HEAD_OID'] = self.base
        self.assertIn('head changed', self.workflow('start', '42', expected=2))
        self.assertFalse(self.path.exists())

    def test_start_uses_current_base_when_pr_metadata_is_stale(self):
        self.env['BASE'] = self.head
        self.env['CURRENT_BASE'] = self.base
        self.workflow('start', '42')
        self.assertIn('local HEAD', self.workflow('status', '42'))
        self.workflow('cleanup', '42')

    def test_start_rejects_base_change_during_fetch(self):
        self.env['CURRENT_BASE'] = self.head
        self.assertIn('base changed', self.workflow('start', '42', expected=2))
        self.assertFalse(self.path.exists())

    def test_status_fetches_base_that_advanced_after_start(self):
        self.workflow('start', '42')
        other = Path(self.temp.name) / 'other'
        self.git('clone', str(self.remote), str(other), cwd=self.temp.name)
        self.git('checkout', 'main', cwd=other)
        self.git('config', 'user.email', 'test@example.com', cwd=other)
        self.git('config', 'user.name', 'Test', cwd=other)
        (other / 'new-file').write_text('base advanced\n')
        self.git('add', 'new-file', cwd=other)
        self.git('commit', '-m', 'advance base', cwd=other)
        self.git('push', 'origin', 'main', cwd=other)
        self.env['CURRENT_BASE'] = self.git('rev-parse', 'HEAD', cwd=other)
        self.env['BASE'] = self.env['CURRENT_BASE']
        self.assertIn('local HEAD', self.workflow('status', '42'))

    def test_push_rejects_concurrent_remote_update(self):
        self.workflow('start', '42')
        (self.path / 'file').write_text('local review\n')
        self.git('commit', '-am', 'local review', cwd=self.path)
        other = Path(self.temp.name) / 'other'
        self.git('clone', str(self.remote), str(other), cwd=self.temp.name)
        self.git('checkout', 'feature', cwd=other)
        self.git('config', 'user.email', 'test@example.com', cwd=other)
        self.git('config', 'user.name', 'Test', cwd=other)
        (other / 'file').write_text('remote review\n')
        self.git('commit', '-am', 'remote review', cwd=other)
        self.git('push', 'origin', 'feature', cwd=other)
        remote_head = self.git('rev-parse', 'HEAD', cwd=other)
        self.workflow('push', '42', expected=2)
        self.assertEqual(self.git('--git-dir', str(self.remote), 'rev-parse', 'refs/heads/feature'), remote_head)
        self.assertTrue(self.path.exists())


if __name__ == '__main__':
    unittest.main()
