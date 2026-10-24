import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import check

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / ".work/resumed-traceability"


def digest(value):
    return hashlib.sha256(value).hexdigest()


class ReferenceTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(dir=WORK)
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)
        self.code = "#[test]\nfn public_motion() { assert_eq!(move_body(), 2); }\n"
        (self.root / "motion.rs").write_text(self.code)
        self.ref = {
            "kind": "rust-test", "path": "motion.rs", "symbol": "public_motion",
            "source_sha256": digest(self.code.encode()),
            "anchor": "assert_eq!(move_body(), 2)",
            "command": ["cargo", "test", "public_motion"],
        }

    def rejects(self, ref, message):
        with self.assertRaisesRegex(check.Invalid, message):
            check.validate_reference(ref, self.root)

    def test_real_test_symbol_and_assertion_are_admitted(self):
        check.validate_reference(self.ref, self.root)

    def test_absent_test_symbol_rejected(self):
        self.rejects(dict(self.ref, symbol="absent"), "symbol")

    def test_stale_source_rejected(self):
        (self.root / "motion.rs").write_text(self.code.replace(", 2", ", 3"))
        self.rejects(self.ref, "stale|digest")

    def test_helper_function_is_not_a_test(self):
        code = self.code.replace("#[test]\n", "")
        (self.root / "motion.rs").write_text(code)
        self.rejects(dict(self.ref, source_sha256=digest(code.encode())), "test")

    def test_comment_cannot_supply_test_symbol(self):
        code = "/* " + self.code + " */\n"
        (self.root / "motion.rs").write_text(code)
        self.rejects(dict(self.ref, source_sha256=digest(code.encode())), "symbol")

    def test_wrong_assertion_anchor_rejected(self):
        self.rejects(dict(self.ref, anchor="assert_eq!(move_body(), 99)"), "anchor")

    def test_absent_file_and_path_escape_rejected(self):
        for path in ("missing.rs", "../motion.rs", "/etc/passwd"):
            with self.subTest(path=path):
                self.rejects(dict(self.ref, path=path), "path|missing")

    def fixture_ref(self):
        assertion = {"op": "equal", "selector": "world.snapshot", "pointer": "/score", "value": 1}
        fixture = {"profiles": ["compat-2017.1"], "evidence_label": "dejavu-overlay",
                   "assertions": [assertion, dict(assertion, value=2)]}
        (self.root / "fixture.json").write_text(json.dumps(fixture))
        return {"kind": "fixture-assertion", "path": "fixture.json", "index": 0,
                "expected": assertion, "profile": "compat-2017.1",
                "fidelity": "dejavu-overlay", "command": ["compat-kit", "run", "fixture.json"]}

    def test_exact_fixture_assertion_admitted(self):
        check.validate_reference(self.fixture_ref(), self.root)

    def test_wrong_but_existing_assertion_index_rejected(self):
        self.rejects(dict(self.fixture_ref(), index=1), "assertion")

    def test_absent_assertion_index_rejected(self):
        self.rejects(dict(self.fixture_ref(), index=99), "assertion")

    def test_profile_and_fidelity_cannot_be_substituted(self):
        ref = self.fixture_ref()
        self.rejects(dict(ref, profile="source-6000.3"), "profile")
        self.rejects(dict(ref, fidelity="original-font"), "fidelity")

    def test_recorded_identity_mismatch_rejected(self):
        (self.root / "proof.json").write_text('{"native":{"sha256":"recorded"}}')
        ref = {"kind": "record", "path": "proof.json", "pointer": "/native/sha256",
               "expected": "other-version"}
        self.rejects(ref, "identity|record")

    def test_current_binary_change_does_not_rewrite_recorded_identity(self):
        (self.root / "proof.json").write_text('{"native":{"sha256":"recorded"}}')
        (self.root / "current.so").write_text("new build")
        check.validate_reference({"kind": "record", "path": "proof.json",
                                  "pointer": "/native/sha256", "expected": "recorded"}, self.root)

    def test_unknown_reference_kind_is_rejected(self):
        self.rejects(dict(self.ref, kind="markdown-reference"), "kind")


class CatalogTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from build_catalog import build
        cls.catalog = build()

    def rejects(self, data, message):
        with self.assertRaisesRegex(check.Invalid, message):
            check.validate_catalog(data, ROOT)

    def test_control_catalog_has_distinct_populations(self):
        report = check.validate_catalog(self.catalog, ROOT)
        self.assertEqual(report["counts"]["definitions"], 54)
        self.assertEqual(report["counts"]["compatibility"], 50)
        self.assertEqual(report["counts"]["harness"], 4)
        self.assertEqual(report["counts"]["gameplay_requirements"], 95)
        self.assertEqual(report["counts"]["historical_test_requirements"], 5)
        self.assertEqual(report["counts"]["historical_test_scripts"], 11)

    def test_unknown_missing_and_duplicate_case_ids_rejected(self):
        for change in ("unknown", "missing", "duplicate"):
            data = copy.deepcopy(self.catalog)
            if change == "unknown":
                data["cases"][0]["id"] = "UCK-INV-999"
            elif change == "missing":
                data["cases"].pop()
            else:
                data["cases"].append(copy.deepcopy(data["cases"][0]))
            with self.subTest(change=change):
                self.rejects(data, "case|duplicate")

    def test_unknown_missing_and_duplicate_requirement_ids_rejected(self):
        for change in ("unknown", "missing", "duplicate"):
            data = copy.deepcopy(self.catalog)
            if change == "unknown":
                data["requirements"][0]["id"] = "A-R9999"
            elif change == "missing":
                data["requirements"].pop()
            else:
                data["requirements"].append(copy.deepcopy(data["requirements"][0]))
            with self.subTest(change=change):
                self.rejects(data, "requirement|duplicate")

    def test_historical_test_requirement_cannot_enter_gameplay_population(self):
        data = copy.deepcopy(self.catalog)
        data["requirements"][0]["id"] = "C-R0028"
        self.rejects(data, "requirement")

    def test_assertion_counts_cannot_replace_definition_counts(self):
        data = copy.deepcopy(self.catalog)
        data["counts"]["definitions"] = 1466
        self.rejects(data, "count")

    def test_changed_contract_and_source_identity_rejected(self):
        for field in ("contract", "source"):
            data = copy.deepcopy(self.catalog)
            data["requirements"][0][field] = "unrelated"
            with self.subTest(field=field):
                self.rejects(data, "identity|contract|source")

    def test_gap_requires_specific_reason_and_matching_status(self):
        for gap, status in (("pending-expectation", "pass"), ("missing-provider", "blocked")):
            data = copy.deepcopy(self.catalog)
            data["requirements"][0]["resolution"] = {"reason": "", "cause": gap, "status": status}
            with self.subTest(gap=gap):
                self.rejects(data, "resolution|reason|status")

    def test_unmapped_required_requirement_fails_gate(self):
        report = check.validate_catalog(self.catalog, ROOT)
        self.assertFalse(report["required_gate"]["passed"])
        self.assertIn("A-R0001", report["required_gate"]["nonpass"])

    def test_unknown_evidence_and_case_links_rejected(self):
        for field, value in (("evidence", ["absent-evidence"]), ("cases", ["UCK-NO-001"])):
            data = copy.deepcopy(self.catalog)
            data["requirements"][0][field] = value
            with self.subTest(field=field):
                self.rejects(data, "evidence|case")

    def test_pending_expected_semantics_cannot_become_pass_from_provenance(self):
        data = copy.deepcopy(self.catalog)
        data["requirements"][0]["resolution"]["status"] = "pass"
        self.rejects(data, "status|resolution")


class RecordedRunTests(unittest.TestCase):
    def setUp(self):
        self.sources = {f"Assets/Scripts/Type{i}.cs": "source-hash" for i in range(7)}
        self.fixture = {"metadata": {"source_sha256": self.sources}, "profiles": ["compat-6000.3"],
                        "evidence_label": "unlit", "assertions": [{"op": "equal", "value": 1}]}
        self.record = {"status": "pass", "profile": "compat-6000.3", "evidence_label": "unlit",
                       "source_before": {"revision": "pinned", "scripts": self.sources},
                       "source_after": {"revision": "pinned", "scripts": self.sources},
                       "assertions": [{"assertion": self.fixture["assertions"][0], "passed": True}],
                       "worker": {"teardown": {"completed": True, "native_worlds": 0,
                                                "managed_roots": 0, "pending_readbacks": 0}}}
        self.proof = {"source_count": 7, "count": 7, "source_hashes": self.sources,
                      "files": list(self.sources), "compiler_inputs_verified": True}
        self.identity = {"revision": "pinned", "gameplay_scripts": 7, "all_scripts": 7}

    def check(self):
        check.validate_run_data(self.record, self.fixture, self.proof, self.identity)

    def test_matching_recorded_source_and_assertions_admitted(self):
        self.check()

    def test_wrong_source_pin_rejected(self):
        self.record["source_before"]["revision"] = "other"
        with self.assertRaisesRegex(check.Invalid, "source|revision"):
            self.check()

    def test_compiler_input_identity_mismatch_rejected(self):
        self.proof["source_hashes"] = dict(self.sources, extra="other")
        with self.assertRaisesRegex(check.Invalid, "source|compiler"):
            self.check()

    def test_wrong_recorded_assertion_payload_rejected(self):
        self.record["assertions"][0]["assertion"] = {"op": "equal", "value": 99}
        with self.assertRaisesRegex(check.Invalid, "assertion"):
            self.check()

    def test_false_recorded_assertion_rejected(self):
        self.record["assertions"][0]["passed"] = False
        with self.assertRaisesRegex(check.Invalid, "assertion"):
            self.check()

    def test_historical_population_cannot_replace_gameplay_source_count(self):
        self.proof["source_count"] = 18
        with self.assertRaisesRegex(check.Invalid, "count"):
            self.check()


if __name__ == "__main__":
    unittest.main()
