"""Execute selected Pong scenarios and validate current gameplay observations.

Run desktop scenarios through tools/compat-display/run.py.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
from driver import ROOT
from verify import verify_record


def execute(command, output, log):
    with output.open('x') as out, log.open('x') as err:
        process = subprocess.Popen(command, cwd=ROOT, stdout=out, stderr=err,
                                   start_new_session=True)
        try:
            status = process.wait(timeout=330)
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
    if status:
        raise RuntimeError(f'harness exited {status}: {log}')


def compare_pixels(left, right):
    from PIL import Image, ImageChops
    assert [capture['path'] for capture in left['captures']] == [capture['path'] for capture in right['captures']]
    for capture in left['captures']:
        with Image.open(ROOT / left['run_dir'] / capture['path']) as a:
            with Image.open(ROOT / right['run_dir'] / capture['path']) as b:
                assert a.size == b.size
                assert not ImageChops.difference(a.convert('RGB'), b.convert('RGB')).getbbox(), capture['path']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture', type=Path, default=ROOT / 'fixtures/pong-journey.json')
    parser.add_argument('--harness', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--profile', default='compat-2017.1')
    parser.add_argument('--desktop', action='store_true')
    parser.add_argument('--matrix', action='store_true')
    args = parser.parse_args()
    variants = ([(profile, desktop) for profile in ['compat-2017.1', 'compat-6000.3']
                 for desktop in [False, True]] + [('source-2017.1', False)]
                if args.matrix else [(args.profile, args.desktop)])
    if any(desktop for _, desktop in variants):
        if not os.environ.get('COMPAT_DISPLAY_CONTAINMENT') or not os.environ.get('DISPLAY'):
            parser.error('desktop replay requires tools/compat-display/run.py')
    args.output.mkdir(parents=True, exist_ok=True)
    completed = {}
    for profile, desktop in variants:
        label = profile + ('-desktop' if desktop else '-offscreen')
        output = args.output / (label + '.json')
        command = [str(args.harness.resolve()), 'run', str(args.fixture.resolve()), '--profile', profile]
        if desktop:
            command.append('--desktop')
        execute(command, output, args.output / (label + '.log'))
        result = verify_record(output, args.fixture)
        completed[profile, desktop] = json.loads(output.read_text())
        if desktop and (profile, False) in completed:
            compare_pixels(completed[profile, False], completed[profile, True])
        print(json.dumps(dict(profile=profile, desktop=desktop, run_id=result['run_id'], status='pass')))


if __name__ == '__main__':
    main()
