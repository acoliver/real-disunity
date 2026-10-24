# Unity-shaped compatibility kit: engine integration and test plan

**Real Disunity · design revision 1 · 20 October 2026**

Engine developers need to know which public contracts work, what prevents a game from running, and whether an engine change breaks an established game journey. This kit will compile unchanged C# game scripts against independently implemented UnityEngine-shaped assemblies, execute them through the Rust engine, and retain assertions, logs and captured frames for each selected contract.

The implementation sequence is Snake, Breakout, historical Pong, then rd-stranded host convergence. Each game adds a bounded set of engine and content requirements. The [case catalog](unity-compatibility-kit-cases.md) defines the initial 54 cases, and the [Snake fixture specimen](unity-compatibility-kit-fixture.example.json) shows the input, replay and evidence contract. The [PDF](unity-compatibility-kit-architecture.pdf) and [HTML](unity-compatibility-kit-architecture.html) combine these sources with diagrams and a dashboard layout.

## 1. Developer workflow and delivery state

A developer selects a case or fixture, its source and package profiles, an execution mode and a gate policy. Preflight validates the input lock and required capabilities. A worker loads the scene, advances the engine timeline, supplies input and collects public observations. The result records expected and observed values alongside artifacts and teardown status. The dashboard then exposes the dependency that blocked execution or the assertion that failed.

For example, Snake's original-content fixture currently stops at its missing sprite prerequisite. An explicit substitute-content variant could later exercise steering and growth while retaining its own asset digest and fidelity label. Both results would preserve the original script bytes. The original-content result remains blocked until its asset closure is complete.

The first usable release must compile public declaration probes and unchanged Snake scripts in independent assemblies; execute object, transform, lifecycle, clock and input assertions against native state; import a declared content subset; and complete a finite desktop journey with control, gameplay action, reload and renewed control. It must also retain partial results and distinguish assertion failures from infrastructure failures. Adding a game should require fixture data, profiles and, where necessary, an external driver over the shared test protocol.

### Current implementation and gaps

This delivery consists of design sources and generated presentation files. The executable kit, UnityEngine-shaped assemblies, native scene importer and game drivers remain planned. The repository already contains a CoreCLR host, a native managed-object store, resource admission, camera controllers and a desktop scene renderer. The following local sources define the starting point.

| Capability | Existing implementation | Integration work |
|---|---|---|
| Rust application API | [worldrunner facade](../crates/worldrunner/src/lib.rs) exposes feature-gated gfx, scene, world, script and asset modules. | Add general host/runtime services through this facade so games can use the supported application API. |
| Content admission | [world-model](../crates/world-model/src/lib.rs) holds semantic documents; [world-runtime](../crates/world-runtime/src/lib.rs) provides registry/planner, prepared contributions and verified publication. | Preserve exact-schema, capability and content-address checks; materialize admitted contributions into live simulation state. |
| Asset resources | [asset-id](../crates/asset-id/src/lib.rs), [catalog](../crates/catalog/src/lib.rs) and [package](../crates/package/src/lib.rs) provide identities, dependency/variant resolution and bounded packages. [Texture](../crates/texture/src/lib.rs), [mesh](../crates/mesh/src/lib.rs) and [material](../crates/material/src/lib.rs) define artifacts. | Map imported GUID/fileID/subasset references to these canonical identities and artifacts. |
| Native object state | [disunity-ffi](../crates/disunity-ffi/src/lib.rs) owns generation-checked world/object slots, position, Y yaw, material slot and queued commands under STORE. | Extract/generalize this state into the shared simulation service and connect it to admitted content and rendering. |
| Managed execution | [DisunityHost](../crates/disunity-runtime/src/lib.rs) and [ManagedBridge](../crates/disunity-runtime/src/bridge.rs) boot CoreCLR, select a bridge, export objects and unload the slice. | Add live phase/event dispatch and script binding. |
| Managed API | [WorldRunner](../csharp/Disunity/WorldRunner.cs), [GameObject](../csharp/Disunity/GameObject.cs), [MonoBehaviour](../csharp/Disunity/MonoBehaviour.cs) and [Time](../csharp/Disunity/Time.cs) provide the current Disunity namespace. | Preserve existing consumers and add independently defined UnityEngine-shaped contracts. |
| Cameras | [camera](../crates/camera/src/lib.rs) supplies perspective FreeFly/RTS controllers; [SceneView](../crates/render-scene/src/view.rs) uses a separate governed-scene camera representation. | Introduce a shared resolved-view boundary, arbitrary orientation and an orthographic lens. |
| Rendering and desktop | [RenderScene](../crates/render-plan/src/lib.rs), [SceneGpu](../crates/render-scene/src/draw.rs), [ScenePipelines](../crates/render-scene/src/renderer.rs) and [OffscreenTarget](../crates/render-scene/src/offscreen.rs) supply rendering/readback. The [viewer app](../crates/scene-viewer/src/app.rs) supplies a winit/wgpu surface and resident-scene interaction. | Extract reusable host services; add sorted alpha sprites, UI and gameplay input with presentation-correlated capture. |
| Observation conventions | [ScriptHost](../crates/script/src/host.rs) and [WorldView](../crates/script/src/world.rs) provide sandboxed Lua hooks, records, seed/budget controls and read-only handles. | Reuse lifetime/observation conventions when designing managed services. |
| Verification | [ABI layouts](../crates/disunity-ffi/tests/layout.rs), [store tests](../crates/disunity-ffi/tests/store.rs), [managed integration](../crates/disunity-runtime/tests/managed.rs) and [quality gates](quality.md) cover existing facilities. | Extend these checks with public managed contracts, importer integration and desktop journeys. |

General gameplay input, simulation time, entity/component attachment and Physics2D integration still require implementation. The current native step drains commands, and the managed runner supplies its own narrow callback sequence. Camera representations differ in up-axis and projection support. The current renderer's opaque/masked object pipeline needs alpha ordering and sprite/UI contributions.

Initial scope covers selected source contracts and content schemas on admitted platforms. Full Editor tooling, a Unity package implementation distribution, native plugin emulation, full URP/HDRP, exact shader/solver equivalence and arbitrary precompiled Unity binaries are outside that scope. Physics3D and broader guide topics are expansion work. Desktop profiles beyond the first Linux X11/Vulkan target require their own host/driver integration.

### Expected behavior and the reference guide

Agents wrote the [training-only reference](training-only-unity6-reference/index.md) from existing knowledge; reviewers corrected it, and pinned third-party games were inspected to compare practical usage with the guide. That creation, review and comparison process used no official Unity documentation or source. Its [final summary](training-only-unity6-reference/final-summary.md) records 18 groups, 108 topics, 3 errors, 25 omissions, 11 clarifications and 57 remaining questions.

