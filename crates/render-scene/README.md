# Pathless scene renderer

The renderer consumes `RenderScene` and explicit view/device settings. It uploads
owned meshes and textures, retains material pipeline batches, renders optional
terrain and directional shadows, and supports tightly packed RGBA offscreen
readback. It has no asset lookup or file-loading API.

Terrain uses an independently defined separable 3×3 tent filter for coverage and
a normalized material mixture. Reordering a dense palette with the matching grid
remap does not change the rendered surface. Blur radii, tints, and rotations come
from scene inputs; no content-specific blur table is embedded.

Terrain material footprints reconstruct their triangle plane from exact packed
heightfield positions. A draw's sample window and stride select its grid cell
against the stored f32 axis boundaries, including shortened final intervals.
Internal boundaries belong to the next cell; the outer edge belongs to the last
cell. Skirts use their vertical edge plane.
Terrain uses the standard shared-vertex mesh layout. Each fragment solves its
quad-aligned adjacent-pixel displacement on that plane without screen derivatives
or subtraction of world positions. At grazing angles, a relative 1e-5 determinant
floor and a 1e4-metre displacement cap keep the filter gradients finite.

The named `Frame.raster_viewport.xy` field carries the actual raster extent.
`FrameUniform` encodes 336 bytes: a 320-byte lighting prefix and a 16-byte raster
extent vector.
Perspective and orthographic projections use the same displacement solve.
Forward, normal prepass, material metadata and reflection replay share this path.
Non-terrain footprint and sampling logic is unaffected.

Terrain group 1 binding 23 (`terrain_plane_data`) stores exact f32 source
coordinates and eight-word sample-window descriptors. Its payload is
`16 + 4 * (columns + rows + columns * rows) + 32 * chunk_count` bytes;
instances add `INSTANCE_BYTES * chunk_count`. These are immutable scene uploads,
like the existing terrain mesh, outside the streaming leaf and material-page
budgets. The triangle source checks both buffer and storage-binding size limits.
Instance bytes use checked arithmetic and check only the buffer size limit,
before allocation or freezing a previous streaming generation. Rejection returns
`DrawError::TerrainInstanceSizeOverflow` or `DrawError::TerrainInstanceLimit`.

Terrain uses one vertex storage buffer. Fragment storage counts for palette,
cached palette, layered and cached layered draws are 1, 2, 4 and 5 respectively.
All fit wgpu's default limit of 8, including Linux. Downlevel's limit of 4
supports palette paths; layered pipelines already require 5 for their compute
preparation and remain unavailable there. WebGL2's zero-storage limits do not
support this terrain path. Binding indices require at least 24 slots per group.

Streaming terrain residents remain sorted by `TerrainChunk`'s derived key
(minimum sample coordinates, maximum coordinates, then stride), including retained
source replacements. Forward, shadow, depth and metadata passes traverse the same
ordered residents. Equal-depth boundary overlap therefore has stable ownership
when the resident leaf set is identical, independently of upload retirement timing.
Ordering does not change chunk geometry, streaming budgets or draw counts for
an identical leaf set. Selection ranks candidates by descending projected error,
then quadtree level from the root (descending stride), then the chunk's derived
coordinate key. Exact-error ties therefore have explicit admission priority under
constrained budgets, independent of resident storage and upload-completion history.
Selected leaves are returned in derived-key order. Relative to history-based tie
admission, only exact-error ties can change which geometry becomes resident.

Terrain material normals use +X along layer U and +Y along layer V. Unrotated
U follows east and V follows north in the Z-up scene. Appearance rotation applies
to world-to-UV mapping; its inverse brings each layer normal into the common
east/north frame before blending and page storage. From the normalized geometric
normal N, the draw constructs T = normalize([N.z, 0, -N.x]) and B = cross(N, T).
T follows the eastward heightfield direction at constant north. Live layered draws
and cached pages use the same final normal composition.

`SceneTerrain::with_material_normal_strength(Some(value))` accepts finite values
in [0, 4]; `None` restores the default 1. Zero uses the interpolated geometric
normal, one preserves the unscaled shading bytes, and larger values scale the
material normal's XY components before normalization. Strength is applied after
page lookup, so it has identical semantics for live and cached materials.

Raw-texel applications can request an empty GPU feature set. DDS block-compressed
scenes must request BC support. Invalid texture payloads return typed errors.
Opaque and alpha-masked materials, metallic-roughness lighting, exposure, and
explicit single-map and cascaded directional shadows are implemented. The HDR
profile also supports SSAO, water, SSR, adaptive exposure, bloom and FXAA through
validated scene settings. The legacy profile rejects post requests before upload.
Textured-sky rendering, vignette, nonneutral contrast and saturation remain
unsupported. Zero-weight unsupported requests still fail.

A scene without an environmental profile renders unlit albedo plus emission,
without lighting or grading. Directional shadows require an environmental
profile. The desktop viewer takes camera intrinsics explicitly in `ViewerOptions`
and validates the selected frame profile before startup.

The tests construct procedural geometry and textures. GPU tests require a real
hardware or software adapter and fail if one cannot be acquired. For a software
Vulkan run on Linux, pass the driver selection explicitly to Cargo, for example:

```sh
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json CARGO_BUILD_JOBS=2 \
  cargo test --locked -p worldrunner-render-scene -- --test-threads=1
```

## Player program draws

