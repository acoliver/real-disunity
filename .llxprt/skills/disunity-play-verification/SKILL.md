---
name: disunity-play-verification
description: Use when verifying that Snake, Pong, Breakout or rd-stranded actually run on the engine. Describes driving the real player through a harness: launching it on a private display, playing with scripted input or simulated clicks and keys, capturing screenshots and checking them.
---

# Play verification

Claims that a game works come from playing it, not from unit tests alone.

1. Build the player and native library from the current tree.
2. Run the game under a harness you control. `tools/compat-display/run.py` starts a private Xvfb display; use it for headless runs. Scripted journeys use the fixtures under `fixtures/` and drive the real worker.
3. Drive input the way a player would: scripted key sequences, or `xdotool` key and mouse events sent to the window.
4. Capture screenshots at meaningful moments (after load, after moving, after scoring, after reload). Look at them. Check that the expected objects are present, move, and respond to input.
5. Exercise reload and teardown, and check that the process exits cleanly.
6. Report what was played, what was seen in the screenshots, and any failure. Keep only the screenshots worth showing; delete the rest.

Games to cover: Snake (movement, food, growth, reload), Pong (paddles, ball, goal, score), Breakout (paddle, ball, bricks, loss/respawn). For rd-stranded, build it against the current engine revision and explore the scene with the camera.
