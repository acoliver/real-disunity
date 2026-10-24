"""Regression checks for reviewed mappings and selected-run evidence boundaries."""
import copy
import json
from pathlib import Path
import tempfile
import unittest

import build_catalog
import check
from records import verify_compile_files
from references import digest

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / ".work/resumed-traceability"


class MappingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.catalog = build_catalog.build()

    def test_all_95_requirements_have_bounded_executable_mappings(self):
        rows = self.catalog["requirements"]
        self.assertEqual(len(rows), 95)
        self.assertEqual([r["id"] for r in rows if not r["claims"]], [])
        report = check.validate_catalog(self.catalog, ROOT)
        self.assertEqual(report["counts"]["requirements_with_executable_claims"], 95)
        self.assertFalse(report["required_gate"]["passed"])

    def rejects(self, mutate, message):
        data = copy.deepcopy(self.catalog)
        mutate(data)
        with self.assertRaisesRegex(check.Invalid, message):
            check.validate_catalog(data, ROOT)

    def test_missing_required_mapping_rejected(self):
        self.rejects(lambda d: d["requirements"][0].update(claims=[]), "mapping|claim")

    def test_wrong_but_existing_executable_link_rejected(self):
        def change(data):
            row = data["requirements"][0]
            row["evidence"] = ["native-random"]
            row["claims"][0]["evidence"] = "native-random"
        self.rejects(change, "reviewed|mapping")

    def test_missing_declared_case_link_rejected(self):
        self.rejects(lambda d: d["requirements"][0].update(cases=[]), "case.*mapping|declared")

    def test_duplicate_case_link_rejected(self):
        def change(data):
            row = data["requirements"][0]
            row["cases"].append(row["cases"][0])
        self.rejects(change, "duplicate|mapping")

    def test_duplicate_claim_rejected(self):
        def change(data):
            row = data["requirements"][0]
            row["claims"].append(copy.deepcopy(row["claims"][0]))
        self.rejects(change, "duplicate|mapping|claim")

    def test_fixture_claim_requires_its_selected_run_proof(self):
        def change(data):
            data["evidence"] = [r for r in data["evidence"] if not (
                r["kind"] == "run-proof" and r["path"] ==
                ".work/results/snake-cleanup-final/source-6000.3-offscreen.json")]
        self.rejects(change, "selected|run.*proof|lane")

    def test_existing_symbol_cannot_replace_reviewed_evidence_identity(self):
        def change(data):
            first, second = [r for r in data["evidence"] if r["kind"] == "rust-test"][:2]
            first.update({k: v for k, v in second.items() if k != "id"})
        self.rejects(change, "reviewed|identity")

    def test_execution_marker_must_name_selected_test(self):
        def change(data):
            first = next(r for r in data["evidence"] if r["kind"] == "rust-test")
            first["execution"]["marker"] = "test layers::tests::ci_profiles_select_disjoint_managed_and_gpu_requirements ... ok"
        self.rejects(change, "execution|marker|symbol")

    def test_recorded_run_profile_cannot_be_relabelled(self):
        def change(data):
            ref = next(r for r in data["evidence"] if r["kind"] == "run-proof")
            ref["profile"] = "compat-2017.1"
        self.rejects(change, "profile|identity")

    def test_recorded_assertion_total_cannot_be_relabelled(self):
        def change(data):
            ref = next(r for r in data["evidence"] if r["kind"] == "run-proof")
            ref["assertions_in_this_run"] = 1
        self.rejects(change, "assertion.*count|assertion.*total")

    def test_each_required_gap_stays_nonpass(self):
        report = check.validate_catalog(self.catalog, ROOT)
        for identity in ("A-R0038", "A-R0042", "B-R0022", "C-R0027", "UCK-LIFE-005"):
            self.assertIn(identity, report["required_gate"]["nonpass"])

    def test_required_selection_cannot_hide_gap(self):
        self.rejects(lambda d: d["requirements"][0].update(required=False), "required|selection")

    def test_execution_log_digest_mismatch_rejected(self):
        def change(data):
            ref = next(r for r in data["evidence"] if r["kind"] == "rust-test")
            ref["execution"]["sha256"] = "0" * 64
        self.rejects(change, "execution.*digest")

    def test_unreviewed_executable_command_rejected(self):
        def change(data):
            ref = next(r for r in data["evidence"] if r["kind"] == "rust-test")
            ref["command"] = ["sh", "-c", "arbitrary executable"]
        self.rejects(change, "command|reviewed")

    def test_full_spec_completion_is_explicitly_partial(self):
        report = check.validate_catalog(self.catalog, ROOT)
        self.assertEqual(report.get("full_spec", {}).get("completion"), "partial")
        self.assertTrue(report["full_spec"]["remaining_obligations"])


class CompilerArtifactTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(dir=WORK)
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)
        source = self.root / "original/Assets/Scripts/Body.cs"
        source.parent.mkdir(parents=True)
        source.write_text("public class Body {}\n")
        build = self.root / "run/build"
        build.mkdir(parents=True)
        self.assembly = build / "OriginalGame.dll"
        self.assembly.write_bytes(b"recorded assembly")
        self.proof = {"source_root": str(self.root / "original"),
                      "files": ["Assets/Scripts/Body.cs"],
                      "source_hashes": {"Assets/Scripts/Body.cs": digest(source.read_bytes())},
                      "compiler_inputs": [str(source)], "assembly": str(self.assembly),
                      "assembly_sha256": digest(self.assembly.read_bytes())}
        (build / "actual-compile-items.txt").write_text(str(source) + "\n")
        self.source = source

    def test_matching_actual_compiler_source_bytes_admitted(self):
        verify_compile_files(self.root, "run", self.proof)

    def test_actual_source_hash_mismatch_rejected(self):
        self.source.write_text("public class Changed {}\n")
        with self.assertRaisesRegex(check.Invalid, "source.*digest|source.*hash"):
            verify_compile_files(self.root, "run", self.proof)

    def test_duplicate_compiler_inputs_rejected(self):
        self.proof["files"].append(self.proof["files"][0])
        self.proof["compiler_inputs"].append(self.proof["compiler_inputs"][0])
        (self.root / "run/build/actual-compile-items.txt").write_text(
            "\n".join(self.proof["compiler_inputs"]) + "\n")
        with self.assertRaisesRegex(check.Invalid, "duplicate|source.*list"):
            verify_compile_files(self.root, "run", self.proof)

    def test_unrelated_current_engine_source_change_preserves_recorded_proof(self):
        (self.root / "new_engine.rs").write_text("fn changed() {}")
        verify_compile_files(self.root, "run", self.proof)

    def test_recorded_assembly_must_belong_to_selected_run(self):
        other = self.root / "unrelated.dll"
        other.write_bytes(self.assembly.read_bytes())
        self.proof["assembly"] = str(other)
        with self.assertRaisesRegex(check.Invalid, "assembly.*run|run.*assembly"):
            verify_compile_files(self.root, "run", self.proof)

    def test_source_symlink_escape_rejected(self):
        self.source.unlink()
        self.source.symlink_to("/etc/hostname")
        with self.assertRaisesRegex(check.Invalid, "escape|symlink"):
            verify_compile_files(self.root, "run", self.proof)

class BreakoutContactTests(unittest.TestCase):
    def results(self):
        path = ROOT / ".work/breakout-journey/journey-qualification.json"
        return json.loads(path.read_text())["runs"]

    def test_real_qualified_contact_counts_admitted(self):
        from recheck_games import verify_breakout_contacts
        verify_breakout_contacts(self.results())

    def test_missing_actual_physical_contact_rejected(self):
        from recheck_games import verify_breakout_contacts
        rows = self.results()
        for row in rows:
            row["contacts"].pop("Ball/Brick/solid", None)
        with self.assertRaisesRegex(AssertionError, "contact"):
            verify_breakout_contacts(rows)



if __name__ == "__main__":
    unittest.main()
