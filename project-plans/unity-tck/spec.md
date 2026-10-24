# Unity-shaped compatibility kit specification

Plan ID: `PLAN-20261020-UNITY-TCK`. This specification governs the [implementation phases](plan.md) and [execution/coordination record](execution.md). Status: planned, with source inspection completed. No runtime implementation or game execution is claimed by these documents.

## Outcome and authority

Deliver a working local dashboard and generic Rust game harness first. Use that same harness to compile and play the complete selected Snake, Breakout and historical Pong games with unchanged gameplay source. Rust owns engine objects, components, transforms, cameras, rendering, physics, input, clocks, randomness, resources and scheduling. C# supplies source-compatible declarations, marshalling, managed script instances and callback dispatch. Original games retain their application logic in C#.

The current authorized interval is **2026-10-21T08:28:26-03:00 through 2026-10-22T08:28:26-03:00**. The user authorized a fresh 24-hour window after a system upgrade stopped execution. The original interval and prior evidence remain in the execution record and checkpoint history. All new files, checkouts, caches, temporary files and build outputs stay within `/home/acoliver/projects/real-disunity/branch-1`. The entire effort has a **500,000,000,000-byte** disk ceiling. Neighbors and supplied external fixtures are read-only. The reset leaves the requirements and acceptance criteria unchanged.

This specification updates the sequencing and mutable-fixture policy in the earlier [architecture](../../docs/unity-compatibility-kit-architecture.md), [case catalog](../../docs/unity-compatibility-kit-cases.md) and [fixture specimen](../../docs/unity-compatibility-kit-fixture.example.json). Those documents remain useful behavioral inputs. Their frozen source-lock language, optional disabled Snake substitution and later-dashboard sequencing do not override this task. No official Unity/Unreal documentation, websites, source, binaries or package implementation may be consulted or used as implementation material. Use our documents, permitted third-party game source and training knowledge. Existing links to official material in historical research are not instructions to visit them.

## Inspected baseline and integration points

The inspected committed baseline is `cd06502`, which already includes shared render-shadow mechanics and validated explicit generation requests. A final read found concurrent foreground changes in this worktree: `crates/compat-kit`, `crates/compat-content`, `csharp/UnityCompat`, native protocol/simulation modules, `CompatibilityHost` and `render-scene::DynamicQuad`. These are in-flight implementations, not verified completions. Reuse and extend them. Do not create competing tck/import/facade crates or a second protocol. The table describes the committed starting behavior; the concrete in-flight interface below takes precedence for source coordination:

| Boundary | Existing source and behavior | Required change |
|---|---|---|
| Native storage | [disunity-ffi](../../crates/disunity-ffi/src/lib.rs) owns `STORE`, world/object generation slots, position, yaw, material and queued commands. `step_world` drains commands. | Extract state into one Foundation-layer simulation module. Keep one registry in the loaded native image and preserve v1 command semantics. Add components, hierarchy, phase scheduling and services there. |
| ABI | [abi.rs](../../crates/disunity-ffi/src/abi.rs), [manifest](../../crates/disunity-ffi/disunity_interface.json), [layout tests](../../crates/disunity-ffi/tests/layout.rs) define 12 append-only exports. | Append session protocol exports; intentionally regenerate bindings and update manifest/version. Preserve old signatures and 16/24/32-byte Result/Command/ObjectState layouts. |
| Managed host | [DisunityHost](../../crates/disunity-runtime/src/lib.rs) dynamically loads one selected native image; [bridge](../../crates/disunity-runtime/src/bridge.rs) binds Configure/Run/WorldHandle/Unload/token entries. | Add live session attachment and Cdecl dispatch while retaining the portable bridge adapter. Never introduce a statically linked second registry alongside the loaded cdylib. |
| Managed APIs | [WorldRunner](../../csharp/Disunity/WorldRunner.cs) currently schedules Start/Update/LateUpdate, creates an object per behavior, and flushes writes next step. [GameObject](../../csharp/Disunity/GameObject.cs) caches transform values. | Characterize compatibility first. Replace engine-state caches and scheduling with native-backed operations. Add separate Unity-shaped facade identities and private-message binding; retain existing Disunity consumers. |
| Camera | [camera](../../crates/camera/src/lib.rs) supplies Y-up FreeFly/RTS and a narrow `CameraView`; [SceneView](../../crates/render-scene/src/view.rs) independently resolves Z-up perspective and rejects axial views. | Establish one resolved-view contract in camera, with explicit basis and orthographic lens. Existing constructors/controllers adapt to it. Add a valid top-down/planar orientation without an independent camera implementation. |
| Rendering | [render-plan](../../crates/render-plan/src/plan_scene.rs) composes validated contributions; [render-scene](../../crates/render-scene/src/lib.rs) owns GPU drawing/readback; [scene-viewer](../../crates/scene-viewer/src/app.rs) owns a resident static scene loop and orbit controls. | Extend shared contributions and resident updates for sprites, tile geometry and UI. Generalize the viewer loop for live sessions, input and correlated capture; keep its existing `run` entry as an adapter. |
| Admission | [world-model](../../crates/world-model/src/lib.rs), [world-runtime](../../crates/world-runtime/src/lib.rs), asset-id/catalog/package/texture/material/mesh define semantic admission and resources. | Import selected real scene/prefab/meta/settings schemas into canonical admission/materialization. Immutable documents remain content inputs; they are not a second live object world. |
| Facade and policy | [worldrunner](../../crates/worldrunner/src/lib.rs), [layer registry](../../crates/xtask/src/layers.rs), [quality guide](../../docs/quality.md) | Expose host/simulation/import services through the facade and register each new package/layer explicitly. Update quality package selections when adding packages. |
| Generation | [request.rs](../../crates/world-generation/src/request.rs): `WorldGenerationRequest::from_profile(profile, name)` validates complete named inputs. | Preserve explicit generation; no implicit seed, dimensions, density or profile fallback in harness/native migration adapters. |

