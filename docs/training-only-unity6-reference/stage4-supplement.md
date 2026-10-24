# Stage 4 supplemental contracts (not inventory topics)

**Training-derived and unverified.** This companion records selected J3 additions without changing the 18-group/108-topic inventory or its IDs. Names and semantics below are recollections, not a Unity 6.3 or package-version guarantee. Proposed examples/tests are not executed. Package-dependent details are explicitly bounded; the 57 U entries remain unresolved.

## J3-005 — Baseline physics pair rules

These simplified matrices assume active/enabled colliders, simulation enabled, compatible layers, no ignored pair, and ordinary body/collider setup. Contact existence, solver response, and callback delivery are distinct.

**2D non-trigger contacts, recalled default:** Dynamic–Dynamic, Dynamic–Kinematic, Dynamic–Static pairs interact; Kinematic–Kinematic and Kinematic–Static non-trigger contacts are ordinarily excluded unless the relevant `Rigidbody2D.useFullKinematicContacts` setting enables full kinematic contacts; Static–Static is excluded. Trigger overlap/event behavior is not this response matrix: triggers have separate participation rules, and a static trigger against another static collider is not promised an event merely because one has a `Rigidbody2D` whose body type is Static. Verify special trigger/body-type cases on the selected version (U019/U021).

**3D ordinary collision callbacks, recalled baseline:** at least one participant must have a non-kinematic `Rigidbody`; two bodyless static colliders, kinematic–static, and kinematic–kinematic do not follow the ordinary dynamic collision callback path. For ordinary trigger events, at least one participant needs a `Rigidbody`, which may be kinematic; two bodyless static colliders are insufficient. CharacterController, articulations, contact/detection overrides, and callback timing on disable/removal are outside these tables (U024).

Proposed positive/negative pair tests (not run) should separately record callbacks, contacts/overlaps, and physical motion; do not infer one from another.

## J3-006 — Input System binding override persistence

For a package `InputActionAsset`, recalled helpers include `SaveBindingOverridesAsJson()` and `LoadBindingOverridesFromJson(string json)`; `RemoveAllBindingOverrides()` and `RemoveBindingOverride(...)` are remembered reset families. Treat the JSON as application-owned persisted data, not a replacement action asset. Overrides depend on corresponding action/binding identities; stale or malformed data needs a handled failure path. Interactive rebinding should dispose its operation on completion/cancel and restore the intended maps/actions after the operation. Exact optional overloads, identity migration behavior, and package version remain U027.

Proposed round trip (not run): apply an override, save JSON, load it into an equivalent asset, verify effective binding; separately test reset and changed-asset/malformed-data handling.

## J3-007–009 — Optional rendering families

### Lighting/environment (J3-007)

A `Light` exposes recalled common configuration such as `type`, `color`, `intensity`, `range`, `spotAngle`, and `cullingMask`; realtime shadow settings and shadow quality depend on render pipeline, quality, and platform. `RenderSettings` includes environment concepts such as ambient lighting, `skybox`, and fog. Baked lightmaps/probes represent authored/baked inputs and are not the same as realtime light evaluation. URP/HDRP additional light data and supported light counts are package-specific. Do not treat a built-in pipeline shader example as pipeline-neutral.

### Particle systems (J3-008)

`ParticleSystem` has playback/control families including `Play`, `Pause`, `Stop`, `Clear`, `Emit`, and `IsAlive`; `Stop` behavior/options matter: stopping emission can leave live particles to drain, unlike clearing immediately. `main`, `emission`, `shape`, and `GetComponent<ParticleSystemRenderer>()` expose configuration families. Modules such as `main`/`emission` are engine-backed module views, not independent systems. Main settings include lifetime/simulation space and curves; emission supports rates/bursts; shape controls spawn geometry. Renderer/culling bounds affect visibility. MinMaxCurve/MinMaxGradient support richer values than one scalar/color. Exact overloads and stop defaults need package/editor verification; VFX Graph is distinct.

### Lines and trails (J3-009)

`LineRenderer` uses `positionCount`, `SetPosition`/`SetPositions`, `useWorldSpace`, width/color curves and gradients, `loop`, and a material/rendering path. `TrailRenderer` generates motion history with `time`, `minVertexDistance`, and `emitting`; `Clear()` removes history. Turning `emitting` off does not itself clear existing trail points. Neither renderer inherently creates collision geometry. Pooling a trail should clear stale history before reuse. Shader/pipeline behavior remains selected-pipeline-specific.

## J3-010 — Navigation scope

Pathfinding/navigation is not supplied by physics casts or `CharacterController`. The remembered runtime family is `UnityEngine.AI.NavMeshAgent`, `NavMeshPath`, and `NavMesh` query APIs; `NavMeshObstacle` is another runtime component. Agent workflows distinguish requesting `SetDestination(...)` from completion: inspect `pathPending`, `pathStatus`, `remainingDistance`, and `stoppingDistance`, and check whether the agent is on a NavMesh before assuming a usable path. Transform/root-motion ownership must be singular: do not let both agent movement and an animation/root-motion driver author the same position without an explicit integration policy. NavMeshSurface authoring/baking is associated with the separately versioned AI Navigation package in remembered workflows; exact package and signatures are unverified. This is optional pathfinding coverage, not a claim of baking compatibility.

## J3-011 — Pooling

