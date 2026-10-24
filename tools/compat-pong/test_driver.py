import hashlib
import json
import unittest
from driver import ROOT, Planner, active_object, steering_keys, step_actions, reset_state

class JourneyTests(unittest.TestCase):
    def test_final_fixture_compiles_current_sources_without_cached_paths(self):
        fixture = json.loads((ROOT / 'fixtures/pong-journey.json').read_text())
        for field in ('assembly', 'assembly_qualification', 'assembly_sha256'):
            self.assertNotIn(field, fixture['metadata'])
        self.assertEqual(fixture['limits']['deadline_ms'], 300000)
        self.assertEqual(fixture['evidence_label'], 'original-source/legacy-mappings-dejavu-font')

    def test_generator_defaults_to_fresh_source_compilation(self):
        planner = Planner(None)
        for field in ('assembly', 'assembly_qualification', 'assembly_sha256'):
            self.assertNotIn(field, planner.fixture['metadata'])
        self.assertEqual(len(planner.before), 20)

    def test_final_stimulus_retains_qualified_full_journey(self):
        fixture = json.loads((ROOT / 'fixtures/pong-journey.json').read_text())
        digest = hashlib.sha256(json.dumps(fixture['timeline'], sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        self.assertEqual(digest, '6215d3540a8ffdc6c468761c15345bd4200bfa10e15a380917e851fc1694174e')
        self.assertEqual(len(fixture['timeline']), 1422)
        self.assertEqual(len(fixture['assertions']), 366)

    def test_selects_live_instance_instead_of_prefab(self):
        template = {'name': 'Ball', 'active_in_hierarchy': False}
        live = {'name': 'Ball', 'active_in_hierarchy': True}
        self.assertIs(active_object({'objects': [template, live]}, 'Ball'), live)

    def test_ambiguous_live_object_is_rejected(self):
        with self.assertRaises(ValueError):
            active_object({'objects': [{'name': 'Ball', 'active_in_hierarchy': True}] * 2}, 'Ball')

    def test_paddle_controls_use_keys_without_synthetic_axes(self):
        self.assertEqual(steering_keys(0, 2, 'W', 'S'), ['W'])
        self.assertEqual(steering_keys(0, -2, 'UpArrow', 'DownArrow'), ['DownArrow'])
        self.assertEqual(steering_keys(1, 1.1, 'W', 'S'), [])
        actions = step_actions(7, ['W'])
        self.assertEqual(actions, [
            {'frame': 7, 'phase': 'input', 'action': 'input', 'keys': ['W'], 'axes': {}},
            {'frame': 7, 'phase': 'fixed_update', 'action': 'advance', 'dt': 0.02}])

    def test_reset_requires_every_original_effect(self):
        ball = {'transform': {'position': [0, 0, 0]}, 'components': [
            {'kind': 'Rigidbody2D', 'fields': {'velocity': [0, 0], 'angular_velocity': 0, 'sleeping': True}},
            {'kind': 'SpriteRenderer', 'enabled': False}]}
        self.assertTrue(reset_state(ball))
        ball['components'][0]['fields']['sleeping'] = False
        self.assertFalse(reset_state(ball))

if __name__ == '__main__':
    unittest.main()
