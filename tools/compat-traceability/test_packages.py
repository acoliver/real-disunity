import json
from pathlib import Path
import tempfile
import unittest

from model import Invalid
import package_checks

ROOT = Path(__file__).resolve().parents[2]


class PackageMetadataTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(dir=ROOT / ".work/resumed-traceability")
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)
        self.write("breakout", {"com.unity.render-pipelines.universal": "17.3.0",
                                "com.unity.inputsystem": "1.17.0"})
        self.write("snake", {"com.unity.feature.2d": "2.0.2"})

    def write(self, game, dependencies):
        path = self.root / ".work/games" / game / "Packages/manifest.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps({"dependencies": dependencies}))

    def test_original_metadata_is_separate_from_provider_acceptance(self):
        for probe in (package_checks.breakout_urp, package_checks.breakout_input,
                      package_checks.snake_tooling):
            with self.subTest(probe=probe.__name__):
                result = probe(self.root)
                self.assertEqual(result["metadata"], "pass")
                self.assertEqual(result["provider"], "unmeasured")

    def test_changed_original_package_version_rejected(self):
        self.write("breakout", {"com.unity.inputsystem": "1.18.0"})
        with self.assertRaisesRegex(Invalid, "version|package"):
            package_checks.breakout_input(self.root)

    def test_missing_original_2d_tooling_rejected(self):
        self.write("snake", {})
        with self.assertRaisesRegex(Invalid, "package"):
            package_checks.snake_tooling(self.root)


if __name__ == "__main__":
    unittest.main()
