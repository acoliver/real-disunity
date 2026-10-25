"""Self-contained source tile collision and clone tests against the native C ABI."""
from pathlib import Path
import tempfile
import unittest

from native_support import Native, TARGET, build, import_fresh, materialize


class NativeTileCollision(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        build()

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=TARGET)
        self.addCleanup(self.temporary.cleanup)
        self.output = Path(self.temporary.name)

    def test_original_48_wall_cells_materialize_and_bounce_continuously(self):
        graph = import_fresh("breakout", self.output)
        native = Native()
        try:
            result = materialize(native, "breakout", self.output)
            collider = next(c for c in graph["components"] if c["kind"] == "TilemapCollider2D")
            handle = result["handles"][collider["id"]]
            fields = native.request({"op": "component.get", "component": handle})["fields"]
            self.assertEqual(len(fields["cells"]), 48)
            self.assertEqual(fields["material"]["bounciness"], 1)
            self.assertEqual(fields["material"]["friction"], 0)
            pose = native.request({"op": "transform.get", "object": result["handles"][collider["object"]]})
            self.assertEqual(pose["rotation"], [0, 0, 0, 1])
            for cell in fields["cells"]:
                center = [(cell["position"][i] * (fields["cell_size"][i] + fields["cell_gap"][i])
                           + fields["tile_anchor"][i] * fields["cell_size"][i] + fields["offset"][i])
                          * pose["scale"][i] + pose["position"][i] for i in range(2)]
                hits = native.request({"op": "physics.overlap", "point": center})
                self.assertEqual(sum(h["handle"] == handle for h in hits), 1, center)
            self.assertFalse(any(h["handle"] == handle for h in native.request(
                {"op": "physics.overlap", "point": [0, 0]})))
            ray = native.request({"op": "physics.raycast", "origin": [0, 0],
                                  "direction": [1, 0], "distance": 30})
            self.assertEqual(sum(h["collider"] == handle for h in ray), 1)
            self.continuous_bounces(native, handle)
        finally:
            native.close()
            statistics = native.statistics()
            self.assertEqual(statistics["live_worlds"], 0)
            self.assertEqual(statistics["allocated_world_slots"], 1)

    def test_source_affine_and_cloned_collider_keep_geometry_and_distinct_ownership(self):
        output = self.output / "affine"
        values = {"e00": 0, "e10": 1, "e01": -1, "e11": 0, "e03": 0.25, "e13": 0.125}
        overrides = [{"guid": "8c9cfa26abfee488c85f1582747f6a02", "file_id": 2096728905,
                      "property_path": f"m_TileMatrixArray.Array.data[0].m_Data.{key}",
                      "precondition": {"schema_version": 1, "class_id": 1839735485,
                                       "expected_value": int(key in ("e00", "e11", "e22", "e33"))},
                      "value": value, "reason": "Source affine collision qualification"}
                     for key, value in values.items()]
        graph = import_fresh("breakout", output, overrides)
        native = Native()
        try:
            result = materialize(native, "breakout", output)
            collider = next(c for c in graph["components"] if c["kind"] == "TilemapCollider2D")
            handle = result["handles"][collider["id"]]
            owner = result["handles"][collider["object"]]
            fields = collider["native_fields"]
            self.assertEqual(fields["cells"][0]["matrix"], [0, 1, -1, 0, 0.25, 0.125])
            cell = next(c for c in fields["cells"] if c["position"] == [-5, -6])
            pose = native.request({"op": "transform.get", "object": owner})
            center = [(cell["position"][i] * fields["cell_size"][i]
                       + fields["tile_anchor"][i] * fields["cell_size"][i]
                       + cell["matrix"][4+i]) * pose["scale"][i] + pose["position"][i]
                      for i in range(2)]
            inside = [center[0], center[1] + 0.75 * pose["scale"][1]]
            outside = [center[0] + 0.75 * pose["scale"][0], center[1]]
            self.assertEqual(sum(h["handle"] == handle for h in native.request(
                {"op": "physics.overlap", "point": inside})), 1)
            self.assertFalse(any(h["handle"] == handle for h in native.request(
                {"op": "physics.overlap", "point": outside})))
            clone = native.request({"op": "object.clone", "object": owner})
            cloned_owner = clone["remap"][str(owner)]
            cloned_collider = clone["remap"][str(handle)]
            self.assertNotEqual(cloned_collider, handle)
            hits = native.request({"op": "physics.overlap", "point": inside})
            self.assertEqual({h["handle"] for h in hits}, {handle, cloned_collider})
            delta = 50
            native.request({"op": "transform.set", "object": cloned_owner,
                            "position": [pose["position"][0] + delta, pose["position"][1], pose["position"][2]]})
            shifted = [inside[0] + delta, inside[1]]
            hits = native.request({"op": "physics.overlap", "point": shifted})
            self.assertEqual([(h["handle"], h["object"]) for h in hits], [(cloned_collider, cloned_owner)])
            native.request({"op": "object.destroy", "object": owner})
            self.assertEqual(native.request({"op": "physics.overlap", "point": inside}), [])
            self.assertEqual(native.request({"op": "physics.overlap", "point": shifted})[0]["handle"], cloned_collider)
        finally:
            native.close()
            statistics = native.statistics()
            self.assertEqual(statistics["live_worlds"], 0)
            self.assertEqual(statistics["allocated_world_slots"], 1)

    def continuous_bounces(self, native, wall):
        ball = native.request({"op": "object.create", "name": "collision-test-probe"})["handle"]
        native.request({"op": "transform.set", "object": ball, "position": [0, 0, 0]})
        body = native.request({"op": "component.add", "object": ball, "kind": "Rigidbody2D",
                               "fields": {"velocity": [100, 0], "gravity_scale": 0,
                                          "constraints": 4, "restitution": 1}})["handle"]
        native.request({"op": "component.add", "object": ball, "kind": "CircleCollider2D",
                        "fields": {"radius": 0.1}})
        bounces = 0
        previous = 100
        for step in range(40):
            native.request({"op": "physics.step", "dt": 0.15 if step == 0 else 0.02})
            velocity = native.request({"op": "component.get", "component": body})["fields"]["velocity"][0]
            if step == 0:
                self.assertLess(velocity, 0, "15-unit swept move must bounce before crossing the wall")
            if velocity * previous < 0:
                bounces += 1
                events = native.request({"op": "events.read"})
                self.assertTrue(any(wall in (e["collider_a"], e["collider_b"]) for e in events))
            position = native.request({"op": "transform.get", "object": ball})["position"]
            self.assertLess(abs(position[0]), 10)
            self.assertAlmostEqual(abs(velocity), 100, delta=0.1)
            previous = velocity
        self.assertGreaterEqual(bounces, 4)
        native.request({"op": "object.destroy", "object": ball})
        return bounces


if __name__ == "__main__":
    unittest.main(verbosity=2)
