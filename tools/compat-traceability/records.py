"""Recorded game artifacts qualify their recorded source and image versions only."""
import math
import struct

from model import load, require
from references import digest, local_path


def validate_run_data(record, fixture, proof, identity):
    require(record["status"] == "pass", "recorded run is not pass")
    require(record["profile"] in fixture["profiles"], "recorded profile not admitted by fixture")
    require(record["evidence_label"] == fixture["evidence_label"], "recorded fidelity mismatch")
    before, after = record["source_before"], record["source_after"]
    require(before["revision"] == after["revision"] == identity["revision"], "source revision mismatch")
    require(before["scripts"] == after["scripts"], "original source changed during execution")
    require(len(before["scripts"]) == identity["all_scripts"], "original source population count mismatch")
    gameplay = {p: h for p, h in before["scripts"].items() if p.startswith("Assets/Scripts/")}
    metadata = fixture["metadata"]
    sources = metadata.get("source_sha256", metadata.get("journey_source_hashes"))
    require(gameplay == sources == proof["source_hashes"],
            "compiler/source identity mismatch")
    require(proof["source_count"] == proof["count"] == len(gameplay) == identity["gameplay_scripts"],
            "gameplay compiler source count mismatch")
    require(set(proof["files"]) == set(gameplay), "compiler source file list mismatch")
    require(proof["compiler_inputs_verified"] is True, "compiler inputs not verified")
    require(len(record["assertions"]) == len(fixture["assertions"]), "recorded assertion count mismatch")
    for actual, expected in zip(record["assertions"], fixture["assertions"]):
        require(actual["assertion"] == expected, "recorded assertion payload mismatch")
        require(actual["passed"] is True, "recorded assertion failure")
    require(record["worker"]["teardown"] == {"completed": True, "native_worlds": 0,
            "managed_roots": 0, "pending_readbacks": 0}, "recorded teardown did not drain")


def root_relative(root, value):
    from pathlib import Path
    path = Path(value)
    if path.is_absolute():
        require(path.is_relative_to(root), "recorded artifact path escapes root")
        path = path.relative_to(root)
    return str(path)


def verify_image(root, image, expected):
    path = local_path(root, root_relative(root, image))
    require(digest(path.read_bytes()) == expected, f"recorded image digest mismatch: {path}")


def verify_compile_files(root, run_dir, proof):
    from pathlib import Path
    require(len(set(proof["files"])) == len(proof["files"]), "duplicate compiler source list")
    require(set(proof["files"]) == set(proof["source_hashes"]), "compiler source list mismatch")
    expected = sorted(str(Path(proof["source_root"]) / p) for p in proof["files"])
    require(sorted(proof["compiler_inputs"]) == expected, "compiler input source path mismatch")
    for source, expected_hash in proof["source_hashes"].items():
        name = root_relative(root, Path(proof["source_root"]) / source)
        path = local_path(root, name)
        require(digest(path.read_bytes()) == expected_hash, f"compiler source hash mismatch: {name}")
    path = local_path(root, run_dir + "/build/actual-compile-items.txt")
    actual = path.read_text(encoding="utf-8-sig").splitlines()
    require(sorted(actual) == expected, "actual compiler input manifest mismatch")
    assembly = local_path(root, root_relative(root, proof["assembly"]))
    require(assembly.resolve().is_relative_to((root / run_dir / "build").resolve()),
            "recorded assembly must belong to selected run build")
    verify_image(root, proof["assembly"], proof["assembly_sha256"])


def validate_run_reference(ref, root):
    record = load(local_path(root, ref["path"]))
    fixture = load(local_path(root, ref["fixture"]))
    proof = load(local_path(root, ref["proof"]))
    require(digest(local_path(root, ref["fixture"]).read_bytes()) == ref["fixture_sha256"],
            "recorded fixture identity changed; reviewed requalification required")
    validate_run_data(record, fixture, proof, ref["identity"])
    require(record["run_id"] == ref["run_id"], "recorded run identity mismatch")
    require(record["profile"] == ref["profile"], "selected run profile mismatch")
    require(record["evidence_label"] == ref["fidelity"], "selected run fidelity mismatch")
    require(len(record["assertions"]) == ref["assertions_in_this_run"], "selected assertion count mismatch")
    run_dir = root_relative(root, record["run_dir"])
    require(ref["proof"] == run_dir + "/build/original-source-count.json", "selected run proof mismatch")
    for field, name in (("record_sha256", ref["path"]), ("proof_sha256", ref["proof"]),
                        ("request_sha256", run_dir + "/request.json")):
        require(digest(local_path(root, name).read_bytes()) == ref[field], "selected run artifact digest mismatch")
    request = load(local_path(root, run_dir + "/request.json"))
    require(request["source_root"] == proof["source_root"] == record["source_before"]["root"],
            "selected run source root mismatch")
    require(timeline_matches(request["timeline"], fixture["timeline"]), "recorded timeline mismatch")
    require(request["profile"] == record["profile"] and request["desktop"] == ref["desktop"],
            "recorded profile/presentation mismatch")
    verify_compile_files(root, run_dir, proof)
    native = load(local_path(root, run_dir + "/build/native-image.json"))
    require(native["sha256"] == ref["native_sha256"], "recorded native identity mismatch")
    verify_image(root, native["image"], native["sha256"])
    expected_images = {run_dir + "/build/bridge/bin/CompatibilityBridge/debug/" + name
                       for name in ("CompatibilityBridge.dll", "UnityCompat.dll")}
    require({image["path"] for image in ref["managed_images"]} == expected_images,
            "selected run managed image identity mismatch")
    for image in ref["managed_images"]:
        verify_image(root, image["path"], image["sha256"])
    for capture in record["captures"]:
        path = local_path(root, run_dir + "/" + capture["path"])
        data = path.read_bytes()
        require(digest(data) == capture["sha256"], "recorded capture hash mismatch")
        require(data[:8] == b"\x89PNG\r\n\x1a\n", "recorded capture is not PNG")
        require(struct.unpack(">II", data[16:24]) == (capture["width"], capture["height"]),
                "recorded capture dimensions mismatch")


def timeline_matches(actual, expected):
    if len(actual) != len(expected):
        return False
    for left, right in zip(actual, expected):
        if left.keys() != right.keys():
            return False
        for field in right:
            a, b = left[field], right[field]
            if a == b:
                continue
            # serde_json and Python can round an admitted dt one binary64 ULP apart.
            if field != "dt" or not isinstance(a, float) or not isinstance(b, float):
                return False
            if not math.isfinite(a) or not math.isfinite(b) or abs(a - b) > math.ulp(b):
                return False
    return True
