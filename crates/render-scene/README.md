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
then descending sampling stride, then the chunk's derived coordinate key. When
smaller leaves are enabled, equal-error stride-one candidates use descending
longest-axis texel span before the coordinate key to distinguish refinement levels.
Exact-error ties therefore have explicit admission priority under constrained
budgets, independent of resident storage and upload-completion history.
Selected leaves are returned in derived-key order.

`TerrainStreamingPolicy::chunk_intervals` caps cells per mesh axis. Its
`minimum_leaf_texels` independently limits quadtree refinement and defaults to
that cap. `with_minimum_leaf_texels` accepts a positive power of two no larger
than the cap. On square power-of-two grids, a leaf of N texels has
`min(chunk_intervals, N)` cells per side. With the default minimum, the selector's
cell-axis step is the sampling stride, including below-cap roots and partial
terminal windows. When smaller leaves are enabled, below-cap sampling stride stays
one while the nominal cell-axis step is `N / chunk_intervals` in floating point.
This keeps that LOD metric continuous instead of quantizing it to a source texel.
It approximates projected footprint, not height residuals or the actual finest
mesh spacing. Eye-plane intersections still request finest
detail through infinite error; ordinary in-front minimum leaves have finite
error. A split is refused if halving a stride-one window's longest axis would
cross the minimum; rectangular or partial windows can have shorter other axes.
Camera selection, bounds, readiness, triangle-plane descriptors and shadow
selection share that split rule. Chunk keys keep inclusive coordinates and a
positive sampling stride, so differently sized stride-one leaves remain distinct.
Atomic replacement and upload/resident budgets count chunks regardless of size.
Double-sided skirts on every mesh edge reach the terrain-wide minimum minus one
metre and cover cracks at unequal strides without neighbour-dependent stitching.

Streaming state owns a `TerrainHeightBounds` cache built once per terrain source
and streaming policy. Terminal ranges scan all inclusive source samples; parents
union their children's ranges. Camera selection projects each node's own surface
height range. CPU selector callers construct and reuse the cache with the same
terrain source and policy they pass to `select` and `error`.

Camera terrain submissions use the shared mesh frustum plane test against each
resident leaf's cached height range and the terrain-wide skirt bottom, padded
outward for floating-point rounding. Culling only filters recorded draws:
selection, residency, mesh builds and uploads still cover the whole grid. Each
camera pass uses its own raster matrix, including projection jitter. Shadow
cascades use terrain-wide height ranges for their footprint and geometry bounds,
with their own light-frustum and slice-hull selection; they never use the camera's
culled subset. Nonstreaming terrain stays unculled.

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

`texture::release_texture_sources` converts a `SceneResources` table into a new
table whose textures are `SceneTexture::Released` descriptors. It reads each source
through the preparation parser, including texel colour spaces, `DDS` headers,
`DXGI` formats, colour-space declarations and complete mip chains. Descriptor
admission also requires a valid single-layer, two-dimensional, non-cube DX10
header. Header inspection borrows the bytes and permits trailing container bytes,
which remain part of the logical source length. Descriptor lengths include the
148-byte DDS header and must fit an addressable source allocation. It copies no
payload. The first failing texture stops the conversion
with `TextureReleaseError::Texture` naming its table index and the underlying
`TextureError`; the input is never modified. KTX and opaque encodings cannot be
uploaded and are rejected the same way. `texture::describe_texture` returns the
descriptor of one texture. A released texture fails `prepare_texture`, and so
`PreparedScene::new`, with `TextureError::SourceReleased`. Converting a table that
is already released returns an equal table. The input table and its clones keep
their bytes, so a clone can still be prepared and drawn. The conversion makes no
claim about process memory: reclaiming memory needs every holder of the sources to
drop them. Retained-host release, upload staging and material bind groups do not
use this conversion yet.
Opaque and alpha-masked materials, metallic-roughness lighting, exposure, and
explicit single-map and cascaded directional shadows are implemented. Each recorded
frame retains the census entry for the shadow-map generation it uses: cancellation
releases that hold, while submitted work keeps the generation counted until GPU
completion, even after the scene replaces it. The frame also supports SSAO, water,
SSR, adaptive exposure, bloom and FXAA through validated scene settings. Textured-sky
rendering, vignette,
nonneutral contrast and saturation remain unsupported. Zero-weight unsupported
requests still fail.

`ShadowedScene::encode_term_capture` replays initialized scene/frame state against the
host's `ConfiguredFrameRenderer` with its matching selected slot. It returns
`Result<(), ShadowError>` and requires the encoder of the active prepared transaction. Transaction or map-lease
errors propagate as `ShadowError::Submission` before replay. A capture-only transaction
retains the shadow-map generation independently of the ordinary frame that initialized it.

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

## Device features

`GpuContext` builds every scene device, production and test, from one
`DeviceRequest` (`device.rs` `requirements`). It requires the request's features
(`REQUIRED_FEATURES` for block compression, timestamps under `TimingMode::Required`) and refuses an
adapter without them by name. It enables these optional features wherever the
adapter offers them, and leaves them off elsewhere:

- `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`, so frame validation uses the
  adapter's own sample counts (2x and 8x) and format capabilities.
- `DEPTH32FLOAT_STENCIL8`, the combined depth-stencil frame depth.
- `FLOAT32_FILTERABLE`. wgpu treats `R32Float`, `Rg32Float` and `Rgba32Float` as
  unfilterable on a device without it, whatever the adapter's format flags say,
  so a pass that samples a 32-bit float texture through a linear sampler (for
  example a game pass reading an `R32Float` copy of device depth) binds only on a
  device that enabled it. Such a pass should test
  `device.features().contains(FLOAT32_FILTERABLE)`, not the adapter's flags, and
  refuse by name without it.
- `TIMESTAMP_QUERY` under `TimingMode::Optional`.

It asks for the adapter's limits, capped at what the engine binds, for the array
layers of a texture, the colour bytes per sample (64), the sampled textures per
shader stage (32) and the storage images per shader stage (16: a single-pass
downsampler binds its source and six levels, past wgpu's default of four). The
storage-image count comes from render-gpu's `storage_images_per_stage`, which the
player's headless device asks too. A program that binds more storage images than the
device allows is refused by name when it is prepared (render-program's
`StorageImageLimit { stage, count, limit }`).

`DeviceRequest::shared` adds a caller's `DeviceRequirements` to this
request, each limit at the more demanding of the two values, and a request without
them adds wgpu's defaults. So an adapter offering 4 to 16 storage images per
stage is granted its own count and one offering more is granted 16, while one offering
fewer than wgpu's default of 4 gets no device (`DeviceError::DeviceRefused`, naming
`max_storage_textures_per_shader_stage`). Tests: `src/device/tests.rs` (the request
negotiated against adapters with and without `FLOAT32_FILTERABLE`, the limits the
device gets, and an `R32Float` view bound to a filterable binding with a linear
sampler on the engine device) and `src/device/storage_images_tests.rs` (the
storage-image limit asked of adapters offering 32, 16, 8 and 4, the count granted
through the negotiation for 4 to 2^20 and the refusal for 0 to 3, a kernel of seven
storage images over seven mips of one texture prepared on a no-op device made with
the request, and refused by name where the adapter offers four).

## Texture upload tiers

`PreparedScene::apply_texture_tier` accepts a `TextureTierPolicy`, known adapter
memory, bytes already resident, and an eligibility predicate over scene texture
handles. Call it before upload. The caller must exclude textures whose consumers
require their authored dimensions or explicit mip levels.

The selector drops one global number of top mip levels from eligible textures,
choosing the fewest levels that fit the policy's memory fraction. Reduction keeps
a base of at least 4x4. Cubes and single-level textures keep their authored chains.
The remaining authored bytes upload directly, without resampling. Missing policy
or unknown adapter memory leaves the chains unchanged. If the maximum reduction
still exceeds the budget, that maximum is used and `TierChoice::fits` is false.

`TierChoice` reports dropped levels and eligible authored/kept bytes;
`saved_bytes()` is their difference. Byte estimates use the census arithmetic,
including all cube faces and block-compressed mip levels. Allocations absent from
`other_resident` and driver overhead remain outside this estimate.

## Player program draws