The [game comparison](training-only-unity6-reference/game-comparison.md) records 95 gameplay/support requirement instances: 25 Snake, 43 Breakout and 27 Pong. Five Pong test-tooling requirements form a separate population. The guide-treatment labels are 57 covered, 24 partial, 6 absent and 8 ambiguous. These counts describe written traceability. Execution results use the catalog's separate population of 50 compatibility-facing and 4 harness engineering definitions.

Guide propositions become versioned **expected** records; engine runs supply **observed** records. Signature, phase and format decisions must be settled before their cases enter the required CI baseline. Pending contracts produce inconclusive attempts. The report keeps requirement, case and assertion counts separate and defines no global API-coverage percentage.

## 2. Engine integration and state ownership

Unchanged scripts must observe the same transforms and bodies that native simulation and rendering use. The design therefore assigns live simulation state and scheduling to one Rust EngineSession. C# facade objects expose public names and profile semantics over that state. Rust clients, managed scripts and test drivers all reach the same session.

```text
Supervisor: fixture + profiles + case selection + gate policy
  -> worker HostSession / RunControl
     -> normalized OS/replay input
     -> EngineSession: entities, components, hierarchy, clock, physics
        -> managed phase/event dispatch -> GameScripts.dll
        <- facade commands/queries through native ABI
     -> immutable render snapshot + resolved camera
     -> RenderScene -> SceneGpu / shared pipelines -> surface or offscreen target
     -> public observations, correlated captures, logs and artifact index

Content: source + explicit sidecar -> versioned importer
         -> existing resource admission -> EngineSession entities/resources
```

These are planned boundaries; package names will be selected during implementation. EngineSession owns live components, resources and advancement. The ABI gateway validates handles, thread entry and buffers. The managed bridge owns assemblies, script instances and cached message binding. Facades own declarations, marshalling and source-coordinate/profile adaptation. Importers own serialized schema and reference translation. HostSession owns window events, input ingress, presentation and capture. The worker supplies assertions and finite plans; the supervisor owns isolation, budgets, aggregation and gate evaluation.

The first structural change is to move the existing FFI store into a CPU simulation service shared by native and managed consumers. Preserve its valid generation/command behavior while migrating the data into that owner. Immutable semantic documents remain admission inputs. Their prepared contributions create live components in EngineSession. A table/component registry is sufficient for the first increment; storage-library selection can follow demonstrated needs.

The [layer registry](../crates/xtask/src/layers.rs) currently classifies FFI as Foundation and the managed host as Managed, with dependencies restricted accordingly. A generic CPU owner can sit in Foundation; a World-layer materializer can depend on semantic admission. Any required policy change needs an explicit registry update and dependency tests. GPU work stays above the ABI layer, and engine modules remain independent of game and test-driver packages.

### Components, resources and visibility

The initial component set is transform hierarchy/TRS, object metadata/activity, managed-script binding, camera/lens, sprite renderer, Rigidbody2D/collider shapes and selected UI layout/text. Resources include the clock/accumulator, normalized device stream, phase input snapshots, seeded randomness, tag/layer registry, asset handles, deferred destruction and physics events.

A facade wrapper is a session-scoped view of a native component. Immediate setter/getter behavior and callback observations require a coherent read model. A native phase command overlay can provide read-after-write visibility and merge at commit; physics updates then refresh that same model. Rendering extracts immutable phase-stamped snapshots. Each operation's visibility and failure behavior belongs in its contract, including whether earlier commands remain applied after a later error.

Source aliases share engine semantics. Historical `Rigidbody2D.velocity` and modern `linearVelocity` access the same native velocity component. Legacy and package input views share device ingress while retaining their selected edge, axis and update-mode policies.

Physics2D needs one native solver behind the body/shape/query service. Selection must evaluate kinematic MovePosition, sleep/wake, elastic contacts, CCD, filters, shape updates and replay on admitted platforms. Solver caches require an explicit synchronization/ownership policy with canonical components. Record the solver revision and tolerances in profiles. Physics3D remains a separately admitted capability within the same world and schedule.

### Shared camera and rendering path

Camera projection must agree across public screen conversions, culling, rendering and screenshots. Extend [camera](../crates/camera/src/lib.rs) with pose, lens and resolved-view contracts covering perspective/orthographic projection, viewport and clip/depth conventions. FreeFly, RTS and Orbit controllers update that contract. [SceneView](../crates/render-scene/src/view.rs) consumes the resolved view while retaining its governed-scene constructor for existing callers. Explicit orientation/up basis supports valid planar views.

A Unity-shaped Camera wraps the canonical camera component. `orthographicSize` means half the vertical span; Snake's [fixture config](unity-compatibility-kit-fixture.example.json) records 11. Test landmark screen positions, inverse conversions and clipping through the actual shared projection.

The existing governed scene uses Z-up, authored integral millimetres and metre rendering, while the camera controller API uses Y-up conventions. Unity-facing content enters through one declared adapter mapping. A proper rotation such as `(x,y,z) -> (x,-z,y)` converts the up-axis; the profile separately resolves forward direction, handedness and winding. Store the mapping and inverse, and apply them consistently to transforms, normals, gravity, rays and the declared 2D physics plane. Screen origin/depth adaptation belongs at presentation. Landmark and round-trip tests cover the complete mapping.

Sprite extraction adds rect/UV, pivot, pixels-per-unit, tint, sorting and alpha blending to canonical render contributions. UI text uses the same renderer and presentation path. Keep existing texture/material resources, GPU context, pipelines and readback limits. Explicit material translations carry a fidelity label, so an unlit profile and a selected URP2D profile retain separate results.

## 3. Managed compilation and phase dispatch

The bridge must support scripts compiled in an external assembly and execute their messages at engine-owned phase boundaries. The current host already supplies the CoreCLR loading mechanism. The work is to extend its contract and lifetime handling while keeping existing Disunity consumers compatible.

### Existing host and ABI

[DisunityHost::boot](../crates/disunity-runtime/src/lib.rs) checks the selected FFI image, assembly, runtimeconfig and bridge type, claims a single active host and loads the cdylib through libloading. It avoids linking the FFI rlib so the process uses the selected native store. [ManagedBridge](../crates/disunity-runtime/src/bridge.rs) calls nethost load_hostfxr, initialize_for_runtime_config and the assembly delegate loader. A OnceLock retains hostfxr for CoreCLR's process lifetime.