Current dependency versions are Rust 2024, toolchain 1.97.0/MSRV 1.95, .NET 8, wgpu 30, winit 0.30, glam 0.33, serde/serde_json and png. The committed baseline has no YAML importer, font decoder or Physics2D solver dependency. Concurrent compat-content now declares serde_yaml 0.9.34 and native physics modules are being written. Validate their actual behavior before selecting another provider; no completed parser/solver/font capability is assumed.

## Requirements and acceptance

The following requirements define observable completion. Catalog priorities describe historical ordering; all selected three-game requirements below are required within this effort.

### REQ-01: Dashboard and generic harness are the first usable deliverable

A developer can inventory fixtures/cases, select any of the three games, choose simulation/offscreen/desktop/interactive mode, launch, stop, rerun, refresh a fixture, inspect a result and open actual images/logs. A local Rust supervisor owns these actions and serves a loopback-only dashboard with checked local assets. A dependency-free HTML/JS client is sufficient; no frontend framework is currently established. Static exported reports remain usable without a server.

Before game completion, the dashboard must already execute a real native session through the production host, show a controlled behavioral failure, show a real successful native probe with capture, and record child crash/cancellation. Cards for missing game capabilities remain selectable and show accurate dependency chains. Fake results and canned screenshots cannot establish this gate. Dashboard code never draws the game scene; its image viewer displays captured output from the canonical renderer.

Result views include case/game/profile/platform filters, expected versus observed values and units, timing/phase/input/contact correlation, prerequisite failures, all attempts, source/overlay identities and frame-correlated images. Text and keyboard operation accompany status colors. API responses and reports escape source text, reject traversal/symlink escapes and keep artifact content inert.

### REQ-02: One canonical engine session

Native drivers and C# wrappers observe the same entity/component generations, transform hierarchy, activity, camera and body state. Setter/read-after-write behavior is coherent within a callback. A later native physics update is visible to C# and to the next render extraction. Destroyed handles cannot access reused slots or a reloaded session. Thread-affinity and recursive advancement violations return diagnostics with no mutation. No independent managed object world, game-only renderer or harness-only camera is permitted.

Retain v1 triangle, queue/error and load/unload behavior through adapters. Old partially applied command batches retain their characterized semantics; new transaction batches validate fully before mutation. The selected native library owns the sole process registry. Rust host calls into that image through a safe loaded-session adapter, rather than linking a second global STORE.

### REQ-03: External source compilation and managed callbacks

Compile all 8 Snake, 7 Breakout and 20 Pong gameplay/support files byte-for-byte in independent game assemblies. Separate the 11 historical Pong tests and their Editor/NUnit/NSubstitute dependencies from player compilation. Do not edit source, inject IL, port game rules to Rust or replace gameplay classes.

