"""Materialize reviewed mappings. Refreshing hashes requires reviewing changed evidence."""
import json
from pathlib import Path

from claims import CLAIMS, GAPS
from locations import CSHARP, PYTHON, RUST
from model import control_catalog, pending
from references import digest, load

ROOT = Path(__file__).resolve().parents[2]
CI = ".work/results/final-ci-all-5.log"
MANAGED = "original_source_facades_and_native_lifecycle_suites_execute_in_managed_ci"


def code_locations():
    refs = []
    log = (ROOT / CI).read_text()
    for identity, path, symbol, anchor, scope in RUST:
        marker = next(line for line in log.splitlines()
                      if line.startswith("test ") and symbol in line and line.endswith(" ... ok"))
        package = "worldrunner-" + path.split("/")[0]
        refs.append({"id": identity, "kind": "rust-test", "path": "crates/" + path,
                     "symbol": symbol, "anchor": anchor, "scope": scope,
                     "source_sha256": digest((ROOT / "crates" / path).read_bytes()),
                     "command": ["cargo", "test", "-p", package, symbol],
                     "execution": {"path": CI, "marker": marker}})
    for identity, symbol, anchor, scope in CSHARP:
        path = "csharp/Compatibility.Tests/" + symbol.split(".")[0] + ".cs"
        refs.append({"id": identity, "kind": "csharp-test", "path": path,
                     "symbol": symbol, "anchor": anchor, "scope": scope,
                     "source_sha256": digest((ROOT / path).read_bytes()),
                     "registration": {"path": "csharp/Compatibility.Tests/Program.cs", "anchor": symbol},
                     "command": ["cargo", "test", "-p", "worldrunner-disunity-runtime", MANAGED],
                     "execution": {"path": CI, "marker": "test " + MANAGED + " ... ok"}})
    for identity, path, symbol, anchor, scope in PYTHON:
        if identity == "pong-journey":
            anchor = "assert final_scores == [2, 1]"
        refs.append({"id": identity, "kind": "python-check", "path": path,
                     "symbol": symbol, "anchor": anchor, "scope": scope,
                     "source_sha256": digest((ROOT / path).read_bytes()),
                     "command": ["python3", "tools/compat-traceability/recheck_games.py", identity]})
    for ref in refs:
        if "execution" in ref:
            ref["execution"].update(sha256=digest((ROOT / CI).read_bytes()),
                                    source_binding="unverified-historical")
        if ref["id"].startswith("package-"):
            ref["command"] = ["python3", "tools/compat-traceability/package_checks.py", ref["symbol"]]
            import package_checks
            ref["metadata_probe"] = getattr(package_checks, ref["symbol"])(ROOT)
    return refs


def assertion(identity, fixture, index, record, root=ROOT):
    data = load(root / fixture)
    run = load(root / record)
    return {"id": identity, "kind": "fixture-assertion", "path": fixture,
            "index": index, "expected": data["assertions"][index], "record": record,
            "profile": run["profile"], "fidelity": data["evidence_label"],
            "command": ["python3", "tools/compat-traceability/recheck_games.py"]}


def select_index(fixture, predicate, root=ROOT):
    data = load(root / fixture)
    return next(i for i, row in enumerate(data["assertions"]) if predicate(row))


