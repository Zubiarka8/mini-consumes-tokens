"""pr-sync.sh against real Git repositories: a bare origin and a clone, no network."""

from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
TRACKER = 'docs/plans/2026-11-10-phase-2-3-tracker.md'


class PrSyncTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        base = Path(self.temp.name)
        self.origin = base / 'origin.git'
        subprocess.check_call(['git', 'init', '-q', '--bare', '-b', 'main', str(self.origin)])
        self.repo = base / 'repo'
        subprocess.check_call(['git', 'clone', '-q', str(self.origin), str(self.repo)], stderr=subprocess.DEVNULL)
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.git('config', 'user.name', 'Fixture')
        self.git('checkout', '-q', '-b', 'main')
        (self.repo / 'scripts/unix').mkdir(parents=True)
        for name in ('lib.sh', 'pr-sync.sh'):
            shutil.copy(SCRIPTS / name, self.repo / 'scripts/unix' / name)
        self.write(TRACKER, 'head\n- [x] first\n')
        self.write('code.txt', 'one\n')
        self.commit('initial')
        self.git('push', '-q', 'origin', 'main')
        self.git('checkout', '-q', '-b', 'feature')

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.repo, text=True, stderr=subprocess.STDOUT).strip()

    def write(self, name, text):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def commit(self, message):
        self.git('add', '.')
        self.git('commit', '-q', '--allow-empty', '-m', message)

    def advance(self, branch, name, text, message):
        self.git('checkout', '-q', branch)
        self.write(name, text)
        self.commit(message)
        self.git('push', '-q', '-u', 'origin', branch)

    def sync(self, *args):
        # lib.sh cd's to the repo the script lives in.
        return subprocess.run(
            ['bash', str(self.repo / 'scripts/unix/pr-sync.sh'), 'feature', '--base', 'main', *args],
            cwd=self.repo, text=True, capture_output=True,
        )

    def diverge(self, feature_tracker, main_tracker, feature_code='one\n', main_code='one\n'):
        self.advance('feature', TRACKER, feature_tracker, 'feature work')
        self.write('code.txt', feature_code)
        self.commit('feature code')
        self.git('push', '-q', 'origin', 'feature')
        self.advance('main', TRACKER, main_tracker, 'main work')
        self.write('code.txt', main_code)
        self.commit('main code')
        self.git('push', '-q', 'origin', 'main')
        # Branch checked out nowhere else: sync must make its own worktree.
        self.git('checkout', '-q', 'main')

    def test_tracker_conflict_keeps_both_sides(self):
        self.diverge('head\n- [x] first\n- [x] from feature\n', 'head\n- [x] first\n- [x] from main\n')
        result = self.sync()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        worktree = Path(f'{self.repo}-sync-feature')
        merged = (worktree / TRACKER).read_text()
        self.assertIn('from feature', merged)
        self.assertIn('from main', merged)
        self.assertNotIn('<<<<<<<', merged)
        self.assertNotIn('=======', merged)
        # A merge commit with both parents.
        parents = subprocess.check_output(['git', 'rev-list', '--parents', '-1', 'HEAD'], cwd=worktree, text=True).split()
        self.assertEqual(len(parents), 3)

    def test_code_conflict_stops_with_the_merge_in_progress(self):
        self.diverge('head\n- [x] first\n', 'head\n- [x] first\n', 'feature\n', 'main\n')
        result = self.sync()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn('code.txt', result.stdout)
        worktree = Path(f'{self.repo}-sync-feature')
        self.assertTrue((worktree / '.git').exists())
        status = subprocess.check_output(['git', 'status', '--porcelain'], cwd=worktree, text=True)
        self.assertIn('UU code.txt', status)

    def test_a_branch_that_already_contains_the_base_is_left_alone(self):
        self.git('push', '-q', '-u', 'origin', 'feature')
        self.git('checkout', '-q', 'main')
        result = self.sync()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('already contains', result.stdout)

    def test_the_base_section_of_a_three_way_conflict_is_dropped(self):
        self.git('config', 'merge.conflictstyle', 'diff3')
        self.diverge('head\n- [x] feature edit\n', 'head\n- [x] main edit\n')
        result = self.sync()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        merged = (Path(f'{self.repo}-sync-feature') / TRACKER).read_text()
        self.assertNotIn('- [x] first', merged)
        self.assertIn('feature edit', merged)
        self.assertIn('main edit', merged)


if __name__ == '__main__':
    unittest.main()
