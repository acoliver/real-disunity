"""Reviewed executable identities and requirement-to-assertion bindings."""
from collections import Counter

from claims import CLAIMS
from locations import CSHARP, PYTHON, RUST
from model import INVENTORY, load, require

EXECUTABLE = {"rust-test", "csharp-test", "python-check", "fixture-assertion"}


def code_identities():
    expected = {}
    for identity, path, symbol, anchor, _ in RUST:
        command = ["cargo", "test", "-p", "worldrunner-" + path.split("/")[0], symbol]
        expected[identity] = ("rust-test", "crates/" + path, symbol, anchor, command)
    for identity, symbol, anchor, _ in CSHARP:
        path = "csharp/Compatibility.Tests/" + symbol.split(".")[0] + ".cs"
        command = ["cargo", "test", "-p", "worldrunner-disunity-runtime",
                   "original_source_facades_and_native_lifecycle_suites_execute_in_managed_ci"]
        expected[identity] = ("csharp-test", path, symbol, anchor, command)
    for identity, path, symbol, anchor, _ in PYTHON:
        if identity == "pong-journey":
            anchor = "assert final_scores == [2, 1]"
        command = ["python3", "tools/compat-traceability/recheck_games.py", identity]
        if identity.startswith("package-"):
            command = ["python3", "tools/compat-traceability/package_checks.py", symbol]
        expected[identity] = ("python-check", path, symbol, anchor, command)
    return expected


def validate_reviewed_evidence(evidence, root):
    from build_catalog import fixture_locations
    expected = code_identities()
    actual = {key: (ref["kind"], ref["path"], ref.get("symbol"), ref.get("anchor"), ref["command"])
              for key, ref in evidence.items() if ref["kind"] in EXECUTABLE - {"fixture-assertion"}}
    require(actual == expected, "reviewed executable identity mapping mismatch")
    fixtures = {ref["id"]: ref for ref in fixture_locations(root)}
    actual_fixtures = {key: ref for key, ref in evidence.items() if ref["kind"] == "fixture-assertion"}
    require(actual_fixtures == fixtures, "reviewed fixture assertion identity mapping mismatch")
    runs = [ref for ref in evidence.values() if ref["kind"] == "run-proof"]
    require(Counter(ref["game"] for ref in runs) == {"A": 12, "B": 6, "C": 5},
            "selected run proof lane population mismatch")
    require(len({ref["run_id"] for ref in runs}) == len(runs), "duplicate selected run identity")
    identities = {g["id"]: g for g in load(root / INVENTORY)["games"]}
    for ref in runs:
        game = identities[ref["game"]]
        require(ref["identity"] == {"revision": game["verifiedPin"],
                "gameplay_scripts": game["scriptCount"]["gameplaySupport"],
                "all_scripts": game["scriptCount"]["allAssetsCSharp"]}, "run source inventory identity mismatch")
    for ref in actual_fixtures.values():
        selected = [run for run in runs if run["path"] == ref["record"] and run["fixture"] == ref["path"]]
        require(len(selected) == 1, "fixture assertion requires its selected run proof")


def validate_requirement_mappings(requirements, cases, evidence):
    require(requirements.keys() == CLAIMS.keys(), "reviewed requirement mapping population mismatch")
    for identity, row in requirements.items():
        declared = [case["id"] for case in cases.values() if identity in case["declared_requirements"]]
        require(row["cases"] == declared, f"declared case mapping mismatch: {identity}")
        ref, scope = CLAIMS[identity]
        require(row["evidence"] == [ref], f"reviewed evidence mapping mismatch: {identity}")
        require(len(row["claims"]) == 1, f"one reviewed bounded claim mapping required: {identity}")
        claim = row["claims"][0]
        require(claim["evidence"] == ref and claim["scope"] == scope,
                f"reviewed bounded claim mapping mismatch: {identity}")
        require(claim["limitation"] == row["resolution"]["reason"], "claim limitation/resolution mismatch")
        require(evidence[ref]["kind"] in EXECUTABLE, "mapping needs executable assertion")
