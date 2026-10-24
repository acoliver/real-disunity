# Source-driven Pong journey

Run the checked-in journey from the repository root after sourcing the local environment:

```sh
source .session/env.sh
python3 -m unittest discover -s tools/compat-pong -v
python3 tools/compat-pong/replay.py
python3 tools/compat-pong/handoff.py
```

The final fixture and generator default to fresh compilation of the twenty original gameplay scripts. The final fixture has no assembly path, cached qualification path or assembly digest. Each replay publishes its own complete source/MSBuild/PE proof under its private run directory. All 31 original C# files, including eleven excluded Editor tests, are checked before and after execution. The 300-second journey deadline remains unchanged.

`replay.py` runs compatibility 2017.1 and 6000.3 both offscreen and desktop, followed by source 2017.1 offscreen. It uses the existing display 97, Vulkan lavapipe and canonical Rust Playback/session. Each run verifies 366 assertions, five actual GPU captures, score-region changes and measured zero-world/root/readback teardown. Runtime evidence records the actual worker executable and native shared-library mapping, their SHA256 identities and unchanged on-disk images before/after execution. Output defaults to `.work/pong-journey/final-native`.

Use `--resume` to reuse verified runs when the current fixture and runtime identities match that batch. A changed fixture can be replayed into a new `--output .work/pong-journey/<batch>` directory; pass the same output directory to `handoff.py`. Fixture digests identify particular batches and do not restrict future metadata changes. Existing attempt evidence is preserved. Each failed run retains its report, logs and measured identities; retries use the same inputs and stop after three attempts.

`handoff.py` rechecks every successful run and compares its native dynamic-pair event stream with the corresponding report under `.work/pong-journey/prior-image`. It requires identical dynamic events, absence of static-only events in the corrected runs, and ten pixel-identical offscreen/desktop capture pairs. The coordinator handoff is `.work/pong-journey/handoff.json`.

## Regenerating keyboard controls

```sh
python3 tools/compat-pong/driver.py
python3 tools/compat-pong/finalize.py
```

The planner executes the real worker with seed 17 and reads `world.snapshot` and native `events`. Only W/S, UpArrow/DownArrow and native 0.02-second advancement affect gameplay. Inactive prefab templates are excluded. Native contacts and velocity reversal establish paddle bounces. Goal triggers, serialized UnityEvent targets, exact Text scores and complete ball reset state establish scoring. Source-defined coroutine timing and reload control are exercised by the final replay.

`driver.py --assembly-run .work/compat/runs/<run-id>` optionally uses that run's complete player-produced qualification proof for planning prefixes. The player revalidates current source selection, DLL hash, actual PE metadata and declared MSBuild inputs before booting cached gameplay. Old count/files-only proofs are rejected. `driver.py --resume` continues saved planning state. Generated final fixtures always remove cached-assembly metadata, including when planning used a qualified cache.

The corrected image filters static-only contact pairs. This shifted twelve event-array pointers while preserving every dynamic event. `finalize.py --contact-record <canonical-report.json>` aligns those phase/trigger pointers by actual session-local Ball/paddle or Ball/goal identities. It requires unchanged timeline, unchanged originals and passing non-event assertions. Assertion values and count are retained. The alignment audit is `.work/pong-journey/final-native/contact-alignment.json`; ordinary finalization preserves existing contact pointers.

The font/profile label remains `original-source/legacy-mappings-dejavu-font`. The original 2017 project uses the same net8.0 facade under these profiles. Historical editor/compiler equivalence, audio, legacy UI navigation/raycasting and smoothed GetAxis remain outside this evidence.
