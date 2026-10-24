"""Keep mapping completeness, recorded execution and full acceptance separate."""
from collections import Counter

RESULT_STATUSES = ["pass", "fail", "unsupported", "not-implemented", "blocked", "skipped",
                   "error", "flaky", "inconclusive"]


def full_spec():
    return {"completion": "partial", "governing_spec": "project-plans/unity-tck/spec.md",
            "gap_plan": ".work/resumed-gap-plan/implementation-plan.md",
            "result_statuses": RESULT_STATUSES, "absence_status": "unmeasured",
            "remaining_obligations": [
                "REQ-01/10: full dashboard/catalog filtering, launch authorization and report integration remain unaccepted.",
                "REQ-02/03: handshake is a separately staged slice; request identity, transactions, phase/thread guards and target budgets remain gaps.",
                "REQ-03/04: native lifecycle/coroutine/event scheduling, callback isolation and rational time remain incomplete.",
                "REQ-04: normalized timestamped ingress and package-specific phase projections remain incomplete.",
                "REQ-05: exact overlay preconditions and immutable complete input closure need further acceptance.",
                "REQ-07/09: original missing art/font and URP renderer fidelity remain distinct from admitted translated variants.",
                "REQ-09/11: unified resolved view, correlated capture stamps and migration consumer remain unaccepted.",
                "REQ-10: these bounded mappings do not independently approve every case expectation/profile or replace current-code execution.",
            ]}


def evidence_summary(evidence):
    runs = [ref for ref in evidence.values() if ref["kind"] == "run-proof"]
    historical = [ref for ref in evidence.values() if "execution" in ref]
    return {"selected_recorded_lanes": len(runs),
            "lanes_by_game": dict(Counter(ref["game"] for ref in runs)),
            "recorded_native_providers": dict(Counter(ref["native_sha256"] for ref in runs)),
            "assertions_across_selected_runs": sum(ref["assertions_in_this_run"] for ref in runs),
            "historical_code_test_locations": len(historical),
            "historical_execution_source_binding": "unverified-historical",
            "current_code_full_contract_passes": 0,
            "qualification": "Selected-run hashes and actual original compiler bytes are checked. "
                             "Historical CI markers do not prove execution of current test source. "
                             "Gameplay records retain their recorded provider; no new handshake-provider gameplay is claimed."}
