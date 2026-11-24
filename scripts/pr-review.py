#!/usr/bin/env python3
"""Prepare and finish an existing GitHub PR in an isolated worktree."""
import argparse
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import quote


def run(*args, cwd=None, capture=True):
    result = subprocess.run(args, cwd=cwd, text=True, stdout=subprocess.PIPE if capture else None,
                            stderr=subprocess.PIPE if capture else None)
    if result.returncode:
        detail = (result.stderr or '').strip()
        raise RuntimeError(f"{' '.join(args)} failed" + (f': {detail}' if detail else ''))
    return result.stdout.strip() if capture else ''


def metadata(repo, number):
    fields = 'state,baseRefName,baseRefOid,headRefName,headRefOid,headRepository'
    query = '[.state,.baseRefName,.baseRefOid,.headRefName,.headRefOid,(.headRepository.nameWithOwner // "")] | .[]'
    values = run('gh', 'pr', 'view', str(number), '--repo', repo, '--json', fields, '--jq', query).splitlines()
    if len(values) != 6:
        raise RuntimeError('could not read PR metadata; the head repository may have been deleted')
    state, base, base_oid, head, head_oid, head_repo = values
    if state != 'OPEN':
        raise RuntimeError(f'PR #{number} is {state}, not open')
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', head_repo):
        raise RuntimeError('invalid or missing head repository')
    for oid in (base_oid, head_oid):
        if not re.fullmatch(r'[0-9a-f]{40,64}', oid):
            raise RuntimeError('invalid commit ID returned by GitHub')
    for ref in (base, head):
        run('git', 'check-ref-format', '--branch', ref)
    # The PR's baseRefOid can lag behind the actual branch ref for stacked PRs.
    base_oid = run('gh', 'api', f'repos/{repo}/git/ref/heads/{quote(base, safe="/")}',
                   '--jq', '.object.sha')
    if not re.fullmatch(r'[0-9a-f]{40,64}', base_oid):
        raise RuntimeError('invalid base branch commit ID returned by GitHub')
    return base, base_oid, head, head_oid, head_repo


def fetch_base(root, base, expected):
    run('git', 'fetch', 'origin', f'refs/heads/{base}', cwd=root, capture=False)
    if run('git', 'rev-parse', 'FETCH_HEAD', cwd=root) != expected:
        raise RuntimeError('PR base changed while fetching; retry')


