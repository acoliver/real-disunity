# Real Disunity

A Rust 2024 workspace for reusable CPU, graphics, and application components. 

## Consumer migration and extensions

See the [frame executor migration guide](docs/frame-migration.md) for render contract v4, material payload v2, current GPU strides, encoder-only capture adapters, asynchronous timing and external feature registration. The headless `frame-feature` example builds and runs through public facade APIs. The [checked bridge proof](docs/bridge-proof.md) demonstrates mixed consumer and stock draws, target exchange, resolves and capture continuation.

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

Terrain casts onto meshes and itself, and receives mesh and masked foliage shadows. The retained frame scheduler renders four directional shadow cascades into independent depth-array layers. Scene extents specify light-space coverage half-widths about the caller-supplied focus; overlapping terrain and mesh casters extend each cascade toward the light. Optional blend bands cross-fade to the next cascade near coverage edges. Water requires the HDR profile described below. Omit `--terrain` for the mesh-only example.

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

### Depth, normals and ambient occlusion

HDR frames can opt into `SceneEffects::ssao` and `SceneEffects::with_ssr`. The stock prepass runs at the full
render extent with one sample, independently of forward 1x or 4x MSAA. It writes
`Depth32Float` camera depth and signed view-space XYZ normals in
`Rgba16Float`; alpha marks covered pixels. SSAO alone uses geometric normals.
Enabling SSR installs shading-normal variants and a matching F0/roughness pass. Meshes and terrain share camera
selection, and masked cards use the colour shader's texture, sampler, billboard
transform and alpha cutoff. Forward colour uses its own matching-sample depth,
so a centre-sampled prepass cannot remove MSAA edge coverage.

PostChain schedules depth linearization, a conservative minimum-depth pyramid,
SSAO and a depth-aware 5x5 bilateral filter before opaque lighting. Pyramid
levels use ceiling-half extents in separate textures, including odd final rows
and columns. Background linearizes to the camera far distance. The deterministic
hemisphere kernel consumes radius, intensity, sample count, surface bias and
horizon angle. AO multiplies ambient diffuse lighting only; sunlight, emission
and unlit colour are unchanged. With both SSAO and SSR absent the original shaders
and pass sequence remain in use.

The scene and document settings now include explicit `sample_count` (1 through
64) and `bias_millimeters`. Authored documents use render contract v4; earlier versions are
rejected rather than migrated implicitly. The stock radius range is 1 through 1,000,000
millimetres, with bias strictly below radius. Unsupported settings fail before
allocation. Legacy upload rejects SSAO with a typed error. Custom HDR hosts use
`ScenePipelines::with_ssao` with a matching `FrameRenderer::hdr` profile. Resize
builds replacement screen resources before publication. Submit successful
encoders in preparation order, as required by the frame admission contract.

Planar water is available in the HDR frame profile at 1x and 4x MSAA. Run the
procedural scene viewer with `--water` (optionally `--msaa` and `--terrain`).
It uses one authored rectangle, procedural animated normals, opaque-only
refraction, Beer absorption, surface depth writes and shared cascade shadows.
Refraction rejects foreground depth samples. Snapshot depth stores positive
view-space metres and selects the nearest covered MSAA sample. Snapshot colour
is the linear average of the retained opaque samples. Three admitted slots
own independent snapshots; resize builds replacements before publication.
Hosts supply animation time with `FrameRenderer::set_frame_time`. Water settings
are immutable for a compiled profile. Water is a retained `FrameFeature`, using
registration, planning, validation, preparation and drawing like external features.

Snapshot production is independent of water. A feature declares reads of
`ResourceRef::new("opaque snapshot color")` or `ResourceRef::new("opaque snapshot depth")`.
Camera-aware composition installs those producers only when an active declaration
reads them. `FrameRenderer::encode_frame` supplies the camera needed for depth
linearization. Features obtain the current slot's checked texture view through
`PrepareContext::resource` and `AttachmentView::view` when creating bindings.
With no reader, there are no snapshot draws or copies. Resource versions and
attachment hazards are validated before any feature prepares.

The typed slot sequence is opaque, opaque snapshots, `TransparentWater`,
`AfterWaterDepth`, then `ScreenSpaceAfterTransparent` and post output. SSAO and
opaque snapshots see pre-water depth. Water loads and writes the camera depth
attachment; subsequent passes reading that attachment see surface depth. The
`post-water depth` resource is a distinct positive-view-metre resolve, refreshed
after water at 1x and 4x. Later depth-based effects must declare this resource,
not reuse the pre-water SSAO pyramid. Optional SSR uses the opaque snapshot as
its reflected source, validates hit coverage and orientation, and adds filtered
Fresnel-weighted specular with a smooth roughness cutoff. Matching pre-water and
post-water sample reductions exclude water-covered receivers. Water is absent
from the reflected source and does not export reflection metadata.

One water writer is supported. Multiple water writers, and the reserved
`TransparentAlpha` slot, return `FrameError::TransparentOrdering` before
preparation. Supporting ordinary alpha features will require stable, cross-feature
back-to-front packet sorting with depth read-only, rather than registration order.
Intersecting transparent geometry remains outside that policy.

## Compatibility runtime and harness

The [compatibility player](crates/compat-player/README.md) runs original Unity game sources against the native world through an independently authored C# facade and the shared renderer. The [process harness](crates/compat-kit/README.md) provides input timelines, evaluated results and the local dashboard. Original-source game fixtures require .NET 8. Compatibility-profile names select an API facade on .NET 8; they do not certify a historical Unity editor or compiler.