The native-to-managed entries are ConfigureNativeLibrary, Run, WorldHandle, Unload, TokenAlloc, TokenFree and TokenLive. Rust currently binds extern system callbacks; the fixture uses default unmanaged calling convention. ConfigureNativeLibrary copies the selected NUL-terminated UTF-8 path, and the [C# resolver](../csharp/Disunity/NativeLibrary.cs) binds imports from the Disunity assembly to that exact image. Its path is fixed in-process.

Managed-to-native calls use generated Cdecl DllImport bindings with exact spelling. The [ABI manifest](../crates/disunity-ffi/disunity_interface.json) defines 12 exports, and the [layout tests](../crates/disunity-ffi/tests/layout.rs) fix the current 64-bit result/command/state sizes at 16/24/32 bytes. Appendix A records the layouts, queue limits and current hardening gaps.

[WorldRunner.Step](../csharp/Disunity/WorldRunner.cs) submits pending commands, calls native step with frame argument 1, updates managed time, then invokes Start/Update/LateUpdate. Native step ignores dt/frame and drains the queue. Callback writes wait for a later step; Transform getters use managed cache. AddComponent(T) creates an object per behavior. The current base requires virtual lifecycle methods and a private-protected constructor. External Unity-shaped scripts therefore need new component attachment and message-binding contracts.

### Independent facade assemblies and unchanged source

Keep Disunity.Engine available for its consumers. Build selected CoreModule, physics, input and UI-shaped assemblies with exact identities recorded in each source profile. If a legacy UnityEngine aggregator is needed, type forwarding must retain one defining identity for each public type. Inventory probes pin actual declarations and assembly identities.

MSBuild compiles original files through Compile Include from a read-only root or byte-identical stage into GameScripts.dll. The lock records compiler SDK, language version, net8.0 TFM, symbols, references, asmdef graph and output hashes. The planner resolves platform rules, version defines and cycles, and separates Editor/test assemblies from player references. Historical source uses .NET 8/CoreCLR with a historical API profile.

The new MonoBehaviour base permits derivation from an independent assembly. Binding scans allowed game assemblies once, resolves exact public/private instance message signatures and inheritance policy, and caches invocation metadata/delegates. Snake's private Start/Update are examples. Required compile-time attributes include Header, Range, SerializeField and CreateAssetMenu. Editor menu execution remains outside the initial runtime scope.

Use an allowlisted offline dependency graph and independently implemented package-facing declarations. Package manifests select requirements. Snake's installed Input System/2D/test tooling is metadata when selected gameplay uses legacy input. Pong's NUnit, TestTools, Editor and NSubstitute dependencies belong to a separate test profile with explicit graph exclusions from gameplay compilation.

### Bidirectional phase contract

Append a handshake covering requested/supported capabilities, native ABI revision, bridge protocol revision and struct sizes. Reject incompatible layouts before loading scripts. Existing v1 exports continue forwarding into canonical state. Host-created session attachment must bind C# to that same session.

Proposed bridge families are DescribeBridge, LoadScripts, DispatchPhase, DeliverEvents, CollectObservations and UnloadScripts. They use paired explicit Cdecl conventions on Rust and UnmanagedCallersOnly C# entries. A version/byte-size-tagged FrameContext contains fixed-width session/frame/fixed-tick IDs, phase, rational simulation time, projected dynamic/fixed/scaled/unscaled deltas and flags. Callback records contain binding handle/generation, message kind and bounded payload offsets/lengths. Freeze offsets/alignment with a C#/Rust round-trip test before implementing providers.

The call flow releases native locks before entering managed code, because script callbacks may immediately P/Invoke back into the session:

```text
Managed write:
GameScripts -> Transform.position setter -> source-space marshalling
 -> native command/query ABI -> EngineSession Transform
 -> coherent read at the operation's declared visibility point

Engine phase:
HostSession -> EngineSession.begin_phase -> build callback/event batch
 -> release mutable world borrow and store lock
 -> ManagedBridge.DispatchPhase(session, frameContext, callbacks)
 -> scripts query/submit through the native ABI
 -> collect structured callback outcomes; end managed invocation scope
 -> commit allowed commands and continue the native schedule
```

Dispatch comes from the native schedule. Synchronous creation/activation can nest approved initialization messages through a reentrancy-safe boundary after locks are released. Recursive Advance is rejected deterministically.

Native owns input batch memory for the call; managed code copies retained data. Commands, observations and diagnostics use bounded caller buffers. CLR object pointers, Rust references, MethodInfo and managed exceptions stay on their respective sides of the boundary. New UTF-8 strings use pointer+byte-length. ABI, bridge and source-profile revisions remain independently versioned.

Guard load/dispatch/unload entries and catch individual callback exceptions. Record managed type/message/stack and phase, then apply an explicit policy for unrelated eligible callbacks. A case can expect that diagnostic or error when progress becomes impossible. Contain native panics at C entry where unwinding is available; the supervisor records abort/crash as infrastructure error. Terminate only the worker's owned process group on deadline.

### Identity, lifetime and thread contract

Identity must survive slot reuse, unload and multiple worlds. Wrapper keys contain session/world generation, object slot/generation, component slot/generation and canonical type ID. Type mapping uses assembly identity, full name and schema/profile revision; importer metadata maps script GUID identity. Retire slots on generation overflow and invalidate all affected wrappers on destroy/unload.

Selected Object equality/bool operators expose destroyed liveness while the CLR reference remains. ReferenceEquals, is null and null-conditional operators retain CLR behavior. Deferred Destroy follows the chosen lifecycle boundary. Existing Disunity IDisposable behavior can remain compatible alongside that facade policy.

EngineSession owns native components/assets. Script bindings root managed instances until destroy/unload and release root tokens during teardown. Finalizers cannot call main-thread APIs. Disposal is idempotent. A fresh process resets static game fields and CoreCLR state; collectible AssemblyLoadContext support is later work with shared facades retained in the default context.

Bind sessions to the engine thread and reject unsupported-thread object queries/mutations with structured errors. Render threads consume immutable snapshots, and asset workers return prepared data for main-thread commit. Pure math and driver computation can run independently. Retained delegates need explicit rooting/unregister lifetimes; caller buffers expire when a call returns.

## 4. Test execution and evidence

A declaration probe answers whether selected source forms compile. A runtime case answers whether public behavior matches its expectation. A content case adds import/reference fidelity, and a desktop journey adds actual control and visible gameplay. Keeping these layers separate lets a developer locate the next missing capability.

| Layer | Entry point and evidence | Established contract |
|---|---|---|
| Inventory | Compile public snippets and inspect independent assembly metadata. | Selected signatures, members and assembly identity. |
| Semantic | Public managed calls and callback records over the engine timeline. | Bounded native-backed behavior. |
| Content | Import pinned scenes/assets; inspect public references and rendered output. | Selected schema, prefab and resource fidelity. |
| Desktop | Shared host/driver, OS input, presentation, public state and reset. | Named game journey on a recorded desktop/backend. |

Use real deterministic collaborators for domain and subsystem tests. Public managed probes may read selected public members through a test-owned reflection adapter. Pass criteria use supported observations, actual contacts and rendered output. Private dumps remain failure diagnostics, and drivers cannot invoke game collision handlers or mutate private gameplay fields. Static dependency/layout checks are recorded as engineering checks.

Each case has a stable caseId and separately versioned contractRevision, priority, layer, subsystem, public entry points, inputs/observations, applicability, prerequisites, seed/timeline, tolerances, traceability and artifact policy. Subordinate assertions also have stable IDs. The [catalog](unity-compatibility-kit-cases.md) is the planning authority until executable contracts can generate it. Past observations retain their original expectation revision.

Open destruction/query order, creation-phase eligibility, alias metadata and solver behavior need bounded decisions before gating. Assert the required partial lifecycle order and leave peer order unconstrained. Tolerances name the quantity and operation; physics cases record solver/backend settings and contact/position windows.

### Runtime schedule

Normal play and tests advance through the same engine schedule. Proposed observation boundaries are:

1. Ingress timestamped device events and platform changes; update the selected legacy/package snapshots.
2. Accumulate scaled time and run zero or more fixed iterations. Ensure per-instance Start eligibility before its first selected update, dispatch FixedUpdate, commit body/shape writes, step physics, publish contacts and dispatch selected messages with locks released.
3. Ensure Start for newly eligible dynamic participants, dispatch Update and dynamic coroutine continuations, then commit coherent writes.
4. Dispatch LateUpdate; apply the selected destruction boundary; extract transforms/render contributions and settle UI layout.
5. Render/present when requested, capture the tagged frame, run selected end-of-frame continuations and publish observations.

These are proposed engine boundaries. Each timing case pins exact sampling and fixed-versus-dynamic first-participation rules before promotion. Activation can synchronously initialize new objects. Managed coroutine enumerators resume from this schedule.

Represent deterministic time as rational values, with integer-nanosecond stamps where needed and recorded float API projections. A 1/60 dynamic step and .02 fixed step then produce five fixed iterations over six frames without accumulating rounded-nanosecond error. A rendered frame may contain zero or several fixed steps. Pause retains dynamic/unscaled progress while scaled fixed work pauses. Clamp/catch-up policy is separate from Snake's own one-tick-per-Update accumulator.

### RunControl and worker lifecycle

RunControl/1 is a proposed engine/test-owned API, usable in-process or through a bounded local pipe. It gives every fixture the same launch, timeline and teardown operations.

| Operation | Result |
|---|---|
| Prepare(runSpec) | Admission, closure, profile and prerequisite report; window creation waits for success. |
| Load(entryScene) | Session identity and scene-ready event, or structured error. |
| QueueInput(events) | Accepted sequence IDs or validation diagnostics. |
| AdvanceTo(point) | Phase/frame/fixed-tick stamps for deterministic progress. |
| Observe(probeIds) | Typed public values, sample phase and units. |
| Capture(request) | Requested/submitted/presented frame IDs and artifact, or error. |
| Reload / Stop(reason) | Bounded reset/teardown and resource-drain report. |

Serialize commands per session. QueueInput rejects past or non-monotonic events unless declared same-timestamp sequencing resolves order. Prepare/Load failure leaves no half-loaded session. Supervisor disconnection cancels the worker, and wall deadlines remain independent of simulation time. Finalize an artifact index after assertion failure; synthesize an error record when a crash leaves an incomplete stream.

Stage hooks such as scene-loaded, after-physics, after-update, render-submitted and before-unload belong to engine/test plumbing. A game-specific driver runs externally or in a test assembly over RunControl and public observations. Original game scripts remain unchanged.

### Input and execution modes

A normalized event carries sequence, run-relative time, device ID, kind, control, value and origin. Initial kinds cover device add/remove, key down/up, pointer motion/button/wheel and focus. Preserve physical key/text distinctions, repeat flags and event order.

OS translation and replay injection meet before derived input updates. Legacy down/up edges remain stable for every poll in a dynamic phase; package controls obey the selected dynamic/fixed/manual mode; axes use imported settings. Short taps between fixed ticks remain observable at the appropriate dynamic phase. Focus loss reconciles held keys through the same service.

Normalized replay and OS delivery have separate checks. The first OS driver targets an isolated Linux X11 display, following the [viewer smoke workflow](quality.md). Windows, macOS and Wayland need admitted drivers. Workers use only an explicitly selected display/test context.

| Mode | Required resources | Evidence |
|---|---|---|
| simulation | Controlled timeline; CoreCLR for C#; CPU only. | Object, lifecycle, input and physics state. |
| visual-offscreen | Same timeline; admitted GPU/software adapter and OffscreenTarget. | Actual geometry/material/pixel output. |
| desktop-replay | Main-thread event loop; isolated/explicit display; advancement gated by render/capture completion. | OS/shared-ingress control, presented frames, captures and gameplay state. |
| interactive | Real clock/input; bounded recorded exploration. | Observed control/action with matching artifacts and a replay plan where feasible. |

Reuse the viewer's surface format selection, Fifo presentation and resize/lost/outdated recovery. Suspend, zero viewport and occlusion cannot count as presented frames. Classify an unmet presentation deadline by cause. Initial desktop CI targets Linux X11/Vulkan with an admitted lavapipe adapter. Record GPU and wall timing per environment.

Capture the render target associated with the snapshot sent to presentation; selected desktop checks also capture the OS window. Metadata includes session/frame/tick, requested/submitted/presented IDs, orientation, linear/sRGB policy, viewport, DPI, cropping, view and adapter/driver. [OffscreenTarget](../crates/render-scene/src/offscreen.rs) currently returns top-to-bottom RGBA8 sRGB with bounded readback and a 30-second map deadline. Add frame/session correlation. Prefer geometry/region predicates and declared image tolerances across drivers.

### Playable journey acceptance

A playable result requires a recognizable presented scene, input-correlated public state and screen changes, a real gameplay action/success, reset or recovery, and continued control afterwards. Retain logs, screenshots, assertions and a linked exploratory recording/note. A window-open event or static image alone provides only that narrower observation.

Journey names define the claim: Snake food/growth and engine reload; Breakout brick hit/loss/respawn; Pong goal/score/reset/relaunch; rd-stranded scene exploration. The next chapter defines the corresponding fixture boundaries.

## 5. Source preservation and game plans

A fixture must identify exactly which scripts, assets and settings produced its result. Keep original inputs read-only and hash every compiler-visible script, metadata/scene/prefab/config/settings file and external dependency used. Recheck the input bytes after execution. Compile original source without source rewriting or IL patches.

Generated build plans, facade references, importer choices, reference fixes, material translations and driver/test assemblies live in overlay/. Each content operation records source object/property, original-value precondition, replacement, rationale and digest. A failed precondition blocks admission. Results identify original-content and unchanged-source/overlaid-content variants separately. Overlays preserve game methods and physics rules; they cannot manufacture supported original art.

The importer walks exact supported text schemas, constructs GUID/fileID/subasset/script identity, validates reachable references and binds fields before activation. Clones remap internal graph references and share external asset references. Diagnose missing edges, duplicate identities, unsupported cycles/structures and mandatory unknown data before partial activation. Preserve stale-field diagnostics until an explicit migration is selected. Converted hand-authored graphs remain useful semantic fixtures with their own format label.

The [comparison inventory](training-only-unity6-reference/game-comparison-inventory.json) retains detailed source/requirement evidence. Pins and entry points below come from that inventory; the Snake [specimen](unity-compatibility-kit-fixture.example.json) additionally records selected byte hashes.

### Snake: first source-to-desktop slice

Fixture B is eguldali--Optimized-Snake-Game at `0ab43c53e834db714439be71c4151d953d241847`, saved with 6000.3.7f1. Its entry is Assets/Scenes/GameScene.unity with eight C# files. The supplied source root is /tmp/real-disunity-modern-games/eguldali--Optimized-Snake-Game.

Its selected closure needs integer math/operators/hash, private lifecycle binding, serialized config/references, component graph cloning, parent-local placement, pooled activity, legacy W/S/A/D edges and orthographic sprites. Config is 40×20, wrap enabled, cellSize 1 and eight ticks/second. This slice can establish the runtime/input/render path before adding native physics or URP.

Both visual prefabs reference sprite GUID `311925a002f4447b3a28927169b83ea6`, fileID `7482667652216324306`, with no defining local Assets metadata. The original-content visual lane is blocked. A separately hashed, independently owned substitute can be an explicit bringup variant. The default specimen leaves that variant disabled. Material remaps also need a declared identity and validated translation.

The source has application limitations: GameLoop consumes at most one tick per Update; its model path omits non-wrap bounds death; FoodSpawner can exhaust attempts and return occupied food; tail/growth occupancy has edge cases; visual/config cell sizes are only aligned in the selected configuration. Preserve these behaviors and use the declared wrap/cellSize settings. Reset uses engine scene reload or a fresh worker because the source has no dedicated restart UI.

The driver observes public FoodSpawner.CurrentFoodPos and active SpriteRenderer positions/enabled state under the admitted SnakeView clone parent. Validate selectors against the imported graph. A bounded public-state driver steers toward food, checks turn/wrap, then observes later growth, food relocation and speed manifestation. Characterize a source-supported self-contact/Game Over scenario separately. After reload, check renewed movement. The specimen's fixed W/A sequence demonstrates steering; food routing is a separate driver task.

### Breakout: physics and package input

Fixture A is admtrv--Breakout at `2b960a3589f30b6e0025f2509ef00bf9155d6f45`, saved with 6000.3.5f2. Assets/Scenes/SampleScene.unity uses seven C# files. The supplied source root is /tmp/real-disunity-modern-games/admtrv--Breakout.

Breakout adds dynamic/kinematic/static 2D bodies, contacts/triggers, CCD, materials, collider resize/disable, coroutine replacement, spritesheet subassets/PPU and live Tilemap wall collision. Its package profile uses Input System 1.17.0 with A/D fixed polling. Quality tiers select a URP17.3 2D closure despite a zero default pipeline setting.

Content admission needs explicit decisions. Ball prefab uses the Ball tag while project tags are empty; a tag sidecar must be recorded. Serialized startSpeed is stale relative to speed, and brick fields also need diagnostics. Any migration belongs to a preconditioned overlaid-content variant. Original URP2D support requires selected camera/light/renderer/material behavior. A named translated-unlit profile can support a separate gameplay result. Missing sprite-pack content and usage authorization remain prerequisites.

The journey moves/clamps the paddle with A/D, observes real ball contacts and brick animation/collider disable/destruction, then records ball loss and respawn. Seeded public-action cases cover power-ups independently. Settle the pending-Destroy query contract before the last-ball/respawn assertion becomes gating.

### Historical Pong: legacy serialization and visible scoring

Fixture C is pong-tdd-licensed-v1 at `e2d3368728e236b0ab5a04afb142a3ae49260dd1`, saved with 2017.1.0f3. Assets/Scenes/Main.unity has 20 gameplay/support C# files; 11 historical tests are a separate population. Source is supplied at /tmp/real-disunity-tck-research/pong-tdd-licensed-v1. The project has no Packages manifest. Seven Resources prefabs support selected test fixtures.

Its separate 2017.1 profile adds legacy aliases/axes, old prefab overrides, persistent enum UnityEvent dispatch, body reset/sleep/elastic response, Canvas/RectTransform/Text and fonts. Goal's serialized team differs from scoresTo, and Goal+ScoreEvent differs from the current top-level ScoreEvent, with asymmetric scene overrides. A reviewed property/type-map sidecar must specify old values and identities. Ambiguous goal ownership blocks that lane. NSubstitute.dll remains an unresolved test-tooling dependency, admitted separately from gameplay.

W/S and arrow Up/Down feed imported Vertical/AlternateVertical raw axes. The journey observes both paddles, native bounce/goal trigger, persistent listener delivery, public ScoreManager/ScoreView values and numeric text pixels. Reset must zero linear/angular velocity, teleport and sleep the body. After the scaled continuation window, the ball shows, wakes and moves again. Sample after the required continuation opportunity and retain renewed control after reset.

### rd-stranded: host convergence and exploration

The previously inspected external checkout at /home/acoliver/projects/worldrunner/branch-2/rd-stranded is outside this workspace. Its games/stranded/Cargo.toml combines engine crates pinned at `0c14471700425aed4012b806f5c48620b02429f3` with renamed local stranded render/host/game/save/world crates. These paths describe migration seams; this design adds no dependency on that checkout.

The recorded games/stranded/src/main.rs creates worldrunner_host::App and supplies profiles, capture commands and native-world paths for a World Explorer. Host app_state.rs pumps winit events. camera_input.rs contains CameraInputState, RtsInputState, CameraBindings and a 1/60 FreeFlyScriptStep replay through real bindings. RenderState owns FreeFly/optional RTS cameras and capture state; camera_matrices.rs and render_state_native.rs contain projection integration to converge with the shared view contract.

The recorded disunity_scene.rs loads a managed slice, exports objects, unloads it, re-grids the snapshot into terrain and builds renderers. lua_shim.rs extracts moon profile data through a selected load-time environment. Those facilities support static content/profile generation. The external Game/GameProfiles composition also carries terrain-specific inputs that should remain at the game edge. These external observations are retained from the earlier inspection and need revalidation against a pinned checkout during migration.

Extract event ingress, timeline, resolved view, presentation/capture and observations into general services. Migrate the current viewer first, then rd-stranded incrementally, preserving its advanced render capabilities through explicit capability migration. The initial journey uses the shared protocol to load an admitted world, move the camera by keyboard/mouse, capture landmarks, reset pose and continue control. Its result is exploration. A full gameplay claim requires a separately inventoried executable content closure.

## 6. Results, dashboard and gate policy

A result must let a developer reproduce an observation and identify whether the engine, facade, importer, fixture or platform prevented progress. The proposed real-disunity.tck-run/1 JSON is authoritative. JUnit projects that record into CI's more limited status model.

Run identity includes engine revision/dirty digest, runner build, catalog/expectation hash, profile tuple, source/overlay/assembly hashes and environment/prerequisite report. Record OS/backend/adapter/driver/display/viewport/DPI, seed and schedule. Each case retains selected contract revision, selection reason, attempts, prerequisite dependencies, ordered assertions and expected/observed typed values with units and tolerances. Artifact entries contain relative path, media type, bytes, SHA-256, origin and session/phase/frame/tick. Final fields include truncation, teardown, gate ID/hash, regressions and exit disposition.

Example record for the blocked original-content Snake fixture:

```json
{
  "schema": "real-disunity.tck-case-result/1",
  "runId": "example-not-executed",
  "caseId": "UCK-GAME-001",
  "contractRevision": 1,
  "profileId": "unity-6000.3-source-v1",
  "status": "blocked",
  "attempt": 1,
  "prerequisites": [{"id": "content.snake.sprite", "available": false}],
  "assertions": [],
  "expected": {"journey": "steer, eat, grow, reload, move"},
  "observed": null,
  "diagnostics": [{"code": "missing.asset_reference", "guid": "311925a002f4447b3a28927169b83ea6"}],
  "artifacts": []
}
```

Assertions record assertionId, observationPoint `{frame,fixedTick,phase,time}`, comparator, expected, observed, tolerance, outcome and diagnostic/artifact references. Encode exceptional numeric values explicitly because JSON numbers exclude NaN/Infinity. Missing probes remain missing. Evidence identifies whether it came from a public API, callback, pixel region, platform event or infrastructure diagnostic.

Use schema-versioned NDJSON for input/phase/callback/log/probe streams with monotonic sequence and run/session/case/attempt identity. Rotate within the run-owned directory and mark truncation. Create-new output and atomic index finalization preserve failed attempts. Incomplete streams retain partial artifacts and receive an error record. Game scripts are executable inputs; workers require process isolation, least privileges and no required network access.

### Status contract

| Status | Meaning |
|---|---|
| pass | All required assertions ran and matched, including evidence and teardown. |
| fail | Completed observations under valid prerequisites contradict an assertion. |
| unsupported | The selected profile deliberately excludes the capability/version/backend. |
| not-implemented | An in-scope provider, case or driver remains unbuilt. |
| blocked | Required external content, configuration, tool or prerequisite is unavailable. |
| skipped | Explicit selection/policy exclusion with a reason. |
| error | Engine, compiler or harness infrastructure failed outside a completed assertion. |
| flaky | Recorded attempts under the same controlled identity have conflicting completed outcomes. |
| inconclusive | Required observations or the approved expectation are insufficient to decide. |

A declaration probe rejected by the compiler is fail if its provider exists; an absent provider is not-implemented; a compiler crash is error. Missing display is blocked at preflight, while an admitted surface/device crash is error. A completion deadline is fail when adequate observations show the goal was missed, inconclusive when required evidence is unavailable, and error when the worker hangs/crashes. A missing-reference microcase can pass by observing its expected diagnostic while the dependent positive game lane remains blocked.

reasonCode, stage, knownIssueId and blameDomain are orthogonal fields. Known failures retain fail with their issue reference. Unmeasured is the dashboard state for an absent result record. Retain every flaky attempt.

JUnit maps fail to failure and error to error; unsupported, not-implemented, blocked, skipped and inconclusive become skipped with original status properties. Flaky maps to failure unless an explicit policy permits it. Gates read JSON so the skipped aggregate cannot conceal a required result.

### CLI and baseline promotion

The CLI below is proposed. Each output directory is new.

```text
cargo xtask tck inventory --profile <profile> --output <new-directory>
cargo xtask tck run --suite micro --profile <profile> --mode simulation --gate harness-smoke-v1 --output <new-directory>
cargo xtask tck run --fixture <manifest> --mode desktop-replay --gate baseline-v1 --output <new-directory>
cargo xtask tck report --runs <run-index> --output <new-directory>
```

Exit 0 means completion with the selected gate satisfied. Exit 1 covers assertion/gating regression, a disallowed mandatory status or required-case disappearance. Exit 2 covers invalid invocation/spec/profile/lock. Exit 3 covers execution/infrastructure error, including missing infrastructure in a required preflight lane even when dependent cases are blocked. Exit 4 covers cancellation/deadline without trustworthy completion. Combined categories use precedence 4, 3, 2, 1 and retain all details in JSON.

An explicitly selected, recorded report-only policy may return 0 while reporting fail/unsupported counts. That disposition reports completion under that policy. Gate definitions record required case/profile/platform selections, allowed statuses, known-issue owner/expiry and allowed deltas from a hashed baseline. Begin with harness self-tests and a small established passing set. Promote stable cases with reviewed expectations and artifacts; reject disappearing selection, new skips/unsupported and unapproved baseline edits.

### Dashboard views

The first dashboard is static HTML over result JSON and relative artifacts. A local run index supplies history. Filters select source, package, layer, subsystem and platform before aggregating; cells show selected/total counts and exact statuses. Every cell links to cases, prerequisite chains and expectation revisions.

Game cards display independent compilation, semantic, content, desktop and exploratory evidence, with source/overlay digests and visible historical/substitution labels. A playable label requires the declared journey's acceptance policy. Case detail places expected/observed values and tolerance beside logs, attempts and actual images. A timeline correlates input, fixed phase, contacts, callbacks and presentation using simulation time, frame/tick and wall time.

The traceability panel keeps the 95 game requirements separate from 54 definitions and assertion totals. Trends compare only matching case revision/profile/platform identities; expectation/source changes appear as discontinuities. Before execution, status cells are unmeasured and pass/fail totals are unavailable. Use keyboard-accessible text alongside color, escape artifact paths/content and keep report artifacts inert.

## 7. Delivery and verification

Start with contracts and state ownership so later game results have reproducible identity, scheduling and evidence. Snake establishes the first source-to-desktop path without a physics dependency. Breakout adds physics/package/content depth; Pong exercises a separate historical profile; rd-stranded migrates existing host capabilities.

| Milestone | Increment | Promotion evidence |
|---|---|---|
| M0: contracts and supervisor | Schema/fixture/result/gate validators, status reducer, quotas, compile probes and static report. | Invalid input/drift rejected; controlled child crash/timeout retains error artifacts; 54 definitions reconcile; absent runs remain unmeasured. |
| M1: native runtime and bridge | Characterize/extract FFI state; add schedule, phase ABI, independent facades, identity, threads, guarded callbacks, clock/input/random and RunControl. | Existing triangle/ABI behavior retained; public object/transform/lifecycle/time/input cases; 100 load/unload cycles drain worlds/tokens; callback reentrancy/lock checks. Add native-body visibility when physics exists. |
| M2: Snake | Field/reference import, clone/activity, integer APIs, resolved orthographic camera, sprite rendering and desktop driver. | Eight unchanged scripts; imported config; turn, food/growth, reload and renewed control with captures/exploratory record. Admit each content variant separately. |
| M3: Breakout | Select/integrate Physics2D; tile/spritesheet, package input, coroutines and selected pipeline capabilities. | Seven unchanged scripts; locked tag/stale-field choices; actual brick/loss/respawn; resize/disable/contact checks; separate fidelity results. |
| M4: Pong | Legacy aliases, serialization/events/axes, UI/text/fonts and body reset/relaunch. | Twenty unchanged gameplay/support scripts; real goal/listener/visible-score/reset/wait/relaunch. Classify eleven historical tests separately. |
| M5: rd-stranded | Migrate host/input/view/capture; preserve advanced rendering and game-edge content/profile adapters. | Shared protocol/launcher/results with exploration input/capture/reset; compare existing behavior on a pinned checkout. |

Implement one public behavior at a time: add a regression test, confirm failure for the intended missing behavior, implement it, then refactor with tests passing. Characterize the current FFI queue/cache/lifetime behavior before extraction. Preserve source-consumer compatibility and register new packages in layer/quality policy. Cross-game driver contract tests should exercise the same prepare/load/input/observe/reload/stop sequence.

### Verification lanes and budgets

The [quality guide](quality.md) defines existing commands. Extend them at the boundary that produces each result.

| Lane | Existing base and extension | Resources and artifacts |
|---|---|---|
| Static/CPU | cargo xtask fmt, policy, source-size, ci-cpu; add schema/lock/profile/gate tests. | Available locked/offline dependencies; diagnostics and status-reducer/catalog checks. |
| Managed/ABI | ci-managed; add public worker cases and round-trip layouts. | .NET 8, hostfxr/nethost, selected native image; metadata, callback traces and drains. Existing workflows declare Linux/Windows/macOS jobs. |
| Simulation | New canonical runtime cases. | Controlled seed/clock and bounded process; state, input/events and teardown. |
| Offscreen GPU | ci-gpu/render tests plus camera/sprite/UI cases. | Admitted adapter/features/driver; PNG, frame metadata, geometry and readback diagnostics. |
| Linux desktop CI | Extend isolated-X11 viewer smoke to game replay. | Xvfb, verified software Vulkan, platform driver; existing smoke also requires Pillow/xdotool. Retain OS/render captures, state/logs and clean exit. |
| Hardware/interactive | New admitted profiles, initially Linux. | Explicit display/test context, actual control and exploratory record; device-specific timing. |

Initial proposed defaults are one session/window and one managed worker per process; two compile jobs; 10 simulated seconds/60 wall seconds per case unless raised; 1 GiB resident limit where enforceable; 10 MiB logs; 64 captures; 256 MiB per readback target; and 512 MiB artifacts. The readback default comes from [OffscreenTarget](../crates/render-scene/src/offscreen.rs); the other values are budget proposals. Game fixtures declare overrides: the Snake specimen uses 60 simulated seconds, 120 wall seconds and 16 captures. Report unenforced resource limits explicitly. Use condition-driven bounded waits and distinguish frame schedules from performance thresholds.

Unit tests cover status/gate/profile/lock/selector/timestamp rules. Integration tests use real FFI/CLR, importer graphs, assets, solver and capture. A small E2E set covers each admitted desktop journey. Disposable children exercise crash, hang, lost prerequisites and truncated streams. Promote only stable, artifact-backed cases into the growing baseline.

The main trade-off is extracting current narrow state/host facilities before broad API expansion. It adds initial integration work and makes later games share corrections. Explicit profiles and fail-closed schemas add metadata maintenance while preserving reproducibility. A local static report is sufficient until concurrent producers require centralized indexing. Extend schemas and policies when additional real fixtures demonstrate a need.

## 8. Report generation and evidence limits

Run the retained [report generator](../tools/generate_unity_compatibility_kit_report.py) from the repository root to rebuild the HTML/PDF. It reads the architecture, catalog and fixture JSON; checks source-text retention, case identity and presentation integrity; and prints through the existing local Chrome/Poppler tools. The generated report includes its exact input digests and regeneration instructions.

Earlier design checks recorded matching pins/clean working trees for the three supplied games and matching byte lengths/hashes for the specimen's 15 source/content records. Those checks are historical evidence. This prose/layout rewrite preserves that data and does not repeat game-tree inspection. Current presentation checks are reported separately at generation time.

This delivery runs documentation and presentation validation only. Engine builds, managed/GPU suites, executable TCK cases and desktop game journeys remain implementation verification work. The status and milestone sections identify the required evidence before any compatibility or playability result can be published.

## Appendix A. ABI layouts and current hardening work

The current [manifest](../crates/disunity-ffi/disunity_interface.json) exports version, world create/destroy, object create/destroy, position/yaw setters, command submit, world step, error message, store statistics and bulk object export. [build.rs](../crates/disunity-ffi/build.rs) compares generated binding bytes with the checked [C# bindings](../csharp/Disunity/NativeBindings.g.cs). Keep existing signatures stable, append new exports, regenerate intentionally and update the manifest/version under its policy.

| Type | Current 64-bit layout |
|---|---|
| DisunityResult | 16 bytes; alignment 8; i32 code at 0, u64 detail at 8. |
| DisunityCommand | 24 bytes; alignment 8; u64 object at 0, u32 kind at 8, f32 x/y/z at 12/16/20. |
| DisunityObjectState | 32 bytes; alignment 8; u64 object at 0, f32 xyz/yaw at 8/12/16/20, u32 material slot at 24. |

[abi.rs](../crates/disunity-ffi/src/abi.rs) uses repr(C); generated C# uses StructLayout.Sequential. New APIs use fixed-width fields and explicit packing tests. Marshal enums with declared widths and encode bool/string values explicitly; managed objects and glam types remain internal.

The [native store](../crates/disunity-ffi/src/lib.rs) copies command pointers synchronously and rejects more than 1 << 20 commands per submission. Bulk export uses caller capacity/probe/retry. Error copying is byte-counted UTF-8 without a terminator and reports required size on BUDGET. The existing managed helper tries 512 bytes; error detail uses a constant last-error token. Add bounded retry and durable diagnostic identity as needed by the new protocol.

World/object handles use generation in the upper 32 bits and slot in the lower 32. Object bits are scoped by the separately supplied world, so equal bits can occur across worlds. Current gaps include component/type handles, explicit affinity, generation-wrap handling and total pending-command bounds. Queued application can partially apply before a later command fails. Mutex serialization currently protects storage access without defining engine-thread semantics.

The [portable bridge](../csharp/PortableFixture/NativeBridge.cs) creates three objects and steps once. Its token is a normal GCHandle root allocated for a managed object. Unload releases worlds/tokens; CoreCLR stays resident, and a collectible game-assembly context is still future work. Configure/Run catch exceptions. Run cleanup can itself throw, and Unload/token entries and native panic containment need uniform guards. These are characterization and hardening tasks for M1.

## Appendix B. Profile selection and diagnostics

A source editor label cannot determine package, importer, solver or platform behavior. The run locks these dimensions independently.

| Dimension | Selected data |
|---|---|
| Engine/API | Repository commit, Rust facade revision, native ABI and bridge protocol. |
| Source API | unity-6000.3-source-v1 for saved 6000.3.5f2/6000.3.7f1; unity-2017.1-source-v1 for Pong. |
| Public modules | Exact assemblies, types, members, overloads, optional arguments, conversions and aliases. |
| Packages | Breakout Input System 1.17.0; Snake installed 1.18.0 with legacy gameplay; Pong legacy UI/events. |
| Import | Scene/prefab/meta revisions, field migrations, texture/sprite settings and stale-field policy. |
| Rendering | Built-in sprite subset, explicit translated-unlit subset, or selected URP17.3 2D capabilities. |
| Simulation | Solver revision/tolerances, collision matrix, .02 fixed step, input mode and axis bindings. |
| Platform | OS/architecture, display/backend/adapter/driver, CPU/GPU mode, DPI and viewport. |
| Build | SDK/language/TFM, symbols, asmdef/package graph, tooling and native plugin requirements. |

Bind cohesive adapters through immutable registries, following the existing [runtime registry](../crates/world-runtime/src/registry.rs). Each declares provided members, dependencies, rejected features and applicable case IDs. Common behavior lives in engine components. A demonstrated semantic version difference gets one named policy with paired tests.

Pong requires velocity, FindGameObjectWithTag and historical inherited find spelling. Modern probes require linearVelocity, FindObjectsByType(None) and FindFirstObjectByType. Pin alias metadata in the profile. Exact Instantiate overloads, two-coordinate Vector3 and conversion/operator declarations remain P0 inventory decisions, as recorded in the [catalog](unity-compatibility-kit-cases.md).

Located diagnostics include unsupported.member, unsupported.pipeline, unsupported.serialized_version, missing.asset_reference, missing.package_contract and plugin.native_unavailable, plus compiler identity/overload errors. Include file/object/member location and dependency chain. Unsupported calls return structured diagnostics and leave the unsupported operation unapplied. Native plugins need an allowlisted ABI/platform contract; otherwise their dependent fixture is blocked while unrelated cases remain selectable.

## Appendix C. Fixture admission contract

The [JSON specimen](unity-compatibility-kit-fixture.example.json) records eight actual Snake script hashes and seven selected content hashes. It is marked designOnly and runnable=false. The script/content records are retained byte-for-byte in this rewrite. Complete input lock, original sprite, compiler/profile locks, selectors and executable drivers remain unresolved.

| Block | Admission responsibility |
|---|---|
| schema/schemaVersion/designOnly/fixtureId/revision | Exact identity and revision; reject a design specimen as a production fixture. |
| source | Local root binding, repository/pin, saved editor, per-file inventory and complete lock state. |
| profiles/compilation | Independent dimensions, package-use classification, compiler/reference/symbol locks and output assembly. |
| entryScene/inputBindings | Selected scene and control-to-action mappings. |
| prerequisites | Required capability/content/platform value, availability, reason and blocking scope. |
| overlay/materialRemaps | Sidecar hashes/preconditions, source/replacement identity and fidelity label. |
| timeline/resources | Finite dynamic/fixed schedule, seed, input times and simulation/wall/resource quotas. |
| tasks/assertions/screenshots | Public selectors, bounded drivers/completion and meaningful capture purposes. |
| artifacts/teardown | New-directory outputs, formats, reload/stop and resource-drain expectations. |

A production validator requires complete closure and resolved required hashes/selectors, supported capabilities/profiles and an implemented driver. Reject unknown schema fields, path traversal, symlink escapes and undeclared executables/plugins. Task parameters are data for a small driver registry. Admission completes before activation or window creation.

The specimen fixes seed 1701, dynamic 1/60 and fixed 1/50 seconds, 1280×720 at DPI 1, and explicit Vulkan adapter admission. It contains separate launch, OS input, steering, food/growth, self-contact, reset and exploration tasks. Captures record initial, turned, grown and post-reload states. Teardown drains managed roots/native worlds, finishes or cancels readbacks, rechecks original bytes and closes only owned windows/processes.
