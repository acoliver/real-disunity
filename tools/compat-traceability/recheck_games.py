"""Recheck recorded real-game evidence without providers, builds or desktop processes."""
import json
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]


def snake():
    sys.path.insert(0, str(ROOT / "tools/compat-play"))
    from check_snake_journey import verify
    qualification = json.loads((ROOT / ".work/results/snake-cleanup-final/qualification.json").read_text())
    results = [verify(Path(row["record"])) for row in qualification["runs"]]
    assert len(results) == 6 and all(row["assertions"] == 1466 for row in results)
    return {"game": "B", "recorded_lanes_rechecked": len(results), "assertions_per_run": 1466}


def pong():
    sys.path.insert(0, str(ROOT / "tools/compat-pong"))
    from verify import verify_record
    handoff = json.loads((ROOT / ".work/pong-journey/final-native-current/handoff.json").read_text())
    results = [verify_record(ROOT / row["record"], ROOT / handoff["fixture"]) for row in handoff["runs"]]
    assert len(results) == 5
    return {"game": "C", "recorded_lanes_rechecked": len(results), "assertions_per_run": 366}


def verify_breakout_contacts(results):
    required = {"Ball/Brick/solid", "Ball/Tilemap/solid", "Ball/Platform/solid", "Ball/DeathZone/trigger"}
    actual = {kind for row in results for kind, count in row["contacts"].items() if count > 0}
    assert required <= actual, ("missing actual physical contact", sorted(required - actual))


def breakout():
    sys.path.insert(0, str(ROOT / "tools/compat-breakout"))
    from qualify import compare_captures, renewed_control
    from verify import verify_record
    qualification = json.loads((ROOT / ".work/breakout-journey/journey-qualification.json").read_text())
    results = []
    for row in qualification["runs"]:
        path = ROOT / ".work/compat/runs" / row["run_id"] / "run.json"
        summary = verify_record(path, ROOT / row["fixture_identity"]["path"])
        for field in ("speed_range", "loss", "contacts", "width_lifetime", "break_sprite_counts"):
            assert summary[field] == row[field], (row["run_id"], field)
        assert all(abs(speed - 6) < .001 for speed in summary["speed_range"])
        if "controls-complete" in row["fixture"]:
            assert renewed_control(json.loads(path.read_text())["worker"])
        results.append(summary)
    verify_breakout_contacts(results)
    comparisons, gaps = compare_captures(results)
    assert len(results) == 12 and not gaps, gaps
    return {"game": "A", "recorded_lanes_rechecked": len(results),
            "scenarios": 3, "pixel_comparisons": comparisons}


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--child":
        result = {"A": breakout, "B": snake, "C": pong}[sys.argv[2]]()
        print(json.dumps(result))
        return 0
    results = []
    for game in ("A", "B", "C"):
        child = subprocess.run([sys.executable, "-B", str(Path(__file__).resolve()), "--child", game],
                               cwd=ROOT, capture_output=True, text=True, timeout=180)
        if child.returncode:
            print(json.dumps({"game": game, "status": "error", "stderr": child.stderr}))
            return 2
        results.append(json.loads(child.stdout))
    print(json.dumps({"status": "pass", "scope": "recorded-version revalidation only",
                      "games": results}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