`SceneGpu::admit_programs` carries draws that `worldrunner-render-program` admitted into the next frame encoded with this scene. `FrameEncoding::encode_scene` takes them, and the scene's feature records them in the stock opaque pass after its own opaque geometry. They draw into the same color target and depth buffer, are depth tested against the scene's geometry, and leave through the frame's one submission with the stock resolve, output and capture copies. No pass, slot or entry point was added, and nothing else can reach the pass. `SceneGpu` accepts only draws from `ProgramRuntime::admit_draws`, so the arena, the frame's authority and the target signature were checked before they arrive. See the [program crate](../render-program/README.md#joining-a-renderer-frame) for admission and lifetime.

Encoding checks the draws before it declares anything:

- A scene with temporal reconstruction (velocity or a jittered projection), screen-space passes or multisampling fails with `FrameError::UnsupportedEffect` and the name `player programs with temporal reconstruction`, `... with screen-space passes` or `... with multisampling`.
- Draws prepared for other attachments than the scene's color format and `Depth32Float` fail with `FrameError::FeaturePreparation` for `player programs`, carrying `ProgramError::TargetMismatch`.
- An arena that retired, was dropped or belongs to another submission authority than the frame fails the same way with `LeaseError::Expired` or `LeaseError::ForeignAuthority`.
- Every refusal comes before the encoder records anything for the frame. The checks take the admitted draws, so a frame they refuse leaves none behind for the next frame, and a later admission replaces draws that no frame has taken.

A frame with no admitted draws is unchanged: `without_admitted_programs_the_frame_is_byte_identical_to_the_stock_scene_draw` compares its output bytes with a direct draw of the scene. A cancelled frame submits nothing, publishes no counters and leaves the target as it was.

The tests are in `src/draw/tests/program_frame*`. `player_program_and_scene_gpu_draw_composite_in_one_submitted_frame` draws a red unlit engine card and two patches of a synthetic SPIR-V program in one frame, with explicit `ProgramParameters`, and checks engine-only, player-only and overlapping pixels, an unchanged `submitted_serial` while recording and a serial rise of exactly one. The patches' clip depths lie on either side of the card's depth, which is derived from the projection, so the expected pixels do not depend on the shaders. Run them with `cargo test -p worldrunner-render-scene --lib -- program_frame --test-threads=1`.

## Dynamic quads on the program runtime

`SceneGpu::set_dynamic_quads` and `SceneGpu::set_dynamic_contribution` reject
`DynamicQuad::corner_colors: Some(_)` with `DynamicError::CornerColorsRequireRetainedUi`
before allocating contribution resources, regardless of the quad's `overlay` flag.
Corner colours require the retained UI overlay path.

The overlay also draws render textures a host owns. `UiOverlay::from_presentation` gives each `UiTexture::Target` a slot (`UiOverlay::targets` lists them) and numbers it after the overlay's own textures; it turns `v` over for such a draw, because a render texture's first row is its bottom and a draw's texture rectangle starts at the top. `SceneGpu::set_ui_overlay` takes the host's `UiTarget`s for those ids (the device texture and sampler; `UiTarget::new` refuses by name what a quad cannot sample: not 2D, multisampled, not bound for sampling, depth or stencil, not filterable as float). A slot with no offered texture draws one transparent texel. The view of a target is level 0 of one layer. Offering another texture for an id means staging the overlay again. `src/compositor/overlay_target_gpu_tests.rs` checks the orientation against a stored image, the sRGB and linear formats against the same texel stored as an image, an unbound slot and a rebound texture.

Dynamic quads, native sprites and tilemaps, and UI glyphs routed into the world draw with the scene shader's `vs_unlit` and `fs_unlit`. That material is a `render-program` graphics program over the unchanged scene WGSL, and `ScenePipelines` no longer builds a pipeline for it. The runtime builds the pipeline for the pass it is given, with straight alpha over location zero, `LessEqual` depth without a depth write, no culling and an empty write mask on every other colour attachment, so the velocity and reflection targets of a pass keep what other materials wrote. Plain, SSAO, velocity and SSR pipeline sets, `Rgba8UnormSrgb` and `Rgba16Float`, and any sample count the pipelines were built for take the same material.

The runtime borrows what the scene already owns, by semantic: the slot's 320-byte frame uniform at group 0 binding 0 (the frame slot of the transaction that committed the frame, not a copy), and for each group of adjacent compatible quads its vertex, instance, tint and material buffers, its albedo texture and its sampler. Each group is one instanced draw over `0..instance_count`. The stable `(overlay, sorting_layer, sorting_order, hierarchy_order)` order and adjacent-only batching are unchanged. A group is adjacent quads with the same clip rectangle, texture, corners, rectangle and UV rectangle, so quads with different clip rectangles are separate draws. The scene counts none of these buffers against an allocation set and neither does the runtime: that accounting does not exist for dynamic geometry, and this change adds none.

Staging quads prepares a draw for each frame slot, which can fail and allocate, beside the buffers' upload. `encode_scene` and `ShadowedScene` prepare the quads for a pass the staging pipelines did not describe, if there are any, and retain the staged draws on the frame before the pass opens. Recording is state setting at the position in the pass where the quads draw: objects, terrain, sky and celestial meshes, then quads. The staged quads own what they lent through a `Keepalive`, so replacing or clearing them while a frame that recorded them is unfinished leaves the buffers alive until that frame is cancelled or completed on the GPU.

The scene's pass does not clip: `DynamicQuad::clip_rect` splits draws there and is applied only by the retained-UI overlay. The overlay keeps its own fixed pipeline (`vs_ui_overlay`, `fs_ui_overlay`) over the same staged quad buffers, with a material bind group per draw and a scissor per draw taken from the clip rectangle (clamped to the output, flipped to the top-left origin, skipped when nothing is left). Its draw returns the calls and instances it recorded for the UI pass statistics. `scissor_rect` in `src/dynamic/gpu.rs` holds the scissor's arithmetic. `src/dynamic/capture_tests/clipping.rs` draws partial, disjoint, full, oversized, outside and mixed clip rectangles in every target set through the scene's pass and requires each attachment to equal the same quads drawn without clip rectangles in the same run. It draws the same cases through the overlay's own pipeline and requires the overlay to draw exactly when something is left inside the scissors, to leave the output as the unclipped quads make it when the clip is the whole target, and, for one quad over the whole target, to keep the pixels inside the lower-left rectangle and nothing outside it.

`src/dynamic/capture_tests.rs` draws quads, sprites and glyphs with this path under each target set above with the auxiliary attachments pre-filled, and requires the colour attachment to change and every other attachment to stay as it was. It prints a hash per attachment.


## Terrain on the program runtime

Terrain draws through `render-program` in every forward context: plain, velocity, SSAO and its gather receiver, reflection, and each of them as a shadow receiver, at one or four samples, and in the metadata prepass (the normal output and the merged normal and material output, with the camera uniform lent at set 3), the opaque term replays and the shadow casters. The shader source is the scene's own, byte for byte. Only the machinery moved.

- **Programs.** `terrain::program::Context` holds one program per surface source for a `ScenePipelines`: palette (`Plain`), baked `Pages`, `Layered` sources and `LayeredPages`. A surface is a feature set (`TERRAIN_PAGES`, `TERRAIN_LAYERED`); the composition is `terrain::program::source`. The frame's specular convention and winding are override constants of the fragment stage, so a specialization is a cheap program, not a sibling set of compiled pipelines. Derived bindings at set 1 are the ones the shader reads: seven for `Plain`, four for `Pages` and for `LayeredPages` (the same four, because cached pages replace the source composer), fifteen for `Layered`.
- **Sets the pass owner binds.** The frame group (set 0), the shadow map (set 2) and the screen group (set 3) are lent as `ExternalSet`s over the layouts the scene already uses. The pass binds them once, as it did, and the runtime builds no bind group for them. The terrain's own set 1 is a `PreparedSet` built from the buffers and textures the scene retains, once per terrain and pipeline context, with one per page table for the cached surfaces. A set stays with its context's runtime and material layout.
- **Receiver payload.** Paired single-sample shadow contexts declare the lossless displacement arrays at set 2 bindings 3 and 4 in the lent shadow group. Terrain loads its fragment's payload in every forward depth mode, including page and layered surfaces. The material's prepared set stays at set 1; the pass binds the payload with its shadow map.
- **Depth reuse.** A context's `DepthMode` (independent, shared, prepass or reuse, from the forward depth reuse of single-sample frames with a complete prepass) gives the terrain program its depth format and its `RenderState`: the same `DepthMode::state` the object pipelines use, so reuse terrain tests depth `Equal` without writing it and reads the stencil mark with reference `STENCIL_REFERENCE`, which the pass sets once. Every writing depth mode uses `LessEqual`, and reuse leaves the stencil mark intact so the last surface at equal depth wins in API order. The runtime draws stencil from that state (`render-program` README, "Stencil").
- **Cost.** A frame records one `PreparedPipeline` and a `GeometryDraw` per resident chunk: `2 + 4C` commands for C chunks (`3 + 4C` under a shadow map). A chunk's record of the shared instance buffer is its geometry's instance range rather than a bound slice. Pipelines, sets and chunk geometry are prepared the first time a pass needs them and found in maps afterwards. `encode_scene` and `ShadowedScene` prepare the terrain's pipeline for the pass, the prepass and the forward pass alike, after the frame's pages are prepared and before any pass opens, where a refusal is still an error (`SceneGpu::prepare_terrain`); a term replay does the same before it opens its pass. A raw `SceneGpu::draw` prepares what it finds missing as it records. A streamed chunk's geometry is made when the chunk is uploaded and dropped when it retires, before new uploads are admitted, so the geometry never holds a buffer the streaming budget has stopped counting. A prepared set belongs to the runtime and the program layout of the context that built it, so a scene's terrain keeps one per context that has drawn it: independently built pipelines, and the forward pipelines a renderer replacement builds again, each make their own. Sets and pipelines are kept per context and dropped with the context they were made for, so the pipelines a term replay builds for one frame leave nothing in the caches of a long-lived scene, and frames over the same chunks add nothing to `RuntimeStats`. The runtime is shared by a `ScenePipelines` and every sibling specialized from it, so a terrain pipeline compiles once per context for any number of scenes. `src/terrain/cost_tests.rs` holds these claims. Material pages bake through kernels built once: the scene's pages build their page writer when they are made, and a layered source compiles its composition module once per device and keeps a pipeline per binding layout, so baking a page compiles no shader module (`src/terrain/pages/layered_gpu/kernel_tests.rs`).
- **Shadow caster.** `shadows::terrain_caster::TerrainCaster` is a vertex-only program (`vs_terrain_depth` over the shadow depth source) on `TargetSignature::depth_only(Depth32Float, 1)`, with the caster's state (back faces culled, the configured depth test with writes, and its depth bias, slope included). Each cascade's light frame buffer is the program's set 0, lent as an import and prepared once per buffer, so a scene prepares four sets for each frame slot it draws. `ShadowedScene` prepares them with its other per-frame preparation, and recording is `2 + 4C` commands for C chunks, over the same chunk geometry as the forward draw. `SceneGpu::terrain_caster_selection` keeps the cascade's frustum and slice hull test in one place for the draw and the captures. A `vertex_only_program_in` that lends the engine's frame group would remove the four sets; the runtime has no such constructor yet.
- **No fixed pipelines.** `ScenePipelines` holds no terrain pipeline, bind group layout or bind group: every context's terrain, caster and prepass records through the program runtime, and a context whose screen group the program does not lend fails to build its terrain program rather than drawing another way. The captures in `terrain/capture_tests` draw plain, pages, layered and layered pages across the twelve forward target sets and the six single-sample ones again with shared depth and with depth reuse, where the depth and stencil planes are checked too; the gather receivers, shadow receivers, specialization, and streaming residency; the metadata prepass in both outputs with independent, shared and winner-marking depth; and the six capture terms in the plain, shadowed, ambient and reflection replays; the shadow caster's depth texels in four cascades over the whole terrain with and without a depth bias, and over streamed chunks with the cascades' volumes and a slice hull. Each capture asserts that the runtime recorded the terrain once and prepared a pipeline and set for it.

### Stock geometry for caller-owned terrain passes

`SceneGpu::prepared_terrain_chunks` reads the chunks the stock camera pass would
submit. Call it inside the scene's frame transaction after stock scene preparation
or `SceneGpu::prepare_terrain_geometry`, with the same selection projection and
viewport. The supplied raster projection and camera position give the same
frustum culling and nearest-first order as the stock pass. In an owned capture,
declare `SceneGpu::capture_write_keys` for the prefix if it prepares terrain.
After the prefix submits, its continuation reads the prefix's selection with
the same view without re-preparing terrain or declaring suffix writes for that
read. Every segment of a capture, stock draws included, uses the cover the
prefix prepared, even when a later publication in the prefix replaced the
committed cover (a retained replacement coarsening a frozen source). Camera preparation refuses a scene whose
retained replacement has not finished, and a second view in one recording is a
`StreamingSelectionConflict`, including for a frozen source. A cancelled or
unrelated frame cannot read that preparation. A scene without streamed terrain
returns `None`.

Each draw carries its inclusive height-sample bounds and sampling stride, and its
resident vertex and index buffers (`TerrainChunkDraw::buffers`). Edge
neighbors come from the entire admitted cover, including culled leaves. Their
positive-length overlaps are sorted along the edge and can describe multiple
strides. The caller owns material nodes, material page slots and index-map
coordinates and lifetimes; geometric sampling strides do not describe those.

`terrain::build_terrain_chunk_mesh` packs a chunk from the matching `SceneTerrain`
using the same builder as the stock uploads. It produces scene-metre vertices
in the stock geometry layout, little-endian `u32` indices and double-sided skirts.
Both far endpoints are preserved on partial intervals. Invalid chunk windows or
strides and unaddressable mesh sizes return a typed `TerrainError`.

## Objects on the program runtime

Lit objects, foliage and impostors draw through `render-program` in the forward passes: plain, velocity, SSAO and its gather receiver, reflection, and each of them as a shadow receiver, at one or four samples. The metadata prepass, the opaque term replays and the shadow casters draw them through the runtime too, in their own contexts: `SceneGpu::draw_color`, `draw_prepass`, `draw_opaque_capture` and `draw_casters_prepared` record the objects through the runtime. A caster context draws into a depth-only signature with the caster entry points and the map's depth test and bias, and casts every key as the first key of its family. No pass draws objects with fixed pipelines: a production `ScenePipelines` compiles no fixed object shader module and has no public `get`. Celestial meshes keep their own fixed pipelines (`renderer::celestial_specialization`), built from the same object source. The shader source is the scene's own, unchanged.

- **Programs.** `object::program::Context` holds one program per `PipelineKey`, made on first use. The key's five axes are the features `MASKED`, `DOUBLE_SIDED`, `COVERAGE_MULTISAMPLE`, `IMPOSTOR` and `RELIEF` (`PipelineKey::features`); they pick the fragment entry point (`fs_opaque`, `fs_masked`, `fs_relief_opaque` or `fs_relief_masked`) and the `SCENE_ALPHA_TO_COVERAGE` and `SCENE_IMPOSTOR` overrides, and the key's cull mode and `RasterState::alpha_to_coverage` (multisampled passes only). One parse of the context's source serves the vertex stage and every fragment entry point. The depth and stencil state is `DepthMode::render_state`, the translation terrain shares, so forward depth reuse (equal depth and stencil at one sample) is unchanged.
- **Sets the pass owner binds.** The frame group (set 0, with the previous-frame block in the velocity contexts), the shadow map (set 2) and the screen group (set 3) are lent as `ExternalSet`s over the layouts the scene already uses.
- **Material sets.** Set 1 is the material's own. Where the fixed material bind group is built (`upload`, `append_prepared`, `replace_texture_prepared`) the scene also keeps an `ImportTable` for the material: the 576-byte `object.material` block, the five surface maps, the sampler, the contact heights, and the impostor depth, impostor borders and wind field (the placeholder where a material has none). A `PreparedSet` is made from each table for the objects' runtime of a `ScenePipelines` family (a runtime of its own, beside the terrain's) when the scene is uploaded, or when another family first draws it. The derived layout of set 1 is the eleven bindings of the fixed layout in every key and context, so one set serves all 32 keys.
- **Recording.** `draw/lod_material.rs` walks a view's selected ranges and names the buffers and draws to a `DrawSink`. The test-only `FixedSink` sets each buffer as it is named on pipelines a test built; the runtime sink (`draw/object_runtime.rs`) holds the walk's draws and, when the walk ends, records them grouped by pipeline, material and buffers, each once with `PreparedPipeline::draw_borrowed` over references to the buffers it holds, so a draw allocates nothing, with the previous-instance buffer as the third vertex buffer in the velocity contexts. Change tracking is unchanged: a change of pipeline or material binds the prepared pipeline of the pair of key and material, and a pass adapter leaves out the commands a run repeats (the render pipeline when the new pair shares it, and a vertex or index buffer the walk has not named again), so a pass records no state a run repeats. `render-program` has no way to bind a prepared set apart from the pipeline it was prepared with, so there is one prepared pipeline for each pair of key and material that a scene draws; a pair is cheap, and pairs of one key share one render pipeline.
- **Mesh storage.** `draw/mesh_arena.rs` packs the levels of an upload into shared buffers: one pool of chunks for each vertex format (rigid and skinned) and one for indices, which are all `u32`. A level, and the card complement that repacks the level before it, is a `Span` of a chunk, and a draw reads it through `first_index` and `base_vertex` over the chunk's whole buffer, so consecutive draws of levels in one chunk set their vertex and index buffers once (the pass adapter compares buffers by identity, and the grouped order sorts by chunk, then by the level's first vertex, which is the upload order). A chunk fills up to 64 MiB or the device's buffer limit, and the last chunk of an upload is exactly as large as what it holds, so the memory census (`ResourceClass::Mesh`) counts the same bytes as the budget admission. Each append and replacement packs its own chunks. A chunk is reference counted by the spans that read it and is released with its last level; a replaced level leaves its bytes in a shared chunk unused and never overwrites them, so a pass recorded before the replacement still reads what it recorded. The game seam's imported geometry and the terrain keep buffers of their own.
- **Cost.** `src/object/cost_tests.rs` holds the claims. A frame of eight cards with eight keys records 35 commands (43 under velocity), six materials of one key record one pipeline and seven groups, and a level with two material slots records two draws, two pipelines and three groups (`draw/tests/lod_material.rs`). `SceneGpu::prepare_objects` prepares the pairs a frame draws before any pass opens (`encode_scene` and `ShadowedScene`, for the forward pass only), where a refusal is still an error; a raw `draw` prepares what it finds missing as it records. After the first frame `RuntimeStats` does not move, and a second scene drawn with the same pipelines compiles no pipeline and no module.
- **Tests.** `src/object/capture_tests.rs` draws the object scenes in the twelve forward target sets, the gather receivers and the shadow receivers, and requires the objects to change the attachments they write and the draws the scene selected to use all 32 keys (`SceneGpu::pipeline_keys_drawn` reads the keys off the selection through the walk a pass records). The prepass and term replays require three keys. Those captures draw at 0 ms of the animation clock, where the fixture's periodic wind displaces nothing; `capture_tests/variants.rs` draws the foliage cards again at a quarter period and requires the depth plane to differ from the 0 ms one.

## Player offscreen operations

`SceneGpu::admit_offscreen` carries an `AdmittedOffscreen` from
`worldrunner-render-program` into the next frame encoded with this scene:
camera renders into render textures, `Graphics.Blit` material passes and texture
copies, already checked by the render-post executor's contract as
`TypedOperation`s. `FrameEncoding::encode_scene` and `FrameEncoding::encode_shadowed`
take this work before validating the scene's admitted draws. On an accepted
frame, the same executor records the offscreen operations before the stock passes,
on the frame's encoder and in its submission. With timing configured, each
operation is a `PassId::Operation` at the head of `FrameTiming::passes`, ahead of
the stock opaque pass.

A draw admitted with `admit_programs` can sample a render texture that the offscreen operations write, in the stock opaque pass. The host names that read when it builds the offscreen frame (`OffscreenFrame::later_reads`), and the executor checks it as a sampled read of the version the frame leaves, so a read before any write or of a range a later operation overwrites fails at admission by name, before anything is recorded.

- Shadowed frame encoding accepts admitted offscreen operations but still refuses admitted
  program draws.
- Plain and shadowed frame encoding take the offscreen operations whether or not
  the frame is accepted, so a refused encoding attempt leaves none for the next.
  An arena that retired or belongs to another submission authority fails the frame by name.
- The viewer rejects offscreen work whenever shadow depth fitting is configured,
  including interactive `QuantizedPerBound` fitting. It checks before taking the
  host's pending operations or submitting; see the
  [shared-device contract](../scene-viewer/README.md#a-device-another-user-shares).
- A cancelled frame submits nothing; the render textures keep what they held and the arena can retire.

The test is `src/draw/tests/program_frame/offscreen.rs`: a camera renders two of three renderers (selected by layer) over a clear color, a blit inverts the result, a copy puts it in element 2 of a texture array and an admitted scene draw samples that element in the opaque pass, with a readback after each stage, exactly one submission and the offscreen passes listed in the frame's timing. See the [program crate](../render-program/README.md#offscreen-work-through-the-checked-operations).

## Game-screen composition

`compositor::GameScreenComposite` replaces one authored colour pass with a named
screen texture. Register it on `FrameRenderer` with the screen `ResourceRef`, the
`PassEntry`, and the destination colour format. Each frame plan supplies that
resource as an `AttachmentView`; the pass must have exactly one colour target of
the configured format, one sample and no depth attachment. The screen must be a
single-sample `Rgba8UnormSrgb` texture with `TEXTURE_BINDING` usage. Its sample decodes sRGB to
linear, and the target format performs its normal output conversion. The fullscreen
copy scales to the target extent. It presents the last stored source row at the
top, matching the bottom-first row order of the player screen.

Place the composite at the existing opaque entry and omit the stock opaque feature
from that authored plan. Add UI at a later entry that loads the same target. Plans
without a registered composite retain their existing behavior. A current-frame writer exports the screen through a `FrameOperations` group
positioned before the composite. It must supply the matching retained view. A retained view can also supply the last
submitted screen in a frame without a current writer. Operation, composite and UI
commands use the frame's encoder and submission authority. Unsupported source or
target contracts return a named preparation error; multisampled composite targets
are unsupported.

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
Unpaired and multisampled forward cards filter alpha and capture its width, auxiliary
footprints, normal frames, decal position gradients and shadow gradients before
rejecting fragments. Shadow
preparation uses the decal-modified normal and its opposite for transmission. Under
alpha-to-coverage, only zero computed coverage is rejected; below-cutoff or raw zero
alpha can still own samples. Survivors sample AO, emission, shadows and environment
and evaluate lighting. Card term captures reuse the prepared shadow inputs for
sun visibility; every term assembly is covered by the forward derivative guard.
Ordinary materials retain their shading path.
Paired single-sample receiver consumers prepare every card footprint, cotangent
derivative input and decal displacement before albedo rejection. Tangent-frame
arithmetic stays beside normal sampling to retain its floating-point evaluation.
They defer surface/normal samples,
contact, decals and receiver-shadow preparation until the fragment survives the
unchanged visibility, geometry and alpha predicates. Relief still exports its
reconstructed fragment depth. With exactly zero normal bias and no transmission,
receiver-producing metadata captures position derivatives and rejects coverage before
normal, surface, contact and decal evaluation. Normal-bias and transmission receivers
evaluate mapped normals, decals and receiver derivatives before rejecting quad lanes.
Both signed zeros select position capture. Fragment-stage attachment controls compare
finite, signed-zero and nonfinite normal operands with the normal-dependent expression;
IEEE arithmetic gives NaN for zero multiplied by NaN, so this check exercises the
compiled shader's floating-point behavior on each tested backend. Masked card casters
capture the albedo footprint before geometry/fade rejection, then evaluate the same
blended alpha and shadow cutoff. Multisampled and unpaired forward paths keep their
existing evaluation order.
Metadata without receiver publication captures atlas footprints, normal frames and
decal gradients before alpha, distance-fade and relief-visibility rejection.
Only survivors sample normal/surface
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


## Empty distance fades

Per-view selection omits a placement only when every selected material slot,
including the preceding mesh in a mesh/card handoff, rejects all fragments.
Instance fade bands override impostor bands. The distance reference uses the
uploaded instance matrix, local reference and eye-height clamp, independently
of billboard facing, wind and LOD distance scaling. Selection keys include the
actual f32 shader eye as well as the integer LOD eye.

The CPU bounds the shader's f32 arithmetic, including intermediate rounding,
dot-product reassociation, division, square root and byte truncation. Boundaries
whose rounding could change coverage remain submitted. Only proven zero-coverage
fades are removed: hashed inclusive/strict fades, strict R8 fades, and hashed
complementary fades at a shared lower boundary of one. Inclusive R8 zero and
complementary R8 one remain submitted because threshold equality can survive.
Partial dithering and geometry-only mesh/card coverage are not culled.

Forward, depth, reflection MRTs, velocity and metadata use the same fade rejection.
Shadow casters also use it with the camera eye, not the light position; their
flat relief policy does not disable fading. Caster-volume fitting is unchanged.
`ViewStats::culled` includes empty fades. Submitted `SceneStatistics` draw and
instance counts exclude them; resource residency counts do not change. Scenes
without faded placements retain their existing counts.

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
let writes = gpu.flush_instances(context.device(), context.submission());
// Record the next draw using the existing scene and pipelines.
# Ok(())
# }
```

Edits coalesce until `flush_instances`, which reports exactly the contiguous
instance ranges written. Unchanged packed data is not rewritten, including
edits reverted before their first flush. Repeated encoded flushes in one
transaction compare against the latest staged placement and skin bytes, so
reverting an already-staged edit records the restoring upload. Visibility and
removal only change draw ranges. Call flush before recording draws; pending
instance bytes cause a panic rather than drawing stale data. Buffer growth
copies old GPU bytes and uploads only changed slots. Meshes, materials,
textures and pipelines remain shared.

`SceneGpu::draw` consumes a `render_gpu::submission::RecordingRenderPass` opened
with `RecordingRenderPass::begin(encoder, descriptor)`. The constructor validates
the actual encoder before opening its native pass: an unowned encoder returns
`TransactionError::Missing`, and another encoder inside a recording frame returns
`TransactionError::ForeignEncoder`. The boundary has no native-pass constructor
or reusable mutable pass accessor. `SceneGpu::draw_many` records several scenes
inside that same pass. It validates all scene restrictions, physical instance
readiness and history write permissions before recording the first scene.
Capture write refusal returns `WriteError`; preparation requirements still panic.

Raw colour publishes immutable last-colour transform candidates only when that
pass's owning frame successfully enqueues. Upload-only submissions acknowledge
physical bytes and retain the candidates for a later raw colour owner. Fresh
velocity uploads already initialize current and previous transforms, so they
support raw colour without an extra flush. Pending visible-history work remains
pending through uploads, cancellation, device loss and encoder replacement.
Physical readiness checks acknowledged current bytes and the sparse history
frontier's previous bytes, or writes staged in the same live encoder. Dirty
current data, cancelled growth and unsettled previous bytes require preparation.
Raw colour leaves camera/shadow selections and draw statistics unpublished.

Batches sort by pipeline, then mesh and material indices. Placements use stable
batch slots; removal leaves a hole, and insertion reuses the lowest free slot.
Surviving placements never reorder. Visible contiguous slots share an instanced
draw; holes split draws. This avoids shifting and rewriting unchanged placements
at the cost of more draws for fragmented visibility. Empty batches retain their
capacity for reuse. This API does not change resource tables.

`ShadowedScene::scene_mut()` exposes the same API and flushes before both passes.
The desktop viewer applies the same edits to either rendering mode through
`RetainedUpdate::{Add, Replace, Remove, Objects}` and flushes them when it publishes the generation.


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

`SceneObject::with_shadow_only(true)` keeps a visible placement out of the camera
view (every colour, depth-prefix, contour and compaction pass draws the
camera selection) while the shadow cascades still select it when `casts_shadow`
is true; a shadow-only placement that does not cast draws nothing. It is how a
game that draws a family through its own passes keeps the family's stock shadow,
and it serves shadow-only proxies. LOD, fade and the caster depth fit treat it
like any other caster. Tests: the shadow-only cases in
`src/draw/visibility/caster_tests.rs` and
`gpu_shadow_only_caster_shadows_receivers_without_being_drawn` in
`src/shadows/tests/sampling.rs`.

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
metadata prepass. Forward lighting loads them at the raster pixel in reuse, shared
and independent depth modes. Both producer and consumer retain invariant position
outputs even with independent depth textures. Unwritten texels are not receiver
data: their clear value must not affect any published frame product. Stock reuse
draws consume the prepass winner's depth and stencil ownership. Background, water
and dynamic unlit quads do not evaluate stored receiver functions, including when
they replace the paired surface's final colour or depth. One receiver uses
`Rgba32Float` and `Rg32Float` array textures, 24 bytes per pixel. Nonzero normal bias
with live transmission adds an independently evaluated opposite-normal receiver
in a second array layer, for 48 bytes per pixel.
Mesh and card geometry use hardware position displacements `dP`. Terrain uses
its selected triangle's analytic adjacent-pixel displacement in three dimensions,
sharing the material footprint's plane solve and bounds. Producer and local terrain
receivers use the same solve, including ordinary, reflection, velocity, cached,
layered and paged contexts, multisampling and frames without metadata. Local
terrain displacements are prepared before shading and divergent shadow selection.
Hardware position derivatives can span different triangles in a fragment quad.
The selected-plane definition deliberately changes terrain seam shadow bias and
can change shadow pixels there. Geometry, depth and material sampling are unchanged.
On Metal at 257 by 241, an explicit hardware-`dP` reference with the same bias
arithmetic produced these pixel differences. The seam mask compares the selected
plane of adjacent quad lanes; silhouettes and normal quantization limit the mask.

| Fixture | Cascades | Changed pixels | On quad seams | Off quad seams | Maximum channel delta |
| --- | ---: | ---: | ---: | ---: | ---: |
| Flat terrain | 1 | 0 | 0 | 0 | 0 |
| Flat terrain | 3 | 0 | 0 | 0 | 0 |
| Ridge | 1 | 135 | 130 | 5 | 24 |
| Ridge | 3 | 140 | 138 | 2 | 24 |

The old workload's payload differences are not an image bound. Consumer
comparisons need identical presets and camera poses. Measurements made alongside
other windowed GPU workloads are contended and do not establish a speed change.

Nonzero bias evaluates `dP + bias * hardware_dN`, independently for opposing
normals. It does not differentiate a rounded biased position or compress vectors.
Normal derivatives can still depend on helper-lane behavior, so biased receivers
are not guaranteed deterministic across the historical workload. The analytic
solve retains the relative determinant floor and componentwise displacement cap
of 10,000 metres. At grazing angles these bounds approximate the plane displacement;
componentwise clipping need not remain tangent to the plane.

Zero normal bias, including negative zero, defines the displaced receiver as
exactly `B = P` and its displacements as exactly that geometry's `dP`, without any
normal term. This applies to Position and Normal producers, transmission, and all
local consumers, including unpaired, multisampled and prepass-free frames.
Runtime-uniform bias selects the position and position displacements without
varying control flow before hardware normal derivatives.
WGSL does not guarantee preservation of a zero's sign. On a given backend,
paired Position and Normal producers and local consumers must produce identical
displacement bits, numerically equal to that geometry's dP regardless of zero sign.
Local derivative selects choose integer bit representations before bitcasting back
to f32, so floating-point selection cannot change a zero sign relative to the producer.

A zero bias applies no normal offset. NaNs from multiplying nonfinite normal
operands by zero, and signed-zero results dependent on the normal, are arithmetic
defects rather than intended shadow behavior. Constant UVs without an authored
tangent can produce a nonfinite cotangent frame even in helper lanes. Such a frame
must not contaminate the zero-bias receiver position or displacements; normal-map
lighting itself is outside this receiver contract. The cascade projection and
degeneracy rule consume the resulting f32 displacements unchanged.

Only covered metadata depth winners publish receiver values. Forward consumers use
that pixel's winner under the same coverage and depth/stencil ownership contract.
No receiver derivative depends on a texture load from another invocation. Helper
lanes and fragments that later fail depth can execute loads, but their values do
not contribute to surviving outputs. A GPU control replaces the uncovered clear
values with large signed displacements and requires every downstream attachment
and output byte to remain unchanged in all three depth modes and both layouts.
The fixtures cover ordinary opaque, masked and transmitting surfaces, relief
cards, runtime terrain, water and a visible unpaired quad. Test-only positive
controls route a verified uncovered texel through each live ordinary front/back,
card front/back and terrain consumer into both reflection MRTs, requiring exact
layer-specific sentinels to prove the comparisons detect a bad load.

`ReceiverGradientReservation` bounds this storage in `RendererConfiguration`.
The default reserves both receivers for dynamic scene admission; `Front` reserves
24 bytes per metadata pixel and `None` excludes paired shadow receivers. The
viewer selects `None` without shadows, `Front` for zero normal bias and `Opposing`
for biased shadows that can gain transmission materials. Textures are created
at the reserved maximum layout when live shadow receivers first need them, and
remain owned by that metadata allocation generation even while receivers are
absent. Front/opposing layout, bias, Position/Normal capture and output changes
specialize the producer and select views of the same backing without allocating
replacement textures. Terrain's metadata program carries the selected capture
through palette, page, layered and layered-page composition, with the receiver
outputs included in its target signature. Object metadata uses the same producer
source for all 32 runtime variants, including masked and relief entry points;
relief outputs carry the receiver fields alongside reconstructed depth.
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
Their streamed-terrain control compares, under each forward depth policy, a consumer
that interpolates position derivatives with the stored selected-plane displacements.
Whether interpolated derivatives change with the depth policy is left to the
implementation (Metal: yes, RADV: no), so no test asserts it.
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
and sample count (`ScenePipelines::new` or `ShadowedScene::new` with a `ShadowedTarget`).
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

`FrameProfile::Hdr { samples: 1, tone_map: worldrunner_scene_viewer::ToneMap::Clamp }` retains single-sample sRGB shading and `CopyOutput`, with
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
previous-frame view data are on exactly when the resolve is TAA.

A TAA resolve selects its sequence when the renderer is built
(`PostResolve::effective_jitter`, or the extent-derived sequence under temporal upscaling);
`FrameRenderer::set_projection_jitter` replaces it on an existing renderer. `prepare_uniform` then adds
the sub-pixel offset of the next frame to `FrameUniform::view_projection`, the matrix the GPU
rasterizes with, and fills `FrameUniform::temporal` with a `TemporalView`: the unjittered
matrix, the previous submitted frame's unjittered matrix, both jitter offsets in pixels and
whether the history is valid. `TemporalView::to_bytes` is a separate 176-byte block (it also carries the previous published frame's animation clock and camera position; a clock that moves backwards discards the history).
`encode_frame` turns it into the 96-byte `TaaUniform` (reprojection in f64, depth terms read off the unjittered view-projection) that `encode_output`
uploads for the frame slot. Draw into `targets().depth()` while TAA is resident. The 336-byte scene block keeps its layout, so with jitter on only its first sixty-four
bytes change.

Velocity pipelines (`ScenePipelines::with_taa_velocity`) also write the motion of moving instances. Each instance batch keeps a previous-transform buffer with the instance-buffer layout, bound as a third vertex buffer at locations 12 to 15 of velocity variants only. An edited slot uses its last completed colour transform as previous; added or reused slots start equal to current. After a completed frame, an untouched slot converges to previous equal to current. A submitted metadata prefix may upload current and previous GPU bytes, while its refused colour suffix leaves the CPU history and reset obligations pending for the retry. Cancelling an unsubmitted preparation changes neither GPU buffer. Billboards and impostors do not yet follow their transform.

Object-motion passes reject skinned geometry with `ProgramError::UnsupportedState`,
because it has no previous-pose representation. [The skinning regression](src/draw/tests/skinning.rs)
checks this refusal before drawing.

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
previous transforms then initialize from current placements for the next completed colour
frame. Cancelled preparations and upload-only submissions leave the reset pending.
`ShadowedScene::reset_camera_history()`
also discards depth-fitting evidence, contraction filters and retained cascade maps. A
`ShadowedScene` owns no frame renderer; the host resets the one it passes to `encode_with`
and `render_depth_fitted_host`.
Exposure adaptation has its separate `reset_exposure()` operation.

Velocity. `ScenePipelines::with_taa_velocity` builds pipelines whose object and terrain draws also
write the wind residual to the `Rg16Float` velocity attachment that `FrameTargets` allocates only
while TAA is resident (enabling TAA with `ConfiguredFrameRenderer::reconfigure` allocates it and
dropping TAA releases it). Every other pipeline drawn into the opaque pass lists the attachment
with an empty write mask (`velocity::masked_targets`), which covers the unlit dynamic quads, the sky
and the celestial composition. The policy table is in `src/velocity.rs`. Wind on objects is produced;
per-instance transforms and billboards do not write velocity yet.

Shadow receivers and SSR. A `ShadowedScene` whose `ShadowedTarget` sets `velocity` applies the same rewrite to the shadow
receiver object and terrain pipelines; the caster depth pipelines need none. With SSR on, the
reflection forward pipelines (`ssr::forward`) write the residual at `@location(3)` after the three
reflection targets, which costs 3 x 8 + 4 = 28 bytes per sample on an HDR target. `velocity::reflection_velocity`
compares that with the device's `max_color_attachment_bytes_per_sample` (32 by default); when it
does not fit the pipelines keep three targets and the resolve uses the camera reprojection. The SSR
attachments are inserted ahead of the velocity attachment so the locations line up in both cases.
SSAO-only scenes on `ScenePipelines::with_ssao` do not write velocity.

Jitter alone makes the image shimmer. Enable it only with the resolve, or in tests.

## Temporal reactive mask

A temporal upscale owns an `Rg8Unorm` reactive and composition mask at the internal extent; its contract,
clear, dilation and consumption are in the
[render-post README](../render-post/README.md#reactive-and-composition-mask). No pass in this crate writes
that mask, so the host clear is its only writer and the temporal resolve reads zero.

The water surface has no producer. A producer that redraws the water bodies with a constant reactive value
of 0.9 was measured on lavapipe (64x64 output, 0.67 render scale, 24 frames of stock water, against a 4x
supersampled reference) and raised the mean error on the water texels from 0.00643 to 0.01206, so it was
deleted. The water pass writes four `Rgba16Float` outputs, which is the 32 byte colour attachment limit of a
default device, so any later water producer needs a pass of its own after the water pass.

## Cascade refresh scheduling

`ShadowedScene::set_shadows_enabled` controls directional shadows on the existing
resident scene and defaults to enabled. Disabled frames skip caster passes and
receiver sampling while retaining geometry, placements and cached maps. They do
not advance cascade refresh frames or consume a pending capture request.
Reenabling validates retained maps against caster edits, settings and the next
view before reuse; invalid maps refresh through the existing policy. A capture
requested while disabled runs on the next successfully submitted enabled frame.
The scene-viewer camera shadow policy drives it; `play` sets that policy from the
presented camera's `renderShadows` (see `crates/play/README.md`).

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

`SceneGpu::statistics()` reads the last submitted pass draws and decal counts
without allocating or polling. The transactional compositor publishes both at
submission; cancelled encoders do not publish them. The immediate `set_frame` API
updates decal counts with its CPU frame state.

`passes` counts CPU-encoded calls and instances for main terrain, transparent
water, background sky (including discs), native screen-space text (`passes.ui`),
the main camera's objects in the depth/normal prepass (`passes.prepass_objects`)
and the colour pass (`passes.opaque_objects`), and each of four directional
shadow cascades. Counts are recorded by the code that issues the draw commands
and published only on submission. A skipped cascade reports zero draws, even when
its cached map is reused. Water masks, capture-term replays and separate celestial
mesh passes are excluded.

Object counts are the indexed draws the recorder issued: one per material slot of
each drawn level, plus the preceding level's slots for an LOD transition
complement, with instance counts summed across those draws. A draw counts under
`impostors` when its material is an impostor, otherwise under `meshes`.
`ObjectDraws::recording` says how the pass recorded its selection: `Compacted`
draws each (batch, LOD) group once from the view's gathered instances, `Ranged`
draws each selected instance range because the view's gathered instances did not
hold this selection, and `NotRecorded` means the
pass did not run in that frame. Compaction merges ranges but keeps every material
slot and transition complement, so a compacted count never exceeds the ranged
count of the same selection.
Submitted `SceneStatistics::effects` records directional-shadow installation,
SSAO installation and enablement, prepass execution, and typed mesh/terrain
receiver bindings. These follow the installed features and encoded binding
paths, independently of pixel coverage. A reused shadow cascade remains installed
and bound even when its per-frame draw count is zero. Raw draws and abandoned
transactions preserve the previous submitted observations. Reflection-only and
depth-fitting-only receivers can bind neutral AO while SSAO is disabled. A depth-fitted frame includes its
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

## Composing with the stock scene

A caller outside the crate can draw its own passes around the stock scene and read what the stock scene produced. `compositor::prepare_stock_scene` takes the inputs `FrameEncoding::encode_scene` takes and returns a `StockSceneFeature`, an opaque `PreparedFeature` with `declaration()`, `plan()` and `frame()`. Clone the plan's passes and views, append the caller's own, save the declaration and frame uniform, then take `stock.take_offscreen_operations()`. Keep that owned work alive through encoding and pass `offscreen.as_ref().map(|work| work as &dyn FrameOperations)` as the head of `FrameRenderer::encode_frame`, with `&(uniform, None)` as its input. Borrow the stock feature mutably in a `BorrowedFeature` beside the caller's features for that call. The offscreen work is separate from `plan()` and can be taken only once; the executor records admitted camera, blit and copy work before the stock passes in the same frame. `encode_scene` follows this path with no caller additions. The feature owns the pipelines it selected and holds the scene exclusively until it is dropped, with no borrow of the renderer.

Retaining or taking admitted offscreen work does not execute it. A `None` head in `encode_frame` records the stock draws without refreshing their offscreen inputs. Forward any returned work on the preparation encoder before the stock passes, or abandon the frame. The omission characterizations in `src/draw/tests/program_frame/offscreen/composition/omission.rs` cover both leaving work in the feature and taking it without forwarding; the parent test keeps the forwarded pixel, pass-order and single-submission control.

A frame with screen-space metadata can also split off its depth and normal prepass. `FrameRenderer::stock_depth_prefix` takes the scene and the pipelines the suffix draws with, as `prepare_stock_scene` does, and returns a `StockDepthPrefix`; it refuses a renderer that has no metadata. It decides whether the colour draws write velocity before it plans, so a renderer under TAA lists the velocity target on its first frame as on any later one. `plan()` lists exactly the entries the prefix completes, `PassEntry::new(PassSlot::DepthNormalPrepass)` and `PassEntry::new(PassSlot::DepthPyramid(0))`, with their views. `encode` runs the prefix and appends the caller's own passes, views and features to it before the whole list is validated. A caller feature in a later entry reads the original device depth by declaring the prepass depth attachment of that plan, and reads the lit colour by declaring `scene color`; `PrepareContext::resource` gives it the views and `PrepareContext::completed_in_prefix` tells it which entries the prefix completed. `FeatureTarget::Depth` holds linear metres and is a different resource from the device depth.

`FrameRenderer::capture_texture_keys` and `capture_effect_keys` give the write keys a capture needs when the prefix is submitted alone and the suffix prepared afterwards. When the game has a depth-prefix feature, the prefix also runs the game's passes after the prepass, so a host that submits a `StockDepthPrefix` in its own capture lists the keys `capture_effect_keys` returns for the game's targets in the prefix's write contract as well as the suffix's (see "Game passes after the depth prepass" below). A role stays a label: a read of an earlier resource must be declared and is checked like any other.

Caller attachment bindings include the current view's retained-content permission. Changing that permission on the same texture resolves the bindings again. A `Load` without an earlier producer requires `AttachmentView::with_retained_contents()` on the current frame's view; a previously cached retained view cannot authorize it. Refusal occurs before feature preparation or submission. The two permission transitions are covered in `src/compositor/frozen_tests/retained_permission.rs`.

The public composition regressions in `tests/stock_composition.rs` and its `stock_composition` children cover sampled stock depth and lit colour, prefix completion and retry behavior, and retained operations. The operation test checks one callback across prefix and suffix recording, no submission before the caller submits once, and exact red RGB through a readback copied in that encoder. The retry tests retain their distinct-backing and same-backing load-contract refusal controls; `stock_camera_taa.rs` keeps the warm velocity-to-plain regression.

Instance-buffer uploads acknowledge current and skin bytes independently of the CPU
last-colour transform records. Sparse history candidates publish through a suffix-only CPU
write key after final colour succeeds. A refused suffix retains their previous baseline and
reset obligations. Previous-only settling changes physical content before mirror refresh and
gather; CPU history publication changes neither physical content nor sparse current-write counts.

[The public retained-motion fixture](tests/stock_scene_retry.rs) exercises compacted
instances with TAA and SSR. Its caller-preparation refusal submits nothing and preserves
completed raw and resolved colour, metadata, statistics and temporal state. Its public
`StockDepthPrefix` case submits exactly one prefix before refusing suffix preparation.
That refusal preserves completed colour, confidence, statistics and temporal state while
the prepass coverage and velocity contain the moved geometry. A fresh ordinary retry preserves that motion;
retry and settling products match an uninterrupted control in both cases. Declaring one
scene's resident write keys while encoding a different dirty scene fails capture admission
with an undeclared resident key before submission; both scenes keep their completed products.
The [public prefix-attachment regressions](tests/stock_scene_retry/prefix_identity.rs) complete
an additional caller pass in the stock prefix, then change only its attachment backing or only
its load contract in the same-frame suffix. They require `AttachmentView(resource)` or
`AttachmentContract(entry)`, respectively, before caller preparation or submission, with
completed products, draw statistics and temporal history unchanged.

The [public camera-only TAA regressions](tests/stock_camera_taa.rs) pair resident TAA with the
stock depth prefix and plain SSAO pipelines. They check that the prefix and prepared
stock suffix declare no scene-velocity colour writes, while camera history and jitter advance
across two submitted frames. The retained pipeline-switch case first completes a
velocity-enabled frame, then switches the same scene and renderer to plain pipelines. It
requires both plans to omit velocity writes while the velocity allocation stays resident and
TAA history continues.

## Game passes

A game that draws with its own render-program programs adds frame features through the renderer configuration, with no second renderer or scheduler. `RendererConfiguration::game_passes` is a `GamePasses` inventory of `GameFeatureFactory` entries. The renderer calls each factory's `build` when it allocates a generation and again after every `reconfigure`, and registers the feature it returns through `FrameRenderer::register`. An empty inventory, the default, leaves every frame as the stock frame.

A factory declares its targets with `TargetDeclaration`: a name, a renderable colour format or a block-compressed one, `TargetExtent::Frame` or `TargetExtent::Fixed`, array layers, mips (one unless it declares a chain), and `TargetLifetime::PerFrame` or `Persistent`. A renderable target's extra usage (`with_usage`) must be one its format allows on every device (`guaranteed_format_features`), or the configuration is refused naming the target, the format and the usage, for example `STORAGE_BINDING` on `Rgba8UnormSrgb`. A persistent layer is readable in later frames, and across renderer replacement while its declaration is unchanged, once a *submitted* frame has left contents in it; a cancelled frame leaves the contents and that bookkeeping as they were. A read of a layer nothing wrote is `FrameError::MissingProducer` naming the target at planning.

The targets belong to the renderer. `ConfiguredFrameRenderer::game_targets()` gives the live generation's textures and written layers. A renderer built from a clone of the inventory makes targets of its own, so two renderers never share or rebind a texture. A replacement builds its targets privately from the live ones and keeps every backing whose declaration (every field, with the extent it resolves to) is identical; the renderer holds the new targets only when the whole generation is built, so a replacement that fails, in a factory or anywhere later, leaves the live textures, written layers, generation and budget as they were.

The targets count against the allocation budget. A kept backing is charged once across generations; a backing a replacement makes anew, such as a frame-sized target after a resize, is charged beside the old one until the old generation retires (`AllocationSet::backing_texture`).

A layer is recorded as written when the *last* attachment that wrote that exact texture in a submitted frame stored its contents; a last write that discarded them leaves the layer unwritten. Only the declared texture counts: a stock attachment with the same name is another texture and never marks it, and a pass or read that would stand a declared target in for a same-named attachment of the frame is refused at planning with `FrameError::AttachmentHazard`. A same-frame fitted capture lists each game target's texture and each persistent target's publication in its suffix writes, so a game's passes run inside it, and a capture that fails publishes none of the game's bookkeeping.

`GameTargets::is_written`, `view` and the imports a capture declares answer with the state the frame started with, in every segment of that frame: both segments of one owned capture, or one transaction. A segment of a fitted capture that is submitted first records what it wrote when it is submitted, and the segment after it still plans, loads and imports as the frame's first segment did, so a feature plans the same passes in both and a completed pass keeps the declaration the executor compared it with. A later frame, and a query made outside any frame, sees the published state.

```rust,ignore
let passes = GamePasses::new().with(Arc::new(MyGame::new()));
let mut settings = RendererConfiguration::new(size, format, ToneMap::AcesFitted);
settings.game_passes = passes;
let renderer = ConfiguredFrameRenderer::new(&context, settings, budget)?;

// Inside the feature's `plan`: A writes four targets and the frame depth before Opaque,
// B samples them and writes the frame colour.
let a = GamePassSpec::new(GamePlace::BeforeOpaque, 0)
    .writing(names.map(|n| ColorOutput::Target(ResourceRef::new(n))))
    .with_depth(DepthAccess::ReadWrite);
let b = GamePassSpec::new(GamePlace::BeforeOpaque, 1)
    .writing([ColorOutput::Frame])
    .reading(names.map(ResourceRef::new));
let declared = [targets.plan_pass(&a, passes, views)?, targets.plan_pass(&b, passes, views)?];

// Inside `prepare`: the exact target of the entry, built from the same depth rules as the stock draws.
let target = PassTarget::of(context, entry)?;
let state = target.render_state(raster);   // reuses DepthMode::render_state
let draw = runtime.prepare_imported_draw(&imported, target.signature()?)?;
target.admit("my program", &draw)?;         // a mismatch is refused by name before recording
let camera = frame_camera.current()?;        // the matrices the stock passes use
let chunks = frame_terrain.current()?;       // the streamed chunks this frame admitted
```

A block-compressed format makes a *copy target* with `COPY_DST | TEXTURE_BINDING` usage, of one mip unless it declares a chain (below). The device must have the format's features and the extent must be whole blocks, or the configuration is refused naming the target. Passes may sample a copy target but not render into it (`ColorOutput::Target` of one is `FrameError::AttachmentView`). Its layers are written while the frame prepares: the feature's `plan` calls `GameTargets::plan_prepared(reads, writes, views)` for the layers it writes, and its `prepare` stages a `PreparedDispatch` that encodes the blocks and a `PreparedBlockTransfer` from `ProgramRuntime::prepare_lent_block_transfer` into each layer with `PrepareContext::record_prepared`, declaring the layer it writes; a transfer into another layer than the one declared is `FrameError::PreparedWriteMismatch` naming both, and the frame is refused before anything is staged. The commands run before the frame's first pass, so a game pass of the same frame reads the layers, and a persistent layer counts as written once the frame is submitted, exactly as an attachment's write does; a same-frame capture owns the writes and the bookkeeping like any other game target's. A layer a frame does not compress again is read from the earlier frame. Tests: `tests/game_passes/prepared.rs` and the block cases in `refusals.rs`, `shadowed.rs`, `persistence.rs` and `accounting.rs`.

A copy target may carry a mip chain: `TargetDeclaration::mips(n)` (default 1) makes the texture with `n` levels, each half the one above it and at least one texel; a level smaller than a block, or not a whole number of blocks, is stored as the whole blocks that cover it, as wgpu sizes compressed mips, so only the base extent must be whole blocks. A count of 0 or past the extent's full chain (`1 + floor(log2(max(width, height)))`, checked against the frame extent for `TargetExtent::Frame`) is refused naming the target and the range, and so is a chain on a format that cannot render (a renderable target may carry one too, below). Every mip of every layer is a subresource of its own: `TargetDeclaration::mip(layer, mip)` names it, `plan_prepared` plans and marks it, the frame versions it and refuses a transfer into another mip than the one declared (`PreparedWriteMismatch`), and a submitted frame records it written (`GameTargets::is_mip_written`); a plan of a mip past the chain is `FrameError::AttachmentView`, and a transfer into one is the runtime's `BlockTransferError::MipOutOfRange` at preparation. A pass's read of a layer is a read of every mip of it, which is what the target's `GameTargets::sampled_view(name)` (a 2D array of every layer and mip) samples, so a pass may read a layer only once every mip of it is written, earlier in the frame or by a submitted frame (`GameTargets::is_written(name, layer)` is that test), and a missing mip is `FrameError::MissingProducer` naming it. The backing is one allocation, imported once and published under one key whichever mips a frame writes, and the budget is charged every mip's blocks. Tests: `tests/game_passes/mips.rs` (a BC4 target of three mips, each filled by a dispatch and a transfer and read back at its own level, in an ordinary frame, a shadowed same-frame capture and across frames, and the refusals) and the mip cases in `accounting.rs`.

A renderable colour target may carry a mip chain as well, which a game's passes fill one mip at a time: a depth pyramid or a convolution chain, with pass m rendering into mip m and sampling mip m-1 of the same texture. The declaration is `TargetDeclaration::new(name, format, extent).mips(n)` with a format the device renders to (a chain on any other format, a depth format included, is refused when the renderer is configured, naming the target: `format is not a renderable colour format`), and the allocation is charged every mip.

- *Render into one mip.* `ColorOutput::Target(target.mip(layer, mip))` attaches that single (layer, mip) (`TargetDeclaration::layer(layer)` is mip 0, which is what every single-mip target uses). The pass opens with the attachment view of that one mip and the extent of the mip, `max(1, size >> mip)` on each side, so the viewport and scissor wgpu defaults to are the mip's too; `PassTarget::extent()` reports it for a program that needs the texel size. A mip past the chain is `FrameError::AttachmentView` naming it at planning.
- *Sample one mip.* `GamePassSpec::reading_mips(target.mip_range(layer, 0..2))` (or single `target.mip(layer, mip)` values) declares a read of exactly those subresources, so the frame orders the pass after the writer of each and requires each to have one: an earlier pass of the same frame, the frame's preparation, or kept contents of a persistent target, else `FrameError::MissingProducer` naming the mip. The view to bind is `GameTargets::sampled_mips(name, mips)` (every layer of those mips: a 2D view of a one-layer target, a 2D array of several; a range past the chain is `FrameError::AttachmentView` naming the first mip it cannot cover), which covers different subresources than the attachment. `GamePassSpec::reading` keeps meaning every mip of a layer, and `sampled_view(name)` is its view.
- *One texture, two mips, one pass.* The frame versions, orders and checks every (layer, mip) apart, and wgpu tracks the usage of a texture per subresource, so a pass may sample mip m-1 and render into mip m of one texture. A pass that samples the mip it renders into, directly or through a read of the whole layer, is `FrameError::AttachmentHazard` naming the mip, as it is for any attachment, because wgpu refuses a texture subresource both sampled and rendered into in one pass.
- *Written state.* Each (layer, mip) of a persistent target is recorded written, retained, imported at its version and named in a same-frame capture's write contract exactly as the mips of a copy target are: a frame that renders into some mips leaves the others as they were, and `GameTargets::is_mip_written` and `is_written` report the same truth.

Tests: `tests/game_passes/render_mips.rs` (a two-layer, 4-mip R32Float target on a 16 by 16 extent whose layer 1 is built: one pass per mip reducing 2x2 blocks as the average or the minimum, every mip read back and compared with the CPU's values, a digest pass reading the whole chain, six frames over a persistent target that rewrite only some mips, three shadowed same-frame captures, and the refusals by name), `accounting.rs` (every mip charged, a format that cannot render refused) and the `targets_tests` unit module; the per-subresource frame checks are in render-post's `frame_mip_tests`.


`GamePlace::AfterDepthPrepass` passes run after the depth/normal prepass (see below), `BeforeOpaque` passes immediately before Opaque, `AfterOpaque` before the contour IDs and opaque snapshots, `AfterOpaqueSnapshots` after the opaque snapshot passes and before the water and every other transparent pass, `AfterTransparent` before the reflection and screen-space tail, and `BeforePost` after every other pass of the frame (see "Drawing over the lit frame before post" below). The first game pass to write the frame colour or depth clears it; later game passes and Opaque load what was left, so stock objects compose with the game's output by depth. `PassTarget::of` also works for stock entries (`Opaque`, `DepthNormalPrepass`, `ShadowCasters(n)`) through `PassTarget::in_plan`.

`GameBuild::shadows`, a `FrameShadowHandle`, lends the stock shadow cascades of the frame in flight. The shadowed route publishes them when the frame's cascades are final (after any same-frame depth fit and refresh, before the ordinary features prepare), and only when the renderer has game features. `current()` returns `FrameShadows`: the cascade depth array view the stock casters wrote this frame, the comparison sampler the receivers use with it, and each cascade's `view_projection`, `split_distance` and `depth_bias`, with the selection rule, near distance, view-depth vector, blend, fade, normal bias and shadow strength the stock receivers apply, all from the receiver uniform the frame uploads. It refuses with `FrameInputError::OutsideFrame` when no frame is being prepared, and with `NotPrepared` when the frame in flight published nothing (an unshadowed route, shadows off, or a fit with no receivers); a later frame never receives an earlier frame's cascades. The view and sampler belong to the frame: read them in `prepare` and `draw`, never write through them and never keep them, because the next frame writes the same texture and a renderer replacement frees it. A game pass declares no read of the view: every game place whose feature may read the cascades runs after the shadow casters (except `AfterDepthPrepass`: a depth-prefix feature reads the lent terrain only; shadow, occlusion and environment inputs are `NotPrepared` there, see below), and `GamePassSpec::reading` refuses a stock resource such as `shadow depth` with `FrameError::AttachmentView`. Tests: `src/shadows/tests/game_shadows.rs` (including a scene that weakens the shadows to 0.8), `tests/game_passes/shadow_cascades.rs`.

### Reading the opaque snapshots from a game pass

The frame already copies its lit opaque colour and depth after Opaque for the stock water, reflections and depth-tested overlay. A game that draws its own water, or any forward pass that refracts or fades with depth, samples the same copies from a pass at `GamePlace::AfterOpaqueSnapshots`, which runs after the snapshot passes and before the transparent passes. Three things are involved:

- **The factory declares what it may read.** `GameFeatureFactory::snapshot_reads()` returns a `SnapshotReads { color, linear_depth, device_depth, post_water_depth }` (default none), where `post_water_depth` is a `PostWaterDepth`: `Unread`, `Linear` or `LinearAndDevice`. The renderer reserves the snapshots in its allocation budget, allocates them in all three frame slots when it is built (also after every replacement and resize) and admits their writes to a same-frame capture, whether or not the frame has water, reflections or an overlay. A renderer whose factories declare nothing allocates no snapshot of its own, and its frames are the stock frames. `ConfiguredFrameRenderer::snapshot_statistics()` reports the slots that hold snapshots, the ones that also hold the opaque device depth copy and the ones that hold the post-water device depth copy.
- **The pass declares what it samples.** `GamePassSpec::reading_snapshots(reads)` adds the stock resources `opaque snapshot color`, `opaque snapshot depth`, `opaque snapshot device depth`, `post-water depth` and `post-water device depth` to the pass's reads, which orders it after the snapshot writers through the frame's ordinary validation and asks the frame to record them. A pass that reads more than its factory declared is refused at planning with `FrameError::FeaturePreparation` from `frame snapshots`, and a reader placed before the snapshot passes (`BeforeOpaque`, `AfterOpaque`) is `FrameError::MissingProducer` naming the snapshot. The post-water depth is written after the transparent passes, so only a pass at `GamePlace::AfterTransparent` can read it: a reader at any earlier place, `AfterOpaqueSnapshots` included, is `MissingProducer` naming `post-water depth` (or `post-water device depth`), and so is a pass of the stock depth prefix. This is separate from `GamePassSpec::reading`, which names the game's own targets.
- **`GameBuild::snapshots`, a `FrameSnapshotHandle`, lends the textures.** The renderer publishes them when the frame is planned, before any feature prepares, and only when the frame wrote one. `current()` returns `FrameSnapshots { color, linear_depth, device_depth, post_water_depth, post_water_device_depth }`, each an `Option<SnapshotImage { texture, view }>` present when the frame wrote it. It refuses with `FrameSnapshotError::OutsideFrame` when no frame is being prepared and `NotPrepared` when the frame wrote none; a later frame never receives an earlier frame's textures. Read them in `prepare` and `draw`, never write through them and never keep them: the slot's next frame writes them again.

The snapshots, all at the frame's extent:

| Snapshot | Format | Holds |
| --- | --- | --- |
| `color` | `Rgba16Float` | The lit opaque colour after Opaque and the game passes at `AfterOpaque`, with the samples of a multisampled frame averaged. No transparent pass, water included, has drawn into it. |
| `linear_depth` | `R32Float` | Positive view-space metres along the camera's forward axis, reconstructed from the nearest sample of each pixel. A pixel no opaque surface covers holds the far plane's distance. |
| `device_depth` | `R32Float` | The depth buffer's own values. The engine's depth is not reversed: `0.0` is the near plane, `1.0` is the far plane and the cleared value, and nearer surfaces have smaller values. A multisampled frame keeps the smallest sample of a pixel, the nearest. |
| `post_water_depth` | `R32Float` | The frame's depth buffer after every pass before the game's `AfterTransparent` passes, as positive view-space metres from the nearest sample of each pixel, like `linear_depth`. It is the stock `post-water depth` product, a copy of the frame's own depth at that point, so it holds the depth a game pass wrote at `AfterOpaqueSnapshots` (a game water drawn with depth writes, for one) and the depth the stock water pass writes when the stock passes draw the water. Where no pass wrote depth after the opaque snapshots it equals `linear_depth`. Only a pass at `AfterTransparent` can read it. |
| `post_water_device_depth` | `R32Float` | The same depth as the depth buffer's own values, in the convention of `device_depth`. The stock product is linear metres only, so this is a companion that the same pass writes as a second colour attachment. It is lent when the game declares `PostWaterDepth::LinearAndDevice`, which reads both. |

Both depth formats sample as unfilterable float (`texture_2d<f32>` with `textureLoad`, or a `filterable: false` binding). The device depth is the second colour attachment of the linear depth snapshot pass, so asking for it adds one attachment to a pass the frame already draws and no pass of its own; the other snapshot passes are untouched. The post-water device depth is added the same way to the post-water depth pass, and only a renderer whose game declares it allocates the texture (`"snapshot post-water device depths"` in the allocation budget, three slots of `R32Float`).

The stock frame plans the post-water depth pass (`AfterWaterDepth`) when the stock water draws, when the stock reflections or a depth-tested overlay read it, or when a caller feature or a game pass declares a read of it. With water that a game draws and none of those, the pass is not planned and nothing writes the post-water depth. A game that declares `PostWaterDepth::Linear` or `LinearAndDevice` gets the pass planned and written whoever draws the water, with no change to the stock frame beyond that pass and the second attachment; a frame whose game declares no post-water read plans exactly the passes it did before. The one pass and its views take part in a same-frame capture's write contract, the device depth copy included, so a shadowed capture of a game that reads them is admitted and draws the unshadowed frame's depths. Every route plans through the same forward order, so the place is ordered the same in unshadowed, shadowed, resident and same-frame-capture frames; a frozen plan authors its own order.

```rust,ignore
impl GameFeatureFactory for MyWater {
    fn snapshot_reads(&self) -> SnapshotReads {
        SnapshotReads { color: true, device_depth: true, ..SnapshotReads::NONE }
    }
    // build() keeps `context.snapshots.clone()` in its feature.
}

// Inside `plan`: draw the water after the opaque scene and before the transparents.
let water = GamePassSpec::new(GamePlace::AfterOpaqueSnapshots, 0)
    .writing([ColorOutput::Frame])
    .with_depth(DepthAccess::ReadOnly)
    .reading_snapshots(SnapshotReads { color: true, device_depth: true, ..SnapshotReads::NONE });

// Inside `prepare`: bind the textures of this frame's slot.
let snapshots = self.snapshots.current()?;
let scene_colour = snapshots.color.expect("declared").view;
let scene_depth = snapshots.device_depth.expect("declared").view;
```

Tests: `tests/game_passes/post_water_depth.rs` with the game in `post_water_game.rs` (the post-water depth equals the opaque depth where nothing wrote depth and the depth of a pass at `AfterOpaqueSnapshots` where one did, exact device and metre values; it is produced with the water drawn by the stock passes and by the game, where the stock water's own depth appears only in the first; the device companion is allocated and budgeted only when declared and survives a replacement; three shadowed same-frame captures sample the unshadowed frame's depths; an early reader is `MissingProducer` and an undeclared read is refused by name), `tests/stock_composition/prefix_operations.rs` (a stock depth prefix pass reading any snapshot is `MissingProducer` naming it), `src/draw/tests/stock_plan/post_water_depth.rs` (which frames plan the post-water pass and which attachments it carries), `tests/game_passes/snapshots.rs` (colour that equals the stock opaque frame and is unreached by later draws into the frame, the `AfterOpaque` draws it includes, exact device and linear depth values at a covered and an uncovered pixel, allocation statistics with and without a reading game, the typed refusals), `tests/game_passes/snapshot_capture.rs` (three same-frame shadowed captures with a reading game give the unshadowed frame's pixels and run the readers before the transparent passes), `src/water/device_depth_tests.rs` (the nearest sample of a multisampled pixel) and the forward-order tests in `src/compositor/forward_order.rs`.

A game feature declares which stock surfaces it draws with `GameFeatureFactory::replaces_stock`, which returns a `StockReplacement { terrain_surface, water }` (`StockReplacement::NONE`, `TERRAIN_SURFACE` and `WATER`, combined with `union`). The default replaces nothing. A renderer reads the declarations of every factory in its inventory when it installs them, so the declaration holds for every frame of that generation, and a renderer built from an inventory without the declaring factory draws the stock surfaces again: there is no setting to switch between frames. A frame with game features is planned without the stock plan cache.

`terrain_surface` says the game draws the scene's terrain, which stays resident, streamed and updated in every case. Every stock camera pass then skips the terrain: the opaque colour pass, the depth prepass, the metadata prefixes and the capture terms. The stock shadow casters keep drawing it into the cascades, so a game that draws the camera-visible terrain itself keeps the terrain's shadows in its lit frame, and the retained shadow cascades are the ones of a renderer without the declaration. A renderer whose inventory declares nothing is byte-identical to one built without an inventory. Tests: `tests/game_passes/terrain.rs` and `tests/game_passes/terrain_lender.rs` (a game feature that declares the terrain, with the stock draws back after a rebuild without it), `src/shadows/tests/declared_terrain_routes.rs` (camera depth equals the terrain-free scene on the shadowed frame, and the cascade depth texels equal the stock ones), the retained-cascade test in `src/shadows/tests/refresh.rs` (a declaring game holds the retained maps), and the caster and camera pass captures in `src/terrain/capture_tests/declared.rs`.

A game that draws the terrain itself reads `GameBuild::terrain`, a `FrameTerrainHandle`. Each stock route (unshadowed, shadowed and resident, including the depth prefix and the suffix of a same-frame capture) publishes it after it prepares the frame's terrain, with the finalized frame and raster extent, so `current()` in a feature's `prepare` returns the chunks the stock camera draw would submit, in its order, as `TerrainChunkDraw`s with their geometric neighbours and resident buffers (`TerrainChunkDraw::buffers`, in the stock vertex layout of `geometry::vertex_attributes`). It returns `None` when the scene has no terrain. Otherwise it refuses with a `FrameTerrainError`: `OutsideFrame`, `NotPrepared` when the frame's route prepared no terrain, `NotStreamed` for terrain without a streaming policy, or `View` with the lender's `TerrainChunkViewError` (for example `Unprepared` or `ViewMismatch`). The route records a refusal instead of failing the frame, so a feature that does not read the terrain is unaffected. The depth prefix publishes right after it prepares the terrain geometry, with the raster extent of the screen-space metadata and before any prefix feature prepares, so a feature that prepares in the depth prefix reads the same chunks as the suffix of the frame; the suffix must lend with the same extent, the colour target's. It never returns an earlier frame's chunks. The lent buffers are the renderer's own: record draws from them in the frame that lent them, and do not keep, write or destroy them. Lending works the same without the terrain declaration. `tests/game_passes/terrain_lender.rs` declares the terrain surface and draws the lent chunks with an imported program draw. `src/shadows/tests/game_terrain.rs`, `src/shadows/tests/game_terrain_extent.rs` (the extent rules) and `src/draw/tests/streaming/game_lending.rs` read the lent chunks on the shadowed route, in a same-frame capture and on the resident route; the capture test also checks that the prefix and the suffix publish equal chunk lists and that a renderer without game features publishes nothing. A depth prefix planned from caller metadata of another size than the colour target cannot give the suffix other chunks: the frame's terrain is selected for one raster extent, so the suffix's terrain preparation fails the frame with `FrameError::StreamingSelectionConflict` before it publishes, and a second publication with another extent lends `Err(FrameTerrainError::View(TerrainChunkViewError::ViewMismatch))`. `src/shadows/tests/game_terrain_shadows.rs` runs a game that declares the terrain surface and draws the lent chunks in a same-frame fitted shadowed frame and shades them by sampling the lent cascade depth with the lent comparison sampler: the pixels the game draws are partly shadowed by the stock terrain casters. A same-frame fit sizes the cascade from the depth the stock camera passes write, which under the declaration excludes the terrain, so a frame whose only camera-visible surface is the game's terrain lends `NotPrepared` shadows; that test keeps two thin stock quads in the scene for the fit.

`StockReplacement::WATER` keeps the scene's water resident but takes it out of every stock frame. The water bodies are uploaded, and the water environment and surface inputs are kept and updated, exactly as without the declaration. The renderer registers no stock water feature, so the frame records no stock water pass and no water receivers, and the screen-space reflections get no water inputs, so a frame plans, renders and captures as the same scene without water and no refusal arises from it. The opaque snapshots and the post-water depth are planned only when something other than the water reads them. Every stock water consumer reads one query, `FrameRenderer::stock_water`, which is empty while a game draws the water. A capture's write contract follows the same rule: `capture_effect_keys` lists the opaque snapshot banks only when the stock water, reflections, depth-tested overlay geometry or a game feature that declares a snapshot read draws into them (or a caller feature has already asked for a snapshot), so a renderer whose game draws the water reports the keys of a renderer without water. The geometry-only `FrameRenderer::encode` refuses installed water only while the stock passes draw it; with a game drawing the water it encodes as it does for a scene without water. A host clears the temporal reactive mask before each frame. The stock plan cache is not used for a frame with game features. The two declarations are independent. Tests: `src/draw/tests/stock_plan/water_draws.rs`, `src/compositor/temporal_water_draws_tests.rs`, `src/compositor/temporal_declared_water_tests.rs` (capture keys and `encode`) and `src/shadows/tests/declared_water.rs`.

### Game compute passes

A game runs compute work between the frame's passes in a compute pass of its own, planned like its render passes. `GameComputeSpec::new(place, occurrence)` places it at a `GamePlace` with an occurrence it shares with the game's render passes in that place (a collision is `FrameError::DuplicatePass`); `reading` (every mip of target layers), `reading_mips` (single (layer, mip) subresources), `sampling_frame` (stock products such as the post-water depth) and `reading_snapshots` (snapshots its factory declared) say what it samples, and `writing_mips` the (layer, mip) subresources of declared targets it writes through storage images. `GameTargets::plan_compute_pass(spec, passes, views)`, called from the feature's `plan`, adds the pass (`PassDeclaration::compute_pass`, no attachments) and the views it names, and returns its `FeaturePass`; the frame then orders it with every other pass and validates it with them, so a read needs an earlier writer (`MissingProducer` naming the subresource) and a pass after it may read what it writes, mip by mip. Planning refuses `GamePlace::AfterDepthPrepass` and the depth-prefix feature (`UnsupportedEffect`: the prefix is encoded apart), a write of an undeclared target, layer or mip or of a target declared without `STORAGE_BINDING` (`AttachmentView`), a subresource written twice or read and written (`AttachmentHazard`) and undeclared snapshots (`FeaturePreparation` for "frame snapshots"), planning nothing.

In `prepare` the feature prepares its dispatches with the program runtime, binding target levels with `GameTargets::import_level` (`LevelAccess::Storage` for a level it writes, `Sampled` for one it reads; the import states that level as its range, so one dispatch may read level n-1 and write level n of one texture, or write seven levels through seven storage images), and stages each with `PrepareContext::record_in_compute_pass(spec.entry(), label, access, Box::new(dispatch))`, declaring what it reads and writes (see render-post's "Compute passes" for the staging rules: a dispatch reads what earlier dispatches of the pass wrote without the plan declaring it, and every level the pass declares must be written). The frame records them in one compute pass at the entry's place. A persistent target a compute pass writes counts as written once the frame is submitted, as an attachment's write does. Tests: `src/compositor/game/compute_tests.rs` (ordering between the places around it, producers before and after it, shared occurrences, snapshots and every planning refusal, on a no-op device) and `tests/game_passes/compute.rs` (a render pass at `BeforeOpaque` writes a seed, a compute pass at `AfterOpaque` builds a three-level chain from it, one dispatch per level reading level n-1 and writing level n of one texture, and a render pass at `AfterOpaqueSnapshots` samples every level, all read back exactly in two frames; and one dispatch writing seven levels of a 64 x 64 texture through seven storage images, read back exactly).

### Resources a game's passes use

A game's features own no GPU resource. `GameBuild::resources` is a `GameResources`, one per factory and renderer generation, shared by the factory's depth-prefix and ordinary features and released with the generation. It holds the program runtime the features build programs, sets and draws with (`runtime()`, made with the factory's `GameFeatureFactory::resource_budget`, zero by default) and the buffers, textures and samplers the game declares. `declare_buffer(&GameBufferDeclaration { label, usage, contents })` creates a buffer of the role `usage` names (`Uniform`, `Storage`, `Vertex`, `Index`, each also writable) holding `contents`; `declare_texture(&GameTextureDeclaration { label, format, size, dimension, mip_levels, texels })` creates a sampled texture (`D2`, `D2Array` or `D3`) of `mip_levels` levels holding `texels` or zeros (always zeros for a depth format and for a 3D texture deeper than one slice); `declare_sampler(&wgpu::SamplerDescriptor)` creates a sampler. Buffers and textures are counted in the memory census and in the runtime's ledger (`ProgramRuntime::hold_external`), so one budget bounds the declared resources and what the runtime allocates, in either order (`GameResourceError::Budget`). A texture the device cannot create, because its extent or layers pass the device's limits or it is a 3D texture of a depth format, is a `Declaration` refusal and creates nothing. The ledger counts a texture's bytes as they are (`AllocationSet::external`), so no buffer rule applies to them. The game addresses them by `GameBuffer`, `GameTexture` and `GameSampler` handles, which another store refuses (`Foreign`). It binds them with `import_buffer`, `import_texture` (through the declared view dimension) and `import_sampler`, which return render-program `Import`s, and draws from them with `vertex_input` and `indices` (a buffer of another role is `Usage`). `write(context, buffer, offset, bytes)` stages bytes into a buffer in the frame being prepared, in order with the frame's other uploads; a write outside the buffer or not four-byte aligned is `Write`. Declaring during `prepare` is allowed. `GameTargets::import(name, semantic)` gives a declared target as an `Import` through `GameTargets::sampled_view(name)` (every layer and mip). One level of one layer is imported by handle as a `D2` view of exactly that level: `import_texture_level(semantic, texture, GameLevel { layer, mip }, access)` for a declared texture and `GameTargets::import_level(name, semantic, GameLevel, access)` for a target, where `LevelAccess::Sampled` needs `TEXTURE_BINDING` and `LevelAccess::Storage` needs `STORAGE_BINDING` (a target declared with it; a declared texture is never a storage image). `GameTargets::import_level_array(name, semantic, GameLevel, access)` imports the same level as a one-layer `D2Array` view with the same range, for a binding declared as an array of 2D images (HLSL `RWTexture2DArray`), such as each image of a downsample kernel that binds every level of one chain; its range is that one layer, so it aliases a `D2` import of the same level and binds beside one of another layer (tests: `level_import_tests.rs` `a_level_for_an_array_binding_is_lent_as_a_one_layer_array_view` and `an_array_level_aliases_the_same_level_imported_as_2d_and_no_other_layer`, `tests/game_passes/compute.rs` `a_kernel_writes_levels_of_one_layer_through_images_it_declares_as_arrays`). Another store's texture is `Foreign`; an undeclared target, a 3D texture, a mip or layer outside the texture or target and a missing usage are `GameResourceError::Import`, naming the label and the cause, so a game binds one layer of a splatted page array, or writes one layer of a target from a kernel, without making a view (tests: `src/compositor/game/level_import_tests.rs` for the refusals, `tests/game_passes/level_import.rs` for a kernel writing layer 2 of a four-layer target with the other layers untouched, a pass loading exactly layer 1, mip 1 of a declared array, and a pass loading the layer a kernel wrote in the same frame through a one-layer import). Render-program's `prepare_external_set` builds the group of a set a game pass binds itself.

A declared texture's `texels` hold every level, laid out as `GameTextureDeclaration::levels()` lists them (`GameTextureLevel { layer, mip, width, height, bytes_per_row, rows, offset, len }`): layer after layer, each layer's chain finest first, each level tightly packed rows of texels, or of blocks for a block-compressed format (so a BC level's row is its whole blocks times the block's bytes, and a level smaller than a block is one block). The renderer uploads them through render-gpu's `TextureUpload`, the plan the scene's own textures use, so block-compressed chains and arrays whose layers have mip chains take the same validated path; a volume one slice deep goes through render-gpu's `VolumeUpload`. A declaration is refused by its label (`Declaration`) for an empty or partial-block extent, a mip count the extent does not have, a block-compressed volume, texels that are not exactly the levels, and a format the device cannot sample (named with the features it lacks, for example `TEXTURE_COMPRESSION_BC`). Every level counts against the budget.

`declare_shared_texture(&GameTextureDeclaration)` declares a texture that the other factories of the same renderer generation may declare too, under the same label, for example one height grid read by two features of different factories. The first declaration creates it; a later one, from any factory, returns a handle of its own store to the same texture, and is refused (`Declaration`) if it differs in format, extent, dimension, mip count or texels (by digest). Each factory that declares it counts it against its own budget, as it would if it held it alone, so a factory's budget does not depend on which other factories are installed or in which order: its runtime's ledger holds the texture's bytes once for as long as that factory's store lives, however often the factory declares it. The memory census counts it once. Neither factory has to know the other: the texture exists for as long as any store of the generation holds it.

A game moves blocks its kernels encoded into a declared texture with `GameResources::transfer_block_image(&BlockSource, GameBlockDestination { texture, mip, layer, extent })`: the source is a block image of the runtime (one `Rg32Uint` or `Rgba32Uint` texel per block, as a compressor kernel writes it), the destination a level and layer of a declared block-compressed texture, named by handle (Stranded's terrain pages: BC4/BC5 kernels, then a transfer into the 4-mip page arrays). It reads levels back by handle with `read_back_texture(texture, GameLevel { layer, mip })` and `read_back_target(&targets, name, GameLevel)`, which return a `GameReadback`, and `take_read_back(readback)`, which returns `Ok(None)` until the frame that copied the level was submitted and completed and the host polled the device, then the level once as a render-program `TargetImage { size, format, bytes }`, rows tightly packed (rows of whole blocks for a block-compressed level; any format with a block copy size, so the 8-bit, 16-bit integer, float and BC levels a game dumps). Declared colour and block-compressed textures carry `COPY_SRC` and `COPY_DST`, with or without texels; a target is read back only when declared with `COPY_SRC`. A transfer is refused when asked only for another store's handle (`Foreign`), a destination that is not block-compressed, and a mip or layer outside the texture (`GameResourceError::Transfer`, naming the label and the cause). Its other rules are render-program's `BlockTransferError`s (for example the source's format, a source texel size that is not the destination's block size, a source region that does not hold the destination's blocks, an empty extent or one past the destination mip), and they are checked only when a frame records the transfer: the frame fails with `FrameError::FeaturePreparation` from `game resources`, whose reason names the texture and the rule, the refused transfer is dropped, the work asked after it waits for the next frame, and the next frame renders. A readback is refused when asked for another store's handle (`Foreign`), an undeclared target, a mip or layer outside the texture or target, a format that cannot be copied (`Depth24Plus`) and a target without `COPY_SRC` (`GameResourceError::Readback`, naming the label and the cause). The work is queued in the order asked and recorded into the frame's encoder after the frame's passes on every route that encodes game features, so on a route that encodes the frame in one piece a transfer asked in frame N is sampled from frame N+1, a readback holds what frame N left, and a readback asked after a transfer sees it. A route that submits the depth prefix ahead of the frame (the same-frame fitted capture, the interactive fitted route and `StockDepthPrefix`) also records the queue at the end of the prefix, after the prepass and the game's depth-prefix passes and before the rest of the frame's passes. Everything queued by then, such as work asked when the game was built or by a feature that prepares in the prefix, is recorded there: the frame's later passes sample those transfers, and those readbacks hold the level as it was before the frame's later passes and kernels ran, not what the frame left. Work asked by the features that prepare after the prefix is recorded after the frame's passes. Work recorded into a cancelled frame is recorded again in the next one. The copies are render-program's readback (`read_lent_texture`, `prepare_lent_block_transfer`, retained and recorded at once), so the staging counts against the factory's `resource_budget` with what the store and runtime hold, and the frame holds it until the GPU is done; the map is render-post's `AsyncReadback`, so no second readback path exists. render-post's `Readback` slot, which reads mip 0 of one layer in ten formats, stays the scene-viewer's retained capture slot. Offering game readbacks to a capture as named products is not done. Tests: `tests/game_passes/transfer_readback.rs` (a block moved into layer 1, mip 2 of a BC5 array and read back exactly with its neighbours as uploaded; layer 1 of an `R32Float` target bit for bit after a cancelled frame; BC4 mip 3; `R32Float`, `Rgba16Float`, `Rgba8Unorm`, `R16Uint` and `R8Uint` levels; every level of both layers of 12 x 20 BC1, BC3, BC4, BC5 and BC7 chains, down to 1 x 1; transfers into mips smaller than a block and into levels whose block rows end inside the level; four record-time transfer refusals, each failing its frame naming the texture and the rule, with the next frame rendering; a transfer and its readbacks inside a shadowed same-frame capture; a pass reading a level of the float target no pass wrote is `MissingProducer`) and `src/compositor/game/resource_work_tests.rs` (the refusals).

`GameBuild::scene_textures` is a `FrameSceneTexturesHandle` that lends the scene's resident textures, the ones the stock renderer uploaded from the scene's texture table (with any texture tier the host chose), so a game's material maps are stored once. `current()` returns `FrameSceneTextures`, whose `import(semantic, TextureHandle)` binds one through its default view (`FrameSceneTextureError::Unknown` beyond the table). The table is one shared snapshot until a scene append or texture replacement changes it; `same_snapshot` tells a game when to rebuild what it bound. Every route publishes it before features prepare, beside the lent terrain, and, unlike the frame's products, also inside a depth prefix submitted ahead of the frame, so a depth-prefix feature may read it. Tests: `tests/game_passes/resources/contracts.rs` (writes outside a buffer, a cancelled or refused frame that copies nothing, a replacement and a removed factory that release their bytes and refuse the old handles, the byte count by format and layers, the shared budget, the device limits, the lent textures outside a frame, and a shadowed same-frame capture that rewrites a declared buffer every frame), `tests/game_passes/resources.rs` (two factories draw a declared vertex buffer, a uniform written every frame, a declared texel and a lent scene texel into their targets; a texture replacement makes a new snapshot that the next frame binds; the budget, usage, empty, foreign and misaligned-write refusals), `tests/game_passes/prefix_state.rs` (the textures are lent in a frame drawn whole and inside a same-frame capture's prefix), `tests/game_passes/resource_mips.rs` (two factories sample every level of a BC1 chain, every layer and level of an array chain, a one-texel volume and a texture both declared shared; one texture behind both handles, counted by both; a differing shared declaration and an ASTC format on a device without it are refused by name) and `src/compositor/game/declared_textures_tests.rs` (levels, layouts and refusals on the CPU, and a shared BC chain counted once by the census and once by each declaring factory).

`GameBuild::scene_meshes` is a `FrameSceneMeshesHandle` that lends the resident scene meshes beside the textures, by `MeshHandle`, and `GameBuild::engine` is an `EngineConstants` for the generation. `FrameSceneMeshes` holds one snapshot of the mesh table, shared by every frame until an append or a mesh replacement changes it (`same_snapshot`), and is published wherever `scene_textures` is, including inside a depth prefix. A mesh is a chain of levels: `levels(mesh)`, `index_count(mesh, level)` and `material_slots(mesh, level)` (the authored material partition of the indices, empty when the level takes its placement's material) describe it, `vertex_input(mesh, level)` returns the `ImportedVertexInput` over the retained vertex buffer in the stock layout (`geometry::vertex_attributes`, stride `geometry::VERTEX_STRIDE`, positions in the mesh's own space, no placement), and `indices(mesh, level, range)` returns the `ImportedIndices` (`u32`, base vertex zero) of a range of the level's indices. A handle beyond the table is `FrameSceneMeshError::Unknown`, a level beyond the chain `UnknownLevel`, a range outside the level `IndexRange`, and a skinned level `Skinned`, because its palette is not lent. A separate handle, rather than a field on `FrameSceneTextures`, leaves `FrameSceneTexturesHandle` as it was and lets a feature that draws no scene geometry take only what it reads.

`EngineConstants` holds what the engine makes once per renderer generation and shares with every feature of every factory: `placeholder(semantic, EnginePlaceholder::{White, Black, FlatNormal})` lends a one-texel `Rgba8Unorm` texture (white is the stock renderer's own placeholder texel); `brdf_table(context, semantic, convention)` lends the `BRDF_TABLE_SIZE` square split-sum table that the stock environment preparation integrates (`BRDF_TABLE_SAMPLES` samples, neutral intensity, red and green the scale and bias, view cosine along x and roughness along y), integrated by a compute job staged in the first frame that asks for it under that convention and kept for the generation (a cancelled frame integrates it again); and `fullscreen_triangle()` is an `ImportedGeometry::Vertices` draw of three vertices over no vertex buffer, whose vertex stage reads `@builtin(vertex_index)`, with `FULLSCREEN_TRIANGLE_WGSL` giving `fullscreen_position(index)` and `fullscreen_uv(index)` to put in front of a program. `FrameSceneTextures::water_normal_map(semantic, body)` lends the resident texture that installed water body `body` names as its normal map (`FrameWaterNormalError::NoWater` without installed water, `UnknownBody`, `NoNormalMap`). It reads the water settings the renderer was configured with, so it does not depend on the stock passes drawing the water. Tests: `tests/game_passes/lent.rs` (a game pass draws a resident mesh that covers what the stock draw covers, with the stock texture coordinates and normals; the snapshot changes on append and replacement and keeps otherwise; unknown handle, level and range are refused by type; nothing is lent outside a frame), `tests/game_passes/lent_engine.rs` (the placeholders and the BRDF table, equal to the stock table, are sampled; the triangle covers the frame; the water normal map is lent or refused) and `tests/game_passes/lent_prefix.rs` (the meshes are lent inside the depth prefix, in a frame drawn whole and in a same-frame capture).

`EngineConstants` also lends the scene terrain's textures without a copy on the CPU. `material_array(context, semantic, channel, layers)` takes the frame's `PrepareContext`, a `TerrainMaterialChannel` (`AlbedoMetallic`, `NormalRoughness` or `Height`) and a list of raw terrain surface types, the values `SceneTerrain::surface_indices` holds, and returns a 2D array (always bound as an array, also for one layer) whose layer `i` is the resident scene texture that `surfaces()[layers[i]]` names for that channel in its `TerrainLayer`, with every mip and in the format the scene prepared it in. The layers are copied from the resident textures on the GPU, one staged `PrepareContext::copy_texture` for each layer and mip (whole blocks for a compressed format), so a game never uploads the texels again. The resident scene textures are uploaded with `COPY_SRC` for this. The copies are staged in the first frame that asks for the array and are encoded into that frame's own encoder, in staging order, before its passes draw; they go through the frame's submission like every other staged operation. The array is made on the first request for a channel and list, shared by every feature, counted in the census under the texture class, charged to the shared game resource budget, and dropped when the scene's texture table or terrain changes (the same snapshots as `FrameSceneTextures::same_snapshot`, an append, a texture replacement or another scene), so a layer never shows a texture the scene no longer holds. The charge is the array's physical bytes, every mip and whole blocks for a compressed format (render-gpu's `texture_backing_bytes`, the count declared textures use), held in the runtime ledger of the factory whose feature asks, so one budget bounds the arrays and that factory's declared resources and runtime allocations together; a factory that asks for an array another factory made is charged for it in its own ledger, as for a shared texture, and each hold is released when the array is dropped. The charge is made before the texture is created, so a refused request allocates nothing. A factory whose features ask for arrays states a `resource_budget` that covers them; the default budget is zero, which refuses every array. The array counts as filled only once the frame that staged the copies has submitted, in the way of the BRDF table: a cancelled or failed frame copies nothing, and the next frame that asks stages the copies again into the same array, so no frame draws with an array that was never filled. The refusals are typed in `TerrainMaterialArrayError`: `Input` (the context belongs to a frame that published no scene), `NoTerrain`, `NoLayers`, `TooManyLayers` (beyond the device's `max_texture_array_layers`), `UnknownType`, `NoChannel` (a terrain without layered appearance), `NotCopyable`, `Budget` (a `GameResourceError::Budget`, when the array would pass the factory's budget), and `Disagree` when a layer differs from layer 0 in format, extent or mip count (`LayerShape` names both). It reads the scene that the route publishes before features prepare, so a depth-prefix feature may use it. The engine keeps the scene's shared `SceneTerrain` (`SceneGpu::lent_terrain`, held by the uploaded terrain) because the surface table that names each layer's textures lives there; it keeps nothing else of the terrain. The engine lends no height texture: a game that needs the terrain's heights in the units of its own data declares a shared texture through `GameResources` and fills it from its own save data. `EnginePlaceholder` also has `ZeroR16Float`, `ZeroR8`, `ZeroRg8` (one texel in 2D) and `ZeroRg8Volume` (1x1x1, bound as `D3`). The stock renderer has a neutral texel only for a missing material map, which is `White`, so these hold zero: no value in the float and mask formats, and (0, 0) rather than a midpoint for a signed pair stored as unorm. Block-compressed layers copy by the same code, but the lavapipe tests use uncompressed formats, so the compressed path is not exercised there. Tests: `tests/game_passes/lent_terrain.rs` with its game in `lent_terrain_game.rs` (a pass reads layer `i` of each channel's array at mip 0 and mip 1 from distinct solid textures, a reordered list reorders the array, a repeated request keeps the array and the census, an append or another scene rebuilds it and releases the old one, every refusal is typed, a cancelled or failed frame leaves the array to be copied by the next frame, and two requests in one frame share one array), `tests/game_passes/lent_terrain_budget.rs` (an array beyond the remaining budget is refused with nothing held, a built array holds exactly its bytes and an append or another scene returns them, declared resources and arrays share one budget in both directions, and each factory is charged in its own ledger for a shared array), `tests/game_passes/lent_engine.rs` (the zero placeholders are sampled, including the 3D one through a `D3` binding, and are counted in the census) and `tests/game_passes/lent_prefix.rs` (a depth-prefix feature is lent an array).

### Game passes after the depth prepass

A game whose objects are not in the stock depth prefix, for example shadow-only scene objects that it draws itself, can draw their depth and prepass products before anything reads them. `GameFeatureFactory::build_depth_prefix` returns a second feature, `None` by default, which plans passes at `GamePlace::AfterDepthPrepass`. Those passes run after the `DepthNormalPrepass` and before the depth pyramid, the occlusion metadata, SSAO and shadow depth fitting, in every route: the forward order puts them at stage 2 after the prepass, and a route that submits the depth prefix ahead of the frame (the same-frame fitted capture, the interactive fitted route and `StockDepthPrefix`) plans and runs them inside that prefix. The prefix registers its writes of the game's target textures in the capture's prefix contract. The rest of the frame lists the prefix's passes as completed entries: a renderer with screen-space metadata plans the prepass in the frame, so the depth-prefix feature plans its passes there as it did in the prefix, and a renderer without it (whose prefix is the depth fitting's) adds the prefix's prepass to the frame's plan when the prefix ran game passes. The frame's later passes therefore load and can read what the prefix's passes wrote, and the game's persistent-target bookkeeping, which the frame publishes when it is submitted, includes persistent targets written only inside the prefix. A host that submits a `StockDepthPrefix` in its own capture lists the game's texture keys from `capture_effect_keys` in the prefix's write contract as well as the suffix's.

At this place `GameTargets::plan_pass` takes the attachments from the prepass, not from Opaque:

- `DepthAccess::ReadWrite` loads the prepass's depth and writes it. That write completes the depth prefix together with the prepass. When the prepass writes Opaque's depth, the depth Opaque then loads and tests includes it; `prepass_writes_frame_depth(passes)` says whether it does, which a later game pass needs to choose between testing that depth read-only and writing the frame depth itself.
- `ColorOutput::Prepass(resource)` names a colour product of the prepass: `FrameProduct::normals()` (`Rgba16Float`, view-space unit normal in `rgb`, alpha 1 where covered, 0 where not), `FrameProduct::material()` when reflections are installed (`Rgba16Float`, F0 in `rgb`, roughness in `a`), or a shadow receiver output. The pass loads what the prepass wrote and stores what it adds, with the prepass's attachment. A product the frame's prepass does not write is `FrameError::AttachmentView` naming it.
- Declared targets can be written and read as at the other places. `ColorOutput::Frame` is refused here, and `ColorOutput::Prepass` is refused at every other place (`FrameError::UnsupportedEffect`).

Only the depth-prefix feature plans this place and it plans nothing else; the ordinary feature planning `AfterDepthPrepass`, or the depth-prefix feature planning another place, is `FrameError::UnsupportedEffect`. A frame without a depth prepass (no SSAO, reflections or depth fitting) has no such place, and `plan_pass` refuses it with `FrameError::MissingPass(DepthNormalPrepass)`; the feature checks for the prepass and plans nothing. Whenever the feature plans no pass it takes no part in a prefix: the frame prepares, commits and finishes it as an ordinary feature without passes, once, and a route that encodes the prefix separately leaves it to the rest of the frame, so its `prepare` must accept a frame that lists none of its entries. The renderer builds the depth-prefix feature before the ordinary one and prepares it first in a frame drawn whole. A route that encodes the prefix separately prepares it once, in the prefix, and not again in the rest of the frame. A depth-prefix feature reads the lent terrain only; shadow, occlusion and environment inputs are `NotPrepared` there. Such a route lends the terrain (above) inside the prefix, right after the prefix prepares the terrain geometry and before the prefix's features prepare, with the chunks the rest of the frame lends. It publishes the lent shadow cascades, occlusion and environment (below) as it begins encoding the rest of the frame, after the prefix, so inside the prefix their `current()` is `FrameInputError::NotPrepared` (the cascades are not yet fitted and SSAO has not run), while in a frame drawn whole the same feature is prepared after they are published. Sample those three from the ordinary feature: the depth-prefix feature's passes run before SSAO and, in a fitted frame, before the shadow casters. A pass here depth-tests `LessEqual` and leaves the prepass's stencil marks as they are; stock Opaque in reuse mode tests `Equal`, so it draws nothing where the pass wrote a nearer depth. Tests: `tests/game_passes/prefix.rs` (SSAO and Opaque see a block only the game pass draws; a same-frame fitted capture fits to it and prepares it once per frame), `tests/game_passes/prefix_loads.rs` (later writers of a target keep what prefix passes wrote, with and without screen-space metadata), `tests/game_passes/prefix_state.rs` (a persistent target written only inside the prefix reaches later frames, with and without screen-space metadata; the lent inputs inside the prefix, where only the terrain is lent), `tests/game_passes/prefix_idle.rs` (a depth-prefix feature that plans no pass is prepared once per frame, in a frame drawn whole and after a separately submitted prefix), `src/compositor/game/prefix_tests.rs`, `src/compositor/game/inputs_tests.rs` (a prefix whose game passes cannot be placed keeps no plan) and the forward-order tests.

### Stock ambient occlusion for game passes

`GameBuild::occlusion`, a `FrameOcclusionHandle`, lends the screen-space ambient occlusion of the frame in flight. Every route publishes it as it begins encoding the frame, before any feature plans (a route that submits the depth prefix separately publishes it after that prefix, see above; the geometry-only `FrameRenderer::encode`, which refuses screen-space metadata, publishes `None`), and `current()` refuses as the shadow handle does: `FrameInputError::OutsideFrame` outside a frame, `NotPrepared` before the frame has published. The two handles share that one error type. `Ok(None)` means the renderer computes no screen-space ambient occlusion. Otherwise `FrameOcclusion` holds the last filter stage's texture and view: red is visibility in `[0, 1]` (1 unoccluded), `OcclusionDepth` says whether green holds nothing, linear view depth in metres (gathering receivers, `Rg32Float`) or depth over the far distance (scalable kernel, `Rg16Float`), and `stock_receivers` says whether the stock receivers sample it. The texture can be smaller than the frame; sample it at the normalised screen position. The view is made once with the texture, so lending it each frame creates nothing.

A pass that samples the occlusion declares it with `GamePassSpec::sampling_frame([FrameOcclusion::read()])`. The read attaches nothing; frame validation orders the pass after the occlusion filter, and the declaration makes the frame compute the occlusion even when no stock receiver samples it. The SSAO stages come before `GamePlace::BeforeOpaque` in the forward order. Tests: `tests/game_passes/lighting.rs` (unshadowed frame without occlusion, frame with occlusion, a depth-fitted frame that computes the occlusion only for the declaring pass, the depth channel in metres for gathering receivers and over the far distance for the scalable kernel), `gpu_lent_occlusion_is_the_computed_stage_with_one_view_for_every_frame` in `src/ssao/tests.rs`, `src/compositor/game/inputs_tests.rs` (the geometry-only encode) and the outside-frame test in `src/compositor/game/lent.rs`.

### Stock specular environment for game passes

`GameBuild::environment` is a `FrameEnvironmentHandle`, frame-scoped like the shadow and occlusion handles and published by every route as it begins encoding (after a separately submitted depth prefix, see above), the geometry-only `FrameRenderer::encode` included. `current()` is `Ok(None)` when the host installed no environment cube (`set_scene_environment`, `set_reflection_environment` or `set_water_environment`, cleared by `clear_environment`). Otherwise `FrameEnvironment` holds the cube the surface reflections sample (water's when only water has one, named by `consumer`): the texture, a cube view of every level, `mip_count`, the `radiance_scale` the stock consumers multiply sampled radiance by (the environment's specular intensity, not baked into the texels) and the `SceneSpecularConvention`. It is the texture the host installed from `SceneGpu::environment_maps`: the engine's GGX prefiltered `Rgba16Float` cube when the scene's `SceneSpecularCube` requests preparation, else the uploaded mips as authored. The filter stores at level `m` of `n` the radiance for roughness `m / (n - 1)` (`level_roughness`, before the convention's remap, GGX alpha the square of the remapped roughness) in the source's radiance units, and the stock consumers sample level `roughness * (n - 1)` (`level`) for the roughness after the remap. Under the perceptual conventions, whose remap is the identity, a pass that samples that level reads the level filtered for its roughness; under `LinearAlpha` the filter remaps the level's roughness again, so check the pairing before relying on it. The cube is a retained texture, not a frame product: a pass samples it without declaring a read. The water cube is lent from the installed water, whether or not the stock passes draw it: a game that declares `StockReplacement::WATER` has that water's environment installed for its own water passes, so it is lent although no stock pass samples it. Tests: `lent_environment_is_the_installed_reflection_cube_else_water` in `src/compositor/configuration_tests.rs`, `tests/game_passes/environment.rs` (frames with an installed water cube, with the stock water drawn and drawn by the game), the unshadowed frame in `tests/game_passes/lighting.rs` and the outside-frame test in `src/compositor/game/lent.rs`.

### Drawing over the lit frame before post

`GamePlace::BeforePost` is the last place of the forward frame. Its passes run after every pass that writes the lit scene colour: Opaque, the water and transparent passes, the reflection trace and the stock screen-space pass after transparency, and the game's passes at every other place. The post chain (resolve, bloom, tone mapping) then reads the colour they leave. The place is named for what follows it rather than for the stock screen-space pass, which a frame has only with screen-space reflections; with or without it, nothing in the frame draws over a pass here. It is where a game draws what the original drew over the finished scene, such as blended particles. In a frame without HDR post the colour is copied to the output as it is, so the place is still its last writer.

- **Where it sits.** The forward order puts it after `ScreenSpaceAfterTransparent` in every route, unshadowed, shadowed, resident and in the suffix of a same-frame capture. A frozen plan must list it after every other role, or it is refused with `FrameError::StockOrder` naming the pass and the later writer (`render-post` `frame/frozen.rs`). Its passes take the frame's attachments from Opaque, as every place but `AfterDepthPrepass` does, so a frame without Opaque refuses it with `FrameError::MissingPass(Opaque)`, and the depth-prefix feature planning it is `FrameError::UnsupportedEffect`.
- **Blending.** `ColorOutput::Frame` loads the colour every earlier pass left. Blending is the state of the pass's draws: `PassTarget::render_state(raster)` keeps the pass's depth and stencil rules and has one colour state per attachment in `colors`, which the draw sets to its blend and write mask. The original's alpha blend over RGB only is `wgpu::BlendState::ALPHA_BLENDING` with `wgpu::ColorWrites::COLOR`, which leaves the colour's alpha as it was.
- **Testing and sampling the depth in one pass.** `DepthAccess::ReadOnlySampled` attaches the frame's depth read-only and loaded, as `ReadOnly` does, and adds it to the pass's reads, so frame validation orders the pass after the depth's writers and admits the sampled read of a read-only loaded attachment. wgpu allows a texture to be a read-only depth attachment and a sampled texture of the same pass, and render-post's contracts accept it on every backend except GL, which lacks read-only depth attachment transitions and refuses it with `ContractError::DepthSampling` (`contracts_read_only_depth.rs`). In `prepare`, `PassTarget::sampled_depth(context, entry)` returns the frame's `AttachmentView` of that depth: bind `sampled_view()`, which selects the depth aspect of a combined depth-stencil format, with `texture()`, as `texture_depth_2d` (or `texture_depth_multisampled_2d` in a multisampled frame), and do not keep it past the frame. A pass cannot write a depth and sample it: listing the frame's depth in `GamePassSpec::sampling_frame` of a `DepthAccess::ReadWrite` pass is refused at planning with `FrameError::AttachmentHazard` naming the depth, and `sampled_depth` refuses an entry whose depth is writable or cleared the same way. The depth holds what every earlier writer of the frame depth left: the prepass or Opaque, the stock water surface, which writes it, and any game pass with `ReadWrite`.
- **What it does not write.** A pass here writes no velocity and no temporal mask; under temporal antialiasing its pixels carry the motion of the surface behind them.

```rust,ignore
// Inside `plan`: soft particles over the lit frame.
let particles = GamePassSpec::new(GamePlace::BeforePost, 0)
    .writing([ColorOutput::Frame])
    .with_depth(DepthAccess::ReadOnlySampled);
declared.push(targets.plan_pass(&particles, passes, views)?);

// Inside `prepare`: blend over RGB only, test the depth and sample it.
let target = PassTarget::of(context, particles.entry())?;
let mut state = target.render_state(raster);
state.colors[0] = Some(ColorState {
    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
    write_mask: wgpu::ColorWrites::COLOR,
});
let depth = PassTarget::sampled_depth(context, particles.entry())?;
let scene_depth = Import::texture(semantic, depth.texture().clone(), depth.sampled_view());
```

Tests: the forward-order test `the_place_before_post_follows_every_other_pass_of_the_frame`, `src/compositor/game/before_post_tests.rs` (the planned pass loads the colour an `AfterTransparent` game pass and the stock screen-space pass wrote, attaches the depth read-only and reads it, validates as a frame; the missing-Opaque, depth-prefix and write-and-sample refusals), the frozen-order test in `render-post` `frame/game_tests.rs`, and the Metal frames in `tests/game_passes/before_post.rs`: a quad blended at alpha 0.25 over a game fill drawn after the transparent passes and over the stock quad equals the blend of the two to half precision with the alpha kept; a pass that tests the depth read-only and samples it writes the expected fade, sampled depth and fragment depth in front of the stock quad and off it, and leaves the stock colour where the test rejects it; and a game that plans nothing before post, or blends nothing there, leaves the HDR frame bit for bit as the stock frame.

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
observations on the owning composition submission. Absence means no streaming terrain or nonstreaming terrain draw;
`Static` identifies a nonstreaming mesh. `Streaming` carries the selector's desired
leaves, including its split and hysteretic merge decisions, alongside admitted
coverage before view culling, unresolved replacements, residency and completion. Budget-blocked merges
remain unresolved until admitted, even when resident fine leaves already exceed
the selected detail. Fine leaves retained by hysteresis are selected and complete. Its bounded drawn-leaf
fingerprint includes only submitted sample windows after view culling. Streaming
observations remain present with zero drawn leaves when the camera misses terrain. See the
[host statistics contract](../scene-viewer/README.md#retained-frame-statistics)
for counting units and identity encoding. Raw and cancelled draws do not publish.

## Visible object contour IDs

`SceneObject::with_contour_id` assigns a byte ID, with zero meaning no contour.
`SceneEffects::with_contours` supplies a `SceneContours` table with exactly 256
optional styles and a positive reference extent, defaulting to 1920 by 1080.
Entry zero must be absent. Every nonzero object ID must name a defined style,
including hidden live placements. Widths and shifts are integer millionths of
reference pixels in 0 to 32; linear RGB is in 0 to 16, and alpha and strengths
are in 0 to 1. Pulse powers are in (0,16] for a nonzero millisecond period;
period zero is steady and ignores power. These quantities are authored in
millionths. Fill and blend descriptions are retained CPU state; no contour
colour composite is applied.

`SceneGpu::set_instance_object` changes membership without changing the 80-byte
GPU instance layout. `set_contours` replaces the table atomically and rejects
removing any referenced ID. A sparse index tracks membership on edits, removals
and slot reuse. Extraction from object templates preserves IDs.

Stock, resident and shadowed composition schedule `PassSlot::ContourIds` once
after every Opaque occurrence and before `AfterOpaqueSnapshot`. Opaque draws the
scene's opaque surfaces, so the ID pass sees the completed scene depth. Frozen
composition refuses a contour
entry before any later Opaque entry with `FrameError::StockOrder`, preserving
both occurrence identities in the error. The pass
writes only selected members, reusing their object programs, original instance
ranges and skin streams. Its R8Uint target is cleared to zero and has the
completed scene depth's sample count.
Depth is loaded read-only, tested Equal and never written; the pass has no
stencil ownership test, blending or integer alpha-to-coverage. Material alpha,
fade, LOD, wind, skinning and relief visibility still participate. Fully hidden
surfaces write no ID. Coplanar selected and unselected surfaces cannot be
distinguished by depth alone.

A table without live nonzero IDs prepares no contour pipelines and allocates no
ID target or pass. Clearing the last ID removes the selected frame slot's target
on successful submission; abandonment preserves its submitted target. Targets
are isolated by admitted frame slot and replaced when their extent changes.
Same-frame shadow depth-fitting captures provision the active slot's target before
building the capture write contract and declare its physical allocation in the
suffix. Suffix scheduling reuses that allocation without publishing it early.
`RendererConfiguration::contour_ids` explicitly reserves the three-slot target
budget and admits the format/sample combination; it defaults to false. The
unconfigured `FrameRenderer` constructors allocate lazily when membership appears.
Unsupported format/sample combinations fail instead of reducing MSAA.

`FrameRenderer::contour_target` returns the selected slot's last submitted ID
texture; `contour_statistics` describes that published target's pass, target and
sample counts. These are snapshot values, not execution or allocation counters.
`encode_contour_capture` records an explicit capture-only extraction into a
single-sample R8Uint COPY_SRC texture. For sample index s, source pixel (x,y)
appears at `(s * width + x, y)`. This reads exact unsigned IDs with textureLoad,
without filtering, averaging or a hardware resolve. The ordinary ID target has
no COPY_SRC usage. Capture may follow scene encoding in the same transaction or
read its submitted result in a later transaction.
