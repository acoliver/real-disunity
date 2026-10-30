# Pathless scene renderer

The renderer consumes `RenderScene` and explicit view/device settings. It uploads
owned meshes and textures, retains material pipeline batches, renders optional
terrain and directional shadows, and supports tightly packed RGBA offscreen
readback. It has no asset lookup or file-loading API.

Terrain uses an independently defined separable 3×3 tent filter for coverage and
a normalized material mixture. Reordering a dense palette with the matching grid
remap does not change the rendered surface. Blur radii, tints, and rotations come
from scene inputs; no content-specific blur table is embedded.

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

## Material sampling

Resident mesh and impostor materials share one sampler across all channels.
`SceneMaterial::with_anisotropy` selects 1, 2, 4, 8 or 16, defaulting to 1;
all levels use linear texel/mip filtering and repeat wrapping. Texture-level
filter/wrap settings affect dynamic quads and sprites only, whose anisotropic
sampling is outside this implementation's scope. Samplers are cached by material
anisotropy, independently of texture handles. wgpu clamps the effective level to
the adapter's supported capability. Terrain anisotropy is not implemented for
sources or cached pages and is tracked by
[#347](https://github.com/acoliver/real-disunity/issues/347). See the
[sampling contract](../../dev-docs/rendering/2d.md#texture-sampling).

## Masked alpha-to-coverage

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
The blend fraction is in millionths; zero disables blending. Array layer count
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
uploads for the frame slot. Draw into `targets().depth()` while TAA is resident. The 320-byte scene block keeps its layout, so with jitter on only its first sixty-four
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
after it.

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

## Submitted geometry statistics

`SceneGpu::statistics()` reads the last submitted main-camera mesh/impostor draw
selection and decal counts without allocating or polling. The transactional
compositor publishes both at submission; cancelled encoders do not publish them.
The immediate `set_frame` API updates decal counts with its CPU frame state.
Counts include each material slot and
LOD transition complement, with instance counts summed across those draws.
Shadow, metadata, dynamic quad, terrain, water and sky draws are excluded.

Resource counters describe logical authored texture payloads and mesh vertex,
index and transition bytes. They exclude internal resources, terrain, instances,
driver overhead and staging alignment. `take_statistics()` acknowledges initial
resource uploads for this resident generation; later calls retain residency and
draw counts but report zero new resource uploads. Both APIs return owned values.

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
