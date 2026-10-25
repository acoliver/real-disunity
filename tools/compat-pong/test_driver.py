import json
import unittest
from driver import ROOT, Planner, active_object, steering_keys, step_actions, reset_state

class JourneyTests(unittest.TestCase):
    def test_final_fixture_keeps_its_deadline_and_label(self):
        fixture = json.loads((ROOT / 'fixtures/pong-journey.json').read_text())
        self.assertEqual(fixture['limits']['deadline_ms'], 300000)
        self.assertEqual(fixture['evidence_label'], 'original-source/legacy-mappings-dejavu-font')

    def test_generator_names_the_pinned_game(self):
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder)
            scripts = source / 'Assets/Scripts'
            scripts.mkdir(parents=True)
            for index in range(20):
                (scripts / f'Script{index}.cs').write_text(f'class Script{index} {{}}')
            planner = Planner(source / 'worker', source, source / 'output')
        self.assertEqual(planner.fixture['game'], 'pong')
        self.assertNotIn('source_root', planner.fixture)

    def test_final_stimulus_uses_keyboard_and_observable_checkpoints(self):
        fixture = json.loads((ROOT / 'fixtures/pong-journey.json').read_text())
        self.assertEqual(fixture['timeline'][0]['action'], 'load')
        self.assertEqual(fixture['timeline'][-1]['action'], 'unload')
        self.assertEqual({a['action'] for a in fixture['timeline']},
                         {'load', 'input', 'advance', 'observe', 'capture', 'reload', 'unload'})
        self.assertTrue(all(not a.get('axes') for a in fixture['timeline']))
        self.assertEqual(len([a for a in fixture['timeline'] if a['action'] == 'capture']), 5)
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
