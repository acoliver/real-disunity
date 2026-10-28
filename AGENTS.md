# Agent rules for Real Disunity

These rules apply to all coding agents in this repository. Andrew (@acoliver) is
the maintainer. Read
[CONTRIBUTING.md](CONTRIBUTING.md), the [developer guide](dev-docs/README.md),
[architecture](dev-docs/architecture.md) and [quality gates](dev-docs/quality-gates.md)
before changing code. The [agent procedure](dev-docs/contribution-agent.md) links
the detailed workflow and skills. Explicit task restrictions on GitHub or Git
operations still apply; document a blocker when they prevent a delivery step.

## Contribution scope and ownership

External contributors fork `real-disunity`. Game-driven contributions use a game
they own or have permission to test, with an optional private `rd-gamename` harness.
Keep original scripts, assets and behavior unchanged apart from API binding and
harness configuration. All contribution types are welcome. A coherent milestone
PR, such as rendering before playability, does not require a complete game port.

Implement generalized engine improvements in Rust. Keep C# compatibility thin in
`Disunity.Engine`; managed values and delegate utilities may remain managed.
The mandatory owners are:

- `world-simulation`: the single running world and its state, lifecycle and rules.
- `disunity-ffi`: ABI validation, handles and result ownership only.
- One `DisunityHost`: managed execution.
- One `Disunity.Engine`: C# `UnityEngine` names.
- `render-plan` and `render-scene`: extraction and drawing with existing services.
- `unity-import`: Unity content import.

Extend the owner. Never add per-game overlays, a parallel engine, or a second
world, host, Unity API, renderer, mode or profile for the same behavior.

## Source use: required boundaries

Use original gameplay as the black-box behavioral reference. Derive the needed
API surface from permitted game usage and observed behavior. Inspect game-specific
code, data, assets, savegames, serialized content or compiled shaders only within
that particular game's license, modding policy or explicit permission. A purchased
copy gives no automatic right to redistribute its contents. Keep restricted
material out of public commits and attachments.

The following prohibitions apply to manual work and every agent tool:

- Do not consult Unity documentation or add links to it.
- Do not inspect Unity implementation source, including Unity engine, library or
  package implementation. Do not decompile Unity libraries, even bundled ones.
- Do not broadly decompile, index or scan a game installation into agent context.
  Select permitted game material deliberately and exclude Unity implementation.
- Apply these boundaries to browsing, retrieval/RAG, tool output and autocomplete.
  Do not send excluded material through any of them.
- Stop uncertain inspection and ask Andrew before continuing that inspection.

Runtime loading may execute Unity packages. Existing tools read assembly metadata,
serialized content and compiled shaders, as described in the
[C# guide](docs/csharp-games.md#built-players) and [import guide](docs/unity-projects.md).
This functionality grants no permission for agent inspection of Unity implementation.
Game-specific content and shaders require the game's permissions; engine and
library implementation remains excluded. Stop ambiguous manual inspection.

Summarize references and the permission basis briefly in the PR. Do not create
provenance manifests, qualification receipts or legal clean-room claims. Do not
claim absence of model training exposure.

## Coordination and review authority

Claim a feature-scoped change before starting and check open claims and PRs.
Claims cannot reserve whole crates. Acknowledged external claims bind maintainer
agents equally. Do not repossess a quiet claim without notice and an answer or
Andrew's decision. Use issue and PR comments for coordination.

Identify agent-authored GitHub text as `[github-user/agent-name]` for the authenticated
account, optionally followed by a human name. Internal sessions sharing an account
use the identity assigned to their checkout, such as `[mini-llxprt-branch-4]`.
Do not assume there are only two sessions.

Never modify, clean or manage another contributor's worktree or build output.
Do not close their PR. Obtain the branch owner's explicit permission in a PR
comment before pushing to their branch. Force pushes need explicit permission
for that operation. Prepare integration work in your own checkout; offer help
and agree its scope with the contributor first.

Andrew intends human review daily. Maintainer agents must also review daily.
Daily attention does not promise a same-day merge. **Only Andrew manually merges
all PRs. Agents never merge, including their own PRs.** This applies to the web UI,
API and CLI. Never push directly to main, enable auto-merge, use a merge queue or
run `gh pr merge`. Updating your own topic branch from main is allowed.

External PRs are exempt from the internal daily review-ready cadence. Never close
them merely because they await maintainer review or merge.
Internal sessions target daily review-ready delivery, at most two own worktrees
and stacks no more than one deep. End with a posted review-ready PR or a blocker
recorded on the issue or PR. Waiting for Andrew's merge is acceptable.

## Tests, limits and delivery

Write a behavior test first and confirm the intended failure. Preserve assertions,
tolerances and required coverage. Never add a retry, ignore a test or suppress a
warning to turn a failure green. Never loosen these limits or add lint allowances:
60 lines/function, six arguments, cognitive complexity 15, type complexity 250,
three boolean fields/struct, 750 lines/file target and 1000 hard limit.
Keep Rust incremental compilation disabled.

Run `cargo xtask ci-cpu` and the relevant `ci-managed` and `ci-gpu` profiles for
code changes. Protected `main` requires all seven hosted checks: CPU and managed
on Linux, macOS and Windows, plus software-Vulkan GPU on Linux. Report checks
actually run and blockers; do not infer another platform passed.

Private `rd-stranded` validation and pin updates belong to its owner. External
contributors need only the public verification path. Its current game pin protects
the existing consumer until updated. Keep public regressions free of restricted game material and fork
CI independent of secrets and private checkouts. Do not require a private repo to
become public.

Update affected README pages, user docs and contracts in the same PR. Keep scratch
inside your assigned checkout, small and disposable. Clean only output you own;
do not leave receipts, source copies or persistent task scripts. Follow the
[writing guide](https://github.com/acoliver/vibetools/blob/main/docs/writing/styleguide.md).
