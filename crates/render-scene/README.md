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
Opaque and alpha-masked materials, metallic-roughness lighting, exposure,
explicit single-map directional shadows, orthographic views, and sorted unlit
alpha-blended runtime quads are implemented. Water, SSAO, post
processing, anti-aliasing, distance-based mesh LOD selection, and textured-sky
rendering are not implemented by this renderer; their presence in the shared
scene vocabulary is not a rendering guarantee. The desktop viewer additionally
rejects terrain and water before startup.

The tests construct procedural geometry and textures. GPU tests require a real
hardware or software adapter and fail if one cannot be acquired. For a software
Vulkan run on Linux, pass the driver selection explicitly to Cargo, for example:

```sh
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json CARGO_BUILD_JOBS=2 \
  cargo test --locked -p worldrunner-render-scene -- --test-threads=1
```

## Runtime sprite and UI geometry

`SceneView::orthographic` resolves a canonical Z-up camera with a vertical half
extent and explicit near/far distances. `orthographic_with_up` additionally
accepts the camera's canonical up direction for roll. Existing perspective views
keep their contract. Both return the same `SceneView` used by frame uniforms.

Use `SceneGpu::empty` for a runtime-only scene, or attach runtime contributions
to a scene created by `SceneGpu::upload`. Upload a pathless `SceneTexture` table
with `set_dynamic_textures`, then replace the current contribution with
`set_dynamic_quads`. A `DynamicQuad` carries canonical metre-space TRS, a local
XZ rectangle, top-left UV rectangle, linear RGBA tint, optional texture index,
and explicit sorting layer/order. The renderer uses its existing environment
vertex transforms, material layout, attachments and upload helpers. Tint is an
additional instance stream for the unlit pipeline variant; existing material
and mesh/instance buffer ABIs are unchanged.

Quads sort in ascending layer/order with stable submission ties. Adjacent quads
sharing rectangle, UVs and texture are instanced together. They render after
opaque/masked objects and terrain, test the same depth attachment, and do not
write depth. Equal-depth quads composite in painter order. The host owns UI
layout, glyph generation, sorting policy and coordinate admission. The existing
material sampler is linear/repeating; atlas authors should provide padding.
Unlit colors bypass scene lighting and exposure. PNG decoding belongs to the
host; raw sRGB RGBA8 texels can be passed directly as `SceneTexture::Texels`.

Invalid quad data or texture/domain/device limits return `DynamicError` before
replacing resident contributions. A successful texture-table replacement clears
runtime quads; submit quads against the replacement table afterward. GPU device
loss/allocation failure follows wgpu's existing error model.

Use `FrameUniform::unlit` for a runtime-only view. Capture through the existing
`OffscreenTarget::render` or `render_with_clear_color` (linear RGBA clear), and
submit the same `SceneGpu::draw` into desktop attachments. The renderer performs
no source-engine conversion, file lookup, font layout or game-specific fallback.
