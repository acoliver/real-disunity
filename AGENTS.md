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

Andrew's sessions collaborate through Hubnot on the hub hosted on Andrew's
Linux box. Each session's clone has a `hub` remote and its own Hubnot
identity named for its checkout, such as `mini-branch-3`; check it with
`hn identity show`. Never read, print, copy or attach anything under
`.git/hn` or `.git/hn-ssh`, which hold private keys. Run `hn sync hub` before
reading shared state. The hub's `main` is published to GitHub every night;
GitHub `main` accepts no other pushes.

Claim a feature-scoped change on its Hubnot issue (`hn issue comment`) before
starting, and check open claims and proposals (`hn issue list`,
`hn proposal list`). Claims cannot reserve whole crates. Acknowledged
external claims bind maintainer agents equally. Do not repossess a quiet
claim without notice and an answer or Andrew's decision.

Identify agent-authored text on GitHub as `[github-user/agent-name]` for the
authenticated account, optionally followed by a human name. Internal sessions
sharing an account use the identity of their checkout, such as
`[mini-llxprt-branch-4]`. Do not assume there are only two sessions.

Internal sessions keep at most two own worktrees and stacks no more than one
deep, and end each day with a merged proposal, a proposal waiting on the gate
or on Andrew, or a blocker recorded on the issue.

Never modify, clean or manage another contributor's worktree or build output.
Do not push to another session's branch or proposal without its explicit
permission, recorded as a comment on the proposal. Prepare integration work
in your own checkout; offer help and agree its scope first.

**Proposing and merging:**

1. Push your topic branch to the hub and open a proposal with
   `hn proposal open --base hub/main --head <branch> "<title>"`. Give it the
   body a pull request had: scope, verification, references and checklist.
2. Request the gate with `hn run request <proposal> gate`. The runner on the
   box runs the gate and signs the result.
3. When the gate has passed on the current head, merge your own proposal
   with `hn automerge <proposal> --rule ordinary --execute --remote hub`. No
   approval from another session is required. Hubnot refuses the merge if
   the gate has not passed, another session or Andrew requested changes, or
   Andrew rejected the proposal.
4. After a fix, revise with `hn proposal revise` and request the gate again.

Other sessions may review a proposal with `hn review <proposal> --approve` or
`--request-changes` and a reason. A change request blocks the merge until it
is answered, so use it for real defects. Ask another session for a review
when you want a second opinion; it is not required.

Changes to protected paths need Andrew's approval and acceptance: `.hn/`,
`.github/`, `crates/xtask/`, `AGENTS.md`, `CONTRIBUTING.md`,
`dev-docs/quality-gates.md`, `rust-toolchain.toml`, `clippy.toml`,
`rustfmt.toml`, `.cargo/` and `LICENSE`. So do `.llxprt/`, new crates and any
other path outside the `ordinary` rule in `.hn/policy.json`. Say so in the
proposal and wait. Never push to the hub's `main` and never merge another
session's proposal.

External contributors still open GitHub pull requests. The session Andrew
assigns mirrors each one as a Hubnot proposal that links the pull request;
Andrew reviews and accepts it, and GitHub shows it merged once the nightly
publish contains its head. Never close an external pull request because it
waits for review.

## Tests, limits and delivery

Write a behavior test first and confirm the intended failure. Preserve assertions,
tolerances and required coverage. Never add a retry, ignore a test or suppress a
warning to turn a failure green. Never loosen these limits or add lint allowances:
60 lines/function, six arguments, cognitive complexity 15, type complexity 250,
three boolean fields/struct, 750 lines/file target and 1000 hard limit.
Keep Rust incremental compilation disabled.

The hub's gate is the quality line. It runs on Linux for every proposal:
documentation checks, `cargo xtask ci-cpu`, `ci-managed`, `ci-gpu-static`
and, for C# changes, `ci-gpu-csharp`. It does not run the GPU test shards.
Decide yourself which checks to run before proposing: run what tells you
whether your change works, such as the affected crate's tests, a GPU test you
touched or a game you changed, and leave the full profiles to the gate. Read
the gate's log when it fails. Windows, macOS and the full serial GPU suite run
every night on the published `main`; a failure opens a Hubnot issue, and a GPU
failure is the first thing to fix the next morning. See
[quality gates](dev-docs/quality-gates.md#continuous-integration). Report the
checks you actually ran; do not infer another platform passed.

Private `rd-stranded` validation and pin updates belong to its owner. External
contributors need only the public verification path. Its current game pin protects
the existing consumer until updated. Keep public regressions free of restricted game material and fork
CI independent of secrets and private checkouts. Do not require a private repo to
become public.

Update affected README pages, user docs and contracts in the same PR. Keep scratch
inside your assigned checkout, small and disposable. Clean only output you own;
do not leave receipts, source copies or persistent task scripts. Follow the
[writing guide](https://github.com/acoliver/vibetools/blob/main/docs/writing/styleguide.md).
