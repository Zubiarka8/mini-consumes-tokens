#!/usr/bin/env python3
"""Check a built dashboard's HTTP asset delivery on a disposable fixture.

Starts `mct-cli web` from --bin-dir on a temporary project, compares the served
CSS/JS/Alpine/sprite with the checkout's source files, records the security
headers and two rejected writes, then stops only its own server. The private
link's token is never printed. Output is TOON; exit status 1 on any mismatch.
"""
import argparse
import hashlib
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

HEADERS = ('Content-Security-Policy', 'X-Content-Type-Options', 'Referrer-Policy',
           'X-Frame-Options', 'Cache-Control', 'Cross-Origin-Resource-Policy')


def request(url, method='GET', data=None, headers=None):
    req = urllib.request.Request(url, method=method, data=data, headers=headers or {})
    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            return response.status, response.headers, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.headers, error.read()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', required=True, type=Path)
    args = parser.parse_args()
    spa = Path(__file__).resolve().parent.parent / 'crates/mct-cli/src/web/spa'
    binary = (args.bin_dir / ('mct-cli.exe' if os.name == 'nt' else 'mct-cli')).resolve(strict=True)
    print('binary_sha256:', hashlib.sha256(binary.read_bytes()).hexdigest(), flush=True)
    failures = []
    with tempfile.TemporaryDirectory(prefix='mct-phase91-http-') as folder:
        root = Path(folder).resolve()
        body = '(v: i64) -> i64 {\n let mut t = v;\n t += 1;\n t *= 2;\n t -= 4;\n if t > 12 { t -= 3; }\n t += 8;\n t\n}\n'
        (root / 'copies.rs').write_text('pub fn alpha' + body + 'pub fn beta' + body)
        subprocess.run([str(binary), '--root', str(root), 'init'], check=True, capture_output=True)
        child = subprocess.Popen([str(binary), '--root', str(root), 'web', '--port', '0', '--no-obsidian'],
                                 stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        try:
            base = None
            deadline = time.monotonic() + 60
            while base is None and time.monotonic() < deadline:
                line = child.stdout.readline()
                if not line:
                    break
                found = re.search(r'http://(127\.0\.0\.1|localhost):(\d+)/([^/\s]+)/', line)
                if found:
                    base = found.group(0)
                    origin = 'http://' + found.group(1) + ':' + found.group(2)
            if base is None:
                raise SystemExit('server printed no private link')
            print('link_shape:', origin + '/<redacted>/', flush=True)
            sources = {'spa.css': spa / 'spa.css', 'spa.js': spa / 'spa.js', 'alpine.js': spa / 'vendor/alpine.js'}
            print(f'assets[{len(sources)}]{{route,status,content_type,bytes,identical,ms}}:')
            for route, source in sources.items():
                started = time.perf_counter()
                status, headers, payload = request(base + route)
                elapsed = (time.perf_counter() - started) * 1000
                identical = payload == source.read_bytes()
                if status != 200 or not identical:
                    failures.append(route)
                print(f'  {route},{status},"{headers.get("Content-Type")}",{len(payload)},{identical},{elapsed:.3f}')
            status, headers, payload = request(base + 'app')
            shell = payload.decode('utf-8', 'replace')
            sprite = (spa / 'sprite.svg').read_text() in shell
            csrf = re.search(r'name="csrf-token" content="([^"]+)"', shell)
            if status != 200 or not sprite or '__CSRF__' in shell:
                failures.append('app shell')
            print(f'app: status={status},sprite_inlined={sprite},csrf_substituted={"__CSRF__" not in shell}')
            print('headers:')
            for name in HEADERS:
                value = headers.get(name)
                if value is None:
                    failures.append('missing ' + name)
                print(f'  {name}: "{value}"')
            wrong = re.sub(r'/[^/]+/$', '/not-the-token/', base)
            status, _, _ = request(wrong + 'spa.css')
            if status != 404:
                failures.append('wrong token served')
            print('wrong_token_status:', status)
            json_headers = {'Content-Type': 'application/json', 'Origin': origin}
            status, _, _ = request(base + 'api/comment', 'POST', b'{}', json_headers)
            if status != 403:
                failures.append('write without CSRF')
            print('post_without_csrf_status:', status)
            if csrf:
                foreign = dict(json_headers, Origin='http://evil.example', **{'X-CSRF-Token': csrf.group(1)})
                status, _, _ = request(base + 'api/comment', 'POST', b'{}', foreign)
                if status != 403:
                    failures.append('cross-origin write')
                print('post_cross_origin_status:', status)
            else:
                print('post_cross_origin_status: SKIPPED (no csrf meta tag found)')
        finally:
            child.terminate()
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
    print('failures:', len(failures))
    for failure in failures:
        print('failure:', failure)
    raise SystemExit(bool(failures))


if __name__ == '__main__':
    main()
