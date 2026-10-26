# Architecture

Real Disunity has one native world, one C ABI over it, one .NET host, one `UnityEngine` implementation and one renderer. Rust games and C# games use the same implementation of each. This page says which crate owns which responsibility, how a frame flows through them, and which rules keep it that way. The original design and its reasoning are in [#38](https://github.com/acoliver/real-disunity/issues/38).

## Ownership

Every behavior has exactly one owner. When something is missing, extend its owner. Do not add a second world, host, Unity API, renderer, or a selectable mode or profile for the same behavior.

| Responsibility | Owner |
| --- | --- |
| Running object state: identity, typed components, hierarchy, clone, lifetime, lifecycle scheduling, clock, input, coroutines, 2D physics, UI layout and text | `crates/world-simulation`. Typed Rust with no JSON, CLR, wgpu or filesystem dependency. |
| The C ABI: foreign-buffer validation, handles, result and buffer ownership, calls into the world | `crates/disunity-ffi`. No domain rules. |
| Managed execution: booting .NET, loading assemblies, dispatching callbacks | `DisunityHost` in `crates/disunity-runtime`. One host per process. |
| `UnityEngine` names for C# code | `csharp/Disunity` (`Disunity.Engine`), one assembly. `Disunity.Hosting` holds host helpers and `Disunity.Native` holds generated imports. |
| Turning world state and scene descriptions into render input | `crates/render-plan` (CPU only). |
| Drawing | `crates/render-scene` on the `render-gpu`, `render-lighting`, `render-shadow` and `render-post` services. |
| Window, input events and presentation | `crates/scene-viewer`. |
| Reading Unity projects | `crates/unity-import`, an offline content tool with no runtime, simulation or renderer dependency. |
| The single dependency a game takes | `crates/worldrunner`, a facade that only re-exports. |

## Crates

The layer of each crate is declared in `crates/xtask/src/layers.rs`, and `cargo xtask policy` rejects dependencies that point the wrong way. The allowed directions are listed in [quality gates](quality-gates.md#dependency-and-layer-policy).

| Layer | Crates |
| --- | --- |
| Foundation | `core-types` (identifiers, units, hashes), `asset-id`, `catalog`, `package` (content-addressed packages), `texture`, `mesh`, `material`, `import-schema`, `camera` (CPU camera math), `script` (sandboxed Lua 5.4), `world-simulation`, `disunity-ffi` |
| World | `world-schema` (world documents), `world-model` (semantic world documents), `world-runtime` (capability negotiation over documents), `world-generation` (deterministic seed-based generation), `save` |
| Bridge | `render-plan` |
| Graphics | `render-gpu`, `render-lighting`, `render-shadow`, `render-post`, `render-scene` |
| Host | `scene-viewer` |
| Managed | `disunity-runtime` |
| Tool | `unity-import`, `content-tool`, `xtask` |
| Facade | `worldrunner` |

`world-runtime` prepares validated documents and their resources. Running object state belongs to `world-simulation`.

## A frame

For a Rust game:

1. The game pushes timestamped input into the `World` and calls `World::advance(elapsed)`.
2. The world admits whole fixed steps, then runs lifecycle work, fixed updates, physics, contacts, the dynamic update, coroutines, late update and end-of-frame cleanup in a fixed order ([frame advancement](simulation/advancement.md)). With no scripts attached, `advance` returns `Advance::Complete` directly.
3. `render_plan::simulation::extract_world` reads sprites, tiles and the camera into a `WorldRenderPlan`, and `extract_ui` adds UI glyphs. Native XY geometry becomes scene XZ coordinates here, once.
4. `render-scene` prepares the plan as a dynamic contribution, and `scene-viewer` presents it, or `render-scene` captures it offscreen.

For C# scripts, step 2 is driven through the ABI. `DisunityHost::advance` calls `disunity_world_advance`. Each time a script must run, the world returns one callback work item. The host releases the ABI call, invokes the managed method, then calls `disunity_world_resume` with the outcome. Managed code never scans for ready scripts itself. Step 3 runs inside the native library through `export_presentation`, and the result is passed to `render_plan::simulation::from_presentation`.

The C# side and the Rust host load the same `disunity-ffi` dynamic library. `disunity-runtime` never links the FFI crate statically, so both sides see one object table.

3D content bypasses the world: a game builds a `RenderScene` directly and gives it to the renderer.

## Invariants

- **Memory safety.** The workspace lint table sets `unsafe_code = "forbid"`. Only `disunity-ffi` (the C ABI) and `disunity-runtime` (loading the .NET runtime and resolving its entry points) contain unsafe code, and they do not inherit the workspace lints. `script` also does not inherit them; it has no unsafe code of its own but links the vendored Lua C library through `mlua`. Keep new unsafe code inside those two crates.
- **Explicit inputs.** Library code takes devices, settings, paths and budgets as arguments. There is no global state. CPU crates do not read environment variables, and `cargo xtask policy` scans them for lookups. `render-gpu` honors wgpu's diagnostic variables such as `WGPU_BACKEND` when it creates an instance.
- **Fail explicitly.** Unsupported Unity API, unsupported content and out-of-range settings return typed errors or throw named exceptions. There are no no-op stand-ins and no silent fallbacks.
- **No per-game code.** The engine and importer contain no per-game overlays, field aliases, substituted assets or ignored-script lists. A game that needs something gets the general behavior, tested with synthetic fixtures.
- **Unchanged games.** The original Unity game repositories are never modified. They are fetched at pinned commits by an external harness.
- **Generated ABI bindings.** The C# bindings and layout checks are generated from the Rust ABI and checked in. A build fails if they drift; regenerating them is a deliberate copy. The procedure is in [`csharp/README.md`](../csharp/README.md).
- **Determinism where it is promised.** World generation is bit-identical across platforms ([portable generation contract](../crates/world-generation/DETERMINISM.md)). The simulation clock is exact rational time.

## Coordinates and units

The scene renderer is right-handed and Z-up. Scene documents use integer millimetres, micro-units for fractions and millidegrees for angles, and convert to metres at the GPU boundary. `world-schema` documents are Y-up metres, converted to the scene by `(x, -z, y)` in `render-plan`. `world-simulation` uses metres and radians; 2D physics runs in the XY plane by default and can select XZ. Any new boundary between these frames must make its conversion explicit at that boundary.
