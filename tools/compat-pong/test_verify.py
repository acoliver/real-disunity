import unittest
import copy
import hashlib
import json
import tempfile
from pathlib import Path
from verify import ROOT, validate_native_image_proof, verify_wait, verify_runtime


def snapshot(t, hidden):
    return {'time': {'time': t}, 'objects': [{
        'name': 'Ball', 'active_in_hierarchy': True,
        'transform': {'position': [0, 0, 0]}, 'components': [
            {'kind': 'Rigidbody2D', 'fields': {'velocity': [0, 0] if hidden else [12, -12],
             'angular_velocity': 0, 'sleeping': hidden}},
            {'kind': 'SpriteRenderer', 'enabled': not hidden}]}]}


class WaitTests(unittest.TestCase):
    def test_original_wait_uses_native_time(self):
        result = verify_wait(snapshot(1, True), snapshot(2.98, True), snapshot(3.02, False), 2)
        self.assertAlmostEqual(result['wait_observed'], 2.02)

    def test_early_relaunch_is_rejected(self):
        with self.assertRaises(AssertionError):
            verify_wait(snapshot(1, True), snapshot(2.98, False), snapshot(3.02, False), 2)

    def test_frame_count_cannot_replace_native_time(self):
        with self.assertRaises(AssertionError):
            verify_wait(snapshot(1, True), snapshot(1.5, True), snapshot(1.6, False), 2)

    def test_hidden_ball_after_deadline_is_rejected(self):
        with self.assertRaises(AssertionError):
            verify_wait(snapshot(1, True), snapshot(2.98, True), snapshot(3.02, True), 2)

class RuntimeIdentityTests(unittest.TestCase):
    def setUp(self):
        self.images = {name: dict(path=name, sha256=digest) for name, digest in
                       [('player', 'player-hash'), ('native', 'qualified-native'), ('harness', 'harness-hash')]}
        self.runtime = dict(before=copy.deepcopy(self.images), after=copy.deepcopy(self.images), exit_code=0,
            workers=[dict(player_sha256='player-hash', native_sha256='qualified-native', native_mapped=True,
                          native_map_lines=['mapped private image'])])

    def test_admitted_manifest_identity_does_not_require_canonical_native_hash(self):
        del self.runtime['before']['native']
        del self.runtime['after']['native']
        verify_runtime(self.runtime, self.images)

    def test_manifest_identity_can_establish_first_run_native_image(self):
        del self.runtime['before']['native']
        del self.runtime['after']['native']
        verify_runtime(self.runtime, self.runtime['before'])

    def test_canonical_rebuild_does_not_invalidate_unchanged_loaded_private_image(self):
        self.runtime['after']['native']['sha256'] = 'later-canonical-build'
        verify_runtime(self.runtime, self.images)

    def test_different_loaded_native_image_is_rejected(self):
        self.runtime['workers'][0]['native_sha256'] = 'later-canonical-build'
        with self.assertRaises(AssertionError):
            verify_runtime(self.runtime, self.images)

    def test_changed_player_executable_is_rejected(self):
        self.runtime['after']['player']['sha256'] = 'different-player'
        with self.assertRaises(AssertionError):
            verify_runtime(self.runtime, self.images)


class NativeImageProofTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=ROOT / '.work')
        self.root = Path(self.temp.name)
        self.run_build = self.root / 'run' / 'build'
        self.run_build.mkdir(parents=True)
        self.image = self.root / 'process-image.so'
        self.image.write_bytes(b'loaded native image')
        self.proof_path = self.run_build / 'native-image.json'
        self.proof = dict(image=str(self.image), bytes=self.image.stat().st_size,
                          sha256=hashlib.sha256(self.image.read_bytes()).hexdigest(),
                          scope='process', process_id=12345)
        self.write_proof()

    def tearDown(self):
        self.temp.cleanup()

    def write_proof(self):
        self.proof_path.write_text(json.dumps(self.proof))

    def test_process_scoped_image_may_outlive_its_run_build_directory(self):
        self.assertEqual(validate_native_image_proof(self.proof_path, self.run_build.parent), self.proof)

    def test_process_image_must_be_inside_worktree_work(self):
        outside = ROOT / 'tools/compat-pong/verify.py'
        self.proof['image'] = str(outside)
        self.proof['bytes'] = outside.stat().st_size
        self.proof['sha256'] = hashlib.sha256(outside.read_bytes()).hexdigest()
        self.write_proof()
        with self.assertRaises(AssertionError):
            validate_native_image_proof(self.proof_path, self.run_build.parent)

    def test_process_image_must_not_be_a_symlink(self):
        link = self.root / 'image-link.so'
        link.symlink_to(self.image)
        self.proof['image'] = str(link)
        self.write_proof()
        with self.assertRaises(AssertionError):
            validate_native_image_proof(self.proof_path, self.run_build.parent)

    def test_process_image_requires_positive_process_id_and_matching_hash(self):
        self.proof['process_id'] = 0
        self.write_proof()
        with self.assertRaises(AssertionError):
            validate_native_image_proof(self.proof_path, self.run_build.parent)
        self.proof['process_id'] = 12345
        self.proof['sha256'] = '0' * 64
        self.write_proof()
        with self.assertRaises(AssertionError):
            validate_native_image_proof(self.proof_path, self.run_build.parent)

    def test_legacy_run_scoped_image_is_still_supported(self):
        self.proof.pop('scope')
        self.proof.pop('process_id')
        self.proof['image'] = str(self.run_build / 'native.so')
        self.image.rename(self.proof['image'])
        self.image = Path(self.proof['image'])
        self.proof['bytes'] = self.image.stat().st_size
        self.proof['sha256'] = hashlib.sha256(self.image.read_bytes()).hexdigest()
        self.write_proof()
        self.assertEqual(validate_native_image_proof(self.proof_path, self.run_build.parent), self.proof)


if __name__ == '__main__':
    unittest.main()
