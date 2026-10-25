"""Resolve pinned original sources through the workspace's Rust source fetcher."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def checkout(game):
    result = subprocess.run(
        ["cargo", "run", "--offline", "--quiet", "--jobs", "2",
         "--package", "worldrunner-original-games", "--bin", "original-games",
         "--", "path", game],
        cwd=ROOT, capture_output=True, text=True, timeout=180, check=True)
    return Path(result.stdout.strip())
