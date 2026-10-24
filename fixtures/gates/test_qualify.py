"""Candidate-fixture handoff tests; these do not establish gameplay compatibility."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import qualify


class CandidateHandoff(unittest.TestCase):
    def test_runtime_gate_receives_current_count_and_exact_assembly_proof(self):
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[2] / ".work/resumed-fixture-closure") as temporary:
            root = Path(temporary)
            (root / 'fixtures/gates').mkdir(parents=True)
            output = root / '.work/gate'
            output.mkdir(parents=True)
            source = root / '.work/source'
            source.mkdir()
            fixture = {'name': 'game', 'worker': {'program': 'actual-worker'},
                       'profiles': ['compat-6000.3'], 'metadata': {'script_count': 1}}
            (root / 'fixtures/game.json').write_text(json.dumps(fixture))
            (root / 'fixtures/gates/sources.json').write_text('{"game":{}}')
            hashes = {'Assets/Scripts/A.cs': 'a', 'Assets/Scripts/B.cs': 'b'}
            assembly = output / 'Candidate.dll'
            proof = {'assembly': str(assembly), 'assembly_sha256': 'assembly-hash',
                     'source_root': str(source), 'source_count': 2,
                     'source_hashes': hashes, 'metadata': {'types': ['A', 'B']},
                     'content_receipt': {}, 'local_identity': {}}
            proof_path = output / 'candidate-assembly.json'
            proof_path.write_text(json.dumps(proof))

            def runtime(command, **_kwargs):
                candidate = json.loads(Path(command[2]).read_text())
                self.assertEqual(candidate['metadata']['script_count'], 2)
                self.assertEqual(candidate['metadata']['assembly_qualification'], str(proof_path))
                self.assertEqual(candidate['metadata']['assembly'], str(assembly))
                self.assertEqual(candidate['source_root'], str(source))
                return type('Completed', (), {'returncode': 0})()

            environment = {'COMPAT_ROOT': str(root), 'COMPAT_SOURCE_ROOT': str(source),
                           'COMPAT_GATE_OUTPUT': str(output)}
            with patch.dict(os.environ, environment), patch.object(sys, 'argv', ['qualify.py', 'game', 'test']), \
                    patch.object(qualify, 'select_sources', return_value=hashes), \
                    patch.object(qualify, 'verify_sources'), \
                    patch.object(qualify, 'local_identity', return_value={}), \
                    patch.object(qualify, 'content_receipt', return_value={}), \
                    patch.object(qualify, 'verify_content'), \
                    patch.object(qualify, 'verify_identity'), \
                    patch.object(qualify, 'digest', return_value='assembly-hash'), \
                    patch.object(qualify, 'inspect_assembly', return_value=proof['metadata']), \
                    patch.object(qualify.subprocess, 'run', side_effect=runtime):
                self.assertEqual(qualify.main(), 0)


if __name__ == '__main__':
    unittest.main()
