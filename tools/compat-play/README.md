# Original-source Snake journey

`snake_journey.py` generates a complete fixture from observations of the eight unchanged scripts under `.work/games/snake/Assets/Scripts`, then executes that fixture through the existing `compat-kit` harness. All game state is produced by `compat-player`, its native imported world, and the private-callback C# bridge. The Python code performs grid navigation and checks externally observable outcomes.

## Commands

Run from the repository root. Every worker and harness subprocess also sources `.session/env.sh` before executing.

```sh
source .session/env.sh
python3 -m unittest discover -s tools/compat-play -p 'test_*.py'
python3 tools/compat-play/snake_journey.py probe
DISPLAY=:97 python3 tools/compat-play/snake_journey.py run --desktop --profile source-6000.3
DISPLAY=:97 python3 tools/compat-play/snake_journey.py run --desktop --profile compat-2017.1
```

An explicit output path is optional and must be a new directory inside `.work`:

```sh
python3 tools/compat-play/snake_journey.py run --desktop \
  --output .work/snake-journey/my-source-run
```

`run` without `--desktop` executes the GPU journey and reports overall `inconclusive`, even when GPU assertions pass. Full acceptance requires the real desktop replay. No display server is started or substituted by the driver. An unavailable display remains an execution limitation.

The checked-in `fixtures/snake-journey-provider.json` is a short provider smoke fixture:

```sh
.work/target/debug/compat-kit run snake-journey-provider
```

Its passing status establishes provider admission, observations, and a capture. It does not establish complete Snake gameplay. The full fixture is generated at `.work/snake-journey/<run>/fixture.json` only after the source-backed prepass validates every journey step. The published `fixtures/snake-journey.json` is named `SnakeJourney` in the dashboard and contains the complete observed seed-17 timeline. The provider fixture and the original `snake` fixture remain separate smoke cases.

Replay the same published journey without repeating planning:

```sh
.work/target/debug/compat-kit run snake-journey --profile source-6000.3
DISPLAY=:97 .work/target/debug/compat-kit run snake-journey --profile source-6000.3 --desktop
.work/target/debug/compat-kit run snake-journey --profile compat-2017.1
DISPLAY=:97 .work/target/debug/compat-kit run snake-journey --profile compat-2017.1 --desktop
```

The display must already exist. `:97` is the canonical desktop Xvfb display used for the recorded runs. Every replay compiles the eight originals afresh; the published fixture contains no cached assembly path. Regenerate the timeline after a source or authoring update changes gameplay. Stored hashes describe the evidence used to generate the timeline and do not restrict future source admission.

## Behavioral assertions

1. Read actual imported `GameConfig`, `SnakeView.segmentPrefab`, `SnakeView.cellSize`, and `FoodSpawner.foodVisual` references. Require eight discovered original scripts and record current SHA256 hashes. Hashes are discovered on each run; there is no pinned-source admission list.
2. Observe the first native clone and source spawn position after one controlled Update. Resolve the authored segment name through its prefab reference. The selected original spells it `SneakSegment`; the driver accepts the native name and its `(Clone)` form. Inactive templates never count as visible body.
3. Submit an opposite WASD direction and verify movement continues in the prior direction. Submit all four orthogonal turns and verify the resulting native head positions.
4. Navigate to the right boundary and cross from `gridSize.x - 1` to zero, then cross the upper boundary from `gridSize.y - 1` to zero using the imported authored wrap setting.
5. Read `FoodSpawner.CurrentFoodPos` through its public getter. Cross-check it against the native transform of the serialized `foodVisual`. BFS routes through observed free cells and submits WASD. It does not predict or mutate snake body evolution.
6. Verify that eating moves the head onto the previously observed food cell, respawns public/native food, and preserves current visible length. The following tick must add exactly one visible segment. Consume at least five foods and require at least six visible segments. Capture each food-consumption and growth checkpoint.
7. After each observed food consumption, use `1 / min(initialRate + observedFoods * increment, maximum)` as the next logical tick duration. This duration is shorter than the initial interval. Planning uses one controlled Update per logical tick. The final immutable replay subdivides each duration into native Updates of 1/60 second plus a final remainder. Input, observation and capture frame labels identify logical ticks; the desktop presents those logical frames. Verify actual native movement and deferred growth within each shortened duration. No private timer or speed fields are read.
8. Move through an observed free straight corridor, turn toward the visible body, and verify the final proposed destination is currently occupied by a non-tail segment. Submit the collision input. Verify the visible head/body and food stop changing, and the original `Debug.Log("Game Over")` appears in actual worker stdout. Two later WASD ticks must remain stationary and produce the source's repeated Game Over log.
9. Use generic fixture `reload`, then verify the original spawn, one-segment body, seeded initial food, and renewed W control. The original has no reset UI/key; all artifacts label this operation `fixture-reload`.
10. Emit one immutable timeline with explicit equal, tolerance, event-order and capture assertions. Execute it through `compat-kit` GPU playback, then pass the identical fixture to `compat-kit --desktop`. Revalidate every native/public state against the source-validated prepass, verify source stdout and zero-root teardown, decode canonical GPU PNG pixels, reject uniform images, and require changed pixels across captures.

