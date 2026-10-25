import importlib.util
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
import zlib

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).parent))
import snake_journey as journey


class RoutingTests(unittest.TestCase):
    def test_routes_around_observed_body_without_reversing(self):
        route = journey.route_to((2, 2), (0, 2), [(1, 2), (1, 1)], "D", (5, 5), False)
        self.assertEqual(route[0], "W")
        cursor = (2, 2)
        for key in route:
            cursor = journey.neighbor(cursor, key, (5, 5), False)
            self.assertNotIn(cursor, [(1, 2), (1, 1)])
        self.assertEqual(cursor, (0, 2))

    def test_wrap_route_crosses_actual_authored_bounds(self):
        self.assertEqual(journey.route_to((4, 1), (0, 1), [], "D", (5, 3), True), ["D"])

    def test_unreachable_food_fails_instead_of_fabricating_path(self):
        with self.assertRaisesRegex(journey.JourneyError, "route"):
            journey.route_to((1, 1), (0, 0), [(2, 1), (1, 0), (1, 2)], "D", (3, 3), False)


class ObservationsTests(unittest.TestCase):
    def test_body_uses_visible_clones_and_pool_order_not_template(self):
        def row(handle, name, point, active=True):
            return {"handle": handle, "name": name, "active_in_hierarchy": active,
                    "transform": {"position": [*point, 0]}}
        snapshot = {"objects": [row(1, "SnakeSegment", (9, 9), False),
                                 row(30, "SnakeSegment(Clone)", (2, 1)),
                                 row(20, "SnakeSegment(Clone)", (3, 1)),
                                 row(40, "SnakeSegment(Clone)", (0, 0), False)]}
        self.assertEqual(journey.visible_body(snapshot, 1, [20, 30]), [(3, 1), (2, 1)])

    def test_absent_public_food_is_a_provider_error(self):
        with self.assertRaisesRegex(journey.JourneyError, "managed:FoodSpawner:CurrentFoodPos"):
            journey.observation({"observations": []}, journey.FOOD, 1)

    def test_missing_worker_is_reported_as_blocked(self):
        with tempfile.TemporaryDirectory() as folder:
            runner = journey.Worker(ROOT, Path(folder), Path(folder) / "absent-worker", {}, Path(folder), Path(folder) / "harness")
            with self.assertRaisesRegex(journey.ProviderBlocked, "worker unavailable"):
                runner.run([], "source-6000.3")


class NativeUpdateTests(unittest.TestCase):
    def test_subdivision_preserves_input_observation_boundaries_and_total_delta(self):
        timeline = [journey.action(1, "input", keys=["W"], axes={}),
                    journey.action(1, "advance", dt=0.125001),
                    journey.action(1, "observe", selector=journey.WORLD)]
        actual = journey.native_updates(timeline)
        self.assertEqual(actual[0], timeline[0])
        self.assertEqual(actual[-1], timeline[-1])
        advances = actual[1:-1]
        self.assertEqual(len(advances), 8)
        self.assertTrue(all(0 < item["dt"] <= 1 / 60 for item in advances))
        self.assertTrue(all(item["frame"] == 1 for item in advances))
        self.assertAlmostEqual(sum(item["dt"] for item in advances), 0.125001)

    def test_zero_delta_start_is_preserved(self):
        timeline = [journey.action(2, "advance", dt=0)]
        self.assertEqual(journey.native_updates(timeline), timeline)


class CaptureTests(unittest.TestCase):
    @staticmethod
    def png(path, pixels):
        def chunk(kind, value):
            return struct.pack(">I", len(value)) + kind + value + struct.pack(">I", zlib.crc32(kind + value))
        header = struct.pack(">IIBBBBB", 2, 1, 8, 6, 0, 0, 0)
        path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) +
                         chunk(b"IDAT", zlib.compress(b"\0" + pixels)) + chunk(b"IEND", b""))

    def test_capture_decodes_pixels_and_rejects_blank_and_truncated(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "capture.png"
            self.png(path, b"\xff\0\0\xff\0\xff\0\xff")
            decoded = journey.decode_png(path)
            self.assertEqual((decoded[0], decoded[1]), (2, 1))
            journey.validate_capture_pixels([decoded])
            self.png(path, b"\0\0\0\xff" * 2)
            with self.assertRaisesRegex(journey.JourneyError, "uniform|blank"):
                journey.validate_capture_pixels([journey.decode_png(path)])
            path.write_bytes(path.read_bytes()[:-4])
            with self.assertRaises(journey.JourneyError):
                journey.decode_png(path)

    def test_unchanged_capture_pixels_do_not_pass_changed_image_gate(self):
        decoded = (2, 1, b"\xff\0\0\xff\0\xff\0\xff")
        with self.assertRaisesRegex(journey.JourneyError, "unchanged"):
            journey.validate_capture_pixels([decoded, decoded])

class TransitionTests(unittest.TestCase):
    def test_food_defers_growth_then_next_tick_adds_one_visible_segment(self):
        before = {"body": [(2, 2)], "food": (3, 2)}
        eating = {"body": [(3, 2)], "food": (4, 4)}
        journey.check_transition(before, eating, (3, 2), pending_growth=False)
        grown = {"body": [(3, 3), (3, 2)], "food": (4, 4)}
        journey.check_transition(eating, grown, (3, 3), pending_growth=True)
        with self.assertRaisesRegex(journey.JourneyError, "growth"):
            journey.check_transition(before, {"body": [(3, 2), (2, 2)], "food": (4, 4)},
                                     (3, 2), pending_growth=False)

    def test_shorter_interval_with_stationary_head_fails_speed_observation(self):
        before = {"body": [(2, 2)], "food": (4, 4)}
        with self.assertRaisesRegex(journey.JourneyError, "head"):
            journey.check_transition(before, before, (3, 2), pending_growth=False)

    def test_self_collision_does_not_move_visible_body_or_respawn_food(self):
        state = {"body": [(2, 2), (2, 1), (1, 1), (1, 2), (1, 3)], "food": (4, 4)}
        journey.check_transition(state, state, (2, 2), pending_growth=False, stationary=True)
        with self.assertRaisesRegex(journey.JourneyError, "collision|stationary"):
            journey.check_transition(state, {**state, "food": (3, 4)}, (2, 2),
                                     pending_growth=False, stationary=True)

    def test_imported_authoring_overrides_source_default_without_sidecar(self):
        graph = {"components": [
            {"script": None},
            {"script": {"type_name": "GameConfig", "fields": {
                "gridSize": {"x": 40, "y": 20}, "wrapAround": 1, "ticksPerSecond": 8,
                "speedIncreasePerFood": 0.3, "maxTicksPerSecond": 20}}},
            {"script": {"type_name": "SnakeView", "fields": {
                "cellSize": 1, "segmentPrefab": {"$ref": "segment-transform"}}}},
            {"script": {"type_name": "FoodSpawner", "fields": {
                "foodVisual": {"$ref": "food-transform"}}}}
        ], "nodes": [{"name": "SneakSegment", "components": ["segment-transform"]},
                     {"name": "Food", "components": ["food-transform"]}]}
        config = journey.config_from_graph(graph)
        self.assertEqual(config["size"], (40, 20))
        self.assertTrue(config["wrap"])
        self.assertEqual(config["segment_name"], "SneakSegment")
        graph["components"][1]["script"]["fields"]["wrapAround"] = 0
        with self.assertRaisesRegex(journey.JourneyError, "config-sidecar variant"):
            journey.config_from_graph(graph)



if __name__ == "__main__":
    unittest.main()
