import unittest
import copy
from verify import verify_wait


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

if __name__ == '__main__':
    unittest.main()