The imported authoring currently has grid 40×20, ticksPerSecond 8, speedIncreasePerFood 0.3, maximum 20, and wrapAround true. `GameConfig.cs` declares wrapAround false as its field default. Serialized authoring takes precedence. `GridSystem.Step` returns unbounded coordinates when wrap is false; `SnakeModel.Tick` contains no InBounds/death check. This journey refuses to silently change false authoring. A future wrap-false test must be a separately labeled configuration-sidecar variant and assert the behavior in the source.

## Observations and provider contract

Required selectors are `world.snapshot` and `managed:FoodSpawner:CurrentFoodPos`. The latter returns `{ "x": integer, "y": integer }`. No access to `GameLoop.snake`, private unserialized state, setters, or method-based game control is permitted. Public `Debug.Log` stdout supplies Game Over evidence. The proposed generic bridge operation is `{ "op":"managed.public", "type":"FoodSpawner", "path":"CurrentFoodPos" }`; the player maps its selector to that operation and unwraps the protocol data.

See `.session/snake-driver-contract.md` for the earlier provider handoff. The public getter and post-unload statistics now work in the recorded full runs. `managed.logs` remains absent from the C# observation dispatcher; this lane uses exact source stdout lines and needs no log-selector substitute.

The generic harness has a separate name-resolution issue: the catalog/dashboard displays the JSON `name` (`SnakeJourney`), while named CLI/API lookup constructs a filename from that value. This scope permits `fixtures/snake-journey*.json`, so the published file is `fixtures/snake-journey.json`. Dashboard status rows show the actual passing runs, and the documented CLI command `run snake-journey` works. The dashboard Run action for `SnakeJourney` requires the harness owner to resolve catalog names to their fixture paths. Reproduction: `compat-kit fixture check SnakeJourney` fails with `No such file or directory`; `fixture check snake-journey` passes. Evidence: `.work/results/snake-journey-name-lookup.stderr` and `snake-journey-fixture-check.json`. No harness files were changed in this lane.

`source-6000.3`, `compat-6000.3` and `compat-2017.1` identify facade-backed source-execution profiles. These runs do not certify compilation by an official Unity compiler, an editor version, or every API in those versions. The local training guide is unofficial and unverified. The driver relies on the actual game source and observed behavior.

The visual sidecar is `crates/compat-content/overlays/snake.json`, with `snake-source.json` import profile and `snake-admission.json` admission policy. The evidence label is `original-source/authored-missing-sprite`. The missing sprite is the explicitly authored unit-square overlay. Every capture comes from the canonical renderer; Python never draws game images.

## Artifacts and failure handling

Each run records:

- `summary.json`: status, exact blocker, source hashes, config, completed checks, canonical records and capture paths.
- `progress.json`: checks completed before any later failure.
- `prefix-NNN/request.json`, `response.json`, `stdout.log`, `stderr.log`, and imported scene/build manifests from actual worker runs.
- `observed-trace.json`: native-visible body cells, snapshot indices, and public food at each observed frame.
- `fixture.json`: complete generated harness timeline and assertions after successful planning.
- `gpu/record.json` and `desktop/record.json`: actual harness run records. Their referenced run directories contain real captures and teardown evidence.

Each prefix runs in a fresh process with seed 17 and replays all earlier inputs. Previously observed states must reproduce exactly. This avoids requiring a game-specific worker controller or an IPC extension. Bounds are 100 worker prefixes, 600 timeline frames, 180 seconds per worker and 240 seconds per canonical harness invocation. Output/temp/cache paths remain under `.work`; the runner checks the 500GB worktree budget and the harness enforces its own budget.

