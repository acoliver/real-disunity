"""Read-only catalog integrity and required-completion gate."""
import argparse
from collections import Counter
import json
from pathlib import Path
import sys

from model import CAUSES, COUNTS, Invalid, control_catalog, definitions, load, populations, require
from references import validate_reference

from records import validate_run_data


EXECUTABLE = {"rust-test", "csharp-test", "python-check", "fixture-assertion"}


def indexed(rows, label):
    result = {}
    for row in rows:
        require(row["id"] not in result, f"duplicate {label} ID: {row['id']}")
        result[row["id"]] = row
    return result


def check_population(rows, expected, label):
    actual = indexed(rows, label)
    reference = indexed(expected, label)
    require(actual.keys() == reference.keys(), f"unknown/missing {label} IDs: "
            f"{sorted(actual.keys() ^ reference.keys())}")
    for identity, row in actual.items():
        for field, value in reference[identity].items():
            require(row[field] == value, f"{label} {identity} {field} contract/source identity mismatch")
    return actual


def resolution(row):
    state = row["resolution"]
    require(state["cause"] in CAUSES, f"unknown resolution cause: {row['id']}")
    require(state["status"] == CAUSES[state["cause"]], f"resolution status mismatch: {row['id']}")
    require(isinstance(state["reason"], str) and len(state["reason"].strip()) >= 12,
            f"specific resolution reason required: {row['id']}")
    require(type(row["required"]) is bool, f"required selection must be boolean: {row['id']}")


def check_links(row, evidence, cases):
    require(len(set(row["evidence"])) == len(row["evidence"]), "duplicate evidence link")
    require(set(row["evidence"]) <= evidence.keys(), f"unknown evidence link: {row['id']}")
    require(set(row.get("cases", [])) <= cases.keys(), f"unknown case link: {row['id']}")
    for claim in row.get("claims", []):
        require(claim["evidence"] in row["evidence"], "claim evidence must be linked")
        require(evidence[claim["evidence"]]["kind"] in EXECUTABLE, "claim needs executable evidence")
        require(claim["scope"].strip() and claim["limitation"].strip(), "bounded claim needs scope/limitation")
    resolution(row)


def validate_variants(variants, cases, evidence):
    rows = indexed(variants, "variant")
    for row in rows.values():
        require(row["case"] in cases, "unknown variant case")
        require(row["profile"] and row["fidelity"], "variant profile/fidelity required")
        check_links(row, evidence, cases)
    return list(rows.values())


def report_counts(data, requirements, evidence):
    counts = dict(COUNTS)
    claims = [row for row in requirements.values() if row["claims"]]
    counts["requirements_with_executable_claims"] = len(claims)
    counts["requirements_without_executable_claims"] = len(requirements) - len(claims)
    counts["evidence_locations"] = len(evidence)
    counts["evidence_by_kind"] = dict(Counter(ref["kind"] for ref in evidence.values()))
    counts["fixture_assertion_locations"] = sum(ref["kind"] == "fixture-assertion"
                                               for ref in evidence.values())
    return counts


def validate_catalog(data, root):
    require(data["schema"] == "compatibility-traceability/1", "unknown catalog schema")
    require(data["counts"] == COUNTS, "definition/requirement/script count distinction violated")
    cases = check_population(data["cases"], definitions(root), "case")
    gameplay, historical, scripts = populations(root)
    requirements = check_population(data["requirements"], gameplay, "requirement")
    check_population(data["historical"]["requirements"], historical, "historical requirement")
    require(data["historical"]["scripts"] == scripts, "historical script identity mismatch")
    evidence = indexed(data["evidence"], "evidence")
    from mappings import validate_requirement_mappings, validate_reviewed_evidence
    validate_requirement_mappings(requirements, cases, evidence)
    validate_reviewed_evidence(evidence, root)
    for ref in evidence.values():
        validate_reference(ref, root)
    for row in list(cases.values()) + list(requirements.values()):
        require(row["required"] is True, f"required baseline selection changed: {row['id']}")
        check_links(row, evidence, cases)
    variants = validate_variants(data["variants"], cases, evidence)
    for run in data["recorded_runs"]:
        require(set(run["evidence"]) <= evidence.keys(), "recorded run evidence missing")
        require(run["acceptance"] == "recorded-only", "recorded execution cannot promote acceptance")
    selected = list(cases.values()) + list(requirements.values()) + variants
    nonpass = {row["id"]: row["resolution"] for row in selected
               if row["required"] and row["resolution"]["status"] != "pass"}
    from reporting import evidence_summary, full_spec
    return {"schema": "compatibility-traceability-report/1", "integrity": "pass",
            "counts": report_counts(data, requirements, evidence),
            "full_spec": full_spec(), "evidence_summary": evidence_summary(evidence),
            "required_gate": {"passed": not nonpass, "nonpass": nonpass},
            "unmapped_requirements": [r["id"] for r in requirements.values() if not r["claims"]],
            "recorded_runs": data["recorded_runs"],
            "acceptance_note": "Provenance validates recorded versions and bounded claims. "
                               "No global support percentage or automatic contract acceptance."}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--catalog", type=Path, default=Path("fixtures/compatibility-catalog.json"))
    parser.add_argument("--gate", choices=("integrity", "required"), default="integrity")
    args = parser.parse_args(argv)
    try:
        report = validate_catalog(load(args.root / args.catalog), args.root)
    except (Invalid, OSError, ValueError, KeyError, TypeError) as error:
        print(json.dumps({"integrity": "error", "error": str(error)}))
        return 2
    print(json.dumps(report, indent=2))
    return int(args.gate == "required" and not report["required_gate"]["passed"])


if __name__ == "__main__":
    sys.exit(main())
