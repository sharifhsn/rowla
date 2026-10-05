#!/usr/bin/env python3
"""Fail packaging when dependency or toolchain notices need refreshing."""
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output(
    ['cargo', 'metadata', '--locked', '--format-version', '1'],
    cwd=ROOT, text=True,
))
expected = {(p['name'], p['version'], p['license'])
            for p in metadata['packages'] if p['name'] != 'taskbar-rs'}
notice = (ROOT / 'THIRD_PARTY_NOTICES.md').read_text()
recorded = set(re.findall(r'^\| ([^|]+) \| ([^|]+) \| ([^|]+) \|$', notice, re.M))
recorded.discard(('Component', 'Version', 'Declared license'))
recorded.discard(('---', '---', '---'))
if expected != recorded:
    sys.exit('Refresh THIRD_PARTY_NOTICES.md for the locked dependencies. '
             f'Missing/changed: {sorted(expected - recorded)}; obsolete: {sorted(recorded - expected)}')
version = subprocess.check_output(['rustc', '--version'], cwd=ROOT, text=True).split()[1]
rust_notices = ROOT / 'licenses' / f'Rust-{version}-COPYRIGHT-library.html'
if not rust_notices.is_file() or rust_notices.stat().st_size < 1000:
    sys.exit(f'Include the standard-library notices for the actual Rust {version} toolchain.')
if any('MIT' not in p[2] for p in expected):
    sys.exit('A dependency no longer offers MIT; review and refresh selected notices.')
if 'UNICODE LICENSE V3' not in notice:
    sys.exit('The additional unicode-ident data license must be preserved.')
print(f'Checked notices for {len(expected)} locked dependencies and Rust {version}.')
