# Unity compatibility runtime architecture

The compatibility runtime compiles original Snake, Pong and Breakout C# against an independently implemented `UnityCompat` facade and executes those assemblies in CoreCLR. Native Rust owns the world that scripts query and mutate. The player imports scenes, drives execution and renders native snapshots. The process harness evaluates fixture assertions and exposes results through its CLI and dashboard.

The [case list](unity-compatibility-kit-cases.md) points to executable tests and game journeys. [Fixture configuration](../fixtures/README.md) describes the JSON inputs used by the harness.

## Component ownership

Each component has a separate responsibility:

| Component | Responsibility |
| --- | --- |
| [original-games](../crates/original-games/README.md) | Fetches Snake, Breakout and Pong at pinned Git revisions and verifies cached checkouts have the expected HEAD and no local changes. |
| [compat-content](../crates/compat-content/README.md) | Reads Unity text scenes and reachable assets, expands prefabs, applies explicit profiles and sidecars, and produces a graph and native materialization commands. |
| [disunity-ffi](../crates/disunity-ffi/README.md) | Owns generation-checked worlds, objects and components, hierarchy, time, input, 2D physics, lifecycle state and owned protocol responses. |
| [disunity-runtime](../crates/disunity-runtime/src/compatibility.rs) | Loads the native library and CoreCLR bridge, negotiates descriptors, dispatches managed calls and manages world and bridge lifetimes. |
| [UnityCompat](../csharp/UnityCompat) | Defines the selected UnityEngine-shaped types, input, physics, coroutine and UI APIs. Stateful operations reach the attached native world. |
| [CompatibilityBridge](../csharp/CompatibilityBridge) | Loads game assemblies, binds serialized fields and references, dispatches script messages, and reports managed observations and failures. |
| [compat-player](../crates/compat-player/README.md) | Builds the game and bridge, imports and materializes the scene, executes the timeline, renders snapshots, and measures teardown. |
| [compat-kit](../crates/compat-kit/README.md) | Resolves fixtures and original sources, supervises workers, checks observations and captures, and serves reports. |

The player consumes engine services through the [worldrunner facade](../crates/worldrunner/src/lib.rs). Managed callbacks enter the same dynamically loaded native library as the Rust host. Native store and response locks are released before managed execution, allowing callbacks to issue native requests.

```text
compat-kit: fixture selection, worker supervision, assertions, reports
  -> compat-player: compilation, scene import, timeline, window and rendering
     -> CompatibilityHost -> CoreCLR CompatibilityBridge -> original C# assembly
        <- UnityCompat commands and queries -> native FFI world
     -> native snapshot -> shared scene renderer -> offscreen target or window
```

## Original sources and content import

[Game pins and fetch logic](../crates/original-games/src/lib.rs) belong to `worldrunner-original-games`. Resolve an original from the repository root:

```sh
cargo run -j 2 -p worldrunner-original-games --bin original-games -- path snake
```

Checkouts live under `target/original-games`, independently of `CARGO_TARGET_DIR`. First use requires Git and access to the upstream repository. Consumers read these checkouts without editing them. Fixtures select a game by name; the kit supplies its absolute source root to the worker.

The [managed build](../crates/compat-player/src/managed_build.rs) enumerates `Assets/Scripts`, checks the fixture's optional script count, and compiles the selected files against `UnityCompat` for .NET 8. Each run builds its game and bridge under its artifact directory. `managed_configuration` selects Debug or Release.

The importer keeps GUID/file-ID identities, original fields and source-local diagnostics. Materialization creates objects inactive, allocates component handles, binds references, applies hierarchy and local transforms, then activates scene objects. Prefab templates remain inactive for later instantiation. An [admission policy](../crates/compat-content/src/admission.rs) rejects unsupported participants unless the fixture supplies an explicit omission reason.

The committed [profiles](../crates/compat-content/profiles) and [overlays](../crates/compat-content/overlays) supply source-specific translations:

