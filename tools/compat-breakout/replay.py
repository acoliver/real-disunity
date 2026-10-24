"""Execute frozen fresh-compile keyboard fixtures on the canonical GPU player."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from driver import ROOT, WORK, dump
from verify import verify_record, native_identity


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('fixture', type=Path)
    parser.add_argument('--profile', choices=['compat-2017.1', 'compat-6000.3'])
    parser.add_argument('--desktop', action='store_true')
    parser.add_argument('--matrix', action='store_true')
    args = parser.parse_args()
    fixture = args.fixture.resolve()
    if fixture.parent != ROOT / 'fixtures' or not fixture.name.startswith('breakout-journey'):
        raise ValueError('only owned Breakout journey fixtures can be replayed')
    fingerprint = hashlib.sha256(fixture.read_bytes()).hexdigest()
    variants = [(p, d) for p in ['compat-2017.1', 'compat-6000.3'] for d in [False, True]] if args.matrix else [(args.profile or 'compat-6000.3', args.desktop)]
    output = WORK / 'replays' / fixture.stem
    output.mkdir(parents=True, exist_ok=True)
    for profile, desktop in variants:
        label = profile + ('-desktop' if desktop else '-offscreen')
        target = output / (label + '.json')
        command = [str(ROOT / '.work/target/debug/compat-kit'), 'run', str(fixture), '--profile', profile]
        if desktop:
            command += ['--desktop']
        env = dict(os.environ, DISPLAY=':97', WGPU_BACKEND='vulkan',
                   VK_ICD_FILENAMES='/usr/share/vulkan/icd.d/lvp_icd.json')
        with target.open('w') as out, (output / (label + '.log')).open('w') as err:
            subprocess.run(command, cwd=ROOT, env=env, stdout=out, stderr=err, timeout=130, check=True)
        assert hashlib.sha256(fixture.read_bytes()).hexdigest() == fingerprint
        evidence = verify_record(target, fixture)
        record = json.loads(target.read_text())
        identity = native_identity((ROOT / record['run_dir']).resolve())
        evidence.update(fixture_sha256=fingerprint, desktop=desktop,
                        native_library_sha256=identity['sha256'], native_image=identity)
        dump(output / (label + '-verified.json'), evidence)
        print(label, 'PASS finite prefix', evidence['run_id'], flush=True)


if __name__ == '__main__':
    main()
