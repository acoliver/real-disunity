"""Evidence-parser tests only; actual gameplay qualification uses replay artifacts."""
import unittest
from verify import summarize


class EvidenceTests(unittest.TestCase):
    def test_clamp_evidence_requires_both_width_aware_edges(self):
        from verify import clamp_evidence
        self.assertFalse(clamp_evidence([dict(width=4, x=-7), dict(width=4, x=7)], 4))
        self.assertFalse(clamp_evidence([dict(width=2, x=-7), dict(width=2, x=7)], 4))
        self.assertFalse(clamp_evidence([dict(width=4, x=6)], 4))
        self.assertTrue(clamp_evidence([dict(width=4, x=-6), dict(width=4, x=6)], 4))
        self.assertTrue(clamp_evidence([dict(width=2, x=-7), dict(width=2, x=7)], 2))

    def test_width_evidence_requires_native_time_and_return_without_reload(self):
        from verify import width_evidence
        def report(samples):
            return dict(observations=[dict(frame=i, selector='world.snapshot', value=dict(
                time=dict(time=t), objects=[dict(name='Platform', handle=5,
                    active_in_hierarchy=True, components=[dict(kind='BoxCollider2D',
                        fields=dict(size=[w, .62]))])])) for i, (t, w) in enumerate(samples)])
        ongoing = width_evidence(report([(0, 2), (1, 4), (10, 4)]))
        self.assertFalse(ongoing['expired'])
        extended = width_evidence(report([(0, 2), (1, 4), (9.1, 4), (9.2, 2)]))
        self.assertTrue(extended['expired'])
        self.assertTrue(extended['observed_past_initial_duration'])
        self.assertEqual(extended['intervals'][0]['normal_frame'], 3)
        regular = width_evidence(report([(0, 2), (1, 4), (8.9, 4), (9, 2)]))
        self.assertTrue(regular['expired'])
        self.assertFalse(regular['observed_past_initial_duration'])
        reloaded = width_evidence(report([(0, 2), (1, 4), (0, 2)]))
        self.assertFalse(reloaded['expired'])
        self.assertFalse(reloaded['observed_past_initial_duration'])

    def test_loss_evidence_requires_survivor_identity_then_fresh_upward_ball(self):
        from verify import loss_evidence
        def ball(handle, vy):
            return dict(handle=handle, name='Ball', active_in_hierarchy=True,
                        transform=dict(position=[0, -4, 0]), components=[
                            dict(kind='Rigidbody2D', fields=dict(velocity=[0, vy])),
                            dict(kind='SpriteRenderer', enabled=True, fields={})])
        def report(sequence):
            return dict(observations=[dict(frame=i, selector='world.snapshot',
                value=dict(time=dict(time=i * .02), objects=[ball(h, vy) for h, vy in row]))
                for i, row in enumerate(sequence)])
        genuine = report([[(1, -6), (2, -6)], [(2, -6)], [(3, 0)], [(3, 6)]])
        evidence = loss_evidence(genuine)
        self.assertTrue(evidence['no_premature_respawn'])
        self.assertTrue(evidence['last_loss_fresh_launch'])
        self.assertEqual(evidence['survivor'], 2)
        self.assertEqual(evidence['replacement'], 3)
        premature = report([[(1, -6), (2, -6)], [(2, -6), (3, 6)], [(3, 6)]])
        self.assertFalse(loss_evidence(premature)['no_premature_respawn'])
        unchanged = report([[(1, -6), (2, -6)], [(2, -6)], [(2, 6)]])
        self.assertFalse(loss_evidence(unchanged)['last_loss_fresh_launch'])
        reloaded = genuine.copy()
        reloaded['observations'] = list(genuine['observations'])
        reloaded['observations'][2] = dict(genuine['observations'][2],
            value=dict(time=dict(time=0), objects=[ball(3, 6)]))
        self.assertFalse(loss_evidence(reloaded)['last_loss_fresh_launch'])

    def test_repeated_cumulative_event_observations_do_not_invent_contacts(self):
        event = dict(a=1, b=2, phase='enter', trigger=False)
        report = dict(observations=[dict(selector='events', value=[event]),
                                    dict(selector='events', value=[event])],
                      diagnostics=[], teardown={})
        self.assertEqual(summarize(report)['contacts'], {'?/?/solid': 1})

    def test_missing_gameplay_observations_do_not_claim_layout_or_speed(self):
        report = dict(observations=[], diagnostics=['provider error'], teardown={})
        result = summarize(report)
        self.assertIsNone(result['first_bricks'])
        self.assertEqual(result['ball_counts'], [])
        self.assertEqual(result['diagnostics'], ['provider error'])

    def test_native_identity_uses_frozen_run_image_even_after_canonical_rebuild(self):
        import hashlib
        import json
        import tempfile
        from pathlib import Path
        from driver import ROOT
        from verify import native_identity
        with tempfile.TemporaryDirectory(dir=ROOT / '.work') as folder:
            root = Path(folder)
            (root / 'build').mkdir()
            image = ROOT / '.work/compat/native-images/test-process/native/test-process/native.so'
            image.parent.mkdir(parents=True, exist_ok=True)
            image.write_bytes(b'frozen actual worker image')
            canonical = root / 'canonical.so'
            canonical.write_bytes(b'concurrent later build')
            digest = hashlib.sha256(image.read_bytes()).hexdigest()
            manifest = dict(image=str(image), source=str(canonical), sha256=digest,
                            bytes=image.stat().st_size, scope='process', process_id=12345)
            (root / 'build/native-image.json').write_text(json.dumps(manifest))
            self.assertEqual(native_identity(root)['sha256'], digest)
            manifest['process_id'] = 0
            (root / 'build/native-image.json').write_text(json.dumps(manifest))
            with self.assertRaises(AssertionError):
                native_identity(root)
            manifest['process_id'] = 12345
            (root / 'build/native-image.json').write_text(json.dumps(manifest))
            image.write_bytes(b'corrupt image')
            with self.assertRaises(AssertionError):
                native_identity(root)
            image.unlink()
            image.parent.rmdir()
            image.parent.parent.rmdir()
            image.parent.parent.parent.rmdir()


if __name__ == '__main__':
    unittest.main()
