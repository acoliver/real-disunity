"""Immutable populations and lossless catalog definition import."""
import json
import re
from pathlib import Path

CASE_DOC = "docs/unity-compatibility-kit-cases.md"
INVENTORY = "docs/training-only-unity6-reference/game-comparison-inventory.json"
COUNTS = {
    "definitions": 54, "compatibility": 50, "harness": 4,
    "gameplay_requirements": 95, "requirements_by_game": {"A": 43, "B": 25, "C": 27},
    "historical_test_requirements": 5, "historical_test_scripts": 11,
    "gameplay_support_scripts": 35, "all_authored_scripts": 46,
}
CAUSES = {
    "pending-expectation": "inconclusive", "incomplete-assertions": "inconclusive",
    "missing-provider": "not-implemented", "missing-content": "blocked",
    "excluded": "unsupported", "absent-run": "unmeasured",
}


class Invalid(ValueError):
    """The claimed traceability cannot be verified."""


def require(condition, message):
    if not condition:
        raise Invalid(message)


def load(path):
    return json.loads(Path(path).read_text())


def expand_requirements(text):
    result = []
    pattern = r"([ABC])-R(\d{4}(?:[–-]\d{4})?(?:/\d{4}(?:[–-]\d{4})?)*)"
    for game, forms in re.findall(pattern, text):
        for form in forms.split("/"):
            ends = re.split("[–-]", form)
            start, end = int(ends[0]), int(ends[-1])
            result.extend(f"{game}-R{number:04}" for number in range(start, end + 1))
    return sorted(set(result))


def definitions(root):
    rows = []
    group = ""
    for line in (root / CASE_DOC).read_text().splitlines():
        if line.startswith("## "):
            group = line[3:]
        match = re.match(r"\| \*\*(UCK-[A-Z]+-\d{3}) · (P\d)\*\* \|", line)
        if not match:
            continue
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        rows.append({"id": match[1], "priority": match[2], "contract_revision": 1,
                     "population": "harness" if "-HAR-" in match[1] else "compatibility",
                     "group": group, "definition_cells": cells[1:],
                     "declared_requirements": expand_requirements(cells[-1])})
    require(len(rows) == 54, "case definition source no longer has 54 cases")
    return rows


def populations(root):
    inventory = load(root / INVENTORY)
    gameplay, historical, scripts = [], [], []
    for game in inventory["games"]:
        for row in game["requirements"]:
            gameplay.append(requirement(row, game))
        for row in game["testRequirements"]:
            historical.append(requirement(row, game))
        scripts.extend({"game": game["id"], **row} for row in game["scriptInventory"]
                       if row["role"] == "test")
    require(len(gameplay) == 95 and len(historical) == 5, "requirement inventory count changed")
    require(len(scripts) == 11, "historical script count changed")
    return gameplay, historical, scripts


def requirement(row, game):
    return {"id": row["id"], "game": game["id"], "kind": row["kind"],
            "contract": row["contract"], "source": row["source"],
            "comparison_status": row["status"], "guide": row["guide"],
            "source_identity": {"game": game["id"], "revision": game["verifiedPin"]}}


def pending(reason="Independent assertion coverage and contract acceptance remain pending."):
    return {"cause": "pending-expectation", "status": "inconclusive", "reason": reason}


def control_catalog(root):
    cases = definitions(root)
    gameplay, historical, scripts = populations(root)
    for row in cases:
        row.update(required=True, evidence=[], resolution=pending())
    for row in gameplay:
        row.update(required=True, cases=[], evidence=[], claims=[], resolution=pending())
    return {"schema": "compatibility-traceability/1", "counts": COUNTS.copy(),
            "cases": cases, "requirements": gameplay, "evidence": [], "variants": [],
            "historical": {"requirements": historical, "scripts": scripts}, "recorded_runs": []}