Exit status is 0 for complete GPU plus desktop acceptance, 1 for an observed/assertion failure, and 2 for a blocked or inconclusive run. Unit tests exercise routing, observation validation, transition gates and PNG parsing with small input values. They establish driver behavior only. They provide no substitute for a real source-backed game journey.

## First integration checkpoint

Thirteen unit tests passed after recorded red failures (`.work/snake-journey/unit-red.log` and `transition-red.log`). Real provider probe `.work/snake-journey/provider-probe-2` compiled eight originals, moved the visible native head to [21,10], and produced `prefix-001/captures/probe.png`. Its actual 960×720 decoded image contains three RGBA colors. Complete attempt `.work/snake-journey/complete-attempt-1/summary.json` was blocked by the missing player public-getter selector. This checkpoint establishes no complete gameplay or desktop pass. Later run summaries are authoritative for further progress.


## Complete five-food acceptance

The [final matrix](../../.work/results/snake-journey-full-matrix.json) verifies four actual canonical runs. Each passes 1,466 harness assertions over 137 observed logical frames and 1,042 native Updates, with 16 actual 960×720 PNG captures. The maximum native Update delta is 1/60 second. All four runs used the identical immutable timeline and produced identical decoded pixels at every corresponding capture. Source hashes match before and after execution, the source Git checkout reports no script changes, and every teardown measures zero native worlds, managed roots and pending readbacks.

| Profile | Playback | Record |
| --- | --- | --- |
| source-6000.3 | canonical GPU | `.work/results/snake-journey-source-gpu.json` |
| source-6000.3 | actual desktop, DISPLAY=:97 | `.work/results/snake-journey-source-desktop.json` |
| compat-2017.1 | canonical GPU | `.work/results/snake-journey-2017-gpu.json` |
| compat-2017.1 | actual desktop, DISPLAY=:97 | `.work/results/snake-journey-2017-desktop.json` |

Food consumption occurs at logical frames 60, 81, 98, 105 and 122. Deferred growth occurs at 61, 82, 99, 106 and 123, producing lengths 2 through 6. The shortened growth durations are approximately 0.120483, 0.116280, 0.112361, 0.108697 and 0.105264 seconds, compared with the initial 0.125-second interval. Self-collision occurs at frame 133 against the previously visible non-tail cell [10,15]. The source emits exactly three `Game Over` lines at collision and two subsequent dead ticks. Reload recreates the initial state at frame 137; W moves the new head at frame 138.

Run the independent artifact acceptance check:

```sh
source .session/env.sh
python3 tools/compat-play/check_snake_journey.py --require-matrix \
  .work/results/snake-journey-source-gpu.json \
  .work/results/snake-journey-source-desktop.json \
  .work/results/snake-journey-2017-gpu.json \
  .work/results/snake-journey-2017-desktop.json \
  --output .work/results/snake-journey-full-matrix.json
```

This check reads actual canonical request/report files, public/native observations, source stdout, source hashes and decoded capture pixels. It checks the five-food minimum, delayed growth, shortened movement cadence, reversal, four turns, both wrap axes, occupied non-tail collision, continued death, reload and renewed control. It also checks capture hashes against canonical receipts and requires exact profile/desktop timeline and pixel parity. It performs no game-state mutation or image generation.

Representative actual desktop images are under `.work/compat/runs/1792551402272-3324552-0/captures/`: `growth-5.png`, `self-collision.png`, `fixture-reload.png` and `renewed-control.png`. All 64 canonical capture paths and decoded pixel hashes are listed in the matrix artifact. The static dashboard at `.work/snake-journey/dashboard/index.html` shows separate passing `SnakeJourney` rows for source-6000.3 and compat-2017.1. compat-6000.3 remains unattempted in this matrix.

Fifteen driver-support unit tests pass. The recorded behavioral red case `.work/snake-journey/five-food-red.json` rejected an actual earlier four-food canonical run. Native-Update subdivision has a recorded red/green test pair in `native-updates-{red,green}.log`. Earlier smoke and four-food artifacts remain preserved and do not satisfy this final acceptance.

Gameplay acceptance has no remaining failures in the tested matrix. The dashboard action name-resolution issue above remains a harness integration handoff. These results cover this original Snake journey and its authored missing-sprite overlay; they provide no official Unity editor/compiler or broad version certification.

A fresh end-to-end invocation of the final driver also passed: `.work/snake-journey/final-driver-source/summary.json` records 24 source-backed prefixes, regenerated fixture JSON identical to the published fixture, and successful canonical GPU plus actual desktop replays. This run verifies the complete driver command after native-Update subdivision was integrated.
