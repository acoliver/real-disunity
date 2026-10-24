"""Planner-support tests. These do not establish game compatibility."""
import unittest
from driver import live, steps, steer, source_inventory, replay_fixture, tracking_keys


class ReplayTests(unittest.TestCase):
    def test_export_requires_fresh_compilation_and_preserves_keyboard_timeline(self):
        fixture = {'metadata': {'assembly': '/cached.dll', 'assembly_qualification': '/proof.json'},
                   'timeline': [], 'assertions': [], 'refresh': {'remote': 'origin'}}
        timeline = [dict(frame=0, phase='load', action='load')] + steps(1, ['A'])
        frozen = replay_fixture(fixture, timeline, 1, 'wide', {})
        self.assertNotIn('assembly', frozen['metadata'])
        self.assertNotIn('assembly_qualification', frozen['metadata'])
        self.assertEqual(frozen['timeline'][:4], timeline)
        self.assertEqual(frozen['timeline'][-1]['action'], 'unload')
        self.assertEqual(frozen['limits']['deadline_ms'], 120000)
        self.assertIsNone(frozen['refresh'])
        self.assertIn('assembly', fixture['metadata'])

    def test_frozen_replay_retains_requested_animation_and_loss_observations(self):
        timeline = [dict(frame=0, phase='load', action='load')]
        timeline += steps(1, []) + steps(2, ['D'])
        frozen = replay_fixture({'metadata': {}}, timeline, 2, 'samples', {}, sample_frames=[1, 2])
        samples = [a for a in frozen['timeline'] if a['action'] == 'observe']
        self.assertEqual([(a['frame'], a['selector']) for a in samples],
                         [(1, 'world.snapshot'), (1, 'events'),
                          (2, 'world.snapshot'), (2, 'events')])
        self.assertEqual([a for a in frozen['timeline'] if a['action'] not in ['observe', 'unload']], timeline)
        self.assertEqual([a['frame'] for a in frozen['timeline']],
                         sorted(a['frame'] for a in frozen['timeline']))
        with self.assertRaises(ValueError):
            replay_fixture({'metadata': {}}, timeline, 2, 'bad', {}, sample_frames=[3])

    def test_presentation_compaction_preserves_native_stimuli_and_assertions(self):
        from driver import compact_presentation
        timeline = [dict(frame=0, phase='load', action='load')]
        for tick in range(1, 31):
            timeline += steps(tick, ['D' if tick < 15 else 'A'])
            if tick in [1, 3, 30]:
                timeline += [dict(frame=tick, action='capture', phase='render', path=f'{tick}.png')]
        fixture = dict(timeline=timeline, assertions=[dict(frame=30, phase='end', op='equal',
            pointer='/test', value=48)], metadata=dict(journey_checkpoints=[dict(frame=30, label='end')]))
        compact = compact_presentation(fixture, 10)
        def without_frames(actions):
            return [{k: v for k, v in a.items() if k != 'frame'} for a in actions]
        self.assertEqual(without_frames(compact['timeline']), without_frames(timeline))
        self.assertEqual(without_frames(compact['assertions']), without_frames(fixture['assertions']))
        captures = [a['frame'] for a in compact['timeline'] if a['action'] == 'capture']
        self.assertEqual(len(set(captures)), 3)
        self.assertLess(max(a['frame'] for a in compact['timeline']), 10)
        self.assertEqual(compact['assertions'][0]['frame'], compact['timeline'][-1]['frame'])
        self.assertEqual(compact['metadata']['journey_checkpoints'][0]['logical_frame'], 30)
        self.assertEqual(fixture['assertions'][0]['frame'], 30)
        self.assertEqual(compact['metadata']['journey_checkpoints'][0]['frame'],
                         compact['assertions'][0]['frame'])

    def test_export_rejects_state_injection_and_large_updates(self):
        fixture = {'metadata': {}}
        for invalid in [dict(frame=1, action='invoke', method='Apply'),
                        dict(frame=1, action='advance', dt=.02),
                        dict(frame=1, action='input', keys=['Space'], axes={})]:
            with self.subTest(action=invalid):
                with self.assertRaises(ValueError):
                    replay_fixture(fixture, [invalid], 1, 'bad', {})

    def test_tracking_uses_current_observed_position_and_direction(self):
        self.assertEqual(tracking_keys(0, [(2, -2, -1)], .5), ['D'])
        self.assertEqual(tracking_keys(0, [(-2, -2, -1)], .5), ['A'])
        self.assertEqual(tracking_keys(0, [], .5), [])


class PlanningTests(unittest.TestCase):
    def test_inactive_prefab_is_excluded(self):
        rows = {'objects': [dict(name='Ball', active_in_hierarchy=False),
                            dict(name='Ball(Clone)', active_in_hierarchy=True)]}
        self.assertEqual(live(rows, 'Ball'), [rows['objects'][1]])

    def test_logical_ticks_have_bounded_native_deltas(self):
        actions = steps(4, ['D'])
        self.assertEqual(actions[0]['keys'], ['D'])
        updates = [a for a in actions if a['action'] == 'advance']
        self.assertAlmostEqual(sum(a['dt'] for a in updates), .02)
        self.assertTrue(all(0 < a['dt'] <= 1 / 60 for a in updates))
        self.assertTrue(all(a['frame'] == 4 for a in actions))

    def test_steering_has_deadband_and_both_directions(self):
        self.assertEqual(steer(0, 2), ['D'])
        self.assertEqual(steer(0, -2), ['A'])
        self.assertEqual(steer(2, 2.05), [])

    def test_inventory_requires_exact_seven_source_names(self):
        from pathlib import Path
        root = Path(__file__).resolve().parents[2]
        inventory = source_inventory(root / '.work/games/breakout')
        self.assertEqual(len(inventory), 7)
        self.assertTrue(all(len(digest) == 64 for digest in inventory.values()))
        with self.assertRaises(ValueError):
            source_inventory(root / '.work/nonexistent-breakout')


if __name__ == '__main__':
    unittest.main()
