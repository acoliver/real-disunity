"""Bounded original package metadata probes; provider compatibility stays unmeasured."""
import argparse
import json
from pathlib import Path

from model import load, require
from references import digest, local_path


def package_version(root, game, package, expected):
    name = f".work/games/{game}/Packages/manifest.json"
    path = local_path(root, name)
    actual = load(path)["dependencies"].get(package)
    require(actual == expected, f"original package version mismatch: {package}: {actual}")
    return {"metadata": "pass", "provider": "unmeasured", "path": name,
            "sha256": digest(path.read_bytes()), "package": package, "version": actual}


def breakout_urp(root):
    return package_version(root, "breakout", "com.unity.render-pipelines.universal", "17.3.0")


def breakout_input(root):
    return package_version(root, "breakout", "com.unity.inputsystem", "1.17.0")


def snake_tooling(root):
    return package_version(root, "snake", "com.unity.feature.2d", "2.0.2")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", choices=("breakout_urp", "breakout_input", "snake_tooling"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    print(json.dumps(globals()[args.probe](root), indent=2))


if __name__ == "__main__":
    main()
