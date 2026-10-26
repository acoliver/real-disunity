# Getting started

This page gets the engine building on your machine, shows the demos you can run, and lists the test commands. Once it works, continue with [writing a Rust game](rust-games.md) or [writing C# scripts](csharp-games.md).

## Prerequisites

The repository pins its Rust toolchain in `rust-toolchain.toml`. If you have [rustup](https://rustup.rs), the first `cargo` command in the checkout installs the right version along with rustfmt and Clippy.

Rendering uses [wgpu](https://wgpu.rs), so you need a working GPU driver for one of its native backends:

| Platform | Backend | Notes |
| --- | --- | --- |
| Linux | Vulkan | Install your distribution's Vulkan loader and driver (for Mesa, `mesa-vulkan-drivers` on Debian and Ubuntu). |
| macOS | Metal | Nothing extra to install. |
| Windows | DX12 or Vulkan | A current GPU driver. |

C# work also needs the [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0). Pure Rust games do not.

## Build and run the demos

Clone the repository and run the procedural 3D scene viewer:

```sh
git clone https://github.com/acoliver/real-disunity.git
cd real-disunity
cargo run -p worldrunner --features scene-viewer --example scene-viewer
```

The first build compiles the whole renderer and takes a few minutes. In the window, drag with the left mouse button to orbit, use the mouse wheel to zoom, press R or Home to reset the camera, and press Escape to quit.

The viewer example accepts flags that turn on renderer features. They combine freely unless noted.

| Flag | Effect |
| --- | --- |
| `--terrain` | Adds a heightfield ridge under the meshes. |
| `--shadows` | Turns on cascaded directional shadows. |
| `--staggered-shadows` | With `--shadows`, refreshes far cascades less often than the near one. |
| `--hdr` | Uses the HDR frame profile: 16-bit float rendering with tone mapping. |
| `--msaa` | 4x multisampling. Only takes effect in an HDR profile. |
| `--water` | Adds a water surface. Selects the HDR profile. |
| `--render-scale=770000` | Renders at 77% of the window size and upscales with FSR 1 EASU. The value is in millionths, from 500000 to 1000000. Selects the HDR profile. |
| `--bilinear` | With `--render-scale`, upscales bilinearly instead of with EASU. |
| `--rcas` | With `--render-scale`, adds FSR 1 RCAS sharpening. |

For example, to see terrain, shadows, water and multisampling together:

```sh
cargo run -p worldrunner --features scene-viewer --example scene-viewer -- --terrain --shadows --water --msaa
```

Two headless examples render without opening a window. They are mainly useful if you are extending the renderer:

```sh
# Registers a custom render feature through the public facade.
cargo run -p worldrunner --features scene --example frame-feature

# Mixes caller-owned draw code with the engine's executor.
cargo run -p worldrunner-render-post --example bridge
```

## Inspect a Unity project

The `unity-import` tool reads a Unity project without changing it and prints what the importer understands. Point it at any project directory:

```sh
cargo run -p worldrunner-unity-import -- catalog path/to/UnityProject
```

[Importing Unity projects](unity-projects.md) covers the other commands.

## Run the tests

CI runs three test profiles through the `cargo xtask` task runner. You can run the same ones locally:

```sh
cargo xtask ci-cpu       # Simulation, import, tools and policy checks. No GPU needed.
cargo xtask ci-gpu       # Everything except the .NET host. Needs a GPU or software Vulkan.
cargo xtask ci-managed   # The .NET 8 build and the managed host tests. Needs the .NET 8 SDK.
```

GPU tests fail when no adapter is available. On Linux without a GPU you can use Mesa's software Vulkan driver (lavapipe) and a virtual display:

```sh
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a cargo xtask ci-gpu
```

The GPU profile runs every test serially, so it takes a long time on a software driver.
