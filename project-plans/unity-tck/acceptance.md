# P15 independent acceptance assessment

**Assessment:** Partial acceptance. The integrated compatibility slice is usable and has current evidence for Snake, Pong, Breakout, the production dashboard, native/managed lifetime fixes, and workspace quality gates. Breakout's compact three-scenario matrix is complete across 12 profile/mode runs with 96 captures. The bounded gameplay and dashboard criteria described below are accepted for their named profiles, inputs, translations, and Linux environment. Full P15 acceptance remains open because broader protocol, catalog, migration, fidelity, and platform requirements remain unaccepted, and the worktree-only X11 runtime confinement requirement was violated.

This is a bounded evidence review of current outputs, source contracts, and the independent cleanup follow-up. A passing game journey proves the named journey on its recorded input, provider image, compatibility profile, display mode, and content translation. It does not prove general Unity compatibility or the broader engine protocol contract.

## Acceptance by requirement group

| Requirement | Assessment | Evidence and boundary |
|---|---|---|
| REQ-01 dashboard and harness | **Qualified for tested production paths** | Production headful browser qualification has eight passing phases: source-profile and compat-profile Snake runs, actual DOM/timeline/image details, real-worker Stop, rejected refresh, and compositor capture. It records real compile/test gates and reports cancellation as `skipped`. The desktop screenshot is 1280x1000. Evidence: [dashboard results](../../.work/dashboard-production/qualification-zE7xcJ/results.json), [production status](../../.session/dashboard-production-status.md). The compositor page has visible horizontal overflow, so this is a bounded usability check rather than a responsive-layout certificate. The tested server/browser paths do not establish every catalog case, all nine result statuses, full attempt filtering, or every report requirement. |
| REQ-02 canonical native authority | **Substantially demonstrated for exercised flows** | The production path is one native image/STORE shared by Rust and C#, with per-process immutable image manifests and actual SHA verification. Snake/Pong/Breakout observations, component mutations, contacts, reload, and teardown use the live player. Native request/reply exports are in [`abi.rs`](../../crates/disunity-ffi/src/abi.rs); protocol operations are in [`protocol.rs`](../../crates/disunity-ffi/src/protocol.rs). Independent process-image follow-up confirms same/distinct/deleted artifact sessions and one mapping per process. This is not complete proof of every thread-affinity, recursion, stale-generation, transactional command, or callback-order guarantee in REQ-02. |
| REQ-03 unchanged source and callbacks | **Qualified for selected source/profile builds and journeys** | The production source census is 8 Snake, 7 Breakout, and 20 Pong files. Current source hashes are unchanged in [cleanup follow-up](../../.session/integration-review-followup.md); Pong's full journey separately verifies all 20 compiler inputs and fresh PE metadata. Independent cleanup follow-up exercises two throwing coroutine disposals, preserves the error, measures zero managed roots, retries unload, reloads, and observes zero native worlds. Current reports qualify source subsets on the .NET 8 compatibility facade. Historical Unity compilers, the 11 Pong Editor/NUnit tests, and exact Unity lifecycle semantics are not certified. |
| REQ-04 shared timeline and input | **Partial** | Game journeys exercise keyboard/replay input and bounded updates; Pong includes legacy axes, controls, goals, wait/relaunch; Snake records 1,042 native updates at no more than 1/60 second. The native protocol still uses `frame.begin` with float `dt` and `input.set` with keys/axes ([protocol.rs](../../crates/disunity-ffi/src/protocol.rs), `frame.begin` and `input.set` variants). No evidence found for the specified rational native clock, normalized timestamped input shared by OS and replay, repeated edge-poll semantics, full six-frame/five-fixed-step target, focus-loss cases, pause/unscaled wait, or full phase ordering. Do not infer these contracts from game replays. |
| REQ-05 import, overlays, refresh | **Partial, with real tested coverage** | Original sources and per-game import mappings are evidenced; Snake's missing sprite is explicitly replaced by independently authored content, Breakout labels unlit material and sprite-bounds collision translations, and Pong labels legacy mappings/DejaVu font. Production Snake refresh with `accept=false` ran both compile and test gates and left source/revision/lock unchanged. Evidence: [browser results](../../.work/dashboard-production/qualification-zE7xcJ/results.json), [fixture metadata and provider records](../../.session/content-next-metadata.json). The missing-Snake-original-content blocked variant, broad malformed-import matrix, upstream-ref refresh behavior, and atomic adoption/rejection across all fixtures were not independently shown by this P15 evidence. |
| REQ-06 Snake | **Pass for the declared translated-content journey matrix** | Six current runs cover source-6000.3, compat-6000.3, and compat-2017.1 in offscreen and desktop modes. Each passed 1,466 assertions with 16 actual 960x720 captures. Evidence includes five foods, deferred growth to six visible segments, turns/reversal, horizontal/vertical wrap, self-collision and original Game Over logs, reload and renewed control. Pixel comparison passed; measured teardown was zero worlds, roots, and pending readbacks. See [matrix qualification](../../.work/results/snake-cleanup-final/qualification.json) and [per-run identities](../../.work/results/snake-cleanup-final/identities.json). This is source-6000.3 / compatibility-facade profile coverage with the named authored sprite replacement, not historical editor certification or original-art fidelity. |
| REQ-07 Breakout | **Pass for the declared compact three-scenario matrix; broader requirement remains partial** | The current [journey qualification](../../.work/breakout-journey/journey-qualification.json) is `complete`, contains 12 unique run records, and has no missing requirements. Controls-complete, multiball-loss, and wide-complete each ran in compat-2017.1 and compat-6000.3, offscreen and desktop. The four profile/mode qualification records each report all 17 semantic checks true. The runs contain 96 actual captures total. Per-run fixture, native image, managed image, and capture identities are recorded; all runs completed within the unchanged 120-second deadline, including the wide desktop runs (88,491 ms and 59,336 ms). Evidence is explicitly three seeded scenarios, not one continuous campaign. Original layout/speed/upward launch, ten break sprites and removal, normal/wide clamps, extension/expiry, no premature first-loss respawn, fresh last-loss launch, event identities, physical paddle/brick/wall/DeathZone contacts, reload, and renewed keyboard control are covered. Material is translated to unlit and collision to sprite bounds; full URP parity and authored alpha/physics-shape collision remain unestablished. |
| REQ-08 Pong | **Pass for the named source / compatibility-facade journey** | Five current cases are recorded: both compatibility profiles in offscreen and desktop modes plus a source-profile control. The current full replay has 366 assertions, 1,422 actions, 692 frames, 25 actual captures across the journey matrix, actual paddle contacts, both goal directions and scores 0/1→1/1→2/1, reset/sleep/hide, timed relaunch, reload and renewed control. Score-region pixel changes were measured. Evidence: [Pong handoff](../../.work/pong-journey/final-native-current/handoff.json). Historical 2017.1 source compatibility is exercised by the facade profile; the actual Unity 2017 editor/compiler and 11 historical tests remain unverified. The DejaVu font is a declared translation. |
| REQ-09 render, play, captures | **Pass for named tested Linux paths; incomplete overall** | Captures are from the shared renderer, and actual desktop journeys are present for Snake, Pong, and all six Breakout desktop cases. Breakout's compact matrix records 96 captures across all 12 runs. No physical GPU, alternate OS/window system, broad exploratory session, all surface resize/loss cases, or general camera-convention proof is established. |
| REQ-10 reports, catalog, quality | **Partial** | [Final all-features CI](../../.work/results/final-ci-all-5.log) completed successfully in 3m23s, including workspace all-features tests, required software-Vulkan tests, ignored GPU tests, doctests, strict lint/complexity, check/build, and managed Cdecl disposal coverage. Native test count is 74. Dashboard/run behavior and selected fixtures are exercised. There is no evidence that all 54 catalog definitions / 95 requirement instances are represented and mapped as specified or that every required status/result reducer and negative control passes. The release gate remains open because broader requirements remain unaccepted, including confirmed X11 confinement noncompliance. |
| REQ-11 future migration seams | **Not accepted** | Existing shared rendering and camera APIs plus the live session are useful foundations, but no accepted local migration-consumer result was located for all required explicit generation, externally supplied target/stage extension, input/capture readiness and consumer contracts. No real Stranded gameplay is in scope, and none is claimed. |

