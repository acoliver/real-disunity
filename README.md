# Real Disunity

A Rust 2024 workspace for reusable CPU, graphics, and application components. 

## Prerequisites

- Rust 1.97.0 (`rust-toolchain.toml` installs rustfmt and Clippy); the declared MSRV is 1.95.
- Git and a C toolchain where native dependencies require one.
- Vulkan development/runtime support for graphics execution. CI uses Mesa lavapipe for software rendering on Linux.

## Build and quality gates

The `cargo xtask` alias runs the workspace task runner without an additional installation:

```sh
cargo xtask fmt          # non-mutating formatting check
cargo xtask lint         # strict Clippy
cargo xtask complexity   # quantitative Clippy limits
cargo xtask check        # locked workspace/all targets/all features
cargo xtask build        # locked workspace/all targets/all features
cargo xtask test         # locked tests and doctests
cargo xtask policy       # layers, transitive CPU graphs, registered explicit policy
cargo xtask source-size  # 750-line warning / 1000-line hard limit
cargo xtask ci           # full workspace; requires graphics and .NET prerequisites
cargo xtask ci-cpu       # CPU packages and facade without default features
cargo xtask ci-gpu       # all non-managed packages; required full GPU test mode
cargo xtask ci-managed   # explicit .NET 8 fixture and native host verification
```

`cargo xtask ci` requires a current lockfile and runs formatting, policy, strict lint and complexity gates, then locked check/build/test/doctests. CPU, software-Vulkan, and managed CI jobs use separate explicit profiles; missing prerequisites fail rather than skip tests. Compilation subprocesses use two jobs. See [quality gates](docs/quality.md) for the layer graph, exact initial registry/literal scope, behavioral fixtures, and limitations; the environment lookup scan alone is not an explicit-policy guarantee.

## Terrain shadow example

Run the procedural viewer with a heightfield ridge and an elevated mesh:

```sh
cargo run -p worldrunner --features scene-viewer --example scene-viewer -- --terrain --shadows
```

Terrain casts onto meshes and itself, and receives mesh and masked foliage shadows. The retained frame scheduler renders four directional shadow cascades into independent depth-array layers. Scene extents specify light-space coverage half-widths about the caller-supplied focus; overlapping terrain and mesh casters extend each cascade toward the light. Optional blend bands cross-fade to the next cascade near coverage edges. Water remains unsupported and returns a typed error. Omit `--terrain` to retain the original mesh-only example.

## Frame admission and surface lifecycle

The viewer selects two frames in flight in its example. `ViewerOptions::frames_in_flight`
accepts one through three. Admission reserves a slot before surface acquisition; a full
queue returns without acquiring another image. Surface creation, configuration, acquisition
and presentation stay on the event-loop thread. Nonblocking device polls dispatch completion
callbacks on the frame timer; no completion callback calls the window or waits for the event loop.
Shutdown drops the surface before the window without joining a worker. Input callbacks never
wait for GPU completion. Resize requests coalesce to the newest physical extent.
Surface loss remains pending across zero-size suspension until replacement succeeds.
Occlusion and zero-size windows pause redraw scheduling, and resize or visibility events
resume it.

Admission and completion progress never block the event thread. Backend drawable
acquisition can wait for a presentable image through Metal `nextDrawable`, Vulkan
`acquire_next_image`, or the DX12 frame-latency wait. Present mode and frames-in-flight
settings bound that wait according to the backend's presentation policy.

Encoder-only hosts can use `worldrunner_render_gpu::flight::FrameAdmission` and select the reserved
slot on their scene and frame renderer. Submit successful encoders in preparation order.
All submissions to that queue, including unrelated work, must use the same admission authority
so submission and callback registration remain serialized.
Uniform ranges are separate per slot; immutable staging allocations belong to that slot
until reuse after completion. Shared GPU attachments and instance destinations remain
queue ordered. Offscreen capture exposes three independent readback slots and rejects
an unread slot overwrite. Read each capture with the submission index containing its copy;
that read does not request a queue-wide wait.
