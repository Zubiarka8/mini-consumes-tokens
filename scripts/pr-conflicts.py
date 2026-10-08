#!/usr/bin/env python3
"""Diagnose open PRs and optionally prepare one isolated merge for review."""

import argparse
from collections import Counter
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys
from urllib.parse import urlsplit


def run(args, cwd, allowed=(0,)):
    result = subprocess.run(args, cwd=cwd, text=True, capture_output=True)
    if result.returncode not in allowed:
        raise RuntimeError(f"{' '.join(args)}: {result.stderr.strip() or result.stdout.strip()}")
    return result


def merge_conflicts(root, base, head):
    result = run(
        ['git', 'merge-tree', '--write-tree', '--name-only', '-z', base, head],
        root, allowed=(0, 1),
    )
    fields = result.stdout.split('\0')
    tree = fields[0]
    paths = []
    for field in fields[1:]:
        if not field:
            break
        paths.append(field)
    if len(tree) != 40 and len(tree) != 64:
        raise RuntimeError('git merge-tree returned no valid tree')
    return tree, paths


def checks_summary(checks):
    counts = Counter()
    for check in checks:
        state = check.get('conclusion') or check.get('state') or check.get('status')
        if state in ('SUCCESS', 'NEUTRAL', 'SKIPPED'):
            counts['passed'] += 1
        elif state in ('FAILURE', 'ERROR', 'TIMED_OUT', 'CANCELLED', 'ACTION_REQUIRED', 'STALE'):
            counts['failed'] += 1
        else:
            counts['pending'] += 1
    return dict(counts)


def repository_name(url):
    if '://' in url:
        parsed = urlsplit(url)
        host, path = parsed.hostname, parsed.path.strip('/')
    elif ':' in url and '@' in url.split(':', 1)[0]:
        server, path = url.split(':', 1)
        host = server.split('@', 1)[1]
    else:
        raise RuntimeError('origin must be a GitHub SSH or HTTPS URL')
    path = path.removesuffix('.git')
    if not host or len(path.split('/')) != 2:
        raise RuntimeError('origin must identify one owner/repository')
    return f'{host}/{path}'


def inspect_pr(root, pr):
    number = pr['number']
    if pr.get('isCrossRepository'):
        raise RuntimeError(f'PR #{number}: fork head is unsupported; fetch and review it separately')
    base = run(['git', 'rev-parse', '--verify', f"refs/remotes/origin/{pr['baseRefName']}^{{commit}}"], root).stdout.strip()
    head = run(['git', 'rev-parse', '--verify', f"refs/remotes/origin/{pr['headRefName']}^{{commit}}"], root).stdout.strip()
    if head != pr['headRefOid']:
        raise RuntimeError(f'PR #{number}: fetched head differs from GitHub; rerun after fetching')
    tree, conflicts = merge_conflicts(root, base, head)
    base_tree = run(['git', 'rev-parse', f'{base}^{{tree}}'], root).stdout.strip()
    behind = int(run(['git', 'rev-list', '--count', f'{head}..{base}'], root).stdout)
    return {
        'number': number, 'base': base, 'head': head,
        'base_branch': pr['baseRefName'], 'head_branch': pr['headRefName'],
        'state': 'conflicting' if conflicts else ('redundant' if tree == base_tree else 'mergeable'),
        'behind': behind, 'conflicts': conflicts,
        'checks': checks_summary(pr.get('statusCheckRollup') or []), 'url': pr['url'],
    }


def prepare(root, report, path):
    if report['state'] == 'redundant':
        raise RuntimeError('PR adds nothing to its base; review closure instead of preparing another merge')
    path = path.resolve()
    if path.exists():
        raise RuntimeError(f'worktree path already exists: {path}')
    branch = f"codex/resolve-pr-{report['number']}-{report['head'][:8]}"
    run(['git', 'worktree', 'add', '-b', branch, str(path), report['head']], root)
    result = run(['git', 'merge', '--no-commit', '--no-ff', report['base']], path, allowed=(0, 1))
    unresolved = run(['git', 'diff', '--name-only', '--diff-filter=U', '-z'], path).stdout
    if result.returncode and not unresolved:
        raise RuntimeError(f'merge failed in {path}: {result.stderr.strip() or result.stdout.strip()}')
    print(f'prepared: {branch} in {path}')
    print('next: review and resolve files, run scripts/unix/check.sh (Windows: scripts/windows/check.ps1), then commit the merge')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--no-fetch', action='store_true', help='use existing origin refs; still query GitHub')
    parser.add_argument('--pr', type=int, action='append', help='inspect only this open PR (repeatable)')
    parser.add_argument('--prepare', type=int, help='prepare this PR in a NEW isolated worktree; never commit or push')
    parser.add_argument('--worktree', type=Path, help='new worktree path, required with --prepare')
    args = parser.parse_args(argv)
    if bool(args.prepare) != bool(args.worktree):
        parser.error('--prepare and --worktree must be provided together')
    root = Path(__file__).resolve().parent.parent
    repo = repository_name(run(['git', 'remote', 'get-url', 'origin'], root).stdout.strip())
    if not args.no_fetch:
        run(['git', 'fetch', 'origin', '--quiet'], root)
    fields = 'number,url,headRefName,headRefOid,baseRefName,isCrossRepository,statusCheckRollup'
    prs = json.loads(run(['gh', 'pr', 'list', '--repo', repo, '--state', 'open', '--limit', '1000', '--json', fields], root).stdout)
    wanted = set(args.pr or []) | ({args.prepare} if args.prepare else set())
    if wanted:
        missing = wanted - {pr['number'] for pr in prs}
        if missing:
            raise RuntimeError(f'open PRs not found: {sorted(missing)}')
        prs = [pr for pr in prs if pr['number'] in wanted]
    reports = [inspect_pr(root, pr) for pr in prs]
    logdir = root / 'target' / 'script-logs'
    logdir.mkdir(parents=True, exist_ok=True)
    log = logdir / 'pr-conflicts.json'
    log.write_text(json.dumps({'checked_at': datetime.now(timezone.utc).isoformat(), 'prs': reports}, indent=2) + '\n', encoding='utf-8')
    print('pull_requests:')
    overlap = Counter()
    for report in reports:
        checks = report['checks']
        print(f"  #{report['number']}: {report['state']}, behind {report['behind']}, conflicts {len(report['conflicts'])}; CI {checks.get('passed', 0)} passed/{checks.get('failed', 0)} failed/{checks.get('pending', 0)} pending")
        for path in report['conflicts']:
            print(f'    {json.dumps(path, ensure_ascii=False)}')
        overlap.update(report['conflicts'])
    for path, count in overlap.most_common():
        if count > 1:
            print(f'shared_conflict: {json.dumps(path, ensure_ascii=False)} ({count} PRs; sequence their integration)')
    print(f'summary: {len(reports)} open, {sum(r["state"] == "conflicting" for r in reports)} conflicting, {sum(r["state"] == "redundant" for r in reports)} redundant')
    print('log: target/script-logs/pr-conflicts.json')
    if args.prepare:
        prepare(root, next(r for r in reports if r['number'] == args.prepare), args.worktree)
    return int(any(r['state'] == 'conflicting' for r in reports))


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (OSError, ValueError, RuntimeError) as error:
        print(f'error: {error}', file=sys.stderr)
        sys.exit(2)
