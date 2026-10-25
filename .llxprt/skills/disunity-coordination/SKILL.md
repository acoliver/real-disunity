---
name: disunity-coordination
description: Use before starting, claiming or reviewing work in real-disunity or rd-stranded when more than one agent session is active. Defines agent identities, how to claim work on GitHub, and how to exchange reviews and warnings through issues and PRs.
---

# Coordinating agent sessions

Several agent sessions can work on these repositories at once, all under the same GitHub account. GitHub shows every action as `acoliver`, so identity must be stated in the text.

## Identities

- `mini-llxprt-disunity-1`: the session working in `real-disunity/branch-1`.
- `mini-llxprt-disunity-2`: the session working in `real-disunity/branch-2`.

Start every issue, PR body and comment you write with `[mini-llxprt-disunity-N]`.

## Claiming work

1. Before starting, read the open issues and PRs, and the plan issue (#38 for the native world and Disunity API work).
2. Claim a piece of work by opening an issue or draft PR that says who is doing it, the scope, the files or crates it will touch, and what it will not touch. Link it from the plan issue with a comment.
3. Do not start work another session has claimed. If you think a claim is wrong or stale, say so on its issue and wait for an answer or for Andrew.
4. Do not close, merge or force-push another session's PR or issue. Comment instead.

## Reviews and warnings

- When another session's work affects yours, or you see something that will not work, post a review or comment on its PR or issue with specifics: file, behavior, and suggested fix.
- Read comments on your own issues and PRs before each new push, and answer them there.
- Keep shared crates stable: if you need to change an API another session's open PR uses, say so on that PR first.
