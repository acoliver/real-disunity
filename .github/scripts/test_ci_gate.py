import unittest

import ci_gate


def needs(scope, **results):
    jobs = {name: {"result": "skipped"} for name in ci_gate.BUILDS}
    jobs["classify"] = {"result": "success", "outputs": {"scope": scope}}
    jobs["docs"] = {"result": "success"}
    for name, result in results.items():
        jobs[name.replace("_", "-")] = {**jobs.get(name.replace("_", "-"), {}), "result": result}
    return jobs


FULL = {"cpu": "success", "gpu_static": "success", "gpu_tests": "success"}
CSHARP = {"cpu": "success", "gpu_csharp": "success"}


class Gate(unittest.TestCase):
    def test_each_scope_passes_with_exactly_its_jobs(self):
        self.assertEqual(ci_gate.problems(needs("docs")), [])
        self.assertEqual(ci_gate.problems(needs("csharp", **CSHARP)), [])
        self.assertEqual(ci_gate.problems(needs("full", **FULL)), [])

    def test_a_skipped_required_job_fails(self):
        for scope, jobs in (("csharp", CSHARP), ("full", FULL)):
            for name in jobs:
                with self.subTest(scope=scope, job=name):
                    self.assertTrue(ci_gate.problems(needs(scope, **{**jobs, name: "skipped"})))

    def test_failed_or_cancelled_jobs_fail(self):
        for outcome in ("failure", "cancelled"):
            self.assertTrue(ci_gate.problems(needs("full", **{**FULL, "gpu_tests": outcome})))
            self.assertTrue(ci_gate.problems(needs("csharp", **{**CSHARP, "gpu_csharp": outcome})))

    def test_jobs_outside_the_scope_must_be_skipped(self):
        self.assertTrue(ci_gate.problems(needs("docs", cpu="success")))
        self.assertTrue(ci_gate.problems(needs("csharp", **CSHARP, gpu_tests="success")))
        self.assertTrue(ci_gate.problems(needs("full", **FULL, gpu_csharp="success")))

    def test_classify_and_docs_must_succeed(self):
        jobs = needs("docs")
        jobs["docs"]["result"] = "failure"
        self.assertTrue(ci_gate.problems(jobs))
        jobs = needs("docs")
        jobs["classify"]["result"] = "cancelled"
        self.assertTrue(ci_gate.problems(jobs))

    def test_unknown_or_missing_scope_fails(self):
        for scope in (None, "", "true", "partial"):
            self.assertTrue(ci_gate.problems(needs(scope)))
        self.assertTrue(ci_gate.problems({}))


if __name__ == "__main__":
    unittest.main()
