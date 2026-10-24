"""Replay the full journey with fresh compilation and measured runtime identities."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
from driver import ROOT, WORK
from verify import validate_native_image_proof, verify_record, verify_runtime

IMAGES = {
    'player': ROOT / '.work/target/debug/compat-player',
    'native': ROOT / '.work/target/debug/libworldrunner_disunity_ffi.so',
    'harness': ROOT / '.work/target/debug/compat-kit',
}


def identities():
    return {name: dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())
            for name, path in IMAGES.items() if name != 'native'}


def sample_worker(pid, observed):
    """Read the harness process tree and loaded image paths without changing the worker."""
    try:
        proc = Path('/proc') / str(pid)
        children = (proc / 'task' / str(pid) / 'children').read_text().split()
        for child in children:
            sample_worker(int(child), observed)
        if str(pid) in observed:
            return
        if (proc / 'exe').resolve() != IMAGES['player']:
            return
        environment = dict(item.split('=', 1) for item in (proc / 'environ').read_bytes().decode().split('\0') if '=' in item)
        artifacts = Path(environment['COMPAT_ARTIFACTS'])
        proof_path = artifacts / 'build/native-image.json'
        proof = validate_native_image_proof(proof_path, artifacts)
        image = Path(proof['image'])
        if proof.get('scope') == 'process':
            assert proof['process_id'] == pid
        assert proof['source'] == str(IMAGES['native'])
        maps = (proc / 'maps').read_text()
        native_lines = [line for line in maps.splitlines() if line.split()[-1] == str(image)]
        if native_lines:
            assert all(int(line.split()[4]) == image.stat().st_ino for line in native_lines)
            image_sha = hashlib.sha256(image.read_bytes()).hexdigest()
            assert image_sha == proof['sha256'] and image.stat().st_size == proof['bytes']
            observed[str(pid)] = dict(pid=pid, executable=str((proc / 'exe').resolve()),
                native_mapping=str(image), native_source=proof['source'], native_mapped=True,
                native_proof=str(proof_path), native_map_lines=native_lines,
                player_sha256=hashlib.sha256((proc / 'exe').read_bytes()).hexdigest(),
                native_sha256=image_sha)
    except (FileNotFoundError, ProcessLookupError, PermissionError):
        pass


def execute(command, env, output, log, runtime_path):
    before = identities()
    observed = {}
    started = time.monotonic()
    with output.open('w') as out, log.open('w') as err:
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=out, stderr=err)
        try:
            while process.poll() is None:
                sample_worker(process.pid, observed)
                if time.monotonic() - started > 330:
                    raise TimeoutError('harness exceeded outer 330-second process budget')
                time.sleep(.25)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            runtime = dict(before=before, after=identities(), workers=list(observed.values()),
                           exit_code=process.returncode)
            runtime_path.write_text(json.dumps(runtime, indent=2) + '\n')
    assert process.returncode == 0, f'harness exited {process.returncode}: {log}'
    verify_runtime(runtime, before)
    return runtime


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--resume', action='store_true')
    parser.add_argument('--output', type=Path, default=WORK / 'final-native')
    args = parser.parse_args()
    output_dir = args.output.resolve()
    assert output_dir.is_relative_to(ROOT / '.work')
    output_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(['python3', str(ROOT / 'tools/compat-pong/finalize.py')], cwd=ROOT, check=True)
    fixture = ROOT / 'fixtures/pong-journey.json'
    fixture_sha = hashlib.sha256(fixture.read_bytes()).hexdigest()
    images = identities()
    summary = []
    summary_path = output_dir / 'replays.json'
    if args.resume and summary_path.exists():
        prior = json.loads(summary_path.read_text())
        if prior['fixture_sha256'] == fixture_sha and all(prior['images'][k] == v for k, v in images.items()):
            summary = prior['runs']
            images = prior['images']
    matrix = [(profile, desktop) for profile in ['compat-2017.1', 'compat-6000.3']
              for desktop in [False, True]] + [('source-2017.1', False)]
    for profile, desktop in matrix:
        assert hashlib.sha256(fixture.read_bytes()).hexdigest() == fixture_sha
        assert all(images[k] == v for k, v in identities().items()), 'runtime images changed between runs'
        label = profile + ('-desktop' if desktop else '-headless')
        if any(r['label'] == label for r in summary):
            retained = next(r for r in summary if r['label'] == label)
            verify_record(ROOT / retained['record'], fixture)
            print('RETAINED PASS', label, flush=True)
            continue
        command = [str(IMAGES['harness']), 'run', str(fixture), '--profile', profile]
        if desktop:
            command.append('--desktop')
        env = dict(os.environ, DISPLAY=':97', WGPU_BACKEND='vulkan',
                   VK_ICD_FILENAMES='/usr/share/vulkan/icd.d/lvp_icd.json')
        for attempt in range(1, 4):
            stem = output_dir / f'{label}-attempt-{attempt}'
            # Profile names contain a dot, so append suffixes rather than replacing them.
            output = Path(str(stem) + '.json')
            if output.exists():
                raise FileExistsError(f'preserve existing attempt evidence: {output}')
            print('START', label, 'attempt', attempt, flush=True)
            try:
                runtime_path = Path(str(stem) + '-runtime.json')
                execute(command, env, output, Path(str(stem) + '.log'), runtime_path)
                evidence = verify_record(output, fixture)
                evidence['runtime'] = json.loads(runtime_path.read_text())
                native = dict(path=evidence['native_image_proof']['source'],
                              sha256=evidence['native_image_proof']['sha256'])
                if 'native' in images:
                    assert images['native'] == native, 'admitted native image differs from this matrix'
                verify_runtime(evidence['runtime'], {**images, 'native': native})
                images['native'] = native
                Path(str(stem) + '-verified.json').write_text(json.dumps(evidence, indent=2) + '\n')
                record = json.loads(output.read_text())
                summary.append(dict(label=label, run_id=record['run_id'], run_dir=record['run_dir'],
                    record=str(output.relative_to(ROOT)), verified=str(Path(str(stem) + '-verified.json').relative_to(ROOT)),
                    status=record['status'], elapsed_ms=record['elapsed_ms'],
                    assertions=len(record['assertions']), captures=record['captures'],
                    teardown=record['worker']['teardown']))
                summary_path.write_text(json.dumps(dict(fixture_sha256=fixture_sha, images=images, runs=summary), indent=2) + '\n')
                print('PASS', label, record['run_id'], record['elapsed_ms'], flush=True)
                break
            except (AssertionError, ValueError, OSError, TimeoutError) as error:
                Path(str(stem) + '-error.txt').write_text(str(error) + '\n')
                print('FAILED', label, attempt, str(error), flush=True)
                if attempt == 3:
                    raise
    assert hashlib.sha256(fixture.read_bytes()).hexdigest() == fixture_sha
    from PIL import Image, ImageChops
    comparisons = []
    for profile in ['compat-2017.1', 'compat-6000.3']:
        headless, desktop = [next(r for r in summary if r['label'] == profile + '-' + mode)
                             for mode in ['headless', 'desktop']]
        for capture in headless['captures']:
            with Image.open(ROOT / headless['run_dir'] / capture['path']) as a_image, Image.open(ROOT / desktop['run_dir'] / capture['path']) as b_image:
                a, b = a_image.convert('RGB'), b_image.convert('RGB')
                assert a.size == b.size
                identical = ImageChops.difference(a, b).getbbox() is None
                assert identical, (profile, capture['path'], 'offscreen/desktop pixels differ')
                comparisons.append(dict(profile=profile, capture=capture['path'], pixel_identical=identical))
    (output_dir / 'gpu-comparison.json').write_text(json.dumps(comparisons, indent=2) + '\n')


if __name__ == '__main__':
    main()