## Protocol and architecture findings

The concrete native ABI currently appends JSON `disunity_world_request`, `disunity_response_copy`, and `disunity_response_free` to the prior exports while retaining ABI version 2 and 15 exports. The request schema is version 1. `protocol.rs` defines operations including `object.create/get/set/destroy/clone/find`, component and transform reads/writes, `input.set/get`, `time.get/set`, `frame.begin`, random, physics, events, and snapshot. This is a real, shared native-state protocol and it supports the completed journeys.

The spec's broader handshake and request-integrity contract remains unmet in the source inspected for this review: no pure `describe` operation, requestId deduplication, phaseToken enforcement, atomic multi-operation transaction, normalized timestamped input operation, or rational frame-context protocol was found. Managed `CompatibilityHost` exports Configure/LoadFixture/DispatchPhase/InvokeContact/Observe/GetLastError/Unload; the required `DescribeBridge` addition was not found. The single-load/execution semantics, zero-length response sizing behavior, and actual callback processing are implemented, but they do not supply those omitted protocol guarantees. This is a confirmed source contract gap, separate from the successful live gameplay journeys.

Native identity and process cleanup are supported by current production behavior, not only counters in unit tests. The independent follow-up also explicitly bounds its conclusion: measured managed-root counters and native-world counts do not certify CLR garbage-collector reclamation. No broader unload claim is made here.