`SceneGpu::admit_programs` carries draws that `worldrunner-render-program` admitted into the next frame encoded with this scene. `compositor::encode_scene` and `FrameEncoding::encode_scene` take them, and the scene's feature records them in the stock opaque pass after its own opaque geometry. They draw into the same color target and depth buffer, are depth tested against the scene's geometry, and leave through the frame's one submission with the stock resolve, output and capture copies. No pass, slot or entry point was added, and nothing else can reach the pass. `SceneGpu` accepts only draws from `ProgramRuntime::admit_draws`, so the arena, the frame's authority and the target signature were checked before they arrive. See the [program crate](../render-program/README.md#joining-a-renderer-frame) for admission and lifetime.

Encoding checks the draws before it declares anything:

- A scene with temporal reconstruction (velocity or a jittered projection), screen-space passes or multisampling fails with `FrameError::UnsupportedEffect` and the name `player programs with temporal reconstruction`, `... with screen-space passes` or `... with multisampling`.
- Draws prepared for other attachments than the scene's color format and `Depth32Float` fail with `FrameError::FeaturePreparation` for `player programs`, carrying `ProgramError::TargetMismatch`.
- An arena that retired, was dropped or belongs to another submission authority than the frame fails the same way with `LeaseError::Expired` or `LeaseError::ForeignAuthority`.
- Every refusal comes before the encoder records anything for the frame. The checks take the admitted draws, so a frame they refuse leaves none behind for the next frame, and a later admission replaces draws that no frame has taken.

A frame with no admitted draws is unchanged: `without_admitted_programs_the_frame_is_byte_identical_to_the_stock_scene_draw` compares its output bytes with a direct draw of the scene. A cancelled frame submits nothing, publishes no counters and leaves the target as it was.

The tests are in `src/draw/tests/program_frame*`. `player_program_and_scene_gpu_draw_composite_in_one_submitted_frame` draws a red unlit engine card and two patches of a synthetic SPIR-V program in one frame, with explicit `ProgramParameters`, and checks engine-only, player-only and overlapping pixels, an unchanged `submitted_serial` while recording and a serial rise of exactly one. The patches' clip depths lie on either side of the card's depth, which is derived from the projection, so the expected pixels do not depend on the shaders. Run them with `cargo test -p worldrunner-render-scene --lib -- program_frame --test-threads=1`.

## Dynamic quads on the program runtime

Dynamic quads, native sprites and tilemaps, and UI glyphs routed into the world draw with the scene shader's `vs_unlit` and `fs_unlit`. That material is a `render-program` graphics program over the unchanged scene WGSL, and `ScenePipelines` no longer builds a pipeline for it. The runtime builds the pipeline for the pass it is given, with straight alpha over location zero, `LessEqual` depth without a depth write, no culling and an empty write mask on every other colour attachment, so the velocity and reflection targets of a pass keep what other materials wrote. Plain, SSAO, velocity and SSR pipeline sets, `Rgba8UnormSrgb` and `Rgba16Float`, and any sample count the pipelines were built for take the same material.

The runtime borrows what the scene already owns, by semantic: the slot's 320-byte frame uniform at group 0 binding 0 (the frame slot of the transaction that committed the frame, not a copy), and for each group of adjacent compatible quads its vertex, instance, tint and material buffers, its albedo texture and its sampler. Each group is one instanced draw over `0..instance_count`. The stable `(overlay, sorting_layer, sorting_order, hierarchy_order)` order and adjacent-only batching are unchanged. A group is adjacent quads with the same clip rectangle, texture, corners, rectangle and UV rectangle, so quads with different clip rectangles are separate draws. The scene counts none of these buffers against an allocation set and neither does the runtime: that accounting does not exist for dynamic geometry, and this change adds none.

Staging quads prepares a draw for each frame slot, which can fail and allocate, beside the buffers' upload. `encode_scene` and `ShadowedScene` prepare the quads for a pass the staging pipelines did not describe, if there are any, and retain the staged draws on the frame before the pass opens. `encode_resident` takes a shared scene, so it retains and refuses a pass the quads were not prepared for. Recording is state setting at the position the fixed pipeline's draw had: objects, terrain, sky and celestial meshes, then quads. The staged quads own what they lent through a `Keepalive`, so replacing or clearing them while a frame that recorded them is unfinished leaves the buffers alive until that frame is cancelled or completed on the GPU.

The scene's pass does not clip: `DynamicQuad::clip_rect` splits draws there and is applied only by the retained-UI overlay. The overlay keeps its own fixed pipeline (`vs_ui_overlay`, `fs_ui_overlay`) over the same staged quad buffers, with a material bind group per draw and a scissor per draw taken from the clip rectangle (clamped to the output, flipped to the top-left origin, skipped when nothing is left). Its draw returns the calls and instances it recorded for the UI pass statistics. `src/dynamic/capture_tests/clipping.rs` draws partial, disjoint, full, oversized, outside and mixed clip rectangles in every target set and requires the scene's pass to match the unscissored fixed draw and the overlay's draw, recorded with the fixed unlit pipeline, to match the scissored fixed draw and its counts byte for byte.

`src/dynamic/capture_tests.rs` draws quads, sprites and glyphs with this path and with a test-only copy of the former fixed pipeline and material bind group, on one device, under each target set above with the auxiliary attachments pre-filled, and requires every attachment to match byte for byte. It prints a hash per attachment, which was identical before and after the change on the Radeon 780M and on llvmpipe.


## Terrain on the program runtime

Terrain draws through `render-program` in every forward context: plain, velocity, SSAO and its gather receiver, reflection, and each of them as a shadow receiver, at one or four samples, and in the metadata prepass (the normal output and the merged normal and material output, with the camera uniform lent at set 3), the opaque term replays and the shadow casters. The shader source is the scene's own, byte for byte (`terrain/fixtures` freezes `terrain.wgsl` and the page and layered sources, and `the_live_terrain_wgsl_equals_the_frozen_snapshot` fails when the live text drifts). Only the machinery moved.

- **Programs.** `terrain::program::Context` holds one program per surface source for a `ScenePipelines`: palette (`Plain`), baked `Pages`, `Layered` sources and `LayeredPages`. A surface is a feature set (`TERRAIN_PAGES`, `TERRAIN_LAYERED`); the composition is `terrain::program::source`. The frame's specular convention and winding are override constants of the fragment stage, so a specialization is a cheap program, not a sibling set of compiled pipelines. Derived bindings at set 1 are the ones the shader reads: seven for `Plain`, four for `Pages` and for `LayeredPages` (the same four, because cached pages replace the source composer), fifteen for `Layered`.
- **Sets the pass owner binds.** The frame group (set 0), the shadow map (set 2) and the screen group (set 3) are lent as `ExternalSet`s over the layouts the scene already uses. The pass binds them once, as it did, and the runtime builds no bind group for them. The terrain's own set 1 is a `PreparedSet` built from the buffers and textures the scene retains, once per terrain and pipeline context, with one per page table for the cached surfaces. A set stays with its context's runtime and material layout.
- **Receiver payload.** Paired single-sample shadow contexts declare the lossless displacement arrays at set 2 bindings 3 and 4 in the lent shadow group. Terrain loads its fragment's payload in every forward depth mode, including page and layered surfaces. The material's prepared set stays at set 1; the pass binds the payload with its shadow map.
- **Depth reuse.** A context's `DepthMode` (independent, shared, prepass or reuse, from the forward depth reuse of single-sample frames with a complete prepass) gives the terrain program its depth format and its `RenderState`: the same `DepthMode::state` the fixed pipelines use, so reuse terrain tests depth `Equal` without writing it and tests and zeroes the stencil mark with reference `STENCIL_REFERENCE`, which the pass sets once. The runtime draws stencil from that state (`render-program` README, "Stencil").
- **Cost.** A frame records one `PreparedPipeline` and a `GeometryDraw` per resident chunk: `2 + 4C` commands for C chunks (`3 + 4C` under a shadow map), the same as the fixed path. A chunk's record of the shared instance buffer is its geometry's instance range rather than a bound slice. Pipelines, sets and chunk geometry are prepared the first time a pass needs them and found in maps afterwards. `encode_scene`, `ShadowedScene` and `encode_resident` prepare the terrain's pipeline for the pass, the prepass and the forward pass alike, after the frame's pages are prepared and before any pass opens, where a refusal is still an error (`SceneGpu::prepare_terrain`); a term replay does the same before it opens its pass. A raw `SceneGpu::draw` prepares what it finds missing as it records. A streamed chunk's geometry is made when the chunk is uploaded and dropped when it retires, before new uploads are admitted, so the geometry never holds a buffer the streaming budget has stopped counting. A prepared set belongs to the runtime and the program layout of the context that built it, so a scene's terrain keeps one per context that has drawn it: independently built pipelines, and the forward pipelines a renderer replacement builds again, each make their own. Sets and pipelines are kept per context and dropped with the context they were made for, so the pipelines a term replay builds for one frame leave nothing in the caches of a long-lived scene, and frames over the same chunks add nothing to `RuntimeStats`. The runtime is shared by a `ScenePipelines` and every sibling specialized from it, so a terrain pipeline compiles once per context for any number of scenes. `src/terrain/cost_tests.rs` holds these claims.
- **Shadow caster.** `shadows::terrain_caster::TerrainCaster` replaces the fixed caster pipeline: a vertex-only program (`vs_terrain_depth` over the shadow depth source) on `TargetSignature::depth_only(Depth32Float, 1)`, with the caster's state (back faces culled, the configured depth test with writes, and its depth bias, slope included). Each cascade's light frame buffer is the program's set 0, lent as an import and prepared once per buffer, so a scene prepares four sets for each frame slot it draws. `ShadowedScene` prepares them with its other per-frame preparation, and recording is the commands the fixed caster recorded, `2 + 4C` for C chunks, over the same chunk geometry as the forward draw. `SceneGpu::terrain_caster_selection` keeps the cascade's frustum and slice hull test in one place for the draw and the captures. A `vertex_only_program_in` that lends the engine's frame group would remove the four sets; the runtime has no such constructor yet.
- **Fixed pipelines.** `ScenePipelines` still holds the fixed terrain pipelines for a context whose screen group the program does not lend, built on first use, and no context the crate builds is one. `renderer/legacy_terrain/` (a test-only module directory) is a frozen copy of the whole fixed path for the byte-equality captures in `terrain/capture_tests` (plain, pages, layered and layered pages across the twelve forward target sets and the six single-sample ones again with shared depth and with depth reuse, where the depth and stencil planes are compared too; the gather receivers, shadow receivers, specialization, and streaming residency; the metadata prepass in both outputs with independent, shared and winner-marking depth; and the six capture terms in the plain, shadowed, ambient and reflection replays; the shadow caster's depth texels in four cascades over the whole terrain with and without a depth bias, and over streamed chunks with the cascades' volumes and a slice hull). Each capture also asserts which path recorded it: the runtime recorded the terrain and the fixed pipelines stayed unbuilt.

## Player offscreen operations

`SceneGpu::admit_offscreen` carries an `AdmittedOffscreen` from `worldrunner-render-program` into the next frame encoded with this scene: camera renders into render textures, `Graphics.Blit` material passes and texture copies, already checked by the render-post executor's contract as `TypedOperation`s. `compositor::encode_scene` takes them after it has checked the admitted draws and before it declares the stock passes, so they are the first commands the frame records in the same encoder and so the same submission. They are encoded through the frame's pass timer, so with timing configured each one is a `PassId::Operation` at the head of `FrameTiming::passes`, ahead of the stock opaque pass. No second scheduler was added: the scene's only change is where it calls `AdmittedOffscreen::encode`.

A draw admitted with `admit_programs` can sample a render texture that the offscreen operations write, in the stock opaque pass. The host names that read when it builds the offscreen frame (`OffscreenFrame::later_reads`), and the executor checks it as a sampled read of the version the frame leaves, so a read before any write or of a range a later operation overwrites fails at admission by name, before anything is recorded.

- The resident path (`encode_resident`) and the shadowed path refuse a scene that holds admitted offscreen operations by name, as they do for admitted draws.
- The offscreen operations are taken whether or not the frame is accepted, so a refused frame leaves none for the next. An arena that retired or belongs to another submission authority fails the frame by name.
- A cancelled frame submits nothing; the render textures keep what they held and the arena can retire.

The test is `src/draw/tests/program_frame/offscreen.rs`: a camera renders two of three renderers (selected by layer) over a clear color, a blit inverts the result, a copy puts it in element 2 of a texture array and an admitted scene draw samples that element in the opaque pass, with a readback after each stage, exactly one submission and the offscreen passes listed in the frame's timing. See the [program crate](../render-program/README.md#offscreen-work-through-the-checked-operations).

## Material sampling

Mesh and impostor scalar selectors use direct component indexing with uniform integers,
without selector-dependent fragment branches or additional pipelines. The shared
material uniform is 576 bytes: its first 448 bytes retain their layout, followed
by four 16-byte surface index slots and four packed-atlas index slots. Each slot's
first signed integer is an offset from the default component; the rest is padding.
Default uniform tails stay zero, and selection performs no arithmetic on texels. Forward and material metadata sampling share these selectors. See the
[material channel contract](../render-plan/README.md#material-scalar-channels)
for defaults, map roles and typed admission errors.

DDS DXGI formats 98 and 99 upload as BC7 UNORM and BC7 sRGB without CPU decoding,
including supplied mip tails. Layered terrain source arrays accept both BC7
formats. BC7 requires the same `TEXTURE_COMPRESSION_BC` feature as BC1/3/4/5;
colour-space disagreements and truncated block payloads are rejected.

Resident mesh and impostor materials share one sampler across all channels.
`SceneMaterial::with_anisotropy` selects 1, 2, 4, 8 or 16, defaulting to 1;
all levels use linear texel/mip filtering and repeat wrapping. Texture-level
filter/wrap settings affect dynamic quads and sprites only, whose anisotropic
sampling is outside this implementation's scope. Samplers are cached by material
anisotropy, independently of texture handles. wgpu clamps the effective level to
the adapter's supported capability. Layered terrain sources use their own validated
`TerrainLayeredSource::anisotropy` level. Above 1, portable explicit-level taps
average the major-axis footprint at `log2(max(major / taps, minor))`, with the tap
count bounded by the footprint ratio and authored level. Hardware sampler
anisotropy stays 1, preventing backend-dependent double filtering. The 1x path
preserves its isotropic bytes. Cached-page reads share the principal-axis geometry
and authored level, using at most 16 manual trilinear taps with independent
gutter-safe page lookup. See the
[terrain source contract](../../dev-docs/rendering/terrain-pages.md#portable-source-anisotropy) and the
[sampling contract](../../dev-docs/rendering/2d.md#texture-sampling).

`FrameUniform::material_mip_bias` (`lighting_options.w`, bytes 316 to 320) is the mip
bias of the scene's texture sampling, and `FrameRenderer::jitter_frame` sets it from
`HdrPostSettings::material_mip_bias`, which returns exactly `+0.0` in every mode, so no bias is
active and every frame keeps its bytes. The sites that use the lane are the explicit-gradient
material, impostor and relief samples, decal and terrain gradients and layered
terrain gradients; tests force a bias to prove that each selects the finer level. Explicit LOD 0
samples, control fields, environment roughness LOD, water normals and the sky are unbiased, and
shadow caster frames carry exactly 0. See
[docs/rendering.md](../../docs/rendering.md#material-mip-bias) for the measurement on two devices,
the decision and what remains before a policy can be enabled.

## Masked alpha-to-coverage

Masked geometry filters albedo alpha and applies its material factor once.
Forward and metadata passes use that alpha for coverage; shadows use their
shadow cutoff. Flat impostors support varying atlas alpha without a relief
atlas. Atlas footprints are computed in uniform control flow, before
fragment-dependent branches or returns. Optional maps are sampled only when their
material feature is enabled. A material-uniform gate skips atlas coordinates and
derivatives entirely for ordinary materials, and relief depth is sampled only
when relief is enabled. Relief fragments reconstruct their position once and share
it between depth output and surface shading in forward and metadata passes. Sampling
uses mutually exclusive explicit-gradient and explicit-level-zero branches according
to the atlas policy. Gradients include the material mip bias; clamped UVs and
per-texture insets determine each footprint. CPU Naga-IR tests bound reconstruction
calls and require blended level-zero samples to exclude the gradient branch.
Forward cards filter alpha and capture its width, auxiliary footprints, normal frames,
decal position gradients and shadow gradients before rejecting fragments. Shadow
preparation uses the decal-modified normal and its opposite for transmission. Under
alpha-to-coverage, only zero computed coverage is rejected; below-cutoff or raw zero
alpha can still own samples. Survivors sample AO, emission, shadows and environment
and evaluate lighting. Card term captures reuse the prepared shadow inputs for
sun visibility; every term assembly is covered by the forward derivative guard.
Ordinary materials retain their shading path.
Metadata captures atlas footprints, normal frames and decal gradients before alpha,
distance-fade and relief-visibility rejection. Only survivors sample normal/surface
maps and project decals. Samples after either rejection use explicit gradients or
levels. Relief position is reconstructed once and reused by shading and depth output.
Normal, surface-channel and decal evaluation expand from shared source bodies.

Surface and impostor materials specialize `SCENE_IMPOSTOR` independently of alpha
coverage. Surface variants exclude atlas selection and relief sampling at compilation,
including metadata and shadow passes. Mip-zero atlas policies skip atlas UV gradients;
mip-enabled policies retain the clamped, biased footprint before varying control flow.

A CPU Naga-IR test guards the atlas/coverage derivative helpers, normal frames
and their transitive callers in forward, metadata, prepass and shadow shaders.
It requires derivative-sensitive operations before divergent control flow and
recognizes conditions composed solely of uniforms and constants. Explicit-gradient
and explicit-level samples can remain inside fragment-dependent gates. A separate
IR call-tree cost check specializes `SCENE_IMPOSTOR=false` with unconstrained material
lanes, and also checks ordinary material uniforms in the shared source. Both checks
reject reachable atlas selection, packed atlas samples and relief samples for opaque
and masked entry points in every pass. These checks are scoped to these paths rather
than general WGSL uniformity or cost validators.
The GPU sampling-cost regression also reports optional opaque-pass timestamp
medians for ordinary opaque and masked caster scenes. The structural assertion
is deterministic; timestamp values are informational and have no pass threshold.

`SceneMaterial::with_alpha_to_coverage(true)` opts a masked material into MSAA
sample coverage. Opaque materials reject this request. It belongs to the material
because the cutoff, filtered albedo alpha, alpha factor and impostor cutoff policy
already live there; different foliage materials can choose it independently.
The default is false, preserving the hard-cutoff shader output and uniform bytes.
Single-sample passes ignore it and retain the hard cutoff, including equality.

For multisampled colour passes, coverage alpha is
`clamp((alpha - cutoff) / max(fwidth(alpha), 0.000001) + 0.5, 0, 1)`.
Alpha includes the material factor after texture filtering. The cutoff is the
half-covered contour, and the screen-space alpha variation sets a one-pixel
transition. This approximates the filtered silhouette rather than integrating
its exact area. The device selects covered samples, so a four-sample white
surface over black resolves to multiples of one quarter. RGB remains unblended.
Hashed LOD and distance fades retain their existing rejection rules.

The same fixed-function sample mask gates colour, depth, velocity and forward
SSR attachments. Depth is written only for covered samples, allowing background
geometry to fill the others regardless of draw order. Relief impostors use their
camera cutoff and corrected depth. Directional shadow casters retain their own
hard cutoff and flat caster geometry.

The depth/normal and reflection-material prepasses are separate single-sample
metadata targets, never the forward depth attachment. SSR receivers are
pixel-centre majority owners at alpha-to-coverage silhouettes, exactly as at
geometric MSAA edges: every lane reads the same receiver depth, normal and
roughness. The masked foreground owns that metadata if and only if
`alpha >= cutoff`, including equality. The positive derivative denominator
preserves this half-coverage boundary even when clamped to epsilon; below-cutoff
results are capped at the preceding float below 0.5 to prevent rounding onto it.
This pixel-centre approximation does not supply per-sample surface identities.
Metadata cannot occlude uncovered forward samples. Terrain uses
albedo alpha as blend height, so terrain coverage is unchanged. Foliage and
clutter drawn as ordinary material geometry use the masked policy above.


## Projected decal channels and opacity

`SceneDecal::color_modifier` supplies nonnegative linear RGB scales and opacity
in millionths. RGB may exceed one; opacity must lie in `0..=1_000_000`.
The default is unit RGB, full opacity and `SceneDecalAlphaMode::Multiply`.
Texture alpha is multiplied by angle fade before the optional alpha test.
Values below the cutoff are rejected; equality passes. `Multiply` then scales
that faded alpha by opacity. `OpacityEdge` instead applies smoothstep between
`max(1 - opacity, 0.0001)` and `min(1.05 - opacity, 1)`, with zero opacity
producing zero weight. Box edge fade and material alpha scale the resulting weight.

The write mask gates albedo, normal and roughness/metalness independently.
RGB scales affect only written albedo; all written channels share the same
weight. Unwritten channels retain the receiver values. `ReceiverWorld` normal
correction operates in world space independently of these controls.

The uniform has a 16-byte header and 160-byte volumes. In `selection.x`, bits
0..1 select receivers, 2..21 hold the alpha cutoff and 22 enables its test.
In `selection.w`, bits 0..7 and 8..15 hold normal and surface layers, 16..18
hold the inverted write mask, 19 selects `ReceiverWorld` and 20 selects `OpacityEdge`.

## Editing resident placements

`SceneGpu::upload` initializes the same instance table used by later edits.
`instance_handles()` initially follows input object order. Handles are opaque,
scene-local and generation-checked. Stale and foreign handles return errors.
`add_instance(object, visible)` references the uploaded mesh/material tables;
invalid resource indices and transforms return errors before mutation.
`instance(handle)` reads current placement and visibility, so callers can save
that state and restore it through ordinary adds or updates.

```rust,no_run
# use worldrunner_render_scene::{draw::SceneGpu, device::GpuContext};
# fn move_one(gpu: &mut SceneGpu, context: &GpuContext) -> Result<(), worldrunner_render_scene::draw::InstanceError> {
let handle = gpu.instance_handles().next().expect("one resident object");
let (object, _) = gpu.instance(handle)?;
let mut transform = *object.transform();
transform.position_millimeters[0] += 100;
gpu.set_instance_transform(handle, transform)?;
let writes = gpu.flush_instances(context.device(), context.queue());
// Record the next draw using the existing scene and pipelines.
# Ok(())
# }
```

Edits coalesce until `flush_instances`, which reports exactly the contiguous
instance ranges written. Unchanged packed data is not rewritten, including
edits reverted within a frame. Visibility and removal only change draw ranges.
Call flush before recording draws; pending instance bytes cause a panic rather
than drawing stale data. Buffer growth copies old GPU bytes and uploads only
changed slots. Meshes, materials, textures and pipelines remain shared.

Batches sort by pipeline, then mesh and material indices. Placements use stable
batch slots; removal leaves a hole, and insertion reuses the lowest free slot.
Surviving placements never reorder. Visible contiguous slots share an instanced
draw; holes split draws. This avoids shifting and rewriting unchanged placements
at the cost of more draws for fragmented visibility. Empty batches retain their
capacity for reuse. This API does not change resource tables.

`ShadowedScene::scene_mut()` exposes the same API and flushes before both passes.
The desktop viewer's `run_with_updates` accepts a fallible per-frame callback
with `&mut SceneGpu` in either rendering mode and flushes edits automatically.


## LOD and per-view visibility

Packed vertices are `geometry::VERTEX_STRIDE` (40) bytes: position, normal, UV,
billboard mode and AO. Packed instances are `geometry::INSTANCE_STRIDE` (80)
bytes: a column-major `mat4` and linear tint. Billboard mode is a per-vertex
attribute, so LOD selection can change billboard policy without rewriting
instance transforms. Custom buffer layouts
must use these constants and the engine packing helpers.

A single-level `SceneMesh::new` needs no distance policy. Multi-level meshes use
`SceneMesh::with_distances(lods, max_distances_millimeters)`, with one positive,
strictly increasing ceiling per level. Missing or unordered ceilings are rejected
by scene construction. These are the same inclusive ceiling units as the authored
`LodLevel` recipe; adapters must explicitly carry them into the owned scene.
The first ceiling at or beyond camera-to-placement-origin distance selects the
level. Beyond the final ceiling the last level stays selected. Instance scale
affects bounds, not selection distance. Each selected level owns its billboard
policy, so switching levels needs no transform upload.

Every recorded object pass rebuilds contiguous visible ranges in reusable storage,
ordered by pipeline, mesh, material and stable placement slot. Hidden placements
are excluded from every view. Removed slots are excluded from selection and
statistics, but traversal still scans all allocated slots, including holes.
Moving a retained placement changes the next pass's bounds and LOD, without
changing its handle.
Bounds enclose all levels, transform scale and rotation, with an orientation-free
sphere for chains containing camera-facing billboards. Camera planes use `0..=1`
clip depth. Terrain keeps its independent draw and LOD policy.

Directional shadow selection uses light-space coverage independently of camera
visibility and shares the main camera's LOD distance and billboard orientation.
Each cascade keeps its fitted XY receiver square and far depth end; its
toward-light end extends to enclose resident visible casters overlapping that
square. Both the depth and receiver projections use the extended range, so
upstream casters survive clipping as well as CPU selection. Every layer converts
its authored bias using the final projection. Depth extension preserves the
world-space offset for fixed XY coverage, including a single explicit box.
Multiple coverage boxes and camera splits use the texel-relative bias described
below.

`SceneGpu::view_stats()` and `shadow_view_stats()` expose the most recently
recorded pass counts. `ShadowedScene` exposes the same methods. Counts include
live placements considered, hidden/outside placements culled, and submissions per
LOD across meshes. They exclude terrain and do not count rasterized pixels.
Drop each returned statistics guard before recording the next draw of its view;
holding it across that draw causes a runtime borrow panic.
Range and counter vectors retain their capacities between frames.

## Cascade coverage

`SceneEffects::shadow_direction()` overrides only the shadow light. Its default
`None` follows the direct sun without changing existing output. The lighting
service prepares the override; cascade fitting, caster depth extension, refresh
scheduling and both interactive and same-frame depth-fitted captures use that
prepared direction. `ShadowedScene::set_effects` updates it without uploading
geometry. Direct shading, sky and celestial bodies still use the direct sun.

The shadow adapter supports one to four layers at the chosen resolution.
`SceneShadowCascades::new` declares light-space XY half-widths in millimetres,
centred on `DirectionalShadowSettings::focus_meters`. Selection uses the smallest
containing box and blends over the final authored fraction of its half-width.
Fixed coverage boxes remain independent of camera motion and share the exact focus,
so strictly increasing extents remain nested. With one declared coverage box the
fit comes directly from the explicit shadow settings.

`SceneShadowCascades::camera_splits` instead declares each slice's far distance
in camera-view millimetres. Distances must increase strictly, exceed camera near
and not exceed camera far. Invalid policies fail before upload or effect updates.
The optional `render-shadow::practical_splits` utility mixes uniform and logarithmic
distances using an explicit fraction; applications convert these to authored units.
Camera slices use a bounding sphere with quantized radius and texel-snapped light
coordinates. The next slice includes the preceding blend band. LOD and billboard
orientation always use the main camera, while visibility and statistics are separate
for each cascade. All receivers share the same receiver-plane-corrected PCF kernel.
Receiver position and normal derivatives are captured before cascade selection or
coverage rejection. Each cascade transforms those displacements and applies the
projection quotient rule before solving the shadow-UV depth gradient. Differencing
rounded atlas coordinates would magnify cancellation at large world offsets.
Paired single-sample frames publish those receiver displacements once in the
metadata prepass. Forward lighting loads them at the raster pixel in both depth
policies. Both producer and consumer retain invariant position outputs even
with independent depth textures. One receiver uses `Rgba32Float` and `Rg32Float`
array textures, 24 bytes per
pixel. Nonzero normal bias with live transmission adds an independently evaluated
opposite-normal receiver in a second array layer, for 48 bytes per pixel. The
producer evaluates `dpdx(P) + dpdx(N) * bias` and its vertical counterpart; it does
not differentiate a rounded biased position or compress the vectors. The cascade
projection and degeneracy rule consume these f32 values unchanged. Unpaired,
multisampled and prepass-free frames evaluate the local derivatives.

`ReceiverGradientReservation` bounds this storage in `RendererConfiguration`.
The default reserves both receivers for dynamic scene admission; `Front` reserves
24 bytes per metadata pixel and `None` excludes paired shadow receivers. The
viewer selects `None` without shadows, `Front` for zero normal bias and `Opposing`
for biased shadows that can gain transmission materials. Textures are created
at the reserved maximum layout when live shadow receivers first need them, and
remain owned by that metadata allocation generation even while receivers are
absent. Front/opposing layout, bias and output changes specialize the producer
and select views of the same backing without allocating replacement textures.
Reconfiguration or resize replaces the metadata generation through its existing
allocation accounting. Admission rejects a layout
that exceeds the reservation, device colour-attachment count and byte limits, or
the complete forward context's fragment sampled-texture limit, before receiver
pipeline creation or drawing. Uncached layered terrain with reflections and a
payload requires 17 fragment textures; the corresponding local-derivative context
requires 15. An unsupported paired context returns `FrameError::UnsupportedEffect`
without switching to local derivatives. Existing unpaired and multisampled
contexts keep their own limits and depth policies.
The device requests up to 64 attachment bytes per sample when supported; merged
normal/material metadata plus both receivers needs 64 bytes and six attachments.

The CPU shadow call-graph guard forbids derivatives and implicit texture samples in
loops or varying control flow. A relief-depth dependency guard compares shared
reconstruction inputs and return paths. These source guards do not establish
cross-pipeline fragment arithmetic invariance on every backend. GPU attachment
differentials check forward colour, metadata, depth, velocity and receiver payloads.
The blend fraction is in millionths; zero disables blending. Active receiver layout
changes take effect at the next encode without replacing resident scene handles.
Coverage values exceeding the adapter's u32 millimetre range are rejected during fit.

`depth_bias` is authored against the unextended explicit settings box: its world
reference offset is `depth_bias * 2 * depth_half_extent_meters`, and its reference
texel width is `2 * half_extent_meters / resolution`. The ratio of these values
defines a bias in texels. Every cascade converts that bias to world distance with
its own XY texel width, then to normalized depth with its final caster-extended
world-to-depth scale. For a single explicit box, the XY texel width equals the
reference width, so its world offset stays `depth_bias * 2 * depth_half_extent_meters`.
Multiple coverage boxes and camera splits retain their texel-relative offsets.
Depth extension alone cannot increase these world-space offsets; changing XY
coverage can. The sampled layer supplies its own value, including both samples
within a blend band. Single-box output can change when caster fitting extends
depth, and projection changes do not promise byte-identical rendering. Normal
bias remains in world metres.

## Explicit HDR frame profile

`FrameRenderer::hdr` compiles a fixed scene-exposure profile into the retained
post executor. Its scene target is `Rgba16Float`; choose one or four samples.
Object, masked and terrain receiver pipelines must use the same colour attachment
and sample count (`ScenePipelines::with_samples` or `ShadowedScene::with_target`).
Shadow depth arrays stay single-sample. `FrameTargets::depth` supplies the matching
camera depth attachment. Resizing builds colour, depth and output bindings before
replacing the live generation; a rejected extent leaves the old generation usable.

Geometry retains unexposed linear radiance, including values above one. Output
first resolves stored colour samples, then applies the scene contract's linear
`exposure_micro / 1_000_000` multiplier. Component-wise Reinhard maps
`x = max(radiance, 0) * exposure` to `x / (1 + x)`. `ToneMap::Filmic` applies
[the fitted ACES curve](../render-post/README.md#stock-hdr-effects)
to the same exposed input. At this output boundary,
negative radiance (including negative infinity) and NaN map to zero. Positive
infinity, including half-float storage overflow, maps to the curve's limit of one.
Zero exposure maps every radiance input to zero, including positive infinity.
WGSL does not guarantee that NaN and infinity survive texture loads or arithmetic.
This policy is verified on Metal and the lavapipe Vulkan CI job, rather than guaranteed by the language.
The final sRGB attachment performs the only output transfer conversion. No gamma
power is applied in WGSL.
The fixed profile survives resize unchanged. Rebuild the profile when authored
exposure changes; mismatched frame exposure returns `FixedExposureMismatch`
before preparation. Optional `SceneHdrPost` settings install adaptive metering,
bloom and FXAA. `set_frame_time` derives the step since the preceding encoded output,
capped at ten seconds after long pauses;
`set_frame_delta` supplies an explicit simulation step. Fixed capture policies
override wall-clock timing. `reset_exposure` resets the ordered history once at the next output.
The shared temporal carry and slot-local published results survive resize. Nonneutral grading remains
rejected, including requests with zero weights. An authored TAA (`SceneAntiAlias::Taa { history_micro, clip_scale_micro }`) maps to `PostResolve::Taa` with `TaaSettings { history_weight, clip_scale }`. MSAA and FXAA may run together.

`FrameProfile::Legacy` retains single-sample sRGB shading and `CopyOutput`, with
no added display curve. Select the HDR profile explicitly in `ViewerOptions`.
The procedural viewer example accepts `--hdr`, with optional `--msaa`, alongside
`--terrain` and `--shadows`.

## Scene-authored SMAA

`SceneAntiAlias::smaa(SceneSmaaPreset::Ultra)` selects the existing
`PostResolve::Smaa(SmaaSettings::ULTRA)` through scene effects, including retained
HDR consumers. `SceneAntiAlias::Smaa(SceneSmaaSettings { .. })` accepts custom
integer settings and `SceneSmaaEdgeDetection::Color` as well as the default luma
edges. The [scene contract](../render-plan/README.md#hdr-post-processing)
lists the quantity bounds.
The executor schedules `smaa_edges`, `smaa_weights` and `smaa_blend` after tone
mapping, before upscaling, with a separate timing slot for each. SMAA requests
no jitter or temporal history. An explicit descriptor jitter remains explicit.
Selecting another resolve in `hdr_post` or the frame descriptor is rejected as
`PostError::UnsupportedProfile("duplicate resolve")`, including zero-weight FXAA.
CMAA2 remains available through the post descriptor, not the scene recipe.

## Temporal antialiasing

The TAA resolve (`PostResolve::Taa`, issue 256) accumulates jittered frames; see the
[rendering guide](../../docs/rendering.md#temporal-antialiasing). Projection jitter and
previous-frame view data are off unless requested or implied by TAA, and byte-identical when off.

Select jitter with `AntiAliasing::with_jitter` on `FrameRenderer::hdr_antialiased`, or
`FrameRenderer::set_projection_jitter` on an existing renderer. `prepare_uniform` then adds
the sub-pixel offset of the next frame to `FrameUniform::view_projection`, the matrix the GPU
rasterizes with, and fills `FrameUniform::temporal` with a `TemporalView`: the unjittered
matrix, the previous submitted frame's unjittered matrix, both jitter offsets in pixels and
whether the history is valid. `TemporalView::to_bytes` is a separate 176-byte block (it also carries the previous published frame's animation clock and camera position; a clock that moves backwards discards the history).
`encode_frame` turns it into the 96-byte `TaaUniform` (reprojection in f64, depth terms read off the unjittered view-projection) that `encode_output`
uploads for the frame slot. Draw into `targets().depth()` while TAA is resident. The 336-byte scene block keeps its layout, so with jitter on only its first sixty-four
bytes change.

Velocity pipelines (`ScenePipelines::with_taa_velocity`) also write the motion of moving instances. While a pass draws them, each instance batch keeps a previous-transform buffer (same layout as the instance buffer, bound as a third vertex buffer at locations 12 to 15 of velocity variants only). Editing a slot copies its last published transform there before the new one is uploaded; a slot untouched for a frame converges to previous equal to current; added or reused slots start equal to current; a cancelled frame changes neither buffer. Billboards and impostors do not yet follow their transform.

`PostResolve::Taa` selects the default jitter sequence when none was requested
(`PostResolve::effective_jitter`); an explicit sequence wins. `set_projection_jitter` returns
`Result<(), PostError>` and refuses `None` while a TAA resolve is resident.

Every pass that reads the scene block (forward, terrain, instanced and clutter, foliage,
impostors, cards, sky, depth and normal prepass, SSAO, SSR, water and decals) sees the jittered
matrix. Shadow casters draw with their own light matrices and carry no jitter, cascades are fitted
from the unjittered `SceneView`, and CPU selection (culling, level of detail, streaming and decal
choice) reads `FrameUniform::selection_view_projection`, which is unjittered.

The previous camera advances only when the frame's transaction is submitted, like the exposure
history. A cancelled or failed preparation changes nothing. The first frame, `reset_temporal_history`
(call it on a camera cut), a resize, a reconfigure, and a change of jitter settings each discard
the previous camera and restart the sequence; a frame prepared before a reset cannot republish
after it. `projection_jitter_index()` reads the next sequence position without advancing it.
The explicit reset also discards pending native TAA and temporal-reconstruction blocks.
For retained instance draws, pair the reset with `SceneGpu::reset_velocity_history()`:
previous transforms then initialize from current placements at the next submission,
including when a preparation was cancelled. `ShadowedScene::reset_camera_history()`
also discards depth-fitting evidence, contraction filters and retained cascade maps.
Exposure adaptation has its separate `reset_exposure()` operation.

Velocity. `ScenePipelines::with_taa_velocity` builds pipelines whose object and terrain draws also
write the wind residual to the `Rg16Float` velocity attachment that `FrameTargets` allocates only
while TAA is resident (enabling TAA with `ConfiguredFrameRenderer::reconfigure` allocates it and
dropping TAA releases it). Every other pipeline drawn into the opaque pass lists the attachment
with an empty write mask (`velocity::masked_targets`), which covers the unlit dynamic quads, the sky
and the celestial composition. The policy table is in `src/velocity.rs`. Wind on objects is produced;
per-instance transforms and billboards do not write velocity yet.

Shadow receivers and SSR. `ShadowedScene::with_taa_velocity` applies the same rewrite to the shadow
receiver object and terrain pipelines; the caster depth pipelines need none. With SSR on, the
reflection forward pipelines (`ssr::forward`) write the residual at `@location(3)` after the three
reflection targets, which costs 3 x 8 + 4 = 28 bytes per sample on an HDR target. `velocity::reflection_velocity`
compares that with the device's `max_color_attachment_bytes_per_sample` (32 by default); when it
does not fit the pipelines keep three targets and the resolve uses the camera reprojection. The SSR
attachments are inserted ahead of the velocity attachment so the locations line up in both cases.
SSAO-only scenes on `ScenePipelines::with_ssao` do not write velocity.

Jitter alone makes the image shimmer. Enable it only with the resolve, or in tests.

## Temporal reactive mask producer (water)

A temporal upscale owns an `Rg8Unorm` reactive and composition mask at the internal extent; its contract,
clear, dilation, consumption and measured bytes are in the
[render-post README](../render-post/README.md#reactive-and-composition-mask). The water surface is the one
producer in this crate. It ships off.

* Switch. `FrameRenderer::set_water_reactive_mask(bool)` (defined in `water/reactive.rs`) defaults to
  `false`, and nothing in the compositor turns it on. With the switch off, or on a frame without a mask, the
  plan, the pass list, the pipelines and the output are those of a renderer without temporal masks
  (`temporal_water_tests`: the plan lists `[Opaque, TransparentWater]` with one feature pass; scene colour
  is `to_bits` equal with the producer on and off; a renderer without a mask is unchanged with the switch on).
* The pass. With the switch on and a mask in `FrameTargets`, the water feature adds a second pass,
  `PassSlot::TransparentWaterMask`, ordered after `TransparentWater` and before `TransparentAlpha`
  (`validate_transparency` still counts one water pass). It redraws the water bodies with the same `vs_water`
  vertex stage (same specialized module text, vertex constants and layouts) and a constant fragment
  `fs_water_reactive` that writes `(0.9, 0, 0, 0)`: reactive 0.9, composition 0. The value is a policy
  constant fixed before measuring; a value derived from the surface animation is out of scope. The only
  colour target is the mask with `Max` blend on colour and alpha. The depth attachment is the water pass's
  depth (`Load`/`Store`) with `LessEqual` and depth writes off, so water hidden behind nearer geometry
  writes nothing. `AttachmentDeclaration` has no read-only flag and the stock contracts build every depth
  attachment writable, so the pass declares the attachment as the water pass does and the pipeline turns
  the writes off; the depth is unchanged. The mask is declared `Load` with retained contents: the host
  clear (`FrameTargets::encode_mask_clear`) is the producer and the only clear, as the
  [producer contract](../render-post/README.md#reactive-and-composition-mask) requires.
* Why a separate pass. The water pass writes four `Rgba16Float` outputs: colour and the three receiver
  outputs (normal, material, environment), 4 x 8 = 32 bytes per sample. That is the WebGPU default
  `max_color_attachment_bytes_per_sample`, and the frame contracts check it
  (`ContractError::ColorAttachmentBytes`). A fifth `Rg8Unorm` output makes 34 bytes and the pipeline fails
  to build ("The total number of bytes per sample in color attachments 34 exceeds the limit 32"). Packing the
  receiver outputs would change the SSR inputs and their tests for a 2 byte saving, and requiring a higher
  device limit is not portable. The separate pass keeps the four outputs and every no-mask pipeline as they
  are. It costs one extra depth-tested draw of the water vertices, and only when a mask exists. The cost has
  not been timed.
* Why it is off. The rule, fixed before the measurement: on the trail region over the last 8 frames,
  `trail_mask <= reset + 0.5 * (trail_zero - reset)`, and the static opaque region at most 1.10 times its
  zero-mask error. Scene: a wall and animated stock water, 24 frames 0.1 s apart, lavapipe, mean absolute
  error in 8 bit sRGB divided by 255, against a 4x supersampled render of the same frame and water time
  (`temporal_water_producer_tests`, which prints the table):

  | Scale | Trail, zero mask | Trail, mask | Trail, reset | Bound | Static, zero | Static, mask | 1.10 x static zero | Trail rule |
  | --- | --- | --- | --- | --- | --- | --- | --- | --- |
  | 0.50 | 0.00618 | 0.00796 | 0.01126 | 0.00872 | 0.00404 | 0.00428 | 0.00444 | holds only vacuously |
  | 0.67 | 0.00477 | 0.00788 | 0.00726 | 0.00601 | 0.00312 | 0.00317 | 0.00343 | fails (0.00788 > 0.00601) |
  | 0.77 | 0.00441 | 0.00678 | 0.00730 | 0.00585 | 0.00313 | 0.00313 | 0.00344 | fails (0.00678 > 0.00585) |

  With the stock water at these steps the accumulating run is already below the reset floor at every scale,
  and the 0.9 mask raises the error on the water above the zero-mask run. The static region stays within
  1.10 times. At 0.5 the bound exceeds the mask error only because the zero-mask error is below the reset
  floor, so that row holds vacuously and shows no gain. Nothing was tuned: the animation step, region, frame
  count and constant are the first values written. No measurement shows the producer improving the output.
  `temporal_water_producer_tests` asserts only finite, positive values and the static bound. The Radeon
  was not run for the water tests.
* Follow-up. Enabling the producer is tracked in the Hubnot issue "Temporal upscaler: turn on the water
  reactive-mask producer once it meets the PR 6 acceptance rule".
* Contract for further producers (alpha, particles). Declare the mask `Load` with the host clear as its
  producer, write with `Max` blend per channel, never clear it. A producer whose pass would exceed the 32
  byte colour attachment limit needs a pass of its own, ordered after the pass whose depth it reads.

## Cascade refresh scheduling

`DirectionalShadowSettings::refresh_policy` defaults to
`CascadeRefreshPolicy::default()`: every cascade renders every submitted frame.
An opt-in policy declares four frame intervals and periodic phases, a maximum
number of ordinary non-near refreshes, a light-angle threshold in radians and
an XY projection-motion threshold in texels. For example, intervals `[1, 2, 4, 4]`,
phases `[0, 0, 1, 3]` and `max_non_near: 1` refresh the near layer each frame
and stagger the three farther layers. Policy validation rejects zero intervals,
out-of-range phases, invalid thresholds and periodic slots that exceed the budget.
Unused cascade slots must also form a valid policy.

Initialization, explicit `invalidate_shadows`, changed settings or cascade
layout, map resolution, target extent, caster membership, light direction beyond
the threshold, depth projection changes and fitted XY motion beyond the texel
threshold force immediate refresh. Forced refreshes bypass the periodic budget.
Depth projection changes are conservative, including orientation changes affecting
the depth row. Camera-split fits retain their existing stable texel snapping.
Each reused layer retains its last rendered matrix and depth bias; receiver
uploads use those values together. Refresh evidence advances only on successful
submission. Submit encoders in preparation order, as required by frame admission.
`cascade_refresh_states()` exposes the last submitted matrices, frames and reasons.

Caster insertion, removal and visibility edits change the membership generation.
Transform animation may remain stale until the next periodic refresh unless it
changes the fitted depth range. Hosts needing immediate animated shadows should
use the default policy or call `invalidate_shadows()` after their edits.
Far-cascade reuse can produce visible temporal stepping and delayed occluder motion.

Call `request_shadow_capture()` before encoding a retained-scene capture to force
all maps fresh. The desktop viewer and same-frame depth-fitting capture do this
automatically. One-shot `capture_scene_shadowed` starts with fresh resources.
The procedural viewer accepts `--shadows --staggered-shadows` for the example
policy above. `ViewerOptions::shadows` also accepts an explicit refresh policy.

The ignored `cascade_refresh_gpu_timing_comparison` test is an informational
probe, run serially with `--include-ignored --nocapture`. It reports a comparison
only when every sampled pass has a resolvable timestamp pair. Missing timestamp
capability or an unavailable sampled pair prints `SKIP` and omits the comparison;
an unavailable pair is never counted as zero or dropped from a partial total.
Device acquisition, encoding and readback errors still fail the probe.

## Submitted geometry statistics

`SceneGpu::statistics()` reads the last submitted main-camera mesh/impostor draw
selection and decal counts without allocating or polling. The transactional
compositor publishes both at submission; cancelled encoders do not publish them.
The immediate `set_frame` API updates decal counts with its CPU frame state.
Counts include each material slot and
LOD transition complement, with instance counts summed across those draws.
These mesh fields exclude shadow, metadata, dynamic quad, terrain, water and sky draws.

`passes` counts CPU-encoded calls and instances for main terrain, transparent
water, background sky (including discs), native screen-space text (`passes.ui`),
and each of four directional shadow cascades. Counts are recorded alongside the draw commands and published only on
submission. A skipped cascade reports zero draws, even when its cached map is
reused. Metadata, water masks and separate celestial mesh passes are excluded.
Submitted `SceneStatistics::effects` records directional-shadow installation,
SSAO installation and enablement, prepass execution, and typed mesh/terrain
receiver bindings. These follow the installed features and encoded binding
paths, independently of pixel coverage. A reused shadow cascade remains installed
and bound even when its per-frame draw count is zero. Raw draws and abandoned
transactions preserve the previous submitted observations. Reflection-only receivers
can bind neutral AO while SSAO is disabled. A depth-fitted frame includes its
completed geometry-prefix prepass; a later frame never inherits that execution.

`terrain_pages` counts exact selected leaf pages: requested, resident in the
frame's draw table, and pending. Ancestor substitutes do not count as exact
residency. Pages uploaded but not used in that frame remain pending for that
observation. Unpaged terrain reports three zero counts. Cancelled preparation
does not replace the submitted snapshot.

Resource counters describe logical authored texture payloads and mesh vertex,
index and transition bytes. They exclude internal resources, terrain, instances,
driver overhead and staging alignment. `take_statistics()` acknowledges initial
resource uploads for this resident generation; later calls retain residency and
draw counts but report zero new resource uploads. Both APIs return owned values.

`SceneSpecularCube::with_preparation(SceneSpecularPreparation)` marks the owned
cube bytes as a source for GGX filtering. The request lives on the texture because
its bytes, format, mip chain and preparation recipe form one resource identity;
`SceneEnvironment` continues to select the resulting cube by its existing handle.
Both RGBA16Float and BC6H unsigned-float sources use `sky::preparation::encode`
on the host device. Cube size, mip count, sample count, convention and source LOD
tail map directly to `EnvironmentPreparation`. The job's one-texel generated LUT
is discarded; the authored scene BRDF table and scene intensity remain unchanged.
Scene-viewer admits source storage, aligned upload staging, integration uniforms
and disposable intermediates as pending-generation reservations. These expire at
publication or cancellation without replacing presentation targets or temporal history.
Filtered cube storage uses the scene resource inventory and stays charged until
its resident retires. Resize and settings-only replacement count that storage once;
overlapping scene residents count separately. Texture statistics report the output
cube dimensions. Queue ordering requires no CPU completion wait. Dropping pending generations cancels publication;
shutdown uses the host's bounded drain. A cube without a request is uploaded
byte-for-byte without filtering or preparation allocations.

Scene texture cubes and half-float BRDF tables use the common texture uploader.
`SceneGpu::environment_maps` references that uploaded storage through
`PreparedEnvironmentMaps`; the compositor water and reflection setters install
those views without filtering or allocating another cube. `set_water_environment` selects only water;
`set_reflection_environment` installs SSR maps and shares them with installed water.
`set_scene_environment` selects Water, Reflections or Both explicitly. Scene-viewer installs these bindings at startup and on
retained settings publication, including removal and consumer changes.

## Native-resolution RCAS

`RendererConfiguration::post.native_rcas` selects an optional native RCAS pass.
`FrameProfile::HdrNativeRcas` exposes the same setting to hosts. It runs only
at equal render and output extents, after the resolve and CAS, and does not
alter upscale RCAS. See [the post contract](../render-post/README.md#native-resolution-rcas)
for stops, filter domains and output rounding.

## Water lighting and normal filtering

Water specializes the shared lighting shader with the scene's direct-diffuse
convention. `LambertianPrepared` uses prepared sun colour without another Fresnel
or pi factor in diffuse. Water sun disks remain per-body controls. Each distinct
lighting convention has its own cached plain and shadowed water pipelines.

`SceneWaterSettings::shadow_response` optionally supplies a validated
`SceneWaterShadowResponse`. `None` and the neutral default preserve shader source,
uniform bytes, pipeline keys and output. `offset` chooses the animated
`ShadingNormal`, planar Z-up `GeometricNormal`, or zero offset (`None`) for the
shared directional-shadow sampler. The diffuse floor is in millionths, bounded
by `0..=1_000_000`. Optional depth relief blends the floored visibility toward one.
With an authored background tint its weight is `water_tint_weights(...).x`, the
existing background blend weight at the original, undistorted camera-axis water
depth. Without tint it uses `clamp(original_distance, 0, 1)`, the existing one-metre
depth ramp used for refraction. The disk receives this visibility before its
luminance limit; `disk_raw_visibility` multiplies by strength-adjusted raw
visibility again after limiting. Both water lighting paths consume the same sampled
visibility and apply the response after sampling. With `filter: ReceiverDefault`,
zero scene normal bias lets all three offsets reuse the shading-normal lookup,
including its receiver-plane derivatives. A specialized neutral response therefore
preserves both raw visibility and output bytes with multisampling and roughness-aware
occlusion. Shared surface lighting takes explicit visibility inputs.

`SceneWaterShadowFilter::WeightedPcf5` selects nine linear `LessEqual` comparisons
covering a weighted five-texel footprint. Each axis uses `[1, 3, 4, 3, 1]`, with
phase-interpolated pairs and total normalization 144. It uses constant receiver-depth
bias and the selected normal offset, without receiver-plane correction. Sampling
clamps taps to the map edge; receivers outside the projection remain fully lit.
Projection, cascade selection, split/coverage blending and strength share the scene
receiver implementation. The choice is per body, including mixed-filter draws.
Water's AO input keeps its shading-normal lookup and scene filter in both lighting
paths. Opaque shading and immutable refraction snapshots are unaffected. The keyed
existing water pipeline uses spare uniform lanes, so allocation sizes and buffer
counts do not change.

`SceneWaterSettings::normal_anisotropy` selects validated 1x, 2x, 4x, 8x or 16x
filtering when `normal_mips` is enabled. Bodies with the same level reuse one
sampler. The default 1x descriptor preserves trilinear repeat sampling; explicit
mip-zero normals retain their float-precision bilinear path. The synthetic water
normal readback test checks minor-axis detail at a grazing angle and compares the
1x bytes with the original sampler descriptor.

`SceneWaterFlowCoordinates::mode` selects seed addressing independently of normal
repetition. `SceneWaterSeedCoordinates::Repeat` is the default and preserves existing
bytes. `EndpointClamped` places the first and last seeds at the domain endpoints,
using `(clamp(t, 0, 1) * (N - 1) + 0.5) / N` per axis before bilinear filtering.
The domain uses the descriptor's integer-millimetre origin and positive extent.
CPU geometry computes domain-relative seed coordinates before narrowing to float32;
only the endpoint specialization adds their vertex buffer and interpolated varying.
Renderer and presentation allocation plans count this buffer at eight bytes per
expanded vertex, with an eight-byte empty placeholder and conservative basin bounds.
Flow maps need at least two texels per axis in this mode; invalid dimensions fail
before GPU allocation. Normal coordinates and samplers remain unchanged. See the
[water contract](../../dev-docs/rendering/scene-materials.md) for sampling and tests.

`SceneWaterSettings::flow_seed_sampling` selects `SceneWaterSeedSampling::Fragment`
(the default) or `Vertex`, independently of Repeat or EndpointClamped addressing.
Vertex requires a flow map. The GPU samples raw unorm RG at each emitted mesh
vertex, then perspective-interpolates those values. Signed decoding, wind shaping
and depth shaping execute in the fragment stage. Both stages reuse the same
addressing and float-precision bilinear weights. Vertex fetches texel centres with
`textureSampleLevel` at explicit mip zero, avoiding quantized filter weights on
downlevel software adapters. Vertex texture reads
are already required by water wind, so no CPU lattice copy is needed.
The vertex variant adds one two-component varying and no persistent allocation;
endpoint coordinates keep their existing buffer and shared sizing helper. The
renderer and device-free presentation plans therefore have identical byte totals
for both sampling stages. Fragment keeps its shader source, bindings, descriptors,
uniform bytes and allocation totals. Synthetic 17×17 GPU tests cover both channels,
clamping, tile corners, adjacent tile edges and perspective interpolation. A 0.6 m
triangle in a 10 m tile differs from fragment sampling by 0.002811 at its centroid.

AO receiver reconstruction is shared by mesh and terrain lighting. The optional depth-aware gather packs source metre depth beside final AO without adding a sampled texture binding. See [receiver contracts](../../dev-docs/rendering/ambient-occlusion.md#full-resolution-receivers).

`SceneStatistics::terrain_geometry` publishes main-camera geometry replacement
observations on the owning composition submission. Absence means no terrain draw;
`Static` identifies a nonstreaming mesh. `Streaming` carries the selector's desired
leaves, including its split and hysteretic merge decisions, alongside admitted
coverage, unresolved replacements, residency and completion. Budget-blocked merges
remain unresolved until admitted, even when resident fine leaves already exceed
the selected detail. Fine leaves retained by hysteresis are selected and complete. Its bounded drawn-leaf
fingerprint includes every sorted sample window. See the
[host statistics contract](../scene-viewer/README.md#retained-frame-statistics)
for counting units and identity encoding. Raw and cancelled draws do not publish.
