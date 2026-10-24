#!/usr/bin/env python3
"""Recheck qualification immediately before the refresh owner adopts a candidate.

This command reads candidate sources and emits evidence. It never changes the
active source, Git refs, or refresh locks. The supervisor owns leases and adoption.
"""
import argparse
import json
from pathlib import Path
import sys
import subprocess

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'fixtures/gates'))
from candidate import verify_content, verify_identity
from qualification import digest, inspect_assembly, verify_sources


def check(root, fixture, proof, output, executable):
    source = Path(proof['source_root'])
    verify_identity(source, proof['local_identity'])
    verify_content(source, fixture, output, executable, proof['content_receipt'])
    selection = list(proof['source_hashes'])
    verify_sources(source, selection, proof['source_hashes'])
    if proof['source_count'] != len(selection):
        raise RuntimeError('qualification source count mismatch')
    assembly = Path(proof['assembly'])
    if digest(assembly) != proof['assembly_sha256']:
        raise RuntimeError('qualified assembly changed before adoption')
    if inspect_assembly(root, output, assembly) != proof['metadata']:
        raise RuntimeError('qualified assembly metadata changed before adoption')
    verify_content(source, fixture, output, executable, proof['content_receipt'])
    verify_identity(source, proof['local_identity'])
    verify_sources(source, selection, proof['source_hashes'])
    if digest(assembly) != proof['assembly_sha256']:
        raise RuntimeError('qualified assembly changed during adoption check')
    return {'status': 'pass', 'revision': proof['local_identity']['head'],
            'closure_sha256': proof['content_receipt']['closure_sha256'],
            'source_count': proof['source_count'], 'assembly_sha256': proof['assembly_sha256']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('fixture', type=Path)
    parser.add_argument('proof', type=Path)
    parser.add_argument('--receipt', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(ROOT / '.work')
    output.mkdir(parents=True, exist_ok=True)
    result = check(ROOT, json.loads(args.fixture.read_text()), json.loads(args.proof.read_text()), output, args.receipt)
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        print('refresh content check rejected: ' + str(error), file=sys.stderr)
        sys.exit(2)
