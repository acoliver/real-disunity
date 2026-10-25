---
name: disunity-workflow
description: Use for any code change, fix, feature or verification in the real-disunity repository. Covers the single-branch PR workflow, what may and may not be kept on disk, how to verify with the repository's own CI commands, and the rules for original game repositories and rd-stranded.
---

# real-disunity workflow

## Branch and PR flow

1. Work in this one checkout. Do not create extra git worktrees, frozen source copies or "generation" directories.
2. Start from current `origin/main`: `git fetch origin && git switch -c <topic> origin/main`.
3. Commit as you go with messages that explain why. Rebase or merge `origin/main` before opening a PR.
4. Group work into a few coherent PRs. Each PR is a complete, reviewable unit that leaves `main` green. Avoid both one giant PR and a stream of tiny PRs.
5. Push the branch, open the PR with `gh`, wait for CI with `pr.checks watch`, fix failures, then merge. Never merge a red PR.
6. Never let local work sit uncommitted for long. If it is not in a commit on a pushed branch, it does not exist.

## Verification

- CI is the source of truth. Locally run the same commands CI runs:
  - `cargo xtask ci-cpu`
  - `cargo xtask ci-managed` (needs the .NET 8 SDK)
  - `xvfb-run -a cargo xtask ci-gpu` (lavapipe, `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json`)
- Run checks from a clean clone or a clean tree before claiming something works. A pass that depends on untracked files is not a pass.
- Behavior is verified by playing the games through harness runners: launch the player, drive input by script or by simulated clicks/keys, take screenshots and inspect them. See the `disunity-play-verification` skill.
- Tests must be self-contained. A test that needs a native image, managed build or original game source builds or fetches it through committed tooling. No test reads from `.work/`.

## What to keep on disk

- `.work/` is a scratch cache for tool installs and build outputs only. It is never an input to tests and never holds "evidence", receipts, hash manifests or sealed generations.
- Do not write qualification receipts, provenance JSON, before/after hash audits or similar bookkeeping. Tests and CI results are the record.
- Delete scratch output when a task finishes.

## Original games and rd-stranded

- The original Unity game repositories (Snake, Pong, Breakout) are never modified. Compatibility work happens in this repository.
- rd-stranded (`github.com/acoliver/rd-stranded`) depends on this repository by git revision. Engine changes must keep it building and running. After merging engine changes, bump its pinned revision to the new `main` and verify it. It tracks `main`; there is no separate fork or branch of the engine for it.
