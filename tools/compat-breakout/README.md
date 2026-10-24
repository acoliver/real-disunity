# Breakout keyboard journey

The driver runs the seven unchanged original Breakout scripts through the canonical native player. It supplies A/D key states, bounded native advances, public observations, captures, reload and unload. The original scene has 48 bricks and 48 physical wall cells. Gameplay methods, private fields, synthetic contacts, body mutation and reduced levels are excluded.

Run commands from the worktree root after sourcing `.session/env.sh`. Compilation, temporary files and evidence stay under `.work`. The [driver contract](../../.session/breakout-driver-contract.md) records actual successes and retained failures. [Verification code](verify.py) distinguishes observed gameplay from planner-support tests.

## Replay the complete scenario suite

The final suite covers three deterministic source-random branches. Each fixture compiles and qualifies all seven original sources without an absolute cached assembly or qualification path.

* `fixtures/breakout-journey-multiball-loss-compact.json` uses seed7. It observes ten distinct break sprites, brick removal, two active balls, the first ball's loss with the surviving identity retained, and final-ball removal followed by a fresh upward-launched ball.
* `fixtures/breakout-journey-controls-complete-compact.json` uses seed17. It observes normal clamps at x±7, reload, the restored 48-brick layout and renewed D input moving the paddle to x0.4.
* `fixtures/breakout-journey-wide-complete-compact.json` uses seed20. It observes the source-selected width4, both wide clamps at x±6, native paddle contacts, width remaining active beyond its initial eight-second duration and eventual return to width2. The unbatched actual trace first observes normal width at logical tick1208, native time24.16s.

Qualification requires all three scenarios on both profiles and in both presentation modes. Run desktop variants sequentially on the shared display:

```sh
source .session/env.sh
python3 -m unittest discover -s tools/compat-breakout -v
python3 tools/compat-breakout/replay.py fixtures/breakout-journey-multiball-loss-compact.json --matrix
python3 tools/compat-breakout/replay.py fixtures/breakout-journey-controls-complete-compact.json --matrix
python3 tools/compat-breakout/replay.py fixtures/breakout-journey-wide-complete-compact.json --matrix
python3 tools/compat-breakout/qualify.py
```

The qualifier publishes `.work/breakout-journey/journey-qualification.json`, with individual run identities and explicit missing requirements. It reports incomplete until every required replay and gameplay check passes. Coverage is a three-seed scenario suite. No individual fixture claims a complete single-timeline journey.

Every replay retains the120000ms worker deadline and32-capture limit. The Python wrapper allows130seconds for the harness to publish a result. Desktop uses DISPLAY=:97. Both modes use the canonical WGPU Vulkan renderer with the installed lavapipe driver. Evidence retains the declared `original-source/unlit-material-translation/sprite-bounds-collision` fidelity label.

## Native simulation and presentation grouping

Each logical tick contains two ordered .01-second native advances. Every native advance executes the player's established fixed physics, contacts, Update and LateUpdate phases. Source time, random choices and keyboard action order are preserved.

The player's [desktop timeline implementation](../../crates/compat-player/src/desktop.rs) groups actions by their `frame` presentation label. `compact_presentation` batches at most ten logical tick groups per presentation, with a new presentation after each capture. The long width scenario therefore presents125frames while retaining all1210logical ticks. It preserves every action field except that label and adjusts assertion frame selectors consistently. Metadata retains the logical-to-presented mapping and original checkpoint logical frames. The1989/1018/2200 positive assertions remain intact.

This grouping addresses desktop rendering cost without changing the120-second deadline or native update size. The original unbatched requests and their actual desktop timeouts remain available in `.work/compat/runs/` and the contract. The cause of one initial-frame desktop stall during concurrent windows remains unproven.

## Plan and freeze additional traces

Use a new case name for an independent plan:

```sh
source .session/env.sh
python3 tools/compat-breakout/driver.py --case new-controls --scenario controls --export --compact
python3 tools/compat-breakout/driver.py --case new-extra --seed 7 --scenario powerup --export
python3 tools/compat-breakout/driver.py --case new-wide --seed 20 --scenario powerup --export
python3 tools/compat-breakout/driver.py --case new-wide --resume --probe 0 --follow 100 --export
```

`--resume` restores the latest successful prefix. Every prefix replays from load. Failed requests, responses, action traces and compiler logs remain on disk without replacing successful state. Planning is bounded to100prefixes and5000logical ticks.

Tracking uses only currently observed ball positions and downward direction. It selects short keyboard batches near the paddle and longer batches elsewhere. Fixed-target movement uses the observed initial paddle position and original keyboard movement rate. Original `PowerUp.Apply` applies its random result directly after brick animation. The source has no falling collectible objects.

The first planner prefix compiles and qualifies the original scripts. Later exploratory prefixes may reuse that player-produced assembly qualification. Frozen replay fixtures always remove cached assembly fields. `Planner.export(..., sample_frames=[...])` retains observations at selected already advanced ticks. Select enough public observations to establish the behavior while respecting the harness16MiB response cap. The final fixtures retain all ten actual first-brick break sprites and the samples around ball loss or width expiry.

## Evidence checks and limitations

`verify.py` checks fresh compiler inputs, unchanged seven-source hashes, checkpoint ball/brick counts, actual960x720 nonuniform GPU captures and image hashes. The native identity comes from each worker's frozen `build/native-image.json`, with its copied file checked against the manifest. Concurrent canonical Cargo rebuilds cannot change that worker identity. The qualifier also records the actual run-local bridge and facade hashes.

The suite checks native speed6, brick/wall/paddle/DeathZone contacts, ten break sprites followed by removal, both width-aware clamps, two-to-one ball identity preservation, last-loss fresh launch, reload and renewed keyboard input. Successful runs must measure zero native worlds, managed roots and pending readbacks. Timeout records without a worker response cannot establish teardown.

Width refresh is observed through public width and native time. The report states the measured interval beyond the authored8s initial duration and its eventual normal return. It does not claim direct observation of the private coroutine deadline or an intercepted MakeWider call.

Support tests cover stimulus guards, steering, replay sampling, presentation-only compaction, width-aware clamps, loss identity parsing, width intervals, cumulative-event counting and frozen-image identity. These parser/planner tests do not establish game compatibility. Their red/green logs are under `.work/results/breakout-*.log`. The matrix qualifier was implemented before its integration checks; complete gameplay evidence comes from actual canonical-player replays.
