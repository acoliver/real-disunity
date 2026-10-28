import json
import unittest

import ci_report as cr

CONTEXT = {
    "repo": "acoliver/real-disunity",
    "run_id": "42",
    "run_url": "https://github.com/acoliver/real-disunity/actions/runs/42",
    "sha": "abc123",
    "workflow": "Platforms",
}
BOT = {"login": "github-actions[bot]"}
LABELS = [{"name": "ci-failure"}]


def job(name, conclusion, steps=()):
    return {
        "name": name,
        "html_url": f"https://example.invalid/{name.replace(' ', '-')}",
        "conclusion": conclusion,
        "steps": [{"name": step, "conclusion": result} for step, result in steps],
    }


def issue(number, body, user=BOT, labels=LABELS, **extra):
    return {"number": number, "body": body, "user": user, "labels": labels, **extra}


class FakeGh:
    """Serves canned API answers and records every mutating call with its stdin."""

    def __init__(self, jobs=(), issues=()):
        self.jobs = list(jobs)
        self.issues = list(issues)
        self.calls = []
        self.fail_jobs = False

    def __call__(self, args, stdin=None):
        if args[0] == "api" and "/actions/runs/" in args[1]:
            if self.fail_jobs:
                raise RuntimeError("jobs unavailable")
            return "\n".join(json.dumps(item) for item in self.jobs)
        if args[0] == "api":
            return "\n".join(json.dumps(item) for item in self.issues)
        self.calls.append((args, stdin))
        return ""

    def verbs(self):
        return [" ".join(args[:2]) for args, _ in self.calls]


class Pure(unittest.TestCase):
    def test_only_failed_cancelled_or_unknown_results_count_as_failures(self):
        needs = {
            "a": {"result": "success"},
            "b": {"result": "skipped"},
            "c": {"result": "failure"},
            "d": {"result": "cancelled"},
            "e": {"result": "mystery"},
        }
        self.assertEqual(cr.failed_needs(needs), ["c", "d", "e"])
        self.assertEqual(cr.failed_needs({}), [])

    def test_setup_steps_are_infrastructure(self):
        for step in ("Set up job", "Run actions/checkout@abc", "Install software Vulkan",
                     "Post Run actions/checkout", "Complete job", "Run Swatinem/rust-cache@d15",
                     "Run dtolnay/rust-toolchain@683"):
            self.assertEqual(cr.classify_failure("failure", step), "infrastructure", step)

    def test_other_steps_are_build_or_test_and_cancellation_wins(self):
        self.assertEqual(cr.classify_failure("failure", "cargo xtask ci-cpu"), "build or test")
        self.assertEqual(cr.classify_failure("timed_out", "cargo xtask ci-cpu"), "build or test")
        self.assertEqual(cr.classify_failure("cancelled", "cargo xtask ci-cpu"), "cancelled")
        self.assertEqual(cr.classify_failure("cancelled", None), "cancelled")
        self.assertEqual(cr.classify_failure("failure", None), "infrastructure")

    def test_failed_jobs_report_the_first_failed_step(self):
        jobs = [
            job("ok", "success", [("Set up job", "success")]),
            job("cpu (windows-latest)", "failure",
                [("Set up job", "success"), ("cargo", "failure"), ("Post", "failure")]),
            job("lavapipe", "timed_out", [("Run tests", "cancelled")]),
            job("still running", None),
        ]
        found = cr.failed_jobs(jobs)
        self.assertEqual([item.name for item in found], ["cpu (windows-latest)", "lavapipe"])
        self.assertEqual(found[0].step, "cargo")
        self.assertEqual(found[0].kind, "build or test")

    def test_job_lines_parse_one_object_per_line(self):
        self.assertEqual(cr.parse_job_lines('{"a": 1}\n\n{"b": 2}\n'), [{"a": 1}, {"b": 2}])

    def test_table_escapes_pipes(self):
        table = cr.job_table([cr.FailedJob("a|b", "u", "s|t", "infrastructure")])
        self.assertIn("a\\|b", table)
        self.assertIn("s\\|t", table)

    def test_only_bot_issues_with_label_and_marker_match(self):
        marker = cr.marker("Platforms")
        issues = [
            issue(1, f"{marker}\nbody"),
            issue(2, f"{marker}\nbody", user={"login": "acoliver"}),
            issue(3, f"{marker}\nbody", labels=[{"name": "bug"}]),
            issue(4, f"{cr.marker('PR')}\nbody"),
            issue(5, "no marker"),
            issue(6, f"{marker}\nbody", pull_request={"url": "x"}),
            issue(7, None),
            issue(8, f"text {marker}"),
        ]
        self.assertEqual(cr.matching_issues(issues, "Platforms"), [1, 8])
        self.assertEqual(cr.matching_issues(issues, "PR"), [4])


