"""Self-contained scene import and materialization against the native C ABI."""
import math
from pathlib import Path
import tempfile
import unittest

from native_support import Native, TARGET, build, import_fresh, materialize


class NativeAdmission(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        build()

    def test_actual_three_scene_worlds(self):
        for game in ("snake", "breakout", "pong"):
            with self.subTest(game=game), tempfile.TemporaryDirectory(dir=TARGET) as output:
                graph = import_fresh(game, Path(output))
                native = Native()
                try:
                    result = materialize(native, game, Path(output))
                    snapshot = native.request({"op": "snapshot"})
                    self.verify_snapshot(native, graph, result, snapshot)
                finally:
                    native.close()
                    self.assertEqual(native.statistics()["live_worlds"], 0)

    def verify_snapshot(self, native, graph, result, snapshot):
        objects = {o["handle"]: o for o in snapshot["objects"]}
        self.assertEqual(len(objects), len(graph["nodes"]))
        components = {c["handle"]: c for o in objects.values() for c in o["components"]}
        for node in graph["nodes"]:
            obj = objects[result["handles"][node["id"]]]
            self.assertEqual(obj["name"], node["name"])
            self.assertEqual(obj["tag"], node["tag"])
            self.assertEqual(obj["active"], node["active"] and not node["template"])
            parent = result["handles"][node["parent"]] if node["parent"] else None
            self.assertEqual(obj["transform"]["parent"], parent)
            rect = any(c["object"] == node["id"] and c["native_kind"] == "RectTransform" for c in graph["components"])
            for field in (() if rect else ("local_position", "local_scale")):
                for actual, source in zip(obj["transform"][field], node["transform"][field]):
                    self.assertTrue(math.isclose(actual, source, rel_tol=1e-5, abs_tol=1e-5),
                                    f"{node['name']} {node['id']} {field}: {actual} != {source}")
            if not rect:
                for field in ("local_rotation",):
                    for actual, source in zip(obj["transform"][field], node["transform"][field]):
                        self.assertTrue(math.isclose(actual, source, rel_tol=1e-5, abs_tol=1e-5),
                                    f"{node['name']} {node['id']} {field}: {actual} != {source}")
        for component in graph["components"]:
            if component["object"] is None or component["id"] in result["skipped_components"]:
                continue
            value = components[result["handles"][component["id"]]]
            self.assertEqual(value["object"], result["handles"][component["object"]])
            self.assertEqual(value["kind"], component["native_kind"])
            self.assertEqual(value["enabled"], component["enabled"])
            self.verify_refs(value["fields"], objects, components, graph)
            if value["kind"] == "Rigidbody2D":
                self.assertEqual(value["fields"]["constraints"], component["native_fields"]["constraints"])
                self.assertAlmostEqual(value["fields"]["mass"], component["native_fields"]["mass"], delta=1e-5)
                if "material" in component["native_fields"]:
                    self.assertEqual(value["fields"]["material"], component["native_fields"]["material"])
            if value["kind"] == "Text":
                self.assertEqual(value["fields"]["text"], "0")
                self.assertEqual(value["fields"]["font"], "host.builtin-font")
            if value["kind"] == "RectTransform":
                for key, source in component["native_fields"].items():
                    for actual, authored in zip(value["fields"][key], source):
                        self.assertTrue(math.isclose(actual, authored, rel_tol=1e-5, abs_tol=1e-5),
                                        f"{component['id']} {key}: {actual} != {authored}")
        if graph["fallback_sprites"]:
            view = next(c for c in graph["components"] if c["kind"] == "SpriteRenderer")
            sprite = components[result["handles"][view["id"]]]["fields"]["sprite"]
            self.assertEqual(sprite["file_id"], 7482667652216324306)
        if any(c["kind"] == "Tilemap" for c in graph["components"]):
            tiles = next(c for c in components.values() if c["kind"] == "Tilemap")["fields"]["tiles"]
            self.assertEqual(len(tiles), 48)
            self.assertIn([-5, -6, 0], [tile["position"] for tile in tiles])
        self.assertEqual(native.request({"op": "time.get"})["time"], 0)

    def verify_refs(self, value, objects, components, graph):
        if isinstance(value, dict):
            self.assertNotIn("$missing", value)
            if "$ref" in value:
                reference = value["$ref"]
                if isinstance(reference, int):
                    self.assertTrue(reference in objects or reference in components)
                else:
                    self.assertTrue(any(c["id"] == reference and c["object"] is None for c in graph["components"])
                                    or any(reference.startswith(a["guid"] + ":") for a in graph["assets"]))
            for nested in value.values():
                self.verify_refs(nested, objects, components, graph)
        elif isinstance(value, list):
            for nested in value:
                self.verify_refs(nested, objects, components, graph)


if __name__ == "__main__":
    unittest.main(verbosity=2)