def fixture_locations(root=ROOT):
    snake = "fixtures/snake-journey.json"
    sr = ".work/results/snake-cleanup-final/source-6000.3-offscreen.json"
    pong = "fixtures/pong-journey.json"
    pr = ".work/pong-journey/final-native-current/compat-2017.1-headless-attempt-1.json"
    breakout = "fixtures/breakout-journey-controls-complete-compact.json"
    br = ".work/compat/runs/1792558213933-3936420-0/run.json"
    mb = "fixtures/breakout-journey-multiball-loss-compact.json"
    mr = ".work/compat/runs/1792557987347-3925994-0/run.json"
    rows = [("snake-position", snake, 0, sr), ("snake-food", snake, 2, sr),
            ("snake-active", snake, 3, sr), ("snake-turn", snake, 13, sr),
            ("snake-capture", snake, 1450, sr), ("pong-position", pong, 33, pr),
            ("pong-velocity", pong, 35, pr), ("pong-hidden", pong, 107, pr),
            ("pong-contact", pong, 349, pr), ("breakout-clamp", breakout, 253, br)]
    specs = [
        ("pong-score", pong, pr, lambda a: a.get("pointer", "").endswith("fields/text") and a.get("value") == "1"),
        ("pong-paddle", pong, pr, lambda a: a.get("pointer") == "/objects/21/transform/position/1" and a.get("frame") == 41),

        ("breakout-disabled", mb, mr, lambda a: a.get("value") is False and a.get("pointer", "").endswith("enabled")),
        ("breakout-sprite", mb, mr, lambda a: a.get("phase") == "brick-animation" and a.get("pointer") == "/objects/56/components/3/fields/sprite"),

        ("breakout-capture", breakout, br, lambda a: a["op"] == "capture"),
    ]
    rows.extend((name, path, select_index(path, choose, root), record) for name, path, record, choose in specs)
    return [assertion(*row, root=root) for row in rows]


def state(cause, reason):
    from model import CAUSES
    return {"cause": cause, "status": CAUSES[cause], "reason": reason}


def map_requirements(data):
    for row in data["requirements"]:
        identity = row["id"]
        row["cases"] = [c["id"] for c in data["cases"] if identity in c["declared_requirements"]]
        limitation = GAPS.get(identity, "The listed bounded assertion has no reviewed full-contract "
                              "acceptance decision for every inventoried source condition and profile.")
        row["resolution"] = state("incomplete-assertions", limitation)
        if identity not in CLAIMS:
            row["resolution"] = pending(limitation)
            continue
        ref, scope = CLAIMS[identity]
        row["evidence"] = [ref]
        row["claims"] = [{"evidence": ref, "scope": scope, "limitation": limitation}]
        if identity in {"A-R0038", "A-R0042"}:
            row["resolution"] = state("missing-provider", limitation)
        if identity in {"B-R0022", "C-R0027"}:
            row["resolution"] = state("missing-content", limitation)
    for identity in ("A-R0038", "A-R0042"):
        row = next(r for r in data["requirements"] if r["id"] == identity)
        row["resolution"] = state("missing-provider", GAPS[identity])


def map_cases(data):
    extra = {"UCK-LIFE-004": ["managed-ci"], "UCK-LIFE-005": [],
             "UCK-HAR-001": [], "UCK-HAR-002": [], "UCK-HAR-003": ["native-focus"],
             "UCK-HAR-004": ["runtime-soak"], "UCK-GAME-001": ["snake-journey"],
             "UCK-GAME-002": ["breakout-journey"], "UCK-GAME-003": ["pong-journey"],
             "UCK-GAME-004": [], "UCK-VIS-004": ["gpu-depth"]}
    for case in data["cases"]:
        candidates = {ref for r in data["requirements"] if case["id"] in r["cases"]
                      for ref in r["evidence"]}
        case["evidence"] = extra.get(case["id"], sorted(candidates))
        case["resolution"] = pending("Definition retained at revision 1. Candidate bounded evidence "
                                     "does not establish all expected observations, variants and negative cases.")
    gaps = {"UCK-LIFE-005": ("missing-provider", "No selected engine-thread affinity rejection provider/test is admitted."),
            "UCK-GAME-004": ("missing-content", "External rd-stranded migration content pin and executable closure are absent."),
            "UCK-INV-002": ("pending-expectation", GAPS["B-R0009"]),
            "UCK-OBJ-003": ("pending-expectation", GAPS["A-R0014"]),
            "UCK-COR-002": ("pending-expectation", "Inactive-owner stop semantics remain unapproved; local cancellation is characterized."),
            "UCK-PHY-004": ("pending-expectation", "Collider resize synchronization timing remains an unapproved bounded expectation.")}
    for case in data["cases"]:
        if case["id"] in gaps:
            case["resolution"] = state(*gaps[case["id"]])


