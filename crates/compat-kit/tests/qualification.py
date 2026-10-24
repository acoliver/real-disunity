"""Qualification source/proof checks using real files and the public gate command."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("qualification", ROOT / "fixtures/gates/qualification.py")
module = importlib.util.module_from_spec(spec)


class QualificationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec.loader.exec_module(module)

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"])
        self.addCleanup(self.directory.cleanup)
        self.source = Path(self.directory.name)
        (self.source / "Assets/Scripts").mkdir(parents=True)
        (self.source / "Assets/Scripts/Game.cs").write_text("class Game {}")
        self.selection = ["Assets/Scripts/Game.cs"]

    def test_explicit_selection_rejects_missing_and_includes_added_scripts(self):
        self.assertEqual(len(module.select_sources(self.source, self.selection)), 1)
        with self.assertRaisesRegex(RuntimeError, "source selection"):
            module.select_sources(self.source, self.selection + ["Assets/Scripts/Missing.cs"])
        (self.source / "Assets/Scripts/New.cs").write_text("class New {}")
        self.assertEqual(len(module.select_sources(self.source, self.selection)), 2)

    def test_source_hashes_are_current_and_proof_rejects_changed_source(self):
        before = module.select_sources(self.source, self.selection)
        (self.source / self.selection[0]).write_text("class Edited {}")
        after = module.select_sources(self.source, self.selection)
        self.assertNotEqual(before, after)
        with self.assertRaisesRegex(RuntimeError, "source changed"):
            module.verify_sources(self.source, self.selection, before)
        module.verify_sources(self.source, self.selection, after)

    def test_source_selection_rejects_symlinks_and_traversal(self):
        (self.source / "Assets/Scripts/Game.cs").unlink()
        target = self.source / "Outside.cs"
        target.write_text("class Outside {}")
        (self.source / "Assets/Scripts/Game.cs").symlink_to(target)
        with self.assertRaisesRegex(RuntimeError, "symlink"):
            module.select_sources(self.source, self.selection)
        with self.assertRaisesRegex(RuntimeError, "source selection"):
            module.select_sources(self.source, ["../Outside.cs"])


if __name__ == "__main__":
    unittest.main()
