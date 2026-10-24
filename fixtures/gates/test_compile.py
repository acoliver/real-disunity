"""Actual candidate .NET builds and Rust receipts across a changing build boundary."""
import json
import os
from pathlib import Path
import subprocess
import shutil
import importlib.util
import sys
import tempfile
import unittest
from unittest.mock import patch

import qualify
from test_candidate import git

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / '.work/resumed-fixture-closure'

SPEC = importlib.util.spec_from_file_location('refresh_check', ROOT / 'tools/compat-refresh/check.py')
refresh_check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(refresh_check)


class RealCompile(unittest.TestCase):
    def test_original_additive_and_changed_during_build(self):
        with tempfile.TemporaryDirectory(dir=WORK) as temporary:
            root = Path(temporary)
            output = root / '.work/gate'
            output.mkdir(parents=True)
            source = root / '.work/source'
            shutil.copytree(ROOT / '.work/games/snake/Assets/Scripts', source / 'Assets/Scripts')
            (source / 'Assets/Main.unity').write_text('%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Original\n  m_Component:\n  - component: {fileID: 2}\n--- !u!4 &2\nTransform:\n  m_GameObject: {fileID: 1}\n  m_Father: {fileID: 0}\n')
            (source / 'Assets/Main.unity.meta').write_text('guid: 11111111111111111111111111111111\n')
            (source / 'ProjectSettings').mkdir()
            (source / 'ProjectSettings/TagManager.asset').write_text('%YAML 1.1\n--- !u!78 &1\nTagManager:\n  tags: []\n')
            git(source, 'init', '-b', 'main')
            git(source, 'add', '.')
            git(source, '-c', 'user.name=Test', '-c', 'user.email=test@localhost', 'commit', '-m', 'original scripts')
            (root / 'fixtures/gates').mkdir(parents=True)
            (root / 'fixtures/snake.json').write_text(json.dumps({'metadata': {'scene': 'Assets/Main.unity'}}))
            shutil.copyfile(ROOT / 'fixtures/gates/sources.json', root / 'fixtures/gates/sources.json')
            (root / 'csharp').mkdir()
            (root / 'csharp/UnityCompat').symlink_to(ROOT / 'csharp/UnityCompat', target_is_directory=True)
            (root / '.work/tools/dotnet').mkdir(parents=True)
            (root / '.work/tools/dotnet/dotnet').symlink_to(ROOT / '.work/tools/dotnet/dotnet')
            environment = {'COMPAT_ROOT': str(root), 'COMPAT_SOURCE_ROOT': str(source),
                           'COMPAT_GATE_OUTPUT': str(output)}
            with patch.dict(os.environ, environment), patch.object(sys, 'argv', ['qualify.py', 'snake', 'compile']):
                self.assertEqual(qualify.main(), 0)
                proof_path = output / 'candidate-assembly.json'
                first = json.loads(proof_path.read_text())
                self.assertEqual(first['source_count'], 8)
                self.assertIn('SnakeModel', first['metadata']['types'])
                (source / 'Assets/Scripts/AdditionalHelper.cs').write_text('public class AdditionalHelper {}\n')
                git(source, 'add', 'Assets/Scripts/AdditionalHelper.cs')
                git(source, '-c', 'user.name=Test', '-c', 'user.email=test@localhost', 'commit', '-m', 'add helper')
                self.assertEqual(qualify.main(), 0)
                additive = json.loads(proof_path.read_text())
                self.assertEqual(additive['source_count'], 9)
                self.assertIn('AdditionalHelper', additive['metadata']['types'])
                self.assertNotEqual(first['content_receipt']['closure_sha256'], additive['content_receipt']['closure_sha256'])
                checked = refresh_check.check(ROOT, json.loads((root / 'fixtures/snake.json').read_text()), additive, output, os.environ['COMPAT_CONTENT_RECEIPT'])
                self.assertEqual(checked['status'], 'pass')
                self.assertEqual(checked['source_count'], 9)
                assembly = Path(additive['assembly'])
                original_assembly = assembly.read_bytes()
                assembly.write_bytes(original_assembly + b'changed')
                with self.assertRaisesRegex(RuntimeError, 'assembly changed'):
                    refresh_check.check(ROOT, json.loads((root / 'fixtures/snake.json').read_text()), additive, output, os.environ['COMPAT_CONTENT_RECEIPT'])
                assembly.write_bytes(original_assembly)
                revision = git(source, 'rev-parse', 'HEAD')
                settings = source / 'ProjectSettings/TagManager.asset'
                before = settings.read_bytes()
                actual_run = subprocess.run

                def changing_build(command, **kwargs):
                    if str(command[0]).endswith('dotnet') and 'Candidate.csproj' in ' '.join(map(str, command)):
                        settings.write_bytes(before + b'\n# changed during real compilation\n')
                    return actual_run(command, **kwargs)

                with patch.object(qualify.subprocess, 'run', side_effect=changing_build):
                    with self.assertRaisesRegex(RuntimeError, 'content closure changed'):
                        qualify.main()
                self.assertEqual(git(source, 'rev-parse', 'HEAD'), revision)
                self.assertEqual(settings.read_bytes(), before + b'\n# changed during real compilation\n')
                self.assertEqual(json.loads(proof_path.read_text()), additive)


if __name__ == '__main__':
    unittest.main()
