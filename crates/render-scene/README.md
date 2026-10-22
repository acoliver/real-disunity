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
