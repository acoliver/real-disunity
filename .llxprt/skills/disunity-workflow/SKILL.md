---
name: disunity-workflow
description: Use for any code change, fix, feature or verification in the real-disunity repository. Covers the PR flow, limits on worktrees and scratch output, verification with the repository's own CI commands, the rules for the original game repositories and rd-stranded, and the lessons from closed PR #35.
---

# real-disunity workflow

## What went wrong in PR #35

PR #35 tried to run the original Snake, Pong and Breakout and was closed unmerged. Avoid repeating these mistakes:

- It built a parallel stack beside the engine: a second Unity API (`UnityCompat`), a second host (`CompatibilityHost`), a JSON simulation protocol, execution profiles and `compat-*` crates. The engine's own `world-simulation` crate and `csharp/Disunity` were the right owners.
- It patched each game's scene data with per-game overlays instead of fixing the engine gap.
- It committed about 117,000 lines of journey fixtures and a GPL-2+ font copyright file into an MIT repository.
- It spent hours on qualification receipts, hash manifests, frozen "generations" and provenance audits. `.work` reached 325 GB, and the work stayed in one giant PR that never merged.
- It did not coordinate with the other agent working on the same repository.

The rules below exist to prevent those outcomes.

## Branch and PR flow

1. Start from current `origin/main`: `git fetch origin && git switch -c <topic> origin/main`.
2. Prefer one checkout. An extra worktree is acceptable for one other active PR; remove it when that PR merges. Never keep more than two worktrees, and never create frozen source copies or "generation" directories.
3. Commit as you go with messages that explain why.
4. A PR is one coherent, reviewable unit that leaves `main` green: usually a few hundred to a few thousand changed lines. Avoid both one giant PR and a stream of tiny PRs.
5. Do not stack PRs more than one deep. If a PR has not merged within a working day, stop adding work and either fix what blocks it or close it.
6. Push, open the PR with `gh`, watch CI, fix failures, merge. Never merge a red PR.
7. Every working session should end with something merged, or with the blocker written on the issue or PR.

## Verification

- CI is the source of truth. Locally run the same commands:
  - `cargo xtask ci-cpu`
  - `cargo xtask ci-managed` (.NET 8 SDK)
  - `xvfb-run -a cargo xtask ci-gpu` with `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json`
- CI runs on Linux, macOS and Windows. Do not assume `/dev/urandom`, `/` path separators, symlink-free temp directories or non-verbatim canonical paths.
- Tests are self-contained: anything they need is built or fetched by committed tooling. No test reads from `.work/`.
- Games are verified by playing them through a harness, with scripted or simulated input and screenshots. See `disunity-play-verification`.

## Scratch output

- `.work/` holds tool installs and package caches only. Keep anything else in it small and temporary, and delete it when the task finishes. If `.work` plus `target/` grows past about 20 GB, clean it up before continuing.
- No qualification receipts, hash manifests, provenance JSON or before/after audits. Tests and CI results are the record.
- Screenshots and logs worth showing go on the issue or PR, a handful at a time. Do not commit them.

## Original games and rd-stranded

- Never modify the original Unity game repositories. The games run unchanged; gaps are fixed in the engine or the importer, not with per-game patches.
- rd-stranded (`github.com/acoliver/rd-stranded`) is a native Rust game that pins this repository by git revision and uses the typed Rust API. It is not converted to Unity or to Disunity. Engine changes must keep it building and running; bump its pin after engine changes it depends on.

## Licensing

The engine is MIT. Do not add third-party files without an MIT-compatible license and the required notices. Check every recovered or bundled asset, especially fonts.
