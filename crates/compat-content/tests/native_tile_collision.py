"""Fresh original-source import and public materialize closure against canonical C ABI."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import unittest

from native_transport import Native, ROOT

OUTPUT = ROOT / ".work/content-next"


def example(name):
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / ".work/target-content"))
    return str(target / "debug/examples" / name)


def import_fresh(output=OUTPUT, overrides=()):
    output.mkdir(parents=True, exist_ok=True)
    base = ROOT / "crates/compat-content"
    sidecar = json.loads((base / "overlays/breakout.json").read_text())
    sidecar["field_overrides"].extend(overrides)
    (output / "breakout-overlay.json").write_text(json.dumps(sidecar))
    command = [example("import"), str(ROOT / ".work/games/breakout"),
               "Assets/Scenes/SampleScene.unity", str(base / "profiles/breakout-source.json"),
               str(output / "breakout-overlay.json")]
    imported = subprocess.run(command, check=True, capture_output=True, text=True, cwd=ROOT)
    (output / "breakout-ready.json").write_text(imported.stdout)
    policy = json.loads((base / "profiles/breakout-admission.json").read_text())
    (output / "breakout-admission.json").write_text(json.dumps(policy))
    graph = json.loads(imported.stdout)
    for path, expected in graph["source_hashes"].items():
        assert hashlib.sha256((ROOT / ".work/games/breakout" / path).read_bytes()).hexdigest() == expected, path
    return graph


def materialize(native, output=OUTPUT):
    command = [example("materialize"), str(output / "breakout-ready.json"),
               str(output / "breakout-admission.json"), str(output / "breakout-materialized.json")]
    transcript = []
    with subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, text=True, cwd=ROOT) as process:
        for line in process.stdout:
            request = json.loads(line)
            transcript.append(request)
            try:
                response = native.request(request)
            except RuntimeError as error:
                response = {"$transport_error": str(error)}
            process.stdin.write(json.dumps(response) + "\n")
            process.stdin.flush()
        process.stdin.close()
        error = process.stderr.read()
        assert process.wait(timeout=30) == 0, error
    (output / "breakout-native-requests.json").write_text(json.dumps(transcript, indent=2))
    return json.loads((output / "breakout-materialized.json").read_text())


class NativeTileCollision(unittest.TestCase):
    def test_original_48_wall_cells_materialize_and_bounce_continuously(self):
        graph = import_fresh()
        native = Native()
        try:
            result = materialize(native)
            collider = next(c for c in graph["components"] if c["kind"] == "TilemapCollider2D")
            handle = result["handles"][collider["id"]]
            fields = native.request({"op": "component.get", "component": handle})["fields"]
            self.assertEqual(len(fields["cells"]), 48)
            self.assertEqual(fields["material"]["bounciness"], 1)
            self.assertEqual(fields["material"]["friction"], 0)
            pose = native.request({"op": "transform.get", "object": result["handles"][collider["object"]]})
            self.assertEqual(pose["rotation"], [0, 0, 0, 1])
            centers = []
            for cell in fields["cells"]:
                center = [(cell["position"][i] * (fields["cell_size"][i] + fields["cell_gap"][i])
                           + fields["tile_anchor"][i] * fields["cell_size"][i] + fields["offset"][i])
                          * pose["scale"][i] + pose["position"][i] for i in range(2)]
                centers.append(center)
                hits = native.request({"op": "physics.overlap", "point": center})
                self.assertEqual(sum(h["handle"] == handle for h in hits), 1, center)
            self.assertFalse(any(h["handle"] == handle for h in native.request(
                {"op": "physics.overlap", "point": [0, 0]})))
            ray = native.request({"op": "physics.raycast", "origin": [0, 0],
                                  "direction": [1, 0], "distance": 30})
            self.assertEqual(sum(h["collider"] == handle for h in ray), 1)
            bounces = self.continuous_bounces(native, handle)
            snapshot = native.request({"op": "snapshot"})
            (OUTPUT / "breakout-native-snapshot.json").write_text(json.dumps(snapshot, indent=2))
            evidence = {"source_tile_cells": len(centers), "source_centers_covered": len(centers),
                        "interior_clear": True, "continuous_bounces": bounces,
                        "probe_speed": 100, "first_step_dt": 0.15,
                        "first_step_unobstructed_distance": 15,
                        "material": fields["material"], "geometry_fidelity": "sprite_bounds; no alpha tracing",
                        "diagnostics": graph["diagnostics"], "native_objects": len(snapshot["objects"]),
                        "native_components": sum(len(o["components"]) for o in snapshot["objects"]),
                        "skipped_components": result["skipped_components"], "raycast_deduplicated": True,
                        "source_hashes_verified": True, "source_hashes": graph["source_hashes"],
                        "native_image_sha256": hashlib.sha256(native.path.read_bytes()).hexdigest()}
            (OUTPUT / "tile-collision-evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
        finally:
            native.close()
            statistics = native.statistics()
            self.assertEqual(statistics["live_worlds"], 0)
            self.assertEqual(statistics["allocated_world_slots"], 1)
            (OUTPUT / "lifecycle-evidence.json").write_text(json.dumps(statistics, indent=2))

    def test_source_affine_and_cloned_collider_keep_geometry_and_distinct_ownership(self):
        output = OUTPUT / "affine"
        values = {"e00": 0, "e10": 1, "e01": -1, "e11": 0, "e03": 0.25, "e13": 0.125}
        overrides = [{"guid": "8c9cfa26abfee488c85f1582747f6a02", "file_id": 2096728905,
                      "property_path": f"m_TileMatrixArray.Array.data[0].m_Data.{key}",
                      "value": value, "reason": "Source affine collision qualification"}
                     for key, value in values.items()]
        graph = import_fresh(output, overrides)
        native = Native()
        try:
            result = materialize(native, output)
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
            (output / "affine-clone-evidence.json").write_text(json.dumps({
                "source_affine": fields["cells"][0]["matrix"], "rotated_inside_covered": True,
                "rotated_outside_clear": True, "clone_independent_ownership": True,
                "clone_survives_source_destroy": True, "native_image_sha256": hashlib.sha256(native.path.read_bytes()).hexdigest()}, indent=2))
        finally:
            native.close()
            statistics = native.statistics()
            self.assertEqual(statistics["live_worlds"], 0)
            self.assertEqual(statistics["allocated_world_slots"], 1)
            (OUTPUT / "lifecycle-evidence.json").write_text(json.dumps(statistics, indent=2))

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
