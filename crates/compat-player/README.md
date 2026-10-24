# Native-backed compatibility player

`compat-player` executes the shared harness worker protocol using the canonical Rust world, CoreCLR script bindings and shared scene renderer. The player depends on engine services through the `worldrunner` facade. It dynamically loads the same FFI image used by the C# adapters.

## Build and run

Requirements are the workspace Rust toolchain, .NET 8 SDK/runtime, a Vulkan adapter and an X11 display for desktop execution. The current worktree environment points all new build products, package caches and temporary files into `.work`.

```sh
source .session/env.sh
cargo +1.97 build --locked -j 2 -p worldrunner-disunity-ffi -p compat-player -p compat-kit
.work/target/debug/compat-kit fixture check
.work/target/debug/compat-kit run snake --profile compat-6000.3
.work/target/debug/compat-kit run snake-journey --profile compat-6000.3
python3 tools/compat-display/run.py -- .work/target/debug/compat-kit run pong-journey --profile compat-2017.1 --desktop
.work/target/debug/compat-kit serve --port 8787
```

Fixture source roots must contain the selected original projects. Their origins, selected scenes, source count expectations, profile mappings and explicit content substitutions are recorded in `fixtures/*.json`. Game sources are compiled unchanged. Missing source trees, resources, APIs or proof records fail admission. The short `snake`, `pong` and `breakout` fixtures are smoke tests; the journey fixtures exercise longer gameplay sequences.

The current modern and legacy profiles execute the selected source API surface on .NET 8. Their names do not certify historical editor/compiler equivalence. Snake uses an explicitly authored replacement for its missing sprite. Pong uses explicit legacy field/UI mappings and a licensed DejaVu font. Breakout declares its unlit material translation and sprite-bounds wall collision fallback. These labels travel with captures and results.

## Continuous desktop play

The `*-interactive` fixtures select continuous play through the existing harness worker protocol:

```sh
source .session/env.sh
python3 tools/compat-display/run.py -- .work/target/debug/compat-kit run snake-interactive --desktop
python3 tools/compat-display/run.py -- .work/target/debug/compat-kit run pong-interactive --desktop
python3 tools/compat-display/run.py -- .work/target/debug/compat-kit run breakout-interactive --desktop
```

These commands require a matched, activated interactive-capable player/native/managed bundle. The contained runner creates its own private X11 display with worktree-backed socket and lock files, verifies its mount namespace and tmp mapping, and cleans up its owned processes. It does not expose the window on the host desktop. The historical shared-display confinement violation remains recorded. Current private-candidate explorations are tied to their recorded hashes; they do not establish acceptance of later builds. Each fixture freshly compiles its current original scripts: Snake 8, Pong 20 and Breakout 7. The source scenes, profiles and content-substitution labels are inherited from the corresponding replay fixture. The originals remain unchanged. Snake uses W/A/S/D, Pong uses W/S and the up/down arrows, and Breakout uses A/D.

Snake's interactive fixture requests a 1280x640 viewport so its original 40x20 board fits the imported orthographic camera. The original scene and configuration are unchanged. The default 960x720 viewport crops part of that board; a source-backed viewport regression covers the correction.

The interactive controls are F5 to reload, F12 to capture, and Escape or window close to stop. Reload clears scripted keys, OS-held keys and axes in both the driver and the native session. A held key needs a new non-repeated press after reload. Focus loss releases OS-held keys. F5, F12 and Escape are reserved in interactive mode; replay input retains its existing behavior.

Interactive metadata is a strict object with three required unsigned integer fields:

```json
"interactive": {"duration_ms": 30000, "max_frames": 3600, "max_captures": 8}
```

Admission requires desktop execution, duration 1..60000 milliseconds, frame budget 1..36000 and capture budget 1..64. Unknown fields, wrong types, cached assemblies and conflicting timelines fail before activation. The timeline must contain exactly one `load` at frame zero. Authored input, advances, observations, captures, reloads and unloads belong to replay fixtures. Omit `interactive` entirely to retain replay semantics. A null or boolean value is invalid.

After loading, each live update advances the same native-backed `Session` using the actual `LiveTick.dt`. The player subdivides a delta into steps no larger than 1/60 second. Each tick admits at most 250 milliseconds and 15 steps, so long render/OS stalls discard excess catch-up time. The duration budget includes that discarded wall time, and the frame budget is independent. Loading happens before the live host's first timer sample. Native frame admission, fixed callbacks, physics, Update and LateUpdate retain their existing authority; there is no separate gameplay world or managed clock.

