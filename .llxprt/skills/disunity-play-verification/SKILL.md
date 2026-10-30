---
name: disunity-play-verification
description: Use when reporting a game rendering or playing on the engine. Covers permission-aware original-game harnesses, input and screenshot observations, milestone scope and private-consumer validation.
---

# Game milestone verification

Follow [AGENTS.md](../../../AGENTS.md#source-use-required-boundaries) and
[CONTRIBUTING.md](../../../CONTRIBUTING.md). Match the claim to what was observed.
A first rendered scene is a valid milestone; unit tests alone do not establish
playability or completion of an arbitrary game port.

1. Select a game you own or have permission to test. Inspect only game material
   allowed by its license, modding policy or explicit permission. Use original
   gameplay as the black-box reference. Do not consult Unity documentation or
   inspect Unity implementation, including bundled libraries. Stop uncertain inspection.
2. Build the engine and harness from the revision being reported. Keep original
   scripts, assets and behavior unchanged apart from API binding and harness
   configuration. An optional private `rd-gamename` harness is acceptable.
   The public pinned Snake, Pong and Breakout path is documented in the
   [play README](../../../crates/play/README.md).
3. Run windowed or under a private Xvfb display. On macOS use a windowless launcher;
   do not open Terminal. Drive player input through scripted keys or window events.
4. Capture a few meaningful frames and inspect them. For a rendering milestone,
   identify what appears and what remains unsupported. For a playability claim,
   exercise movement, collisions or scoring as appropriate, then restart and teardown.
5. Report the revision, commands and observed outcome. Share only material you may
   redistribute. Keep evidence small, post useful frames on the PR and delete the rest.

## From a failing game to a merged fix

When the game stops at an engine gap, work on the first gap only:

1. Name it from what you observed: the exception, the missing object, the wrong frame.
2. Reduce it to a public regression in the owning crate or managed test project, with
   synthetic fixtures and nothing restricted from the game. Watch it fail for the
   reason the game failed.
3. Fix the general behavior in its owner and watch the regression pass, then rerun the
   game to confirm the gap moved.
4. Where a test could pass for the wrong reason, break the behavior on purpose (flip a
   sign, drop a branch) and confirm the test fails, then restore it. Do this for the
   assertions that carry the claim; there is no quota.
5. Get an independent review of the diff before merging: another agent session or a
   reviewer subagent that did not write it.
6. End the PR with the next blocker: what the game now does, where it stops next, and
   the observation that shows it. A milestone and a correctness fix are separate
   claims; say which one the PR makes.

Keep the record in the PR. Do not create evidence manifests or receipts.

For the public test games, useful playability observations include Snake steering,
food and growth; Pong paddles, goals and reset; and Breakout paddle movement, brick
hits, loss and respawn. Report any unverified behavior explicitly.

`rd-stranded` validation and pin updates belong to its private consumer owner.
External contributors do not need access. Keep public regressions free of restricted game material and
never require secrets or private checkouts in fork CI. The existing game pin stays
in effect until its owner validates and updates it.
