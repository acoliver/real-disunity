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
Textured-sky rendering, vignette, TAA, nonneutral contrast and saturation remain
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

Packed-layout migration: vertex stride changes from 32 to 36 bytes and instance
stride from 68 to 64 bytes. Billboard mode moves from instance offset 64,
shader location 7, to vertex offset 32, shader location 8. `pack_instance` now
takes only a transform. Update custom buffer layouts and previously packed bytes
to match.

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

The shadow adapter supports one to four layers at the chosen resolution.
`SceneShadowCascades::new` declares light-space XY half-widths in millimetres,
centred on `DirectionalShadowSettings::focus_meters`. Selection uses the smallest
containing box and blends over the final authored fraction of its half-width.
Fixed coverage boxes remain independent of camera motion and share the exact focus,
so strictly increasing extents remain nested. One declared coverage box retains the
explicit settings fit for existing callers.

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
`x = max(radiance, 0) * exposure` to `x / (1 + x)`. At this output boundary,
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
The shared temporal carry and slot-local published results survive resize. Filmic curves, nonneutral grading and TAA remain
rejected, including requests with zero weights. MSAA and FXAA may run together.

`FrameProfile::Legacy` retains single-sample sRGB shading and `CopyOutput`, with
no added display curve. Select the HDR profile explicitly in `ViewerOptions`.
The procedural viewer example accepts `--hdr`, with optional `--msaa`, alongside
`--terrain` and `--shadows`.

## Cascade refresh scheduling

`DirectionalShadowSettings::refresh_policy` defaults to
`CascadeRefreshPolicy::default()`: every cascade renders every submitted frame.
The default keeps the existing fits, uniform values and pass sequence.
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