The initial and stopping frames request captures automatically. F12 requests additional captures until the budget is exhausted; further requests are ignored. Each request records read-only `world.snapshot` and `managed.snapshot` observations, plus the actual presented PNG and frame receipt. A 64-capture maximum bounds these observations to 128. `interactive-events.ndjson` records ingress, held keys, actual deltas, admitted steps, reload/stop decisions and successful presentation receipts. The harness still enforces its artifact and worker-deadline limits. Stop and errors return through the existing measured native/managed teardown.

Interactive results establish bounded exploratory execution. Full game qualification still requires the journey assertions and independent evidence review described below. Automated X11 event injection establishes OS-event ingress; it does not establish arbitrary manual-play acceptance.

## Worker execution

The harness supplies absolute `COMPAT_ROOT`, `COMPAT_REQUEST`, `COMPAT_RESPONSE` and `COMPAT_ARTIFACTS` paths. Schema-version-1 requests contain a fixture/profile, current source hashes, timeline and metadata. Actions are `load`, `input`, `advance`, `observe`, `capture`, `reload` and `unload`.

Native `frame.begin` admits fixed work and advances the clock. The player calls script phases around native physics, delivers native contacts, and extracts immutable snapshots for rendering. A native lifetime scope covers the fixed, update and late phases of each admitted frame. Destroy requests remain visible to native queries until the end-of-frame flush retires their generations. This preserves original callback-time object counts during multiball loss. The player attempts that flush when a callback fails and retains ownership for teardown.

The first session in a process copies the selected canonical native image into `.work/compat/native-images`, verifies the source and copied hashes, and retains that path for the process lifetime. Subsequent sessions use the same immutable image, including after their run artifact directories are removed. Every run records its actual image path, hash, byte count and process scope in `build/native-image.json`. Rust and C# load that same image and share its world store. A concurrent Cargo rebuild cannot overwrite the admitted copy. Image changes during admission fail the run. Selecting a rebuilt native provider requires a fresh worker process; the harness launches a new worker for each run.

For private bundle qualification, `COMPAT_NATIVE_LIBRARY` selects an explicit absolute regular native image beneath this worktree's `.work`. Relative paths, symlinks, empty images and files outside `.work` reject before source compilation. Unset it to retain the platform's `.work/target/debug` provider. The override changes the source of the same process-owned immutable image; Rust and C# still share that image and STORE. Record the actual `build/native-image.json` and fresh bridge/facade hashes for each run. A native override does not select a player executable or prove bundle compatibility.

Headless and desktop execution use the same session and timeline. Desktop captures are receipts from the presented frame. Offscreen captures use the shared `SceneGpu` and capture services.

Observations include `world.snapshot`, `managed.snapshot`, `events`, `object:<authored-name>` and `managed:<type>:<public-member-path>`. Name-based object observations return the first matching native object; use `world.snapshot` to inspect distinct clone handles. Unsupported observations fail explicitly. Original `Debug.Log` output is retained in worker stdout. The player does not expose private gameplay setters or manufacture contact events.

Teardown reports actual native world and managed binding/script/coroutine counts. Readbacks complete synchronously before capture returns. Failed loads remain owned until cleanup. A failure can retain earlier actual captures, but it cannot produce a passing teardown result.

## Source and assembly qualification

Fresh builds independently discover the current gameplay source set, validate hashes, use explicit absolute MSBuild `Compile` items and inspect actual CLI/PE type and assembly-reference metadata. `build/original-source-count.json` records the evaluated inputs and reusable proof.

Cached compilation requires both `metadata.assembly` and `metadata.assembly_qualification`, using absolute, non-symlink paths below this worktree's `.work`. Admission independently rechecks current originals, the exact source set, DLL bytes and PE metadata. Empty, changed, stale or unrelated assemblies are rejected before loading. Source updates can be rebuilt and qualified normally; recorded hashes identify those inputs.

## Tests and acceptance

```sh
source .session/env.sh
cargo +1.97 test --locked -p compat-player --all-targets -j 2 -- --test-threads=1
python3 -m unittest discover -s fixtures/gates -p 'test_qualify.py'
```

The player tests include actual original-source compilation, independent cached-assembly rejection, bounded texture decoding, native glyph extraction, source tile geometry, and live Snake binding/reload/failed-load cleanup. They also repeat public sessions using the same build directory and after prior artifact removal. Managed CI executes the native-backed throwing-coroutine disposal regression; cleanup errors remain visible while independent cleanup drains measured roots.

