"""Check complete Snake acceptance against actual canonical worker artifacts."""

import argparse
import hashlib
import json
from pathlib import Path

import snake_journey as journey


def verify(record_path):
    record = journey.read_json(record_path)
    journey.require(record["status"] == "pass", f"canonical run did not pass: {record_path}")
    journey.require(all(item["passed"] for item in record["assertions"]), "canonical assertion failed")
    run_dir = journey.work_path(journey.ROOT, journey.ROOT / record["run_dir"])
    request = journey.read_json(run_dir / "request.json")
    updates = [entry["dt"] for entry in request["timeline"] if entry["action"] == "advance"]
    journey.require(bool(updates) and all(0 < dt <= 1 / 60 for dt in updates),
                    "final journey must use native Updates bounded by 1/60 second")
    cfg = request["metadata"]["journey_observed_config"]
    checkpoints = request["metadata"]["journey_checkpoints"]
    report = record["worker"]
    journey.require(report["teardown"] == {"completed": True, "native_worlds": 0,
                    "managed_roots": 0, "pending_readbacks": 0}, "teardown leaked resources")
    pool, states = [], {}
    for entry in report["observations"]:
        if entry["selector"] != journey.WORLD:
            continue
        frame = entry["frame"]
        if frame == checkpoints["fixture-reload"]:
            pool = []
        states[frame] = journey.state_at(report, frame, cfg, pool)
    before_reload = [(frame, state) for frame, state in states.items()
                     if frame < checkpoints["fixture-reload"]]
    foods = []
    for (before_frame, before), (frame, after) in zip(before_reload, before_reload[1:]):
        if after["food"] == before["food"]:
            continue
        journey.require(after["body"][0] == before["food"], "food respawn without reaching observed food")
        journey.require(len(after["body"]) == len(before["body"]), "growth occurred on eating tick")
        grown = states[frame + 1]
        journey.require(len(grown["body"]) == len(after["body"]) + 1, "missing deferred visible growth")
        dt = [action["dt"] for action in request["timeline"]
              if action["action"] == "advance" and action["frame"] == frame + 1]
        journey.require(bool(dt) and sum(dt) < 1 / cfg["rate"], "growth tick did not use increased cadence")
        journey.require(grown["body"][0] != after["body"][0], "shortened cadence did not move native head")
        foods.append({"eating_frame": frame, "growth_frame": frame + 1,
                      "length": len(grown["body"]), "update_dt_total": sum(dt), "native_updates": len(dt)})
    journey.require(len(foods) >= 5, f"requires at least five actual foods; observed {len(foods)}")
    collision = checkpoints["self-collision"]
    before = states[collision - 1]
    journey.require(len(before["body"]) >= 6, "collision requires six visible segments after five foods")
    keys = [a["keys"] for a in request["timeline"] if a["frame"] == collision and a["action"] == "input"]
    target = journey.neighbor(before["body"][0], keys[-1][0], cfg["size"], cfg["wrap"])
    journey.require(target in before["body"][1:-1], "collision destination was not an observed non-tail body cell")
    for frame in range(collision, collision + 3):
        journey.require(states[frame]["body"] == before["body"] and states[frame]["food"] == before["food"],
                        "collision/post-death input changed native body or food")
    journey.require(journey.game_over_lines(run_dir / "stdout.log") == 3, "expected three real source Game Over logs")
    initial = states[checkpoints["initial"]]
    reload_state = states[checkpoints["fixture-reload"]]
    journey.require(reload_state["body"] == initial["body"] and reload_state["food"] == initial["food"],
                    "fixture reload did not restore source spawn and seeded food")
    renewed = states[checkpoints["renewed-control"]]
    journey.require(renewed["body"][0] == journey.neighbor(reload_state["body"][0], "W", cfg["size"], cfg["wrap"]),
                    "W control failed after reload")
    journey.require(states[2]["body"][0] == journey.neighbor(initial["body"][0], "D", cfg["size"], cfg["wrap"]),
                    "opposite A input did not preserve initial right direction")
    for frame, key in zip(range(3, 7), ("W", "A", "S", "D")):
        journey.require(states[frame]["body"][0] == journey.neighbor(states[frame - 1]["body"][0], key, cfg["size"], cfg["wrap"]),
                        f"orthogonal {key} input failed")
    for name, axis in (("wrap-x", 0), ("wrap-y", 1)):
        frame = checkpoints[name]
        journey.require(states[frame - 1]["body"][0][axis] == cfg["size"][axis] - 1
                        and states[frame]["body"][0][axis] == 0, f"missing authored {name} crossing")
    source = journey.source_inventory(Path(record["source_before"]["root"]))
    journey.require(source == record["source_before"]["scripts"] == record["source_after"]["scripts"],
                    "original source changed")
    captures = [journey.work_path(journey.ROOT, run_dir / item["path"]) for item in record["captures"]]
    journey.require(len(captures) == len(checkpoints), "missing checkpoint capture")
    for path, capture in zip(captures, record["captures"]):
        journey.require(hashlib.sha256(path.read_bytes()).hexdigest() == capture["sha256"],
                        f"capture differs from canonical receipt: {path}")
    decoded = [journey.decode_png(path) for path in captures]
    journey.validate_capture_pixels(decoded)
    receipts = [{"path": str(path), "width": image[0], "height": image[1],
                 "pixel_sha256": hashlib.sha256(image[2]).hexdigest()}
                for path, image in zip(captures, decoded)]
    return {"status": "pass", "fixture": record["fixture"], "profile": record["profile"],
            "desktop": request["desktop"], "record": str(record_path), "run_dir": str(run_dir),
            "observed_frames": len(states), "assertions": len(record["assertions"]),
            "native_updates": len(updates), "maximum_update_dt": max(updates),
            "timeline_sha256": hashlib.sha256(json.dumps(request["timeline"], sort_keys=True).encode()).hexdigest(),
            "foods": foods, "game_over_logs": 3, "teardown": report["teardown"],
            "source_hashes": source, "captures": receipts}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("records", nargs="+", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--require-matrix", action="store_true",
                        help="require modern/2017 GPU and desktop runs with identical timeline and pixels")
    args = parser.parse_args()
    try:
        result = {"status": "pass", "runs": [verify(path.resolve()) for path in args.records]}
        if args.require_matrix:
            runs = result["runs"]
            expected = {(profile, desktop) for profile in ("source-6000.3", "compat-2017.1")
                        for desktop in (False, True)}
            journey.require({(run["profile"], run["desktop"]) for run in runs} == expected,
                            "requires modern/2017 GPU and actual desktop records")
            journey.require(len({run["timeline_sha256"] for run in runs}) == 1,
                            "profile/desktop timelines differ")
            pixels = [{Path(capture["path"]).name: capture["pixel_sha256"]
                       for capture in run["captures"]} for run in runs]
            journey.require(all(images == pixels[0] for images in pixels),
                            "canonical pixels differ across profile/desktop replays")
    except (journey.JourneyError, OSError, ValueError, KeyError) as error:
        result = {"status": "fail", "message": str(error)}
    if args.output:
        journey.write_json(journey.work_path(journey.ROOT, args.output), result)
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
