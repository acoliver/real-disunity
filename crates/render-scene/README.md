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
explicit single-map directional shadows are implemented. Water, SSAO, post
processing, anti-aliasing, distance-based mesh LOD selection, and textured-sky
rendering are not implemented by this renderer; their presence in the shared
scene vocabulary is not a rendering guarantee. Requested SSAO, nonempty post
stacks, and enabled FXAA/TAA return `FrameError::UnsupportedEffect` before upload
or rendering, including requests with zero weights. Omit SSAO with `None` and
select `SceneAntiAlias::Disabled` when no such operation is wanted. These checks
are limited to SSAO, post stages, and anti-aliasing. Contrast and saturation
grading are not applied or rejected; only exposure is implemented. The plain
renderer neither renders nor rejects water requests. The shadow path and desktop
viewer reject water separately, so rejection is not uniform across stock paths.

A scene without an environmental profile renders unlit albedo plus emission,
without lighting or grading. Directional shadows require an environmental
profile. The desktop viewer takes camera intrinsics explicitly in `ViewerOptions`
and additionally rejects terrain and water before startup.

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
capacity for reuse. This API does not change resource tables or select LODs.

`ShadowedScene::scene_mut()` exposes the same API and flushes before both passes.
The desktop viewer's `run_with_updates` accepts a fallible per-frame callback
with `&mut SceneGpu` in either rendering mode and flushes edits automatically.