class Reporting(unittest.TestCase):
    failing = {"cpu": {"result": "failure"}, "managed": {"result": "success"}}
    passing = {"cpu": {"result": "success"}, "lavapipe": {"result": "skipped"}}

    def test_first_failure_creates_one_labelled_issue_with_marker_and_table(self):
        gh = FakeGh(jobs=[job("cpu (macos-latest)", "failure", [("cargo", "failure")])])
        self.assertEqual(cr.report(gh, CONTEXT, self.failing, False), "failure reported")
        self.assertEqual(gh.verbs(), ["label create", "issue create"])
        args, body = gh.calls[1]
        self.assertIn("CI failure on main: Platforms", args)
        self.assertIn("ci-failure", args)
        self.assertEqual(args[args.index("--body-file") + 1], "-")
        self.assertTrue(body.startswith(cr.marker("Platforms")))
        for text in ("abc123", CONTEXT["run_url"], "cpu (macos-latest)", "build or test"):
            self.assertIn(text, body)

    def test_repeat_failure_comments_on_the_existing_issue(self):
        existing = issue(9, f"{cr.marker('Platforms')}\nold")
        gh = FakeGh(issues=[existing, issue(10, "unrelated")])
        cr.report(gh, CONTEXT, self.failing, False)
        self.assertEqual(gh.verbs(), ["issue comment"])
        self.assertEqual(gh.calls[0][0][2], "9")

    def test_failure_is_reported_even_when_job_details_are_unavailable(self):
        gh = FakeGh()
        gh.fail_jobs = True
        cr.report(gh, CONTEXT, self.failing, False)
        self.assertEqual(gh.verbs(), ["label create", "issue create"])
        self.assertIn("cpu", gh.calls[1][1])

    def test_full_success_comments_and_closes_only_the_matching_bot_issue(self):
        gh = FakeGh(issues=[
            issue(9, f"{cr.marker('Platforms')}\nold"),
            issue(10, f"{cr.marker('PR')}\nother workflow"),
            issue(11, f"{cr.marker('Platforms')}", user={"login": "acoliver"}),
        ])
        self.assertEqual(cr.report(gh, CONTEXT, self.passing, False), "success")
        self.assertEqual(gh.verbs(), ["issue comment", "issue close"])
        self.assertEqual([call[0][2] for call in gh.calls], ["9", "9"])
        self.assertIn("passed again", gh.calls[0][1])

    def test_success_without_an_open_issue_does_nothing(self):
        gh = FakeGh()
        cr.report(gh, CONTEXT, self.passing, False)
        self.assertEqual(gh.calls, [])

    def test_documentation_only_success_never_closes_an_issue(self):
        gh = FakeGh(issues=[issue(9, f"{cr.marker('Platforms')}\nold")])
        cr.report(gh, CONTEXT, self.passing, True)
        self.assertEqual(gh.calls, [])

    def test_documentation_only_failure_is_still_reported(self):
        gh = FakeGh()
        cr.report(gh, CONTEXT, self.failing, True)
        self.assertEqual(gh.verbs(), ["label create", "issue create"])

    def test_context_requires_every_variable(self):
        environment = {"REPO": "r", "RUN_ID": "1", "RUN_URL": "u", "SHA": "s", "WORKFLOW": "w"}
        self.assertEqual(cr.context_from_environment(environment)["workflow"], "w")
        del environment["SHA"]
        with self.assertRaises(KeyError):
            cr.context_from_environment(environment)


if __name__ == "__main__":
    unittest.main()