## Fidelity, confinement, and platform limits

- Snake's complete playable lane uses an authored unit-square replacement for a missing source sprite GUID. The original-content missing-sprite lane remains a separate blocked condition.
- Breakout evidence is labeled `original-source/unlit-material-translation/sprite-bounds-collision`; the importer selected sprite bounds because source authored physics-shape data is empty. Full URP 17.3 2D material/light parity and alpha-derived collision are unproved.
- Pong evidence is labeled `original-source/legacy-mappings-dejavu-font`; the DejaVu font and field/type mapping are compatibility translations.
- Browser and desktop evidence used Linux Xvfb display `:97` with lavapipe. Physical GPU qualification, Windows/macOS/Wayland, and historical Unity editor/compiler certification were not performed.
- The plan requires X11 socket/lock artifacts to remain confined to `.work`. The read-only [current confinement measurement](../../.work/results/desktop-confinement-current.json) confirms that Xvfb `:97` ran in the host mount namespace and created `/tmp/.X97-lock` and `/tmp/.X11-unix/X97` outside the worktree. This is a confirmed runtime-file confinement violation. The shared server and external paths were left untouched; later containment cannot repair the earlier violation. Other in-worktree artifacts and valid gameplay evidence remain usable, with this resource exception recorded.
- The measured workspace budget policy is 500,000,000,000 bytes. The latest coordinator measurement was approximately 42.59 GB, below that ceiling. The 24-hour deadline also remained open at the assessment time; satisfying those limits does not cure the X11 path violation.

## Reproduction entry points

The commands below document existing entry points, not a request to regenerate evidence. The current Xvfb setup wrote outside-worktree files, as recorded above. Do not rerun desktop/browser qualification until X11 runtime-file confinement has been corrected and verified; do not remove or alter the shared Xvfb server or external files as part of this report review.

From the project root, use the worktree environment before running commands:

```sh
. .session/env.sh

# Production browser qualification and Run/Stop/Refresh flows
node tools/compat-dashboard/production-browser.mjs

# Current integrated workspace gates. Full CI has already passed for this snapshot.
cargo xtask ci

# The implemented compatibility harness CLI (not the proposed `cargo xtask tck` interface)
.work/target/debug/compat-kit serve --port 8787
.work/target/debug/compat-kit fixture check snake
.work/target/debug/compat-kit run SnakeJourney --profile compat-2017.1 --desktop
.work/target/debug/compat-kit run pong-journey --profile compat-2017.1 --desktop

# Optional Breakout driver probe. The current 12-run qualification is complete.
# Do not rerun the matrix for this assessment; retain its existing evidence.
python3 tools/compat-breakout/driver.py --probe 1
```

The `compat-kit serve` command is long-running. Driver outputs and run identities belong under `.work`; do not rerun or overwrite an active owner matrix. The suggested `cargo xtask tck inventory|run|report` interface in the planning record is not the production CLI implemented in `compat-kit`.

## Evidence index

- [Current Snake matrix, 6 runs](../../.work/results/snake-cleanup-final/qualification.json)
- [Snake actual run/provider identities](../../.work/results/snake-cleanup-final/identities.json)
- [Pong current journey matrix](../../.work/pong-journey/final-native-current/handoff.json)
- [Current Breakout three-scenario qualification, complete across 12 profile/mode runs](../../.work/breakout-journey/journey-qualification.json)
- [Production dashboard/browser qualification](../../.work/dashboard-production/qualification-zE7xcJ/results.json)
- [Final all-features CI log](../../.work/results/final-ci-all-5.log)
- [Independent Cdecl disposal and sequential Session cleanup follow-up](../../.session/integration-review-followup.md)
- [Process-scoped native image contract and verification](../../.session/player-process-image-status.md)
- [Specification](spec.md), [execution record](execution.md), and [phase plan](plan.md)