A script with private `Start`, `Update`, `FixedUpdate`, collision or trigger methods receives its eligible messages through cached binding of exact signatures. Deserialization and reference binding finish before activation. Awake/enable/start eligibility follows the selected profile and [training lifecycle baseline](../../docs/training-only-unity6-reference/G02-lifecycle-time.md). Assert required per-instance partial order, not arbitrary peer order. Catch exceptions per callback and record type/message/stack/phase; unrelated eligible callbacks continue. Native locks and mutable borrows are released before invoking managed code.

C# may retain CLR script fields, facade wrapper identity caches, reflection metadata, delegates and IEnumerator instances. Native state owns coroutine readiness/deadlines/cancellation and engine lifetime; C# advances enumerators only when dispatched. Pure source value structs may implement local arithmetic. Engine clocks, random streams, physics, transform authority and device state remain in Rust.

### REQ-04: Shared timeline and input

Run six exact 1/60 dynamic frames with a 1/50 fixed step and observe six Updates and five physics steps. Use rational accumulated time, plus documented float API projections. Native schedule: ingress; eligible initialization; fixed callbacks/body commit/solver/contact callbacks; dynamic Update/coroutine continuations; LateUpdate; deferred destroy; layout/extraction; render/present/capture. Newly created objects have per-instance eligibility. Pause stops scaled fixed work/waits while unscaled time and control remain available.

OS events and replay events enter the same normalized Rust input service. Legacy per-dynamic-phase edge snapshots and package fixed/dynamic snapshots are separate projections. Repeated GetKeyDown polls do not consume an edge. Short taps, held keys, simultaneous keys, releases and focus loss are tested. Import Pong named axes and Breakout package update settings. Preserve Breakout's source precedence: D wins when A and D are both held. Import camera/game bindings as data.

### REQ-05: Import and evolving fixtures

Walk the reachable scene/prefab/script/config/settings/meta/texture/material/font closure with located GUID/fileID/subasset references. Bind public and SerializeField fields, arrays/lists/custom serialized records and persistent enum listeners used by the games. Reject unknown required schema/components and incomplete references before activation; explicitly classify inert records. Clone graph-internal references and share external asset resources.

Fixture hashes are editable observations, not permanently frozen approvals. `fixture check` recomputes actual bytes and current upstream-ref identity; `fixture refresh` stages a new revision atomically and runs its affected compile/import/behavior/desktop tests. A changed hash alone triggers refresh, not an indefinite administrative blocker. A passing update becomes the active fixture; a failing update retains diagnostics and the previous runnable revision. Runs already in progress keep their resolved immutable input snapshot. Offline operation checks the selected local ref; remote refresh is explicit and limited to permitted third-party/game or our engine repositories.

Overlays remain separate data: exact source object/property, expected old value, replacement, reason and content identity. Gameplay methods stay unchanged. Source/ref changes re-evaluate overlay preconditions automatically. Never auto-update semantic expectations, widen tolerances, remove assertions, bump thresholds or erase failed attempts to accept an update.

### REQ-06: Complete Snake behavior

Run `Assets/Scenes/GameScene.unity` from eguldali/Optimized-Snake-Game. Initial observed source is `0ab43c53`, saved with 6000.3.7f1; this identity is refreshable. Use the imported 40×20, wrapping, cellSize 1, 8-ticks/second configuration. Source inspection confirms buffered W/S/A/D priority, opposite-turn rejection, at most one tick per Update, delayed growth, increasing tick rate capped by config, pooling/activity and Debug.Log-only Game Over. Preserve those rules and their defects, including absent non-wrap death and bounded food retries.

The real harness must show movement, all turn directions, rejected reversal, wrap, food reached by public-state-driven input, subsequent growth/food relocation/speed change, selected self-contact with Game Over, stopped post-death movement, reset/reload and renewed control. Observe public CurrentFoodPos and visible segment transforms; do not invoke private GameLoop helpers. A source-backed component test may exercise public SnakeModel methods to characterize rare occupancy cases, separately from desktop acceptance.

Both visual prefabs reference missing sprite GUID `311925a002f4447b3a28927169b83ea6`, fileID `7482667652216324306`. Create an independently owned minimal sprite through a declared content-remap overlay for the playable lane. Keep the original-content lane blocked with its exact missing-edge diagnostic. The playable label says unchanged gameplay / substituted content, and the dashboard exposes both results.

