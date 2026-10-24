"""Admission pins for all supplied current-fixture recorded matrix lanes."""
from model import INVENTORY, load
from records import root_relative
from references import digest


def lanes(root):
    snake = load(root / ".work/results/snake-cleanup-final/qualification.json")["runs"]
    for row in snake:
        yield "B", "fixtures/snake-journey.json", row["record"], row["desktop"]
    pong = load(root / ".work/pong-journey/final-native-current/handoff.json")
    for row in pong["runs"]:
        request = load(root / row["run_dir"] / "request.json")
        yield "C", pong["fixture"], row["record"], request["desktop"]
    breakout = load(root / ".work/breakout-journey/journey-qualification.json")["runs"]
    for row in breakout:
        record = f".work/compat/runs/{row['run_id']}/run.json"
        yield "A", row["fixture_identity"]["path"], record, row["desktop"]


def managed_images(root, folder):
    images = []
    for name in ("CompatibilityBridge.dll", "UnityCompat.dll"):
        path = folder + "/build/bridge/bin/CompatibilityBridge/debug/" + name
        images.append({"path": path, "sha256": digest((root / path).read_bytes())})
    return images


def run_locations(root):
    inventory = {g["id"]: g for g in load(root / INVENTORY)["games"]}
    refs = []
    for game, fixture, path, desktop in lanes(root):
        path = root_relative(root, path)
        record = load(root / path)
        folder = root_relative(root, record["run_dir"])
        proof = folder + "/build/original-source-count.json"
        native = load(root / folder / "build/native-image.json")
        counts = inventory[game]["scriptCount"]
        refs.append({"id": "run-" + record["run_id"], "kind": "run-proof", "path": path,
                     "game": game, "run_id": record["run_id"], "fixture": fixture,
                     "fixture_sha256": digest((root / fixture).read_bytes()), "proof": proof,
                     "record_sha256": digest((root / path).read_bytes()),
                     "proof_sha256": digest((root / proof).read_bytes()),
                     "request_sha256": digest((root / folder / "request.json").read_bytes()),
                     "managed_images": managed_images(root, folder),
                     "identity": {"revision": inventory[game]["verifiedPin"],
                                  "gameplay_scripts": counts["gameplaySupport"],
                                  "all_scripts": counts["allAssetsCSharp"]},
                     "native_sha256": native["sha256"], "desktop": desktop,
                     "profile": record["profile"], "fidelity": record["evidence_label"],
                     "assertions_in_this_run": len(record["assertions"]),
                     "outcomes": {"compile": "pass", "content": "pass-for-labelled-variant",
                                  "semantic": "recorded-assertions-pass",
                                  "desktop": "recorded-pass" if desktop else "unmeasured",
                                  "exploratory": "unmeasured", "fidelity": record["evidence_label"]}})
    return refs
