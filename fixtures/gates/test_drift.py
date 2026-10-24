"""Real Rust-importer and Git boundary controls; no gameplay qualification claim."""
import copy
import os
from pathlib import Path
import tempfile
import unittest

from candidate import content_receipt, local_identity, verify_content, verify_identity
from qualification import select_sources, verify_sources
from test_candidate import git

WORK = Path(__file__).resolve().parents[2] / '.work/resumed-fixture-closure'


class ClosureDrift(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=WORK)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / 'source'
        self.output = self.root / 'gate'
        self.output.mkdir()
        (self.source / 'Assets/Scripts').mkdir(parents=True)
        (self.source / 'Assets/Main.unity').write_text('%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Source\n  m_Component:\n  - component: {fileID: 2}\n--- !u!4 &2\nTransform:\n  m_GameObject: {fileID: 1}\n  m_Father: {fileID: 0}\n')
        (self.source / 'Assets/Main.unity.meta').write_text('guid: 11111111111111111111111111111111\n')
        (self.source / 'Assets/Scripts/A.cs').write_text('class A {}\n')
        self.fixture = {'metadata': {'scene': 'Assets/Main.unity', 'import_profile': {},
                                    'compatibility_sidecar': {}, 'admission_policy': {}}}
        self.executable = os.environ['COMPAT_CONTENT_RECEIPT']

    def receipt(self, fixture=None):
        return content_receipt(self.source, fixture or self.fixture, self.output, self.executable)

    def test_changed_closure_while_gating_preserves_source_revision_and_active_lock(self):
        git(self.source, 'init', '-b', 'main')
        git(self.source, 'add', '.')
        git(self.source, '-c', 'user.name=Test', '-c', 'user.email=test@localhost', 'commit', '-m', 'fixture')
        revision = git(self.source, 'rev-parse', 'HEAD')
        lock = self.root / 'active.json'
        lock.write_text('{"revision":"previous"}\n')
        proof = self.receipt()
        for relative, text in [('Assets/Main.unity', '\n# changed\n'),
                               ('Assets/Main.unity.meta', '\n# meta changed\n'),
                               ('Assets/Scripts/New.cs', 'class New {}\n'),
                               ('ProjectSettings/Settings.asset', 'project setting\n'),
                               ('Packages/manifest.json', '{}\n')]:
            with self.subTest(relative=relative):
                path = self.source / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                previous = path.read_bytes() if path.exists() else None
                path.write_bytes((previous or b'') + text.encode())
                changed = path.read_bytes()
                with self.assertRaisesRegex(RuntimeError, 'content closure changed'):
                    verify_content(self.source, self.fixture, self.output, self.executable, proof)
                self.assertEqual(path.read_bytes(), changed)
                self.assertEqual(git(self.source, 'rev-parse', 'HEAD'), revision)
                self.assertEqual(lock.read_text(), '{"revision":"previous"}\n')
                if previous is None:
                    path.unlink()
                else:
                    path.write_bytes(previous)
        verify_content(self.source, self.fixture, self.output, self.executable, proof)

    def test_additive_sources_are_freshly_imported_and_current_counted(self):
        selection = ['Assets/Scripts/A.cs']
        old = select_sources(self.source, selection)
        old_receipt = self.receipt()
        (self.source / 'Assets/Scripts/B.cs').write_text('class B {}\n')
        with self.assertRaisesRegex(RuntimeError, 'source changed'):
            verify_sources(self.source, selection, old)
        self.assertEqual(len(select_sources(self.source, selection)), 2)
        self.assertNotEqual(self.receipt()['closure_sha256'], old_receipt['closure_sha256'])

    def test_profile_overlay_policy_and_source_symlink_controls(self):
        proof = self.receipt()
        for field in ['import_profile', 'compatibility_sidecar', 'admission_policy']:
            changed = copy.deepcopy(self.fixture)
            changed['metadata'][field] = {'name': 'changed'} if field == 'import_profile' else (
                {'field_overrides': [{'guid': '11111111111111111111111111111111', 'file_id': 1,
                                     'property_path': 'm_Name', 'value': 'Replacement', 'reason': 'Test',
                                     'precondition': {'schema_version': 1, 'class_id': 1, 'expected_value': 'Source'}}]}
                if field == 'compatibility_sidecar' else {'ignored_components': {'AudioListener': 'Test'}})
            with self.assertRaisesRegex(RuntimeError, 'content closure changed'):
                verify_content(self.source, changed, self.output, self.executable, proof)
        helper = self.source / 'Assets/Scripts/A.cs'
        helper.rename(self.source / 'actual.cs')
        helper.symlink_to(self.source / 'actual.cs')
        with self.assertRaisesRegex(RuntimeError, 'symlink'):
            self.receipt()
        with self.assertRaisesRegex(RuntimeError, 'symlink'):
            select_sources(self.source, ['Assets/Scripts/A.cs'])

    def test_local_ref_addition_is_drift_even_with_clean_unchanged_head(self):
        git(self.source, 'init', '-b', 'main')
        git(self.source, 'add', '.')
        git(self.source, '-c', 'user.name=Test', '-c', 'user.email=test@localhost', 'commit', '-m', 'fixture')
        identity = local_identity(self.source)
        git(self.source, 'update-ref', 'refs/heads/other', identity['head'])
        with self.assertRaisesRegex(RuntimeError, 'ref/revision changed'):
            verify_identity(self.source, identity)
        self.assertEqual(git(self.source, 'rev-parse', 'HEAD'), identity['head'])


if __name__ == '__main__':
    unittest.main()
