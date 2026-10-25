# Breakout keyboard journey

The planner runs the selected compat-player with the original seven Breakout gameplay scripts. It supplies A/D key states, bounded native advances, observations, capture, reload and unload. Gameplay methods, private state mutation and synthetic contacts are excluded. See the [fixture configuration](../../fixtures/README.md) and [behavior validator](verify.py).

## Replay the scenario suite

The three compact fixtures cover multiball loss, normal clamps/reload/control, and wide-paddle clamps/expiry. Replay each with an explicit harness and separate output directory:

```sh
python3 tools/compat-display/run.py -- python3 tools/compat-breakout/replay.py \
  fixtures/breakout-journey-multiball-loss-compact.json --matrix \
  --harness target/debug/compat-kit \
  --output .work/breakout-replays/breakout-journey-multiball-loss-compact
python3 tools/compat-breakout/check_journey.py --replays .work/breakout-replays
python3 -m unittest discover -s tools/compat-breakout
```

Repeat replay for controls-complete and wide-complete, using their corresponding fixture stems as output directory names. `--matrix` covers both compatibility profiles offscreen and desktop. Desktop requires the contained display harness; no fixed display number is assumed. The worker deadline remains 120 seconds and the wrapper bounds execution to 130 seconds. Existing outputs are never reused as cached acceptance.

`check_journey.py` directly checks all current scenario records, measured speed, layout, break sprites/removal, physical contacts, normal/wide clamps, width expiry, ball-loss identity preservation, fresh launch and renewed keyboard control. It compares decoded capture pixels between corresponding current runs and prints missing requirements.

## Generate a plan

```sh
python3 tools/compat-breakout/driver.py --worker target/debug/compat-player \
  --output .work/breakout-plan \
  --case controls --scenario controls --export --compact
```

The planner resolves the pinned Breakout checkout through `tools/original_games.py` and the original-games crate. Exported fixtures name `"game": "breakout"`. `--resume` continues explicitly selected planner state. Exported fixtures remove cached assembly fields and remain inside the selected output directory. Presentation compaction retains native action ordering and updates frame selectors consistently.

The validator checks checkpoint ball/brick counts, nonuniform decoded captures, current capture integrity and measured teardown. The suite retains the original-source/unlit-material-translation/sprite-bounds-collision label. Parser/planner unit tests establish harness behavior; actual game coverage requires the replay suite with current worker/native/managed builds and pinned source setup.
