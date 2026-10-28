#!/usr/bin/env python3
"""Native normal-build watcher acceptance using isolated stdio fixtures.

Build with cargo build --locked -p mct-mcp-server --bin mct-mcp-server,
not cargo test, so macOS dev-dependency kqueue is not unified into the binary.
Output is TOON. Backend comes from this exact child's startup log.
"""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import platform
import tempfile
import time

spec = importlib.util.spec_from_file_location('phase9_profile', Path(__file__).with_name('phase9-profile.py'))
profile = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profile)
Client, stats = profile.Client, profile.stats


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', required=True, type=Path)
    parser.add_argument('--rounds', type=int, default=10)
    parser.add_argument('--reconcile-seconds', type=int, default=600)
    parser.add_argument('--fallback-only', action='store_true', help='one missed-event recovery check; waits up to interval+8 seconds')
    args = parser.parse_args()
    if not 1 <= args.rounds <= 100 or not 60 <= args.reconcile_seconds <= 3600:
        parser.error('rounds must be 1..100; reconciliation must be 60..3600 seconds')
    binary = (args.bin_dir / ('mct-mcp-server.exe' if platform.system() == 'Windows' else 'mct-mcp-server')).resolve()
    print('environment:', platform.platform(), flush=True)
    print('binary_sha256:', hashlib.sha256(binary.read_bytes()).hexdigest(), flush=True)
    print('reconciliation_seconds:', args.reconcile_seconds, flush=True)
    latencies = []
    failures = []
    with tempfile.TemporaryDirectory(prefix='mct-phase91-watch-') as folder:
        root = Path(folder).resolve()
        source = root / 'lib.rs'
        source.write_text('pub fn initial_marker() {}\n')
        with tempfile.TemporaryFile(mode='w+t') as diagnostics:
            client = Client(binary, root, ('--no-obsidian', '--reindex-delay', '100ms',
                            '--reconcile-interval-seconds', str(args.reconcile_seconds)), diagnostics)
            def found(name):
                text = client.tool('find_symbol', dict(name=name, format='text'))
                return '] function ' + name in text
            def await_state(name, expected):
                started = time.monotonic()
                deadline = started + (args.reconcile_seconds + 8 if args.fallback_only else 8)
                while time.monotonic() < deadline:
                    if found(name) == expected:
                        latencies.append((time.monotonic() - started) * 1000)
                        return True
                    # Bounded polling for external event delivery, not a readiness sleep.
                    time.sleep(.025)
                failures.append(name + ': expected ' + str(expected))
                return False
            try:
                client.request('initialize', dict(protocolVersion='2024-11-05', capabilities={},
                               clientInfo=dict(name='phase91-native-watcher', version='1')))
                client.send(dict(jsonrpc='2.0', method='notifications/initialized'))
                assert found('initial_marker'), 'startup snapshot missing'
                if args.fallback_only:
                    (root / 'fallback.rs').write_text('pub fn fallback_marker() {}\n')
                    if await_state('fallback_marker', True):
                        # The timer starts with the server, not the write, so use
                        # the 8 s event bound: only a later arrival proves the
                        # reconciliation fallback rather than native delivery.
                        path = 'event' if latencies[-1] < 8000 else 'reconciliation'
                        print('recovery_path:', path, flush=True)
                    print('hybrid_recovery:', 'FAIL' if failures else 'PASS', flush=True)
                else:
                    for iteration in range(args.rounds):
                        name = 'created_' + str(iteration)
                        created = root / ('added_' + str(iteration) + '.rs')
                        created.write_text('pub fn ' + name + '() {}\n')
                        if not await_state(name, True):
                            break
                        changed = 'modified_' + str(iteration)
                        created.write_text('pub fn ' + changed + '() {}\n')
                        if not await_state(changed, True) or not await_state(name, False):
                            break
                        renamed = root / ('renamed_' + str(iteration) + '.rs')
                        created.rename(renamed)
                        deadline = time.monotonic() + 8
                        while time.monotonic() < deadline:
                            text = client.tool('find_symbol', dict(name=changed, format='text'))
                            if renamed.name in text and created.name not in text:
                                break
                            time.sleep(.025)
                        else:
                            failures.append('rename did not update path')
                            break
                        renamed.unlink()
                        if not await_state(changed, False):
                            break
                        print('round_' + str(iteration + 1) + ': PASS', flush=True)
                    if not failures:
                        for iteration in range(64):
                            (root / ('burst_' + str(iteration) + '.rs')).write_text(
                                'pub fn burst_' + str(iteration) + '() {}\n')
                        for iteration in range(64):
                            if not await_state('burst_' + str(iteration), True):
                                break
                        print('burst_64:', 'FAIL' if failures else 'PASS', flush=True)
            except Exception as error:
                failures.append(type(error).__name__ + ": " + str(error))
            finally:
                client.close()
                diagnostics.seek(0)
                lines = diagnostics.readlines()
                backends = [line.strip() for line in lines if 'filesystem watcher started' in line]
                print('backend_diagnostic:', backends[-1] if backends else 'UNAVAILABLE', flush=True)
                if failures:
                    # Only the disposable fixture is logged, never a user source/token.
                    print('diagnostic_tail: |')
                    for line in lines[-20:]:
                        print('  ' + line.rstrip())
    if latencies:
        median, p95, variance = stats(latencies)
        print(f'observation_ms: samples={len(latencies)},median={median:.3f},p95={p95:.3f},variance={variance:.3f}')
    print('failures:', len(failures))
    for failure in failures:
        print('failure:', failure)
    raise SystemExit(bool(failures))


if __name__ == '__main__':
    main()
