import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class OverlayPreconditions(unittest.TestCase):
    def test_shipped_fixtures_retain_reviewed_original_canvas_preconditions(self):
        sidecar = json.loads(
            (ROOT / "crates/compat-content/overlays/pong.json").read_text()
        )
        for name in ("pong", "pong-journey", "pong-interactive"):
            with self.subTest(fixture=name):
                fixture = json.loads((ROOT / "fixtures" / (name + ".json")).read_text())
                self.assertEqual(fixture["metadata"]["compatibility_sidecar"], sidecar)
                for change in fixture["metadata"]["compatibility_sidecar"]["field_overrides"]:
                    self.assertEqual(change["precondition"], {
                        "schema_version": 1, "class_id": 224, "expected_value": 0
                    })
                    self.assertEqual(change["file_id"], 1901759440)
                    self.assertEqual(change["value"], 1)


if __name__ == "__main__":
    unittest.main()