### REQ-07: Complete Breakout behavior

Run admtrv/Breakout `Assets/Scenes/SampleScene.unity`, initially `2b960a35`, saved with 6000.3.5f2. All seven scripts participate. Preserve A/D fixed-step movement and width-dependent clamps, random ball launch and per-fixed-update speed normalization, real wall/paddle/brick collisions, sprite break-frame sequence with frameTime waits, immediate collider disable, destruction, DeathZone last-ball respawn, multiball powerup and widening/cancel/replacement/recovery powerup. Exercise both powerup branches with engine random seeds and genuine brick contacts; never call Apply/OnCollisionEnter2D as desktop success evidence.

Use one native Physics2D solver for bodies, MovePosition, shapes, materials, continuous detection, triggers, layer/tag filtering and shape refresh. Test dynamic/static/kinematic participation and actual geometry. Import the tilemap wall; arbitrary Pong-style bounce code cannot replace collision simulation. Characterize pending-Destroy query visibility with a public test before relying on `FindObjectsByType<Ball>().Length <= 1`.

Admission records the missing Ball project tag, serialized `startSpeed` versus current `speed`, and stale brick fields. Provide preconditioned settings/field overlays only where source and scene establish a unique mapping; retain defaults where upstream source ignores stale data and label the decision. All quality tiers select URP17.3 2D. Implement/translate the selected scene's visible requirements through canonical rendering and explicitly identify material/light substitutions. A translated-unlit gameplay lane can pass complete gameplay; original-pipeline fidelity requires its own passing selected-pipeline cases. No full URP execution claim follows from either result. Sprite-pack/space image rights and availability remain separate prerequisites.

### REQ-08: Complete historical Pong behavior

Run Pong-TDD `Assets/Scenes/Main.unity`, initially `e2d33687`, saved with 2017.1.0f3. It remains a distinct historical source profile. Preserve both named input axes, paddle MovePosition, initial diagonal launch, actual paddle/wall bounce and goal triggers, serialized enum UnityEvent delivery, both public score dictionaries/views and readable numeric Text pixels. Goal scoring must stop linear/angular velocity, teleport to initial position, sleep and hide the ball; the source coroutine waits the imported scaled duration, then shows/launches it. Demonstrate both scoring sides, wait-before-deadline, relaunch, repeated goals, reset and renewed control.

Resolve legacy prefab overrides, `team` to `scoresTo` and nested `Goal+ScoreEvent` to top-level ScoreEvent through explicit preconditioned type/property maps. Validate left/right ownership against actual scene overrides. Import the selected Canvas/RectTransform/Text/font closure into native UI/layout/render contributions. Missing fonts receive a labelled independent font overlay, never invisible or fake text. Historical mock-based tests can inform expectations; they do not prove the replacement engine's end-to-end behavior.

### REQ-09: Real rendering, play and captures

Every game runs through the same Prepare/Load/Input/Advance/Observe/Capture/Reload/Stop protocol and the same window/render service. Provide actual desktop keyboard control, scripted replay and a bounded exploratory session per game. Required captures show initial play, input-correlated change, gameplay action, and post-reset control. State assertions and readable scene pixels accompany captures. A static screenshot, compile pass, direct game callback invocation or test-owned fake world cannot qualify a game.

Camera projection, inverse/screen queries, culling and screenshots consume the same resolved view. Orthographic size is half vertical span. Keep governed Z-up millimetre documents and source Y-up metre content behind one explicit conversion adapter; do not apply a second basis rotation during GPU upload. Arbitrary explicit camera basis supports planar views. Sprite UV/rect/pivot/PPU/tint/alpha/sorting, tile transforms, activity and UI text changes reach the existing canonical draw path. Surface loss/resize/zero-area behavior preserves the viewer's recovery semantics.

### REQ-10: Results, dashboard gates and tests

Use the architecture's nine result statuses: pass, fail, unsupported, not-implemented, blocked, skipped, error, flaky, inconclusive. An absent run is unmeasured. Preserve compile, content, semantic, desktop, exploratory and fidelity outcomes independently. Required blocked/unimplemented/unsupported results fail the completion gate. Reports distinguish a known missing-original-content variant from the required admitted overlay variant.

