---
name: disunity-workflow
description: Use for changes and verification in real-disunity. Covers proposal delivery on the Hubnot hub, worktree and scratch limits, public verification, unchanged games and private-consumer ownership.
---

# Real Disunity workflow

Follow [AGENTS.md](../../../AGENTS.md), [CONTRIBUTING.md](../../../CONTRIBUTING.md)
and the [agent procedure](../../../dev-docs/contribution-agent.md). Extend the
existing engine owners, preserve original games and keep delivery reviewable.

## Branch and proposal flow

1. Check the working tree and assigned branch. External contributors work in a
   fork. For new authorized work, run `hn sync hub` and branch from current
   `hub/main`; stay on an
   existing assigned branch when the task specifies one. Claim the feature using
   the [coordination skill](../disunity-coordination/SKILL.md).
2. Prefer one checkout. Internal agents keep at most two own worktrees and stacks
   no more than one deep. Never change another contributor's checkout or build
   output. Push to their branch only with its owner's explicit permission recorded
   on the proposal; force pushes require permission for that operation. Prepare agreed
   integration work in your own checkout. Do not create frozen source copies.
3. Make one coherent proposal. A game rendering before it is playable is a
   useful milestone. Other contribution types are welcome. External PRs are exempt
   from the internal daily review-ready cadence; do not close them merely because
   they await maintainer review or merge.
4. When authorized, commit, push the branch to `hub`, open a proposal
   (`hn proposal open --base hub/main --head <branch> "<title>"`) and request
   the gate (`hn run request <proposal> gate`). Read the gate log and fix
   failures.
   A task that forbids these operations must report the delivery blocker instead.
5. Internal sessions target daily delivery: a merged proposal, a proposal
   waiting on the gate or on Andrew, or a blocker documented on the issue.
6. Once the gate passes on the current head, merge your own proposal with
   `hn automerge <proposal> --rule ordinary --execute --remote hub`. No
   approval from another session is needed. Protected paths need Andrew.
   Never merge another session's proposal or push to the hub's `main`.

## Verification

The hub's gate is the quality line. For every proposal it runs, on Linux,
the documentation checks, `cargo xtask ci-cpu`, `ci-managed`, `ci-gpu-static`
and, for C# changes, `ci-gpu-csharp`; see
[quality gates](../../../dev-docs/quality-gates.md). Decide yourself what to
run first: whatever shows that your change works, for example the affected
crate's tests or a GPU test or game you touched. Running a full profile
locally is optional.

- `cargo xtask ci-cpu` for code changes.
- `cargo xtask ci-managed` for relevant managed changes (.NET 8 SDK).
- `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a cargo xtask ci-gpu`
  for relevant GPU work on Linux with lavapipe.

GPU tests run only in the nightly `Platforms` run on GitHub, together with
Windows and macOS; a failure opens a Hubnot issue to fix first. Report local results and
missing prerequisites accurately. Never weaken tests or quality gates. Keep Rust
incremental compilation disabled.

Tests use committed tooling and isolated fixtures, never `.work/` inputs. Original
game tests fetch pinned public checkouts through the repository tooling. Game
claims require the observations in [play verification](../disunity-play-verification/SKILL.md).
Update affected README pages, user docs and contracts with the change.

## Scratch, source use and consumers

Keep task output in your assigned checkout. `.work/` holds tool installs and
package caches; keep other scratch small and temporary. If your own `.work/` plus
`target/` exceeds about 20 GB, clean owned disposable output before continuing.
Never clean another session's output. Delete disposable task files and processes
at completion. Keep only a handful of useful logs or screenshots on the PR.
Do not create qualification receipts, hash manifests, provenance JSON or audits.

Follow the explicit [source-use rules](../../../AGENTS.md#source-use-required-boundaries).
Do not consult Unity documentation or inspect Unity implementation. Permitted
game material remains subject to that game's permissions. Keep original scripts,
assets and behavior unchanged apart from API binding and harness configuration;
fix generalized behavior in the engine or importer. Third-party files require an
MIT-compatible license and notices. Purchasing a game grants no redistribution rights.

`rd-stranded` is a private native Rust consumer pinned by Git revision. Its owner
handles private validation and pin updates; its current pin protects the existing
consumer until updated. External contributors need only public checks. Keep public
regressions free of restricted game material and fork CI independent of secrets and private checkouts.
No private repository needs to become public.
