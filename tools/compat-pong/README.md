# Source-driven Pong journey

The driver supplies keyboard input and native advances to the selected compat-player executable. It reads snapshots and contacts to plan paddle bounces, goals, score updates, reset/wait/relaunch and reload. It does not mutate gameplay state. See the [fixture configuration](../../fixtures/README.md) and [gameplay validator](verify.py).

## Plan and replay

Pass current worker/output paths explicitly. The planner fetches and verifies the pinned Pong checkout through `tools/original_games.py` and the original-games crate; exported fixtures name `"game": "pong"`. Planning outputs include the generated fixture and checkpoint observations. Finalization adds source-rule clock/contact assertions; `--contact-record` can resolve event-array pointers from a current replay while preserving assertion values.

```sh
python3 tools/compat-pong/driver.py --worker target/debug/compat-player \
  --output .work/pong-plan
python3 tools/compat-pong/finalize.py --fixture .work/pong-plan/pong-journey.json \
  --checkpoints .work/pong-plan/planned-checkpoints.json
python3 tools/compat-display/run.py -- python3 tools/compat-pong/replay.py \
  --harness target/debug/compat-kit --output .work/pong-replay --matrix
python3 -m unittest discover -s tools/compat-pong
```

`--matrix` runs both compatibility profiles offscreen and desktop, then source-2017.1 offscreen. Without it, replay runs the selected `--profile` and optional `--desktop`. Desktop runs require the private display harness. Each invocation creates new outputs and fails immediately on a nonpassing run; there is no historical identity cache, automatic retry or handoff receipt.

The validator checks measured assertions, physical paddle/goal contacts, UnityEvent targets, scores, coroutine timing, reload control, capture completion, decoded score-region changes and zero-world/root/readback teardown. Capture checksums guard current run files. Provider/source/build fingerprints and historical event comparisons grant no acceptance. The original-source/legacy-mappings-dejavu-font label remains explicit about mappings and font substitution. These checks establish only the exercised facade-backed behavior.
