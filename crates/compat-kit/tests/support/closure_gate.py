"""Real compile/receipt and harness-only runtime boundary for refresh integration."""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(os.environ['COMPAT_ROOT'])
SOURCE = Path(os.environ['COMPAT_SOURCE_ROOT'])
OUTPUT = Path(os.environ['COMPAT_GATE_OUTPUT'])
sys.path.insert(0, str(ROOT / 'fixtures/gates'))


def run(command):
    subprocess.run(list(map(str, command)), check=True)


def runtime(kit, worker):
    fixture = json.loads((ROOT / 'fixtures/snake.json').read_text())
    fixture.update(name='candidate-runtime', source_root=str(SOURCE), refresh=None,
                   worker={'program': str(worker), 'args': ['pass']})
    path = OUTPUT / 'runtime-fixture.json'
    path.write_text(json.dumps(fixture))
    for profile in fixture['profiles']:
        for desktop in [False, True]:
            command = [kit, 'run', path, '--profile', profile]
            if desktop:
                command.append('--desktop')
            run(command)


def drift(mode):
    proof_path = OUTPUT / 'candidate-assembly.json'
    proof = json.loads(proof_path.read_text())
    if mode == 'assembly':
        with Path(proof['assembly']).open('ab') as output:
            output.write(b'changed after runtime')
    elif mode == 'proof':
        proof['content_receipt']['closure_sha256'] = '0' * 64
        proof_path.write_text(json.dumps(proof))
    elif mode == 'source':
        (SOURCE / 'Assets/Scripts/AfterRuntime.cs').write_text('public class AfterRuntime {}')
    elif mode == 'closure':
        path = SOURCE / 'Assets/Main.unity'
        path.write_text(path.read_text().replace('Original', 'Changed'))
    elif mode == 'revision':
        run(['git', '-c', 'user.name=Harness', '-c', 'user.email=harness@localhost',
             'commit', '--allow-empty', '-m', 'post runtime drift'])
    elif mode == 'ref':
        run(['git', 'update-ref', 'refs/heads/post-runtime', 'HEAD'])
    elif mode == 'upstream':
        run(['git', '-C', ROOT / '.work/upstream', 'update-ref', 'refs/heads/main', 'HEAD^'])
    elif mode == 'tool':
        path = ROOT / 'tools/compat-refresh/check.py'
        path.write_text(path.read_text() + '\n# changed after runtime\n')
    elif mode == 'fetch':
        with (SOURCE / '.git/FETCH_HEAD').open('a') as output:
            output.write('\n')
    elif mode != 'stable':
        raise RuntimeError('unknown drift mode: ' + mode)


if __name__ == '__main__':
    stage, kit, worker = sys.argv[1:]
    if stage == 'compile':
        import qualify
        sys.argv = ['qualify.py', 'snake', 'compile']
        sys.exit(qualify.main())
    runtime(kit, worker)
    drift(stage)
