#!/usr/bin/env python3
"""Opens, updates and closes the CI failure issue for a workflow on main.

Runs only after pushes to main and scheduled runs. Inputs come from the environment:

  NEEDS       toJSON(needs) of the reporting job
  REPO        owner/name
  RUN_ID      workflow run id
  RUN_URL     link to the run
  SHA         commit under test
  WORKFLOW    workflow name, used in the issue title and the issue marker
  DOCS_ONLY   optional; 'true' when the build jobs were skipped for a documentation change

Only open issues labelled `ci-failure`, created by github-actions[bot] and carrying this
workflow's marker are ever touched. All text reaches `gh` through stdin or argument vectors,
never through a shell.
"""

import json
import os
import subprocess
import sys
from typing import Callable, NamedTuple

LABEL = "ci-failure"
BOT = "github-actions[bot]"
FAILED_RESULTS = ("failure", "cancelled")
PASSING_RESULTS = ("success", "skipped")
FAILED_CONCLUSIONS = ("failure", "cancelled", "timed_out")
SETUP_PREFIXES = (
    "Set up", "Run actions/", "Run Swatinem/", "Run dtolnay/", "Install", "Post ", "Complete job",
)

Gh = Callable[[list[str], str | None], str]


class FailedJob(NamedTuple):
    name: str
    url: str
    step: str
    kind: str


def marker(workflow: str) -> str:
    return f"<!-- ci-failure:{workflow} -->"


def failed_needs(needs: dict) -> list[str]:
    """Names of needed jobs that did not pass. Unknown results count as failures."""
    return sorted(
        name for name, need in needs.items() if need.get("result") not in PASSING_RESULTS
    )


def classify_failure(job_conclusion: str, step: str | None) -> str:
    if job_conclusion == "cancelled":
        return "cancelled"
    if step is None or step.startswith(SETUP_PREFIXES):
        return "infrastructure"
    return "build or test"


def first_failed_step(job: dict) -> str | None:
    for step in job.get("steps") or []:
        if step.get("conclusion") in FAILED_CONCLUSIONS:
            return step.get("name")
    return None


def failed_jobs(jobs: list[dict]) -> list[FailedJob]:
    found = []
    for job in jobs:
        conclusion = job.get("conclusion")
        if conclusion not in FAILED_CONCLUSIONS:
            continue
        step = first_failed_step(job)
        found.append(
            FailedJob(
                job.get("name", "(unnamed job)"),
                job.get("html_url", ""),
                step or "(no step ran)",
                classify_failure(conclusion, step),
            )
        )
    return sorted(found)


def parse_job_lines(output: str) -> list[dict]:
    return [json.loads(line) for line in output.splitlines() if line.strip()]


def cell(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ")


def job_table(jobs: list[FailedJob]) -> str:
    rows = ["| Job | Kind | First failed step |", "| --- | --- | --- |"]
    for job in jobs:
        link = f"[{cell(job.name)}]({job.url})" if job.url else cell(job.name)
        rows.append(f"| {link} | {job.kind} | {cell(job.step)} |")
    return "\n".join(rows)


def failure_text(context: dict, needs: list[str], jobs: list[FailedJob]) -> str:
    lines = [
        f"The {context['workflow']} workflow failed on main.",
        "",
        f"- Commit: {context['sha']}",
        f"- Run: {context['run_url']}",
        f"- Failed jobs: {', '.join(needs) or '(none reported)'}",
    ]
    if jobs:
        lines += ["", job_table(jobs)]
    return "\n".join(lines) + "\n"


def recovery_text(context: dict) -> str:
    return (
        f"The {context['workflow']} workflow passed again at {context['sha']}.\n\n"
        f"Run: {context['run_url']}\n"
    )


def matching_issues(issues: list[dict], workflow: str) -> list[int]:
    """Open `ci-failure` issues opened by the Actions bot for this workflow."""
    wanted = marker(workflow)
    numbers = []
    for issue in issues:
        labels = [label.get("name") for label in issue.get("labels") or []]
        author = (issue.get("user") or {}).get("login")
        if "pull_request" in issue or LABEL not in labels or author != BOT:
            continue
        if wanted in (issue.get("body") or ""):
            numbers.append(issue["number"])
    return sorted(numbers)


def gh_command(args: list[str], stdin: str | None = None) -> str:
    result = subprocess.run(
        ["gh", *args], input=stdin, capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        raise RuntimeError(f"gh {' '.join(args[:3])} failed: {result.stderr.strip()}")
    return result.stdout


def list_jobs(gh: Gh, repo: str, run_id: str) -> list[dict]:
    path = f"repos/{repo}/actions/runs/{run_id}/jobs"
    return parse_job_lines(gh(["api", path, "--paginate", "--jq", ".jobs[]"], None))


def list_open_issues(gh: Gh, repo: str) -> list[dict]:
    path = f"repos/{repo}/issues?state=open&labels={LABEL}&per_page=100"
    return parse_job_lines(gh(["api", path, "--paginate", "--jq", ".[]"], None))


def report_failure(gh: Gh, context: dict, needs: list[str]) -> None:
    try:
        jobs = failed_jobs(list_jobs(gh, context["repo"], context["run_id"]))
    except (RuntimeError, ValueError):
        jobs = []
    text = failure_text(context, needs, jobs)
    existing = matching_issues(list_open_issues(gh, context["repo"]), context["workflow"])
    repo = context["repo"]
    if existing:
        gh(["issue", "comment", str(existing[0]), "--repo", repo, "--body-file", "-"], text)
        return
    gh(["label", "create", LABEL, "--repo", repo, "--color", "B60205", "--force",
        "--description", "Automated report of a failed CI run on main"], None)
    body = f"{marker(context['workflow'])}\n{text}"
    title = f"CI failure on main: {context['workflow']}"
    gh(["issue", "create", "--repo", repo, "--title", title, "--label", LABEL,
        "--body-file", "-"], body)


def report_recovery(gh: Gh, context: dict) -> None:
    repo = context["repo"]
    for number in matching_issues(list_open_issues(gh, repo), context["workflow"]):
        gh(["issue", "comment", str(number), "--repo", repo, "--body-file", "-"],
           recovery_text(context))
        gh(["issue", "close", str(number), "--repo", repo, "--reason", "completed"], None)


def report(gh: Gh, context: dict, needs: dict, docs_only: bool) -> str:
    failed = failed_needs(needs)
    if failed:
        report_failure(gh, context, failed)
        return "failure reported"
    if docs_only:
        return "documentation-only run proves nothing about earlier failures"
    report_recovery(gh, context)
    return "success"


def context_from_environment(environment: dict) -> dict:
    keys = {"repo": "REPO", "run_id": "RUN_ID", "run_url": "RUN_URL", "sha": "SHA",
            "workflow": "WORKFLOW"}
    return {key: environment[variable] for key, variable in keys.items()}


def main() -> int:
    needs = json.loads(os.environ["NEEDS"])
    context = context_from_environment(os.environ)
    docs_only = os.environ.get("DOCS_ONLY", "") == "true"
    print(report(gh_command, context, needs, docs_only))
    return 0


if __name__ == "__main__":
    sys.exit(main())
