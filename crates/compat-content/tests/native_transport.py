"""Content admission smoke against one actual host-selected native FFI image.

Build the Rust materialize example first. This test neither compiles game scripts
nor advances physics, and explicitly reports the selected omission policy.
"""
import ctypes
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[3]
OUTPUT = Path(os.environ.get("COMPAT_CONTENT_OUTPUT", ROOT / ".work/content"))


class NativeResult(ctypes.Structure):
    _fields_ = [("code", ctypes.c_int32), ("detail", ctypes.c_uint64)]


class Native:
    def __init__(self):
        self.path = Path(os.environ.get(
            "COMPAT_CONTENT_NATIVE_LIBRARY",
            ROOT / ".work/target/debug/libworldrunner_disunity_ffi.so"))
        self.library = ctypes.CDLL(str(self.path))
        types = {
            "disunity_world_create": [ctypes.POINTER(ctypes.c_uint64)],
            "disunity_world_destroy": [ctypes.c_uint64],
            "disunity_world_request": [ctypes.c_uint64, ctypes.c_void_p, ctypes.c_size_t,
                                       ctypes.POINTER(ctypes.c_uint64)],
            "disunity_response_copy": [ctypes.c_uint64, ctypes.c_void_p, ctypes.c_size_t,
                                       ctypes.POINTER(ctypes.c_size_t)],
            "disunity_response_free": [ctypes.c_uint64],
            "disunity_store_statistics": [ctypes.POINTER(ctypes.c_uint64), ctypes.POINTER(ctypes.c_uint64)],
        }
        for name, arguments in types.items():
            function = getattr(self.library, name)
            function.argtypes = arguments
            function.restype = NativeResult
        world = ctypes.c_uint64()
        self.check(self.library.disunity_world_create(ctypes.byref(world)))
        self.world = world.value

    @staticmethod
    def check(result):
        if result.code != 0:
            raise RuntimeError(f"native transport failed: code={result.code}, detail={result.detail}")

    def request(self, value):
        source = json.dumps(value, separators=(",", ":")).encode()
        response = ctypes.c_uint64()
        self.check(self.library.disunity_world_request(
            self.world, source, len(source), ctypes.byref(response)))
        try:
            size = ctypes.c_size_t()
            probe = self.library.disunity_response_copy(response.value, None, 0, ctypes.byref(size))
            if probe.code != 2 or size.value == 0:
                raise RuntimeError(f"unexpected native response probe: {probe.code}")
            buffer = ctypes.create_string_buffer(size.value)
            self.check(self.library.disunity_response_copy(
                response.value, buffer, size.value, ctypes.byref(size)))
            value = json.loads(buffer.raw[:size.value])
            if value.get("ok") is not True:
                raise RuntimeError(f"native request rejected: {value}")
            return value["data"]
        finally:
            self.check(self.library.disunity_response_free(response.value))

    def statistics(self):
        live, allocated = ctypes.c_uint64(), ctypes.c_uint64()
        self.check(self.library.disunity_store_statistics(ctypes.byref(live), ctypes.byref(allocated)))
        return {"live_worlds": live.value, "allocated_world_slots": allocated.value}

    def close(self):
        self.check(self.library.disunity_world_destroy(self.world))


class NativeAdmission(unittest.TestCase):
    def test_actual_three_scene_worlds(self):
        outcomes = {}
        for game in ("snake", "breakout", "pong"):
            with self.subTest(game=game):
                outcomes[game] = self.admit(game)
        (OUTPUT / "native-outcomes.json").write_text(json.dumps(outcomes, indent=2) + "\n")

    def admit(self, game):
        native = Native()
        graph = json.loads((OUTPUT / f"{game}-ready.json").read_text())
        target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / ".work/target-content"))
        result_path = OUTPUT / f"{game}-materialized.json"
        command = [str(target / "debug/examples/materialize"),
                   str(OUTPUT / f"{game}-ready.json"),
                   str(ROOT / f"crates/compat-content/profiles/{game}-admission.json"), str(result_path)]
        transcript = []
        process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, text=True, cwd=ROOT)
        try:
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
            self.assertEqual(process.wait(timeout=30), 0, error)
            result = json.loads(result_path.read_text())
            snapshot = native.request({"op": "snapshot"})
            self.verify_snapshot(native, graph, result, snapshot)
            (OUTPUT / f"{game}-native-snapshot.json").write_text(json.dumps(snapshot, indent=2) + "\n")
            return {"scene_objects": sum(not n["template"] for n in graph["nodes"]),
                    "template_objects": sum(n["template"] for n in graph["nodes"]),
                    "native_objects": len(snapshot["objects"]),
                    "native_components": sum(len(o["components"]) for o in snapshot["objects"]),
                    "native_requests": len(transcript),
                    "skipped_components": result["skipped_components"],
                    "diagnostics": graph["diagnostics"],
                    "native_image": str(native.path),
                    "native_image_sha256": hashlib.sha256(native.path.read_bytes()).hexdigest()}
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=30)
            for stream in (process.stdin, process.stdout, process.stderr):
                if not stream.closed:
                    stream.close()
            (OUTPUT / f"{game}-native-requests.json").write_text(json.dumps(transcript, indent=2) + "\n")
            native.close()

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
