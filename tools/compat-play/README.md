# Original-source Snake journey

The planner navigates from actual public food and native body observations. The selected worker executes the eight original Snake scripts. Python supplies WASD and bounded advances, then checks reversal, turns, wrapping, five foods, deferred growth, shortened movement cadence, non-tail self-collision, continued death, reload and renewed control. See the [fixture configuration](../../fixtures/README.md) and [independent validator](check_snake_journey.py).

## Run with explicit inputs

Build the worker and harness, resolve the pinned original, and import a fresh graph for the planner. Run these commands from the repository root:

```sh
cargo build -j 2 -p worldrunner-disunity-ffi -p compat-player -p compat-kit
SNAKE=$(cargo run -j 2 -p worldrunner-original-games --bin original-games -- path snake)
mkdir -p target/compat-inputs
cargo run -j 2 -p worldrunner-compat-content --example import -- \
  "$SNAKE" Assets/Scenes/GameScene.unity \
  crates/compat-content/profiles/snake-source.json \
  crates/compat-content/overlays/snake.json > target/compat-inputs/snake.json
python3 -m unittest discover -s tools/compat-play
python3 tools/compat-display/run.py -- python3 tools/compat-play/snake_journey.py run \
  --worker target/debug/compat-player \
  --harness target/debug/compat-kit --graph target/compat-inputs/snake.json \
  --output .work/snake-plan --desktop --profile source-6000.3
```

The planner resolves the pinned Snake checkout through `tools/original_games.py` and the original-games crate. Exported fixtures name `"game": "snake"`. Output must be a new directory inside this workspace's `.work`. Subprocesses inherit the caller's configured environment. `probe` runs a short prerequisite check. `run` without `--desktop` reports inconclusive overall even after GPU assertions pass. Desktop execution should use the private display harness.

The planner reads authored configuration and prefab references. BFS routes only through currently observed free cells; it never predicts or mutates the body's game rules. Each prefix starts from load with seed 17 and reproduces earlier observations. Final fixture actions subdivide logical tick durations into native updates bounded by 1/60 second, preserving input/observation/capture order. Bounds limit worker prefixes, timeline frames, log sizes and process lifetime.

## Validate current runs

```sh
python3 tools/compat-play/check_snake_journey.py path/to/current-record.json
```

`--require-matrix` requires source-6000.3 and compat-2017.1, each GPU and desktop, with equal current request timelines and decoded pixels. Validation reads actual requests, reports, stdout, assertions, captures and measured teardown. Capture checksums guard run-file integrity. The original-games crate verifies the pinned source checkout before planning and canonical replay.

Outputs contain planning requests/responses, observed states, the generated fixture, bounded logs and current run results. Capture validation rejects corrupt, blank and unchanged images. The explicit original-source/authored-missing-sprite label records the authored visual substitution. This journey exercises the repository's facade and renderer and makes no official Unity compiler/editor equivalence claim.
