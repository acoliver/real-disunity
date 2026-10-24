"""Real local Git/content inputs. No network, active lock or source edits by gates."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import candidate


def git(root, *args):
    return subprocess.run(['git', '-C', str(root), *args], check=True, capture_output=True, text=True).stdout.strip()


class CandidateIdentity(unittest.TestCase):
    def test_local_ref_drift_and_dirty_candidate_are_rejected(self):
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[2] / ".work/resumed-fixture-closure") as temporary:
            root = Path(temporary)
            git(root, 'init', '-b', 'main')
            (root / 'A.cs').write_text('class A {}')
            git(root, 'add', '.')
            git(root, '-c', 'user.name=Test', '-c', 'user.email=test@localhost', 'commit', '-m', 'first')
            revision = git(root, 'rev-parse', 'HEAD')
            (root / '.git/FETCH_HEAD').write_text(revision + '\t\tlocal\n')
            observed = candidate.local_identity(root)
            self.assertEqual(observed['head'], revision)
            (root / '.git/FETCH_HEAD').write_text('0' * 40 + '\t\tlocal\n')
            with self.assertRaisesRegex(RuntimeError, 'ref|revision'):
                candidate.local_identity(root)
            (root / '.git/FETCH_HEAD').write_text(revision + '\t\tlocal\n')
            (root / 'A.cs').write_text('class Changed {}')
            with self.assertRaisesRegex(RuntimeError, 'dirty'):
                candidate.local_identity(root)

    def test_receipt_requires_canonical_importer_and_rejects_bad_references(self):
        executable = os.environ['COMPAT_CONTENT_RECEIPT']
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[2] / ".work/resumed-fixture-closure") as temporary:
            root = Path(temporary)
            source = root / 'source'
            (source / 'Assets').mkdir(parents=True)
            scene = source / 'Assets/Main.unity'
            scene.write_text('%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Source\n  m_Component:\n  - component: {fileID: 2}\n--- !u!4 &2\nTransform:\n  m_GameObject: {fileID: 1}\n  m_Father: {fileID: 0}\n')
            (source / 'Assets/Main.unity.meta').write_text('guid: 11111111111111111111111111111111\n')
            fixture = {'metadata': {'scene': 'Assets/Main.unity', 'import_profile': {}, 'compatibility_sidecar': {}, 'admission_policy': {}}}
            output = root / 'gate'
            output.mkdir()
            proof = candidate.content_receipt(source, fixture, output, executable)
            self.assertEqual(proof['importer'], 'worldrunner-compat-content')
            self.assertIn('Assets/Main.unity.meta', proof['source_hashes'])
            scene.write_text(scene.read_text().replace('m_Father: {fileID: 0}', 'm_Father: {fileID: 99}'))
            with self.assertRaisesRegex(RuntimeError, 'content admission'):
                candidate.content_receipt(source, fixture, output, executable)
            scene.write_text(scene.read_text().replace('m_Father: {fileID: 99}', 'm_Father: {fileID: 1, guid: 123}'))
            with self.assertRaisesRegex(RuntimeError, 'content admission'):
                candidate.content_receipt(source, fixture, output, executable)


if __name__ == '__main__':
    unittest.main()
