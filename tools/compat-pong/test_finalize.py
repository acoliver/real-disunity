import copy
import unittest
from finalize import align_contact_assertions


class ContactAlignmentTests(unittest.TestCase):
    def setUp(self):
        self.fixture = dict(metadata=dict(journey_checkpoints=[
            dict(label='round-1-right-bounce', frame=41)]),
            timeline=[dict(frame=41, action='observe', selector='events')],
            assertions=[dict(op='equal', selector='events', frame=41, phase='observe',
                             pointer='/194/' + field, value=value)
                        for field, value in [('phase', 'enter'), ('trigger', False)]])
        snapshot = dict(objects=[dict(name=name, handle=handle, active_in_hierarchy=True)
                                for name, handle in [('Ball', 77), ('Enemy', 88)]])
        self.observations = [dict(frame=41, phase='observe', selector='world.snapshot', value=snapshot),
            dict(frame=41, phase='observe', selector='events', value=[
                dict(a=12, b=13, trigger=False, phase='enter'),
                dict(a=77, b=88, trigger=False, phase='enter')])]

    def test_filtered_event_stream_keeps_same_contact_assertion_semantics(self):
        before = copy.deepcopy(self.fixture)
        aligned = align_contact_assertions(self.fixture, self.observations)
        self.assertEqual([a['pointer'] for a in aligned['assertions']], ['/1/phase', '/1/trigger'])
        self.assertEqual([a['value'] for a in aligned['assertions']], ['enter', False])
        self.assertEqual(aligned['timeline'], before['timeline'])
        self.assertEqual(len(aligned['assertions']), len(before['assertions']))
        self.assertEqual(self.fixture, before)

    def test_missing_dynamic_pair_cannot_be_replaced_by_unrelated_enter(self):
        self.observations[-1]['value'].pop()
        with self.assertRaises(AssertionError):
            align_contact_assertions(self.fixture, self.observations)


if __name__ == '__main__':
    unittest.main()