def variants():
    specs = [
        ("snake-original-sprite", "UCK-ASSET-003", "source-6000.3", "original-content", False,
         "missing-content", "Original referenced Snake sprite subasset is absent.", ["import-missing"]),
        ("snake-authored-fallback", "UCK-GAME-001", "source-6000.3", "original-source/authored-missing-sprite", True,
         "pending-expectation", "Recorded gameplay/pixels use the explicitly identified authored missing-sprite fallback.", ["snake-journey"]),
        ("snake-explicit-remap", "UCK-ASSET-003", "compat-6000.3", "explicit-sprite-sidecar", False,
         "pending-expectation", "Sidecar diagnostic/remap test is narrower than a complete promoted visual journey.", ["import-overlay"]),
        ("breakout-original-urp", "UCK-VIS-003", "source-6000.3", "original-URP17.3-2D", False,
         "missing-provider", "Selected original URP 2D renderer/light provider is absent.", []),
        ("breakout-unlit", "UCK-GAME-002", "compat-6000.3", "original-source/unlit-material-translation/sprite-bounds-collision", True,
         "pending-expectation", "Recorded unlit/sprite-bounds journey is distinct from original pipeline fidelity.", ["breakout-journey"]),
        ("pong-original-font", "UCK-UI-002", "source-2017.1", "original-font", False,
         "missing-content", "Original font is unavailable; DejaVu admission does not recover its fidelity.", []),
        ("pong-dejavu", "UCK-GAME-003", "compat-2017.1", "original-source/legacy-mappings-dejavu-font", True,
         "pending-expectation", "Recorded legacy mappings and DejaVu glyphs retain their own fidelity claim.", ["pong-journey"]),
        ("pong-historical-editor-tooling", "UCK-ASSET-007", "historical-tests", "NUnit-UnityTest-NSubstitute-Editor", False,
         "excluded", "Eleven historical test scripts/five tooling requirements are excluded from gameplay acceptance.", []),
    ]
    return [{"id": name, "case": case, "profile": profile, "fidelity": fidelity,
             "required": required, "resolution": state(cause, reason), "evidence": refs}
            for name, case, profile, fidelity, required, cause, reason, refs in specs]


def recorded_locations(data):
    sources = [("snake-matrix", ".work/results/snake-cleanup-final/matrix-qualification.json"),
               ("snake-identities", ".work/results/snake-cleanup-final/identities.json"),
               ("snake-qualification", ".work/results/snake-cleanup-final/qualification.json"),
               ("pong-handoff", ".work/pong-journey/final-native-current/handoff.json"),
               ("breakout-qualification", ".work/breakout-journey/journey-qualification.json"),
               ("full-ci-five", CI)]
    for identity, path in sources:
        data["evidence"].append({"id": identity, "kind": "file-digest", "path": path,
                                 "sha256": digest((ROOT / path).read_bytes())})
    data["recorded_runs"] = [
        {"game": "B", "evidence": ["snake-matrix", "snake-identities", "snake-qualification"],
         "lanes": 6, "acceptance": "recorded-only", "assertions_per_run": 1466},
        {"game": "C", "evidence": ["pong-handoff"], "lanes": 5,
         "acceptance": "recorded-only", "assertions_per_run": 366},
        {"game": "A", "evidence": ["breakout-qualification"], "lanes": 12,
         "acceptance": "recorded-only", "scenario_count": 3,
         "assertions_per_scenario": {"multiball-loss": 1989, "controls-complete": 1018, "wide-complete": 2200}},
        {"game": "engineering", "evidence": ["full-ci-five"], "acceptance": "recorded-only",
         "note": "Historical complete CI log, not a current source rerun or catalog acceptance."},
    ]


def build():
    data = control_catalog(ROOT)
    data["evidence"] = code_locations() + fixture_locations()
    map_requirements(data)
    map_cases(data)
    data["variants"] = variants()
    recorded_locations(data)
    from run_locations import run_locations
    data["evidence"].extend(run_locations(ROOT))
    return data


if __name__ == "__main__":
    target = ROOT / "fixtures/compatibility-catalog.json"
    target.write_text(json.dumps(build(), indent=2) + "\n")
