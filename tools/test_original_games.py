"""Exercise the Python source resolver against the committed Rust fetcher."""
import subprocess
import unittest

from original_games import ROOT, checkout


class OriginalGameTests(unittest.TestCase):
    def test_known_games_resolve_to_clean_pinned_checkouts_outside_scratch(self):
        for game in ("snake", "breakout", "pong"):
            with self.subTest(game=game):
                source = checkout(game)
                self.assertTrue(source.is_relative_to(ROOT / "target/original-games"))
                self.assertTrue((source / "Assets/Scripts").is_dir())
                self.assertFalse(source.is_relative_to(ROOT / ".work"))
                self.assertEqual(checkout(game), source)

    def test_unknown_game_is_rejected_by_the_source_fetcher(self):
        with self.assertRaises(subprocess.CalledProcessError):
            checkout("unknown")


if __name__ == "__main__":
    unittest.main()
