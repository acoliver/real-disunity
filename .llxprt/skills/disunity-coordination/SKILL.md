---
name: disunity-coordination
description: Use before claiming, implementing or reviewing work in real-disunity. Defines session identities, feature-scoped claims on the Hubnot hub, external contributor coordination and who may merge.
---

# Coordinating contributors and agents

[AGENTS.md](../../../AGENTS.md) and [CONTRIBUTING.md](../../../CONTRIBUTING.md)
set review authority and contributor rules. Several sessions may share a GitHub
account; external contributors use their own authenticated accounts.

## Identities and claims

Start agent-authored issues, PR bodies and comments with
`[github-user/agent-name]`, using the authenticated GitHub user. An optional human
name may follow. Internal sessions sharing an account use their assigned checkout
identity, for example `[mini-llxprt-branch-4]`. There is no fixed list of two agents.

Before starting, run `hn sync hub` and read open Hubnot issues and proposals
(`hn issue list`, `hn proposal list`), including the relevant plan issue.
Claim a bounded feature with a comment on its Hubnot issue, stating the behavior, likely
files and exclusions. Link it from the relevant plan. Claims cannot reserve an
entire crate. Acknowledged external claims bind maintainer agents equally.

Do not start overlapping work without agreement. If a claim seems quiet or stale,
post notice on its issue and wait for an answer or Andrew's decision before taking
it over. Never repossess it silently. Explicit task restrictions on GitHub writes
still apply; report the coordination blocker when needed.

## Reviews and integration help

Reviews between sessions are optional: a proposal merges on the gate alone.
Review another session's proposal when asked or when you see a real defect,
and use `hn review --request-changes` only for one, because it blocks the
merge. External contributors' GitHub pull requests are mirrored as Hubnot
proposals and need Andrew's review. External milestone PRs are
exempt from the internal daily review-ready cadence. Never close them merely
because they await maintainer review or merge.

Post concrete review findings on the relevant issue or proposal: behavior, file and
suggested fix. Read and answer comments before each new push. Warn collaborators
before changing an API their open work uses. Offer integration help and agree its
scope with the contributor before preparing it in your own checkout.

Never modify, clean or manage another contributor's worktree or build output.
Do not close their PR. Obtain the branch owner's explicit permission in a PR
comment before pushing to their branch. Force pushes need explicit permission
for that operation.
Maintainer sessions merge only their own proposals, with
`hn automerge <proposal> --rule ordinary --execute --remote hub` after the gate
passes on the current head. Protected paths and anything outside the
`ordinary` rule need Andrew. Andrew accepts external contributors' changes.
Never merge another session's proposal or push to the hub's `main`.
