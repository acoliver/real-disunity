---
name: disunity-play-verification
description: Use when verifying that Snake, Pong, Breakout or rd-stranded actually run on the engine. Describes playing the real game through a harness with scripted or simulated input and screenshots, and keeping the evidence small.
---

# Play verification

A claim that a game works comes from playing it, not from unit tests alone.

1. Build the engine and the game from the current tree. The three Unity games are fetched unchanged at their pinned revisions and built against `Disunity.Engine` by the external game harness (issue #38, E9).
2. Run the game headless under a private Xvfb display, or windowed. On macOS use a windowless launcher; never open Terminal.
3. Drive input the way a player would: scripted key sequences, or `xdotool` key and mouse events sent to the window after `windowfocus --sync`.
4. Capture screenshots at meaningful moments: after load, after moving, after scoring or hitting something, after restart. Look at them and check that the expected objects are present, move and respond.
5. Exercise restart and teardown, and check that the process exits cleanly.
6. Report what was played and what the screenshots show. Post the few that matter on the PR or issue and delete the rest.

Coverage per game:
- Snake: movement and steering, food, growth, game over and restart.
- Pong: both paddles, ball, goals, score, reset.
- Breakout: paddle, ball, brick hits and breaking, loss and respawn.
- rd-stranded: build against the current engine revision and explore the scene with the camera.