The remembered `UnityEngine.Pool` surface includes `IObjectPool<T>`, `ObjectPool<T>`, and collection-pool helpers, with acquire/release/clear/dispose and create/get/release/destroy callback roles. Exact generic constraints, callback signatures, and availability must be checked at the selected engine version. Pool `Release` is not `Object.Destroy`; reuse does not rerun `Start`, though deactivation/reactivation ordinarily invokes `OnDisable`/`OnEnable`. A pooled component must reset its own application state. These pools are not thread-safe. Retained capacity limits, active-demand limits, and collection checking are different concerns; a retained cap does not bound simultaneous active objects. Pooling is optional optimization, not a correctness prerequisite.

## J3-012 — Profiling capability

`Unity.Profiling.ProfilerMarker` supports scoped instrumentation (including `Auto()`-style usage in remembered API); `ProfilerRecorder` captures selected recorder data and must be disposed. Counter availability, units, recorder APIs and editor/player behavior depend on version/platform. Separate CPU time, GPU time, allocations, retained native resources, and frame pacing; they are not interchangeable. Deep Profiling has overhead and can perturb measurements. `GC.Alloc` is a diagnostic signal, not proof of the bottleneck. Compare warmed representative player runs with declared platform/backend and units; no performance result is claimed here. Memory Profiler is an optional, separately versioned tool.

## J3-013–014 — Video and runtime IMGUI boundaries

Video may remain out of the minimal subset. If selected, remembered `UnityEngine.Video.VideoPlayer` workflows use clip or URL source, `Prepare()`/`prepareCompleted`, `Play`/`Pause`/`Stop`, target `RenderTexture`, `errorReceived`, and optional audio routing. Preparation is asynchronous; prepared does not mean playback started, and a texture target does not implement decoding. Codec/platform support is not established.

Runtime legacy IMGUI is distinct from editor IMGUI: `MonoBehaviour.OnGUI`, `GUI`/`GUILayout`, `GUIStyle`/`GUISkin`, and `Event.current` belong to an immediate-mode player UI path. OnGUI can run multiple times per rendered frame for Layout/Repaint/input events. These are not retained uGUI/VisualElement widgets. Screen GUI coordinates have a top-left convention in familiar IMGUI use, unlike legacy `Input.mousePosition`'s bottom-left convention. `UnityEditor.EditorGUI` is not player-available. Exact behavior remains unverified; runtime IMGUI can be explicitly excluded.

## J3-015 — Animator state/blend scope

Controller transitions may have conditions and exit-time behavior; normalized exit time is not transition duration. Any State transitions and direct `Animator.Play` are different mechanisms. Triggers are transition-oriented parameters and should not be modeled as persistent bools; exact consumption/transition ordering is controller-evaluation behavior (U037/U038). A 1D blend tree selects contributions by thresholds; 2D blend trees use parameter-space algorithms and are not equivalent to two independent 1D blends. During transitions, outgoing/incoming states can both contribute. Interruption and transition priority depend on configured controller rules and must not be replaced with one universal ordering. Exact editor controller graph behavior remains unsupported.

## J3-016 — Menu focus workflows

For uGUI, a non-pointer path uses an active/interactable `Selectable` as EventSystem selection, navigation configured through automatic or explicit links, and an input module delivering Move/Submit/Cancel. After closing a panel or disabling the selected object, choose and restore a valid selection deliberately. Package Input System uses `InputSystemUIInputModule`; do not assume it is the legacy module.

UI Toolkit has its own focusable elements, `Focus()`, tab/focus order and panel-routed focus/navigation events; this is not `EventSystem.currentSelectedGameObject`. A sensible proposed workflow (not run) is open menu, navigate, activate, close, then restore prior/appropriate focus. Font fallback/localization, contrast/scale and accessibility integration are product choices; neither UI stack alone guarantees full screen-reader integration. Exact UI Toolkit navigation APIs are version-sensitive (U036).

## J3-017 — Desktop delivery checklist

A meaningful player delivery contract must declare OS/architecture, scripting backend, API compatibility level, render pipeline, scenes/assets, packages and player settings. Reflection and dynamic access can defeat linker reachability; `UnityEngine.Scripting.PreserveAttribute` and `link.xml` are remembered preservation mechanisms, but exact linker behavior needs backend/version verification. AOT backends can expose missing generic instantiations that editor/Mono execution did not. Managed plugins and native `[DllImport]` plugins are different: native binaries/import settings must match target platform/architecture and ABI. Assembly definitions, generated code/package processing and script execution order are not runtime namespaces or substitutes for plugin loading contracts.

A proposed player smoke path (not run) checks build report result, startup, scene change, packaged content, and persistent storage on each declared target. Successful compilation or an Editor play session does not establish AOT, stripping, native plugin, SDK, signing/notarization, installer, or deployment support.

## J3-018–020 — Clarifications

Quaternion orientation equivalence does not imply equal components or equal operator/`Equals` results. Recalled operator equality uses a near-positive-unit dot criterion, so do not assume `q` and `-q` compare equal; `Quaternion.Angle` with application tolerance is a better orientation test. Component equality/hash behavior and exact tolerances remain U010/U011.

For recalled Addressables `InstantiateAsync(..., trackHandle: false)`, retain the returned operation handle and use the matching handle-based `ReleaseInstance` path. `ReleaseInstance(GameObject)` relies on instance tracking and cannot be assumed to find an untracked instance; destroying the GameObject alone does not release its Addressables operation. Default tracked instances may use the matching GameObject release path. This differs from manually instantiating a prefab retained through `LoadAssetAsync`. Exact package behavior remains U045.

`MaterialPropertyBlock` supplies supported per-renderer/per-material shader property values, not a replacement for a Material's shader, render queue, keywords, or arbitrary blend/depth/cull state. A value only affects appearance if the shader reads it. Property blocks are not universal batching optimizations; SRP Batcher/instancing effects require the selected pipeline contract (U029/U030).