- Snake uses an authored square for a missing sprite reference. Original tint and gameplay configuration remain in use.
- Breakout maps stale serialized fields, translates its missing sprite material to the unlit path, and selects sprite-bounds collision for wall tiles whose original sprite shape is empty.
- Pong maps older prefab and UI identities, fixes the serialized zero-scale Canvas for invertible native transforms, and uses the bundled DejaVu Sans font.

These substitutions are described in [source observations](../crates/compat-content/SOURCE-OBSERVATIONS.md). The runtime implements the selected sprite and UI rendering paths; it does not execute Unity's URP pipeline.

## Timeline, managed dispatch and cleanup

The default [player session](../crates/compat-player/src/session.rs) admits a frame with `frame.begin`, runs fixed callbacks around native physics, delivers native contacts, and runs Update and LateUpdate. Deferred destruction is flushed at the lifetime boundary. Input and rendering read native state.

An explicit `execution_profile: "native_phase_context_v1"` selects authenticated [phase contexts](../crates/disunity-ffi/PHASE_CONTEXT.md), rational advancement and timestamped input. Its timelines use `advance_rational` and `input_events`. The native sequence controls Initialize, fixed work, contacts, dynamic callbacks and Cleanup. Credentials remain private to the host and bridge.

The runtime also has separately selected [readiness](../crates/disunity-ffi/READINESS.md), [terminal cleanup](../crates/disunity-ffi/TERMINAL_CLEANUP.md), [Cleanup batch](../crates/disunity-ffi/CONTEXTUAL_BATCHES.md) and [transport byte](../crates/disunity-ffi/TRANSPORT.md) contracts. Readiness issues native tickets for lifecycle and iterator operations. Its nested-start profile supports bounded immediate routine and lifecycle children. Yielded iterator stacks and predicates remain unsupported. Cleanup batching is limited to lifecycle work in Cleanup. These profiles are negotiated explicitly; selecting a facade API profile alone does not enable them.

Callback failures remain observable even when a bridge dispatch completes. Unload drains script instances, coroutines and bindings, then destroys the native world. Worker completion reports native worlds, managed roots and pending readbacks. The harness requires completed teardown with zero remaining counts.

## Rendering and desktop input

The player's [extraction layer](../crates/compat-player/src/extraction.rs) converts native transforms, camera, sprites, tiles and UI into shared render-scene inputs. Image decoding and GPU upload belong to the player. The [camera resolver](../crates/camera/README.md) supplies projection and screen-query matrices. Shared rendering services support offscreen captures and window presentation.

Interactive fixtures receive OS keyboard events and continue until their configured duration, frame limit or user stop. F5 reloads, F12 captures, and Escape closes the run. Focus loss releases held keys. The [player README](../crates/compat-player/README.md) lists controls and live-tick bounds. Scripted journeys use the same session with authored actions and observations.

## Harness results and verification

The harness passes request and response paths to a configured worker. [Wire types](../crates/compat-kit/src/model.rs) define timelines, selectors, captures and teardown. Assertions check equality, tolerance, changes, sequences and captures. PNGs are decoded and checked before reports expose them. Run results retain expected and observed values, diagnostics and logs. The live loopback dashboard provides Run and Stop; static dashboard output provides report browsing.

Compatibility profiles such as `compat-2017.1` and `compat-6000.3` select API behavior on .NET 8. Coverage is defined by the tests and fixture assertions that execute. Historical Unity compiler equivalence, arbitrary precompiled Unity assemblies, Editor tooling and complete Unity API coverage are outside the implemented scope.

Native and managed tests build their required artifacts through committed helpers and fetch originals through `worldrunner-original-games`. They require no preselected native or managed artifact paths. The .NET SDK and graphics/display prerequisites still apply to the suites that use them. Standard verification entry points are documented in [build instructions](build.md); the [case list](unity-compatibility-kit-cases.md) gives focused commands and test locations.