Keep the 54 catalog definitions, 50 compatibility-facing plus 4 harness cases, separate from the 95 game requirement instances and per-run assertion totals. Map every selected A-R/B-R/C-R requirement to executable assertions. Source-driven discoveries append requirements and tests. Mutable fixture identity discontinuities remain visible in trends. Never calculate a global Unity support percentage from these counts.

Tests precede production behavior, with independent red verification. Use real session/FFI/CLR/import/solver/GPU boundaries. GPU absence is blocked/error according to admission, never a skipped-green required lane. Test invalid admission, stale handles, callback exceptions/reentrancy, clock/input edges, import failures, focus/resize, crashes/timeouts, output quotas and teardown. Make a deliberate wrong output or disconnected layer fail a relevant test.

### REQ-11: Future rd-stranded migration stays compatible

Read and preserve the [neighbor findings](execution.md#neighbor-source-and-ownership). Additive resolved-camera, session, GPU/resource, render extension and RunControl contracts must support a future native explorer adapter. Existing advanced terrain, HDR, fog, celestial, vegetation, SSR/SSAO, temporal capture and shadow policies stay at their current owning boundary until separately migrated with behavioral tests. No replacement terrain world, dropped effects or neighbor edits are authorized here.

A local contract consumer tests explicit generation requests, FreeFly/RTS projection/reconstruction, externally supplied GPU targets, render-stage extension, input and capture readiness. A future migration adapter keeps the managed session live; the current snapshot/export/unload/re-grid flow cannot host these games. Real Stranded exploration is an additional target when locally admitted content and the foreground coordinator's capacity permit it. All three C# games are unconditional scope; full Stranded gameplay is outside this task.

## Lane contracts

The interfaces below distinguish observed in-flight source from required additions. P02 reconciles actual caller/provider signatures and records corrections before either side changes. Preserve the existing JSON transport and add missing semantics there. A binary callback ABI is unnecessary for this interval unless measured throughput requires it. The committed-v1 portable bridge remains supported separately.

### Ownership and transport

Lane R owns Rust simulation, appended FFI, canonical camera/render changes and existing-store migration. Lane M owns Unity-shaped facade assemblies, native marshalling, script loading/private callbacks and existing C# compatibility. Lane H owns generic Rust supervisor/worker, dashboard, RunControl and fixture manager. Lane T owns behavioral contracts, real-game import/overlay data, public drivers and independent verification. T's importer materialization calls R services; it never introduces alternate runtime storage. H owns shared manifests/policy edits after requests from other lanes. A phase has one accountable worker; lanes supply bounded subtasks without concurrent edits to the same path.

Use Cdecl in both directions for new entries; retain existing extern-system bridge entries only in the legacy adapter. All buffers are borrowed only for the call. All new UTF-8 is pointer plus byte length, no terminator. Native and bridge limits: 1 MiB request, 8 MiB reply, 4096 callback records per batch, 16 MiB outstanding reply storage per session. Over-budget calls fail before allocating their claimed size. All handles are nonzero u64 generation-checked session-scoped identities. No CLR pointer, Rust reference, glam value, exception or bool crosses the ABI.

Native keeps `DisunityResult { i32 code; padding; u64 detail; }`. Existing codes are OK=0, INVALID_HANDLE=1, BUDGET=2, ERROR=3, INVALID_ARGUMENT=4. If new code values are needed, append without renumbering. Detailed errors carry bounded structured diagnostics.

Observed in-flight Cdecl exports in `disunity-ffi/src/abi.rs` and bound by `disunity-runtime/src/compatibility/transport.rs` are:

```text
disunity_world_request(world:u64, request:u8*, len:usize, response:u64*) -> DisunityResult
disunity_response_copy(response:u64, buffer:u8*, cap:usize, len:usize*) -> DisunityResult
disunity_response_free(response:u64) -> DisunityResult
```

`world_create/world_destroy` retain the same world registry and session identity. Request executes once and returns an owned generation-checked response. Copy is a pure sizing/copy operation: null with cap=0 probes length, insufficient capacity returns BUDGET and required size, retry never repeats mutation. Free invalidates that response generation. Preserve these exact names/signatures and update manifest/generated bindings consistently. This admitted desktop profile is 64-bit; usize maps to C# nuint. Reject incompatible pointer width before attachment. P02 must verify actual generated/runtime signatures, since these files are changing concurrently.

Current request schema is `{version:1, op, ...typedFields}` and reply is `{version:1,ok:true,data}` or `{version:1,ok:false,error:{code,message}}`. Observed operation names are `object.create/get/set/destroy/clone/find`, `transform.get/set/parent`, `component.add/get/set/remove/list`, `input.set/get`, `time.get/set`, `frame.begin`, `random.seed/range`, `physics.configure/step/overlap/raycast`, `events.read`, and `snapshot`. `component.add` takes `{object,kind,fields}`; transform.set takes `{object,position?,rotation?,scale?,space}`; current `frame.begin` takes float `dt`. These names are the immediate interop seam. The spec requires rational native scheduling, deferred destruction, event provenance and normalized input, which current request shapes alone do not establish.

Add a pure `describe` request before activation returning supported native/bridge/schema revisions, capabilities, endianness, pointer width, limits and clock policy. Add optional versioned requestId/phaseToken fields, an atomic transaction operation and normalized timestamped input operation to this same protocol. Reusing a retained requestId with identical bytes returns the retained result without re-execution; different bytes are invalid. Retain deduplication until session end with a declared request-count bound. Never repeat side effects when a released response cannot be recovered. Query selectors expose typed public state; private script-field mutation is rejected. Only the engine host can advance phases/time.

### Native-to-managed API

Observed in-flight `CompatibilityHost` binds these exact Cdecl managed entries. JSON buffers are UTF-8; lengths/capacities and written fields are usize/nuint on the admitted 64-bit profile:

```text
ConfigureNativeLibrary(path:u8*, len:usize) -> i32
LoadFixture(json:u8*, len:usize) -> i32
DispatchPhase(json:u8*, len:usize) -> i32
InvokeContact(json:u8*, len:usize) -> i32
Observe(query:u8*, query_len:usize, out:u8*, capacity:usize, written:usize*) -> i32
GetLastError(out:u8*, capacity:usize, written:usize*) -> i32
Unload() -> i32
```

These signatures are in `crates/disunity-runtime/src/compatibility.rs`. Rust uses `unsafe extern "C"`; managed entries must use explicit UnmanagedCallersOnly Cdecl. Existing source loads delegates via the hostfxr System-typed loader then reinterprets to Cdecl, so round-trip tests must prove this boundary. Configure consumes an explicit-length path, unlike the legacy portable bridge's terminated string.

Define LoadFixture JSON as a versioned manifest containing the host-created session/world handle, allowed assembly paths/types and the imported object/component/field bindings. Attach the existing `UnityEngine.INativeWorld` adapter to that exact session. Load creates and roots managed instances, binds admitted fields, then permits native activation; it cannot create a second world. Add DescribeBridge with the same pure CopyEntry signature as GetLastError for handshake and advertised limits. Preserve the current entry family rather than introducing a parallel Attach/LoadScripts transport.

DispatchPhase JSON is `{version:1,context,callbacks}`. Context contains session, frame, fixedTick, rational `{numerator:i64,denominator:u64}`, phase, flags, native-projected dynamic/fixed/unscaled deltas and phaseToken. Callback records contain generation-checked binding/component handles, exact message kind and typed payload. IDs exceeding JSON's exact JS integer range are serialized as decimal strings for dashboard/managed round trips. Phase names are awake, enable, start, fixed, contacts, update, coroutines, late and destroy. The JSON codec must reject malformed/unknown required fields and oversized batches before any dispatch.

InvokeContact accepts only host-supplied real solver events with context, binding/component, canonical other object/collider identities, points/normals and relative velocity. It remains an event adapter into the same dispatcher; test drivers cannot call it to establish gameplay success. Observe performs typed public observation only and supports pure null-buffer sizing; no private gameplay field access is exposed. GetLastError copies bounded structured callback/transport diagnostics and follows the same pure sizing rule. Return codes share the native status integers. Per-callback exceptions record type/message/stack/context while other eligible callbacks continue; a safely consumed batch can return transport success with callback failures in observations.

Unload releases roots, wrappers, delegates and enumerators and is idempotent. Host then destroys its session once. No native lock or mutable world borrow is held across DispatchPhase/InvokeContact. Facade transact/query reentry uses the same session/phaseToken and immediate read-after-write visibility. Recursive Advance and unsupported threads fail without mutation. Synchronous instantiate/activation can dispatch nested initialization only after releasing borrows, bounded to 32 levels by default. Rust owns eligibility, clock advancement, event order and coroutine resume readiness. C# only binds/invokes messages and advances selected enumerators. Implement JSON frame context now; binary FrameContext/CallbackRecord layouts can be a later compatible optimization, with independent layout tests if introduced.

### Shared view and frame API

Extend `worldrunner-camera` with `CameraState { pose, lens, viewport, convention }` and `resolve(&CameraState) -> Result<ResolvedView, ViewError>`. Pose is eye plus explicit orientation/basis in canonical metres; lens is Perspective(vertical_fov, near, far) or Orthographic(half_height, near, far). ResolvedView contains view, projection, inverse view-projection, view-projection, eye, viewport and reconstruction projection/convention. Matrices use glam column-vector semantics and explicitly declared 0..1 depth. SceneView::new and FreeFly/RTS/Orbit APIs remain compatible adapters to the same resolver. Import conversion happens once. Public screen queries and render passes take ResolvedView.

`RenderSnapshot { session, frame, fixedTick, revision, entities, resources, view }` is immutable extraction from the session, consumed by existing render-plan/render-scene resources. Sprite/UI updates extend canonical contributions and resident buffers; they cannot manufacture an unrelated RenderScene with copied live game state. A shared render-stage extension takes borrowed device/queue/encoder/targets and the same ResolvedView, with explicit capabilities/load-store/output ownership. Caller-authored Stranded effects remain possible through this seam.

`CaptureReceipt { session, snapshotRevision, requestedFrame, submittedFrame, presentedFrame?, fixedTick, viewport, colorSpace, orientation, artifact }` distinguishes offscreen completion from actual presentation. Surface texture copying and OS-window capture establish separate observations.

### Harness and dashboard API

RunControl/1 serializes commands for one session: `Prepare(RunSpec) -> Admission`, `Load(scene) -> SessionReady`, `QueueInput(events) -> accepted sequence IDs`, `AdvanceTo(point) -> Stamp`, `Observe(public selectors) -> typed values`, `Capture(request) -> CaptureReceipt`, `Reload -> new generation`, `Stop(reason) -> DrainReport`. Commands include requestId, runId and bounded timeout. Failure leaves no half-active scene. OS input and replay feed the same ingress. Game drivers contain data/public control logic, never native engine alternatives.

Proposed CLI: `cargo xtask tck inventory|run|report|serve`; fixture subcommands are `cargo xtask tck fixture check|refresh --fixture <id>`. Run flags include `--fixture`, `--mode`, `--gate`, `--output`. `serve` binds loopback and provides GET `/api/catalog`, GET `/api/runs`, GET `/api/runs/{id}`, POST `/api/runs`, POST `/api/runs/{id}/stop`, POST `/api/fixtures/{id}/refresh`. Mutations require a per-launch token and same-origin validation; serve cannot accept arbitrary shell commands or paths. Each run uses a new directory below .work/runs.

Run JSON uses `real-disunity.tck-run/1`; input/phase/callback/diagnostic streams use bounded NDJSON. Expected/observed typed values include comparator, tolerance, phase/frame/tick and evidence links. Exit codes follow the architecture: 0 selected gate passed, 1 assertion/required-status failure, 2 invalid invocation/spec, 3 infrastructure error, 4 deadline/cancel; combined precedence is 4,3,2,1. Report-only mode is explicit and never grants a playable badge.

## Completion boundary and uncertainty

Selected source-compatible gameplay and declared content translations on Linux X11/Vulkan are the delivery target. Windows/macOS/Wayland, arbitrary engine API coverage, Unity binary/IL2CPP compatibility, Editor functionality and general URP/HDRP execution remain outside this interval. The [training reference](../../docs/training-only-unity6-reference/final-summary.md) retains 57 unresolved uncertainties; local expected behavior is versioned and labelled as training/source-derived. No Unity oracle is required or authorized.

Missing art, uncertain legacy field mappings, solver behavior, native ABI duplication and active rendering migration are concrete early risks. They receive tests and explicit dispositions in [execution.md](execution.md). A deadline miss retains incomplete statuses. Neither a reduced game nor a weakened threshold can satisfy this specification.
