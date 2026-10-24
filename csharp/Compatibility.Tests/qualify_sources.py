#!/usr/bin/env python3
"""Discover, hash, compile and verify the actual original C# compiler inputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(os.environ.get('DISUNITY_TEST_MANAGED_ROOT', ROOT / '.work/managed'))
COUNTS = {'Snake': 8, 'Breakout': 7, 'Pong': 20}


def discover():
    games = {}
    for name, expected in COUNTS.items():
        root = ROOT / '.work/games' / name.lower()
        paths = sorted(p for p in (root / 'Assets').rglob('*.cs')
                       if not {'Tests', 'Editor', 'obj', 'bin'}.intersection(p.relative_to(root).parts))
        if len(paths) != expected:
            raise RuntimeError(f'{name}: expected {expected} actual sources, discovered {len(paths)}')
        games[name] = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
    return games


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--verify', action='store_true', help='fail on drift against recorded census')
    args = parser.parse_args()
    if os.environ.get('RD_WORKTREE') != str(ROOT):
        raise RuntimeError('source .session/env.sh before running qualification')
    OUT.mkdir(parents=True, exist_ok=True)
    games = discover()
    census = OUT / 'source-census.json'
    if args.verify and json.loads(census.read_text())['sources'] != games:
        raise RuntimeError('original source hash drift: requalify changed sources explicitly')
    for name in COUNTS:
        command = ['dotnet', 'build', str(ROOT / 'csharp/Compatibility.Tests/OriginalGames' / f'{name}.csproj'),
                   '--artifacts-path', str(OUT / 'artifacts'), '-m:2']
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        (OUT / f'compile-{name}.log').write_text(result.stdout)
        if result.returncode:
            raise RuntimeError(f'{name} compile failed; see compile-{name}.log')
        manifest = OUT / 'artifacts/obj' / name / 'debug/original-sources.txt'
        included = {str(Path(p).resolve().relative_to(ROOT / '.work/games' / name.lower()))
                    for p in manifest.read_text(encoding='utf-8-sig').splitlines()}
        if included != set(games[name]):
            raise RuntimeError(f'{name}: compiler inputs differ from discovered sources: {included ^ set(games[name])}')
        print(f'PASS {name}: {len(included)} discovered, hashed and actual compiler inputs')
    if games != discover():
        raise RuntimeError('original sources changed during compilation')
    census.write_text(json.dumps({'total': sum(map(len, games.values())), 'sources': games,
                                  'compiler_inputs_verified': True}, indent=2) + '\n')
    print('PASS aggregate: 35 actual original sources')


if __name__ == '__main__':
    main()
