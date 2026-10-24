"""Local candidate provenance and receipts from the canonical Rust importer."""
import json
from pathlib import Path
import subprocess

from qualification import digest


def git(source, *args):
    return subprocess.run(['git', '-C', str(source), *args], check=True,
                          capture_output=True, text=True).stdout.strip()


def local_identity(source):
    if git(source, 'status', '--porcelain', '--untracked-files=all'):
        raise RuntimeError('dirty candidate source rejected')
    head = git(source, 'rev-parse', '--verify', 'HEAD^{commit}')
    fetch_path = Path(git(source, 'rev-parse', '--path-format=absolute', '--git-path', 'FETCH_HEAD'))
    fetched = None
    if fetch_path.exists():
        try:
            fetched = git(source, 'rev-parse', '--verify', 'FETCH_HEAD^{commit}')
        except subprocess.CalledProcessError as error:
            raise RuntimeError('invalid local fetched ref/revision') from error
        if fetched != head:
            raise RuntimeError('candidate local fetched ref/revision differs from HEAD')
    refs = git(source, 'for-each-ref', '--format=%(refname) %(objectname)')
    return {'head': head, 'fetched': fetched, 'refs': refs,
            'fetch_sha256': digest(fetch_path) if fetched else None}


def content_receipt(source, fixture, output, executable):
    metadata = fixture['metadata']
    inputs = []
    for name, value in [('profile', metadata.get('import_profile', {})),
                        ('sidecar', metadata.get('compatibility_sidecar', {})),
                        ('admission', metadata.get('admission_policy', {}))]:
        path = output / ('receipt-' + name + '.json')
        path.write_text(json.dumps(value, sort_keys=True))
        inputs.append(str(path))
    completed = subprocess.run([str(executable), str(source), metadata['scene'], *inputs],
                               capture_output=True, text=True, check=False)
    if completed.returncode:
        raise RuntimeError('content admission rejected: ' + completed.stderr.strip())
    proof = json.loads(completed.stdout)
    if proof['schema_version'] != 1 or proof['importer'] != 'worldrunner-compat-content':
        raise RuntimeError('content admission requires canonical Rust importer receipt')
    if proof['scene'] != metadata['scene'] or not proof['source_hashes']:
        raise RuntimeError('content admission returned incomplete identity')
    proof['admission_policy'] = metadata.get('admission_policy', {})
    return proof


def verify_content(source, fixture, output, executable, expected):
    current = content_receipt(source, fixture, output, executable)
    if current != expected:
        raise RuntimeError('content closure changed during qualification; adoption rejected')


def verify_identity(source, expected):
    if local_identity(source) != expected:
        raise RuntimeError('local source ref/revision changed during qualification; adoption rejected')
