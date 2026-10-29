# Contributing to Real Disunity

Bug reports, documentation, tests, tooling and engine features are welcome. A game
compatibility effort is one way to contribute. You can submit a coherent milestone,
such as a game rendering its first scene, before the whole game is playable.

Read the [developer guide](dev-docs/README.md) for the subsystem contracts and
verification commands. Coding agents must also follow [AGENTS.md](AGENTS.md).
Andrew (@acoliver) is the maintainer.

## Start with a fork and a scoped change

External contributors should fork `real-disunity` and open a topic branch in their
fork. Describe the intended behavior, acceptance criteria and boundaries in an issue
or draft PR before substantial work. Check existing claims and PRs for overlap.
Claims cover a feature or bounded change; they cannot reserve an entire crate.
An acknowledged external claim also binds maintainer agents. Coordinate overlapping
work on the issue. A quiet claim requires notice and an answer or Andrew's decision
before anyone takes it over.

For game-driven work, select a game you own or otherwise have permission to test.
An optional `rd-gamename` harness repository can hold build configuration and local
integration work. It may remain private. Keep the original game's scripts, assets
and behavior unchanged, apart from API binding and harness configuration, such as
an external build project referencing `Disunity.Engine` instead of Unity modules
(see [the original-game harness](csharp/OriginalGames)). Fix gaps with generalized
engine or importer improvements. Per-game overlays, substituted assets and gameplay
rewrites are outside this contribution path.

A milestone PR should explain what works, what remains unsupported and how to
reproduce its result with material reviewers may access. There is no requirement
to complete a whole game port in one PR. Keep public regressions small and
free of restricted game material, using synthetic fixtures when game material cannot be shared.

## Permitted references and inspection

Use original gameplay as a black-box reference. Derive the needed API surface from
permitted game usage and observed behavior. Game-specific code, data, assets,
serialized content, compiled shaders and savegames may be inspected only to the
extent that the particular game's license, modding policy or explicit permission
allows. Owning a purchased copy does not grant redistribution rights. Keep
restricted material out of public commits, CI artifacts and PR attachments.

These source-use boundaries apply to people and every agent tool:

- Do not consult Unity documentation or add links to it.
- Do not inspect Unity engine or library implementation source, including Unity
  packages. Do not decompile Unity libraries, even when they ship with a game.
- Select permitted game material deliberately. Do not broadly decompile, index or
  scan an installation and feed Unity implementation into an agent. Browsing,
  retrieval/RAG, tool output and autocomplete must respect the same boundary.
- If the ownership or permitted use of material is uncertain, stop that inspection
  and ask Andrew before continuing it.

Current runtime loading can execute listed Unity packages, and existing tooling
reads assembly metadata, serialized files and compiled shaders. The [C# guide](docs/csharp-games.md#built-players)
and [import guide](docs/unity-projects.md) describe that functionality. Runtime
loading and tool processing do not authorize a person or agent to inspect Unity
implementation. Game-specific serialized content and compiled shaders remain
subject to the game's permissions; Unity engine and library implementation is
excluded. Stop ambiguous manual inspection for Andrew's guidance.

In the PR, give a short summary of references used and the permission basis for
any game material inspected or shared. Keep it proportionate to the change. Do not
create provenance manifests or qualification receipts. Do not claim that these
rules establish legal clean-room status or absence of model training exposure.

## Extend the existing engine

Rust owns generalized engine behavior. C# provides thin compatibility in
`Disunity.Engine`; managed value types and delegate utilities are appropriate there.
Keep the established ownership boundaries from [architecture](dev-docs/architecture.md#ownership):

| Responsibility | Existing owner |
| --- | --- |
| Running world state and simulation | `world-simulation` |
| C ABI validation, handles and result ownership only | `disunity-ffi` |
| Managed execution | One `DisunityHost` |
| C# `UnityEngine` names | One `Disunity.Engine` assembly |
| Render extraction and drawing | `render-plan` and `render-scene` with existing services |
| Unity content import | `unity-import` |

Extend these owners. Do not introduce a parallel world, host, Unity API, renderer,
mode or profile for the same behavior. Unsupported behavior must fail explicitly.

Write a behavioral regression test first, confirm its intended failure and then
implement the change. Do not weaken tests, suppress warnings or loosen limits.
Functions are limited to 60 lines, six arguments and cognitive complexity 15;
type complexity is 250, with at most three boolean fields per struct. Aim for
source files under 750 lines; 1000 is the hard limit. Do not add lint allowances.
Update affected user docs, subsystem contracts and examples in the same PR. Follow
the [third-party licensing rule](dev-docs/README.md#rules-that-apply-to-every-change)
when adding files.

## Verification and private consumers

Run the public [quality gates](dev-docs/quality-gates.md) appropriate to the change:

```sh
cargo xtask ci-cpu
cargo xtask ci-managed   # Managed API or hosting changes; requires .NET 8.
cargo xtask ci-gpu       # Rendering or presentation changes; requires an adapter.
```

Report commands, results and unavailable prerequisites. Local Linux checks cannot
establish Windows or macOS compatibility. Protected `main` requires the `PR gate`
check, which runs on Linux. Windows and macOS run after merge and nightly; see
[quality gates](dev-docs/quality-gates.md#continuous-integration). Documentation-only
PRs skip the engine builds. Relevant local checks complement those hosted results.
Never weaken the gates to accommodate a contribution. Workflow runs from a new
fork may wait for maintainer approval; contributors do not need to provide secrets.

`rd-stranded` is a private native Rust consumer pinned to an engine Git revision.
Its owner handles private validation and pin updates. External contributors need
no access to it, and fork CI must not require secrets or a private checkout.
The game's existing pin protects that consumer until its owner updates it; owner
validation is required before advancing the pin. Keep public regressions free of restricted game material.
No contributor is required to make a private repository public.

## Reviews and merging

The maintainer, Andrew (@acoliver), intends to review PRs daily, and maintainer
agents must also review daily. Daily review attention carries no same-day merge
guarantee. External PRs are exempt from the internal daily review-ready cadence
and must not be closed merely because they await maintainer review or merge.

Only Andrew manually merges **all** PRs. Agents must never merge any PR, including
their own, through the web UI, API or CLI. Do not push directly to main, enable
auto-merge, use a merge queue or run `gh pr merge`. Updating your own topic branch
from main is allowed. A green PR waits for Andrew's review and manual merge.

For agent-authored GitHub text, identify the authenticated GitHub account and
agent as `[github-user/agent-name]`, optionally followed by a human's name.
Maintainer sessions sharing an account use their assigned session identity, such
as `[mini-llxprt-branch-4]`. See the [coordination skill](.llxprt/skills/disunity-coordination/SKILL.md).
Review and discuss other contributors' work on their issues and PRs. Never modify,
clean or manage another contributor's worktree or build output. Do not close their
PR. Obtain the branch owner's explicit permission in a PR comment before pushing
to their branch. Force pushes need explicit permission for that operation. Prepare
integration work in your own checkout; offer help and agree its scope with the
contributor first.

The temporary internal delivery cadence favors daily review-ready PRs, at most two
own worktrees and stacks no more than one deep. Internal sessions end with a PR
posted and review-ready, or a documented blocker. Waiting for Andrew to merge is
compatible with that rule. Keep scratch small and remove disposable task output;
never clean another contributor's files or build output.