def clean(path):
    return not run('git', 'status', '--porcelain=v1', '--untracked-files=all', cwd=path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('start', 'status', 'push', 'cleanup'))
    parser.add_argument('number', type=int, help='open PR number')
    parser.add_argument('--worktree', help='isolated worktree path (default: sibling <checkout>-pr-<number>)')
    parser.add_argument('--dry-run', action='store_true', help='show actions without fetching, creating, pushing, or removing')
    args = parser.parse_args()
    if args.number <= 0:
        parser.error('PR number must be positive')
    root = Path(run('git', 'rev-parse', '--show-toplevel')).resolve()
    repo = run('gh', 'repo', 'view', '--json', 'nameWithOwner', '--jq', '.nameWithOwner', cwd=root)
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repo):
        raise RuntimeError('invalid repository returned by GitHub')
    path = Path(args.worktree).expanduser().absolute() if args.worktree else root.parent / f'{root.name}-pr-{args.number}'
    base, base_oid, head, head_oid, head_repo = metadata(repo, args.number)
    print(f'PR #{args.number}: {repo}  {base}@{base_oid[:12]} <- {head_repo}:{head}@{head_oid[:12]}', flush=True)
    print(f'worktree: {path}', flush=True)
    if args.action == 'start':
        if path.exists():
            raise RuntimeError('worktree path already exists; choose another path')
        print('Fetch PR head and base, verify their commits, then create a detached worktree.', flush=True)
        if args.dry_run:
            return
        run('git', 'fetch', 'origin', f'pull/{args.number}/head', cwd=root, capture=False)
        fetched = run('git', 'rev-parse', 'FETCH_HEAD', cwd=root)
        if fetched != head_oid:
            raise RuntimeError('PR head changed while fetching; retry')
        fetch_base(root, base, base_oid)
        run('git', 'worktree', 'add', '--detach', str(path), head_oid, cwd=root, capture=False)
        git_dir = Path(run('git', 'rev-parse', '--absolute-git-dir', cwd=path))
        (git_dir / 'mct-pr-review').write_text(f'{repo}\n{args.number}\n{head_repo}\n{head}\n{head_oid}\n', encoding='utf-8')
        run('git', 'diff', '--stat', f'{base_oid}...{head_oid}', cwd=path, capture=False)
        print('Review the full diff, run relevant checks, and commit only intentional changes.')
        return
    if not path.is_dir():
        raise RuntimeError('worktree path does not exist')
    if Path(run('git', 'rev-parse', '--show-toplevel', cwd=path)).resolve() != path.resolve():
        raise RuntimeError('path is not the root of a worktree')
    if run('git', 'rev-parse', '--git-common-dir', cwd=path) != run('git', 'rev-parse', '--git-common-dir', cwd=root):
        # Git may print relative paths for the main checkout.
        common = lambda p: (p / run('git', 'rev-parse', '--git-common-dir', cwd=p)).resolve()
        if common(path) != common(root):
            raise RuntimeError('worktree belongs to another repository')
    worktrees = run('git', 'worktree', 'list', '--porcelain', cwd=root).splitlines()
    primary = Path(worktrees[0][len('worktree '):]).resolve()
    if path.resolve() == primary:
        raise RuntimeError('refusing to operate on the primary checkout')
    git_dir = Path(run('git', 'rev-parse', '--absolute-git-dir', cwd=path))
    marker = git_dir / 'mct-pr-review'
    if not marker.is_file():
        raise RuntimeError('worktree was not created by pr-review')
    data = marker.read_text(encoding='utf-8').splitlines()
    if len(data) != 5 or data[:4] != [repo, str(args.number), head_repo, head]:
        raise RuntimeError('worktree does not belong to this PR or its head branch changed')
    symbolic = subprocess.run(['git', 'symbolic-ref', '-q', 'HEAD'], cwd=path,
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if symbolic.returncode != 1:
        raise RuntimeError('worktree is no longer detached')
    local_head = run('git', 'rev-parse', 'HEAD', cwd=path)
    run('git', 'merge-base', '--is-ancestor', data[4], local_head, cwd=path)
    print(f'local HEAD: {local_head[:12]}', flush=True)
    if args.action == 'status':
        if not args.dry_run:
            fetch_base(root, base, base_oid)
        run('git', 'status', '--short', '--branch', cwd=path, capture=False)
        run('git', 'log', '--oneline', f'{base_oid}..HEAD', cwd=path, capture=False)
        run('git', 'diff', '--stat', f'{base_oid}...HEAD', cwd=path, capture=False)
        run('git', 'diff', '--stat', 'HEAD', cwd=path, capture=False)
        run('gh', 'pr', 'view', str(args.number), '--repo', repo, '--json',
            'mergeStateStatus,statusCheckRollup', '--jq',
            'def outcome: (.conclusion // .state // ""); '
            'def failed: outcome as $o | ["FAILURE","ERROR","TIMED_OUT","ACTION_REQUIRED"] | index($o) != null; '
            r'"merge: \(.mergeStateStatus); checks: \([.statusCheckRollup[]? | select(outcome == "SUCCESS")] | length) passed, '
            r'\([.statusCheckRollup[]? | select(failed)] | length) failed, '
            r'\([.statusCheckRollup[]? | select((outcome == "SUCCESS" or failed) | not)] | length) pending"',
            cwd=root, capture=False)
        return
    if not clean(path):
        raise RuntimeError('worktree has staged, unstaged or untracked changes; preserve them before continuing')
    if args.action == 'cleanup':
        if run('git', 'status', '--porcelain=v1', '--ignored', '--untracked-files=all', cwd=path):
            raise RuntimeError('worktree contains ignored files; preserve or remove them before cleanup')
        if local_head != head_oid:
            raise RuntimeError('local HEAD differs from the current PR head; work may not be published')
        print(f'Remove clean worktree {path}', flush=True)
        if not args.dry_run:
            run('git', 'worktree', 'remove', str(path), cwd=root, capture=False)
        return
    if local_head == head_oid:
        print('No new commits to push.')
        return
    run('git', 'merge-base', '--is-ancestor', head_oid, local_head, cwd=path)
    url = f'https://github.com/{head_repo}.git'
    print(f'Push {local_head[:12]} to {head_repo}:{head} (fast-forward only).', flush=True)
    if not args.dry_run:
        # An ordinary push rejects a concurrent remote update; no force or lease.
        run('git', 'push', url, f'HEAD:refs/heads/{head}', cwd=path, capture=False)


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, OSError) as error:
        print(f'pr-review: {error}', file=sys.stderr)
        sys.exit(2)