Gameplay acceptance comes from the full generic-harness journey records and their independent checkers under `tools/compat-play`, `tools/compat-pong` and `tools/compat-breakout`. Snake and Pong each have a full journey fixture. Breakout uses three seeded scenarios: `breakout-journey-controls-complete-compact`, `breakout-journey-multiball-loss-compact` and `breakout-journey-wide-complete-compact`. Their compact presentation grouping retains the ordered keyboard input, bounded native advances, observations and assertions while reducing desktop render work. They retain all 48 bricks and 48 physical wall cells and the 120-second worker deadline. This qualifies a scenario suite rather than a single continuous campaign.

Inspect each record's assertions, source manifests, diagnostics, actual image receipts and teardown. A successful smoke test or compiler exit alone does not qualify a full game.

Current integration results and remaining work are recorded in `project-plans/unity-tck/execution.md`. The harness protocol, dashboard and refresh API are described in `crates/compat-kit/README.md`.

## Opt-in rational phase-context execution

Set `metadata.execution_profile` to `"native_phase_context_v1"` to select the strict adapter. Omitting that field preserves the legacy float actions, held-input projection, subdivision thresholds and journals described above. Every unknown explicit value, including null, rejects before compilation. Strict execution requires fresh original-source compilation and rejects cached assembly metadata.

The player opens `CompatibilityHost.open_phase_session(world)` on a fresh world before materializing the imported graph. Its load manifest explicitly binds `requirements.execution_profile`. Rational frames delegate to `CompatibilityHost.advance_rational(world, num, den)`. The runtime and native provider own phase credentials and frame scheduling. The native capability is `native_phase_context_native_v1`; the bridge capability and selected profile are `native_phase_context_v1`. This adapter does not claim `native_scheduler_v1`, native lifecycle/coroutine readiness authority, or a complete phase-token scheduler profile.

The strict actions are `advance_rational` with `dt: {"num":1,"den":60}` and `input_events` with an array of native normalized events. See [the short Snake timeline](examples/strict-snake-timeline.json). Combine that timeline with the existing Snake fixture's source and content metadata plus the explicit execution profile. Strict sessions reject `advance` and `input`; legacy sessions reject the new actions. Worker admission checks the entire authored timeline before loading a world. Ratios require unsigned integer components, numerator at most 9007199254740991, and denominator 1..1000000000000. Native admission imposes its additional operation-specific limits.

Normalized event batches contain 1..4096 records with increasing integer IDs, nondecreasing rational timestamps and one source domain, `os` or `replay`. Keys retain their native canonical spelling. Buttons, finite normalized axes in -1..1, and focus transitions use the native schema. Unknown fields and mismatched event fields reject the complete batch. Native input owns held state, down/up edges, focus clearing and axis projections. A session cannot mix OS and replay domains across batches.

Strict desktop execution stages `LiveInputSample` observations from the shared driver's defaulted `ingress` hook and validates the whole staged batch before sending one `input.events` request. It never derives gameplay held state from `LiveTick.input`. F5, F12, Escape, text, resize, close and capture remain host policy. Filtered observations can leave sequence gaps. Repeated keydown observations remain keydown transitions in the native schema, which has no separate repeat field; the original viewer repeat bit remains in the update journal. `ChannelDelivery` rejects because producer provenance and time mapping are unavailable. Scheduled replay and OS acquisition clocks are admitted separately.

Continuous strict updates preserve every admitted `Duration` nanosecond as numerator/1000000000, with chunks at most 16666666ns and at most 15 chunks. A delta above 249999990ns rejects before input submission or native frame mutation. It is never clamped or silently discarded. Reload explicitly retires the old world's staged queue and resets its input domain, sequence origin and timestamp epoch at the sampled tick endpoint. Stop and reload retire the old session's pending elapsed interval; ordinary updates conserve their complete delta. The legacy interactive path keeps its existing stall-clamping policy.

The current shared viewer starts acquisition timestamps at initialize completion and measures subsequent deltas from its first tick, whose delta is zero. These origins have a small positive offset. The player forwards both values faithfully. Exact common-origin equivalence remains open until the viewer supplies an opt-in common origin while preserving legacy first-tick behavior. The contained OS trial established future-only native admission for its measured events, including reload, without timestamp nudging. This is a bounded observation rather than a proof for all clock boundaries.

Selected-provider qualification and immutable artifact identities are recorded in [the player handoff](../../.session/resumed-player-phase-adapter.md). Snake and Pong have bounded strict real-Cdecl gameplay evidence. Breakout passes a 60-frame paddle and brick-spawn smoke run with reload and zero-root teardown. Its longer 360-frame trial exposes a runtime contact-envelope comparison failure after actual physics contacts and is retained for the runtime owner. Full game journeys, native lifecycle/readiness authority, and fatal active-frame cleanup are outside the current qualification.
