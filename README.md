# Real Disunity

Real Disunity is a game engine written in Rust. It runs games written for it in Rust, and it runs C# game scripts written against the `UnityEngine` API, on a native simulation and a wgpu renderer.

The long-term goal is that Unity games, and later Unreal games, compile and run against Real Disunity without source changes, with current rendering techniques, on an engine whose own code is memory-safe Rust. The engine is not there yet. This page describes what works today, what does not, and how to start building on it.

The project is MIT licensed. See [LICENSE](LICENSE).

## Current state

The engine is pre-release (version 0.1). APIs change without notice and nothing is published to crates.io or NuGet. CI builds and tests every change on Linux, macOS and Windows.

What works today:

- **Native world simulation** (`crates/world-simulation`): objects, typed components, parenting with full affine transforms, cloning with reference remapping, deferred destruction, Unity-style lifecycle ordering (Awake, OnEnable, Start, FixedUpdate, Update, LateUpdate, OnDisable, OnDestroy), coroutines, an exact rational clock with fixed steps, timestamped keyboard and axis input, 2D physics with continuous collision, triggers and queries, and Canvas/RectTransform UI layout with TrueType text.
- **C# scripts on `UnityEngine` names** (`csharp/Disunity`): `MonoBehaviour`, `GameObject`, `Transform`, `Instantiate`, `Destroy`, coroutines, `Input`, `Time`, `Rigidbody2D` and 2D colliders, `SpriteRenderer`, `Canvas`, `Text` and more. Every call goes to the same native world a Rust game would use. See the [supported API list](docs/unity-api.md).
- **Rendering** (`crates/render-*`, `crates/scene-viewer`): sprites, tiles and UI text for 2D games, and a 3D renderer with metallic-roughness materials, cascaded shadows, terrain, water, sky and height fog, SSAO, screen-space reflections, HDR exposure, bloom, FXAA and FSR 1 upscaling. It runs on Vulkan, Metal and DX12 through wgpu, in a window or headless.
- **Unity project import** (`crates/unity-import`): reads Unity text scenes and prefabs in both the current and the Unity 2017 prefab formats, applies prefab overrides and resolves `.meta` GUIDs. It also reads textures, sprites, tilemaps, physics materials and renderer materials. It never writes to the project it reads.

What does not work yet:

- **An unmodified Unity game does not run end to end.** The importer can read the scenes of the three test games (Snake, Pong and Breakout) but nothing yet loads an imported scene into a running world. That loader is in progress ([#77](https://github.com/acoliver/real-disunity/issues/77)).
- There is no player executable. A C# game is currently hosted from Rust code through `DisunityHost`.
- 3D physics, audio, animation, particles, mouse and gamepad input, and most of `UnityEngine.UI` beyond `Canvas` and `Text` are not implemented.
- There is no Unreal support of any kind.

Code that uses a `UnityEngine` member `Disunity.Engine` does not have fails to compile. A supported member called with an unsupported setting throws an exception that names the setting.

## Ways to build a game

There are three entry points, and they all share one world implementation.

1. **A native Rust game.** Depend on the `worldrunner` facade crate, build a `World`, add components, advance it each frame and hand the extracted sprites or 3D scene to the viewer. See [Writing a Rust game](docs/rust-games.md).
2. **C# scripts against `UnityEngine`.** Reference `csharp/Disunity/Disunity.Engine.csproj`, write ordinary `MonoBehaviour` classes, and run them in the native world through `DisunityHost`. See [Writing C# scripts](docs/csharp-games.md).
3. **An existing Unity project.** Inspect what the importer understands with the `unity-import` tool. Running the imported scene is the part still in progress. See [Importing Unity projects](docs/unity-projects.md).

## Quick start

You need the Rust toolchain from `rust-toolchain.toml` (rustup installs it automatically), Git, and a Vulkan, Metal or DX12 capable GPU driver. C# work also needs the .NET 8 SDK.

```sh
git clone https://github.com/acoliver/real-disunity.git
cd real-disunity

# Open the procedural 3D demo with terrain and cascaded shadows.
cargo run -p worldrunner --features scene-viewer --example scene-viewer -- --terrain --shadows

# Run the CPU test suite that CI runs on every platform.
cargo xtask ci-cpu
```

In the viewer, drag with the left mouse button to orbit, use the wheel to zoom, press R to reset and Escape to quit. [Getting started](docs/getting-started.md) lists the other viewer options and the remaining test commands.

## Architecture

Each responsibility has exactly one owner. A game written in Rust and a game written in C# drive the same native world, and both are drawn by the same renderer.

```text
  Rust game                         C# game scripts (UnityEngine API)
      |                                         |
      |                              csharp/Disunity  (Disunity.Engine)
      |                                         |
      |                              disunity-ffi     (C ABI only)
      |                              disunity-runtime (DisunityHost, .NET host)
      v                                         v
  world-simulation: objects, lifecycle, time, input, 2D physics, UI
      |
  render-plan: extract sprites, tiles, UI and 3D scenes into render descriptions
      |
  render-scene + render-post + render-gpu: wgpu renderer and post-processing
      |
  scene-viewer: window, input and presentation (or headless capture)

  unity-import: offline tool that reads Unity projects into engine content
```

The `worldrunner` crate is a facade that re-exports these crates behind feature flags, so a game depends on one crate. Engine library code forbids `unsafe`. The two exceptions are `disunity-ffi`, which implements the C ABI, and `disunity-runtime`, which loads the .NET runtime. [Architecture](dev-docs/architecture.md) has the full crate map and dependency rules.

## Repository layout

| Path | Contents |
| --- | --- |
| `crates/` | All Rust crates. `worldrunner` is the facade a game depends on. |
| `csharp/` | `Disunity.Engine` (the `UnityEngine` API), its tests, and two small fixture projects. |
| `docs/` | Documentation for people using the engine to make games. Start at [docs/README.md](docs/README.md). |
| `dev-docs/` | Documentation for people and coding agents changing the engine. Start at [dev-docs/README.md](dev-docs/README.md). |
| `.github/workflows/` | CI: CPU and managed suites on three platforms, GPU suite on Linux with software Vulkan. |

## Contributing

Read [dev-docs/README.md](dev-docs/README.md) before changing code. It explains where each kind of change belongs, the enforced size and complexity limits, and how to run the same checks CI runs.

## License

Real Disunity is released under the [MIT License](LICENSE). Third-party dependencies keep their own licenses. The FSR 1 upscaler shader in `crates/render-post/src/upscale.wgsl` is ported from AMD FidelityFX FSR 1, also MIT licensed, and keeps AMD's notice in the file.
