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
completion, even after the scene replaces it. The HDR profile also supports SSAO,
water, SSR, adaptive exposure, bloom and FXAA through validated scene settings. The
legacy profile rejects post requests before upload. Textured-sky rendering, vignette,
nonneutral contrast and saturation remain unsupported. Zero-weight unsupported
requests still fail.

`ShadowedScene::encode_term_capture` and `encode_term_capture_with` replay initialized
scene/frame state with its matching selected slot. Both return `Result<(), ShadowError>`
and require the encoder of the active prepared transaction. Transaction or map-lease
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

`SceneGpu::admit_programs` carries draws that `worldrunner-render-program` admitted into the next frame encoded with this scene. `compositor::encode_scene` and `FrameEncoding::encode_scene` take them, and the scene's feature records them in the stock opaque pass after its own opaque geometry. They draw into the same color target and depth buffer, are depth tested against the scene's geometry, and leave through the frame's one submission with the stock resolve, output and capture copies. No pass, slot or entry point was added, and nothing else can reach the pass. `SceneGpu` accepts only draws from `ProgramRuntime::admit_draws`, so the arena, the frame's authority and the target signature were checked before they arrive. See the [program crate](../render-program/README.md#joining-a-renderer-frame) for admission and lifetime.

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

Dynamic quads, native sprites and tilemaps, and UI glyphs routed into the world draw with the scene shader's `vs_unlit` and `fs_unlit`. That material is a `render-program` graphics program over the unchanged scene WGSL, and `ScenePipelines` no longer builds a pipeline for it. The runtime builds the pipeline for the pass it is given, with straight alpha over location zero, `LessEqual` depth without a depth write, no culling and an empty write mask on every other colour attachment, so the velocity and reflection targets of a pass keep what other materials wrote. Plain, SSAO, velocity and SSR pipeline sets, `Rgba8UnormSrgb` and `Rgba16Float`, and any sample count the pipelines were built for take the same material.

The runtime borrows what the scene already owns, by semantic: the slot's 320-byte frame uniform at group 0 binding 0 (the frame slot of the transaction that committed the frame, not a copy), and for each group of adjacent compatible quads its vertex, instance, tint and material buffers, its albedo texture and its sampler. Each group is one instanced draw over `0..instance_count`. The stable `(overlay, sorting_layer, sorting_order, hierarchy_order)` order and adjacent-only batching are unchanged. A group is adjacent quads with the same clip rectangle, texture, corners, rectangle and UV rectangle, so quads with different clip rectangles are separate draws. The scene counts none of these buffers against an allocation set and neither does the runtime: that accounting does not exist for dynamic geometry, and this change adds none.

Staging quads prepares a draw for each frame slot, which can fail and allocate, beside the buffers' upload. `encode_scene` and `ShadowedScene` prepare the quads for a pass the staging pipelines did not describe, if there are any, and retain the staged draws on the frame before the pass opens. `encode_resident` takes a shared scene, so it retains and refuses a pass the quads were not prepared for. Recording is state setting at the position in the pass where the quads draw: objects, terrain, sky and celestial meshes, then quads. The staged quads own what they lent through a `Keepalive`, so replacing or clearing them while a frame that recorded them is unfinished leaves the buffers alive until that frame is cancelled or completed on the GPU.

The scene's pass does not clip: `DynamicQuad::clip_rect` splits draws there and is applied only by the retained-UI overlay. The overlay keeps its own fixed pipeline (`vs_ui_overlay`, `fs_ui_overlay`) over the same staged quad buffers, with a material bind group per draw and a scissor per draw taken from the clip rectangle (clamped to the output, flipped to the top-left origin, skipped when nothing is left). Its draw returns the calls and instances it recorded for the UI pass statistics. `scissor_rect` in `src/dynamic/gpu.rs` holds the scissor's arithmetic. `src/dynamic/capture_tests/clipping.rs` draws partial, disjoint, full, oversized, outside and mixed clip rectangles in every target set through the scene's pass and requires each attachment to equal the same quads drawn without clip rectangles in the same run. It draws the same cases through the overlay's own pipeline and requires the overlay to draw exactly when something is left inside the scissors, to leave the output as the unclipped quads make it when the clip is the whole target, and, for one quad over the whole target, to keep the pixels inside the lower-left rectangle and nothing outside it.

`src/dynamic/capture_tests.rs` draws quads, sprites and glyphs with this path under each target set above with the auxiliary attachments pre-filled, and requires the colour attachment to change and every other attachment to stay as it was. It prints a hash per attachment.


## Terrain on the program runtime

Terrain draws through `render-program` in every forward context: plain, velocity, SSAO and its gather receiver, reflection, and each of them as a shadow receiver, at one or four samples, and in the metadata prepass (the normal output and the merged normal and material output, with the camera uniform lent at set 3), the opaque term replays and the shadow casters. The shader source is the scene's own, byte for byte. Only the machinery moved.

- **Programs.** `terrain::program::Context` holds one program per surface source for a `ScenePipelines`: palette (`Plain`), baked `Pages`, `Layered` sources and `LayeredPages`. A surface is a feature set (`TERRAIN_PAGES`, `TERRAIN_LAYERED`); the composition is `terrain::program::source`. The frame's specular convention and winding are override constants of the fragment stage, so a specialization is a cheap program, not a sibling set of compiled pipelines. Derived bindings at set 1 are the ones the shader reads: seven for `Plain`, four for `Pages` and for `LayeredPages` (the same four, because cached pages replace the source composer), fifteen for `Layered`.
- **Sets the pass owner binds.** The frame group (set 0), the shadow map (set 2) and the screen group (set 3) are lent as `ExternalSet`s over the layouts the scene already uses. The pass binds them once, as it did, and the runtime builds no bind group for them. The terrain's own set 1 is a `PreparedSet` built from the buffers and textures the scene retains, once per terrain and pipeline context, with one per page table for the cached surfaces. A set stays with its context's runtime and material layout.
- **Receiver payload.** Paired single-sample shadow contexts declare the lossless displacement arrays at set 2 bindings 3 and 4 in the lent shadow group. Terrain loads its fragment's payload in every forward depth mode, including page and layered surfaces. The material's prepared set stays at set 1; the pass binds the payload with its shadow map.
- **Depth reuse.** A context's `DepthMode` (independent, shared, prepass or reuse, from the forward depth reuse of single-sample frames with a complete prepass) gives the terrain program its depth format and its `RenderState`: the same `DepthMode::state` the object pipelines use, so reuse terrain tests depth `Equal` without writing it and reads the stencil mark with reference `STENCIL_REFERENCE`, which the pass sets once. Every writing depth mode uses `LessEqual`, and reuse leaves the stencil mark intact so the last surface at equal depth wins in API order. The runtime draws stencil from that state (`render-program` README, "Stencil").
- **Cost.** A frame records one `PreparedPipeline` and a `GeometryDraw` per resident chunk: `2 + 4C` commands for C chunks (`3 + 4C` under a shadow map). A chunk's record of the shared instance buffer is its geometry's instance range rather than a bound slice. Pipelines, sets and chunk geometry are prepared the first time a pass needs them and found in maps afterwards. `encode_scene`, `ShadowedScene` and `encode_resident` prepare the terrain's pipeline for the pass, the prepass and the forward pass alike, after the frame's pages are prepared and before any pass opens, where a refusal is still an error (`SceneGpu::prepare_terrain`); a term replay does the same before it opens its pass. A raw `SceneGpu::draw` prepares what it finds missing as it records. A streamed chunk's geometry is made when the chunk is uploaded and dropped when it retires, before new uploads are admitted, so the geometry never holds a buffer the streaming budget has stopped counting. A prepared set belongs to the runtime and the program layout of the context that built it, so a scene's terrain keeps one per context that has drawn it: independently built pipelines, and the forward pipelines a renderer replacement builds again, each make their own. Sets and pipelines are kept per context and dropped with the context they were made for, so the pipelines a term replay builds for one frame leave nothing in the caches of a long-lived scene, and frames over the same chunks add nothing to `RuntimeStats`. The runtime is shared by a `ScenePipelines` and every sibling specialized from it, so a terrain pipeline compiles once per context for any number of scenes. `src/terrain/cost_tests.rs` holds these claims.
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

Each draw carries its inclusive height-sample bounds and sampling stride. Edge
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
- **Recording.** `draw/lod_material.rs` walks a view's selected ranges and names the buffers and draws to a `DrawSink`. The test-only `FixedSink` sets each buffer as it is named on pipelines a test built; the runtime sink (`draw/object_runtime.rs`) records each draw once with `PreparedPipeline::draw_borrowed` over references to the buffers it holds, so a draw allocates nothing, with the previous-instance buffer as the third vertex buffer in the velocity contexts. Change tracking is unchanged: a change of pipeline or material binds the prepared pipeline of the pair of key and material, and a pass adapter leaves out the commands a run repeats (the render pipeline when the new pair shares it, and a vertex or index buffer the walk has not named again), so a pass records no state a run repeats. `render-program` has no way to bind a prepared set apart from the pipeline it was prepared with, so there is one prepared pipeline for each pair of key and material that a scene draws; a pair is cheap, and pairs of one key share one render pipeline.
- **Cost.** `src/object/cost_tests.rs` holds the claims. A frame of eight cards with eight keys records 49 commands (57 under velocity), six materials of one key record one pipeline and seven groups, and a level with two material slots records two draws, two pipelines and three groups (`draw/tests/lod_material.rs`). `SceneGpu::prepare_objects` prepares the pairs a frame draws before any pass opens (`encode_scene`, `ShadowedScene` and `encode_resident`, for the forward pass only), where a refusal is still an error; a raw `draw` prepares what it finds missing as it records. After the first frame `RuntimeStats` does not move, and a second scene drawn with the same pipelines compiles no pipeline and no module.
- **Tests.** `src/object/capture_tests.rs` draws the object scenes in the twelve forward target sets, the gather receivers and the shadow receivers, and requires the objects to change the attachments they write and the draws the scene selected to use all 32 keys (`SceneGpu::pipeline_keys_drawn` reads the keys off the selection through the walk a pass records). The prepass and term replays require three keys. Those captures draw at 0 ms of the animation clock, where the fixture's periodic wind displaces nothing; `capture_tests/variants.rs` draws the foliage cards again at a quarter period and requires the depth plane to differ from the 0 ms one.

## Player offscreen operations

`SceneGpu::admit_offscreen` carries an `AdmittedOffscreen` from `worldrunner-render-program` into the next frame encoded with this scene: camera renders into render textures, `Graphics.Blit` material passes and texture copies, already checked by the render-post executor's contract as `TypedOperation`s. `compositor::encode_scene` takes them after it has checked the admitted draws and before it declares the stock passes, so they are the first commands the frame records in the same encoder and so the same submission. They are encoded through the frame's pass timer, so with timing configured each one is a `PassId::Operation` at the head of `FrameTiming::passes`, ahead of the stock opaque pass. No second scheduler was added: the scene's only change is where it calls `AdmittedOffscreen::encode`.

A draw admitted with `admit_programs` can sample a render texture that the offscreen operations write, in the stock opaque pass. The host names that read when it builds the offscreen frame (`OffscreenFrame::later_reads`), and the executor checks it as a sampled read of the version the frame leaves, so a read before any write or of a range a later operation overwrites fails at admission by name, before anything is recorded.

- The resident path (`encode_resident`) and the shadowed path refuse a scene that holds admitted offscreen operations by name, as they do for admitted draws.
- The offscreen operations are taken whether or not the frame is accepted, so a refused frame leaves none for the next. An arena that retired or belongs to another submission authority fails the frame by name.
- A cancelled frame submits nothing; the render textures keep what they held and the arena can retire.

The test is `src/draw/tests/program_frame/offscreen.rs`: a camera renders two of three renderers (selected by layer) over a clear color, a blit inverts the result, a copy puts it in element 2 of a texture array and an admitted scene draw samples that element in the opaque pass, with a readback after each stage, exactly one submission and the offscreen passes listed in the frame's timing. See the [program crate](../render-program/README.md#offscreen-work-through-the-checked-operations).

`offscreen/gbuffer.rs` combines the camera, blit and copy work with an unshadowed
stock G-buffer scene, without admitted opaque program draws. It checks Render and
Observation separately: offscreen readbacks, stock scene pixels, ordered frame
timings, submission serials and submitted object counters. Render uses the fused
depth/G-buffer producer before lighting and the forward remainder: the card is
counted once under prepass objects, with no opaque object draw. Observation also
counts the card under opaque objects. Scene pixels are read as exact linear HDR
bytes. The existing screen-space restriction on admitted opaque program draws is
unchanged.

`offscreen/gbuffer/queued.rs` submits two mixed frames on one offscreen state and
submission authority before mapping any readback. Distinct synthetic tints and
clear colors pin each frame's camera, blit and array-copy bytes. The stock card
moves one meter along X between submissions through the ordinary instance update,
so known HDR pixels differ between frames too. Each encoder records its own
readback copies while both frames reuse the scene and G-buffer targets. After both
submissions, each frame's timing record checks that every offscreen operation
executes once before the path's expected geometry passes.

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
hold this selection (the resident path never gathers), and `NotRecorded` means the
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

## Public moving-view timing probe

Run the informational unshadowed Forward versus `GBUFFER_RENDER` comparison
serially. The external integration target links the normal renderer dependency,
without the renderer's `cfg(test)` pass tracing or cached-plan consistency checks.
On the shared Linux host, use its resource limiter and software Vulkan adapter:

```sh
CARGO_INCREMENTAL=0 \
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json WGPU_BACKEND=vulkan \
  heavy --mem 6G -- nice -n 10 env -u RUSTUP_TOOLCHAIN \
  cargo test --locked --release -p worldrunner-render-scene --test public_moving_view \
  -- --exact public_moving_view_renderer_timing_probe \
  --ignored --nocapture --test-threads=1
```

For hardware measurements, select that machine's adapter instead of lavapipe and
use its GPU queue. Prebuild under `heavy` by inserting `--no-run` before the libtest
`--` separator, then submit the same optimized test command through `gpujob`.
Omit the host-specific wrappers where they are unavailable. The pinned Rust
toolchain applies. Software-adapter timings describe the CPU rasterizer.

Record the Cargo profile with each result. Unoptimized runs check the fixture;
CPU cost comparisons use `--release`. Optional timestamp instrumentation remains
inside the measured encode/submit interval. Results describe this synthetic
fixture with serialized submissions. Game-frame costs require a game measurement.

The unit-test probe also records private pass observations and checks fresh-plan
consistency, including on a cached plan in release libtest. Run it with the same
wrappers, replacing `--test public_moving_view` with `--lib` and the exact test name
with `shadows::tests::render_path::moving_timing::public_moving_view_timing_probe`.
Its host timings include those additional checks. The external target validates
public draw statistics and fixed expected timing identities; it cannot observe
the private pass trace.

The synthetic fixture contains terrain, a mesh, a masked impostor and sky at
257×241. For view index `i` in `0..150`, the eye is
`[-400 + 800*i/149, -4000, 6500]` integer millimetres and the target is `[0,0,0]`.
The first 30 views warm up; the remaining 120 are sampled. Each path keeps a
separate scene upload, frame renderer and output resident. A shared immutable
source, single-sample `Rgba16Float` SSAO pipelines and per-view uniform serve both
calls to public `compositor::encode_scene`, with alternating path order. No shadows,
SDSM, image readback or speed threshold is involved.

CPU encode/submit ends when submission returns. Serialized completion ends after
waiting for that submission. Both include optional query instrumentation.
Timestamp resolution, draw checks and printing follow both intervals. Completion
is wall time, including host work and the wait, without overlapped presentation.
Optional timestamp support is reported separately from coverage, and unsupported
devices still produce CPU results. Warm-up records are drained. Every sampled
frame retains its view/path/frame association and submitted draw counters. Render
requires one mesh, impostor and terrain draw through its prepass, with empty opaque
object draws.

GPU per-pass and pass-sum summaries require all 120 frame records, their matching
view/path IDs, the full expected pass identities and multiplicities, available
pairs, and zero dropped/unavailable counters. The external fixture contract fixes
the expected prepass, ten depth-pyramid levels, SSAO and blur, opaque pass, tone
stage and Render's lighting pass independently of received timing records.
Missing records or invalid passes on a supported device fail explicitly. Genuine
zero intervals remain valid. Full `PassId` values are printed. Pass sums exclude
gaps and untimed work, so they are not whole-frame GPU envelopes. Nonignored CPU
checks and short GPU runs cover the measurement contracts; the full probe is
informational.

## Deferred G-buffer

`SceneOpaquePath::GBUFFER_OBSERVATION` (render-plan) selects the compact deferred
G-buffer in observation: the renderer allocates four single-sample targets at the
scene extent and clears them to zero every frame in the `PassSlot::OpaqueGbuffer`
slot, which runs after Celestial and before Opaque. Nothing reads them except the
lighting pass below and captures, so the visible frame and every existing pass are
the forward ones.
`Forward`, the default, allocates nothing. `RendererConfiguration::opaque_path`
carries the choice (scene-viewer copies it from the scene effects), and
`FrameRenderer::hdr_antialiased` reads it from `effects`. A multisampled frame is
refused before allocation with the opaque admission's sample error
(`opaque admission supports one sample, received N`), and any other experiment
tuple with `PostError::UnsupportedProfile`.

| Location | Target | Format | Contents |
|---|---|---|---|
| 0 | G0 | `Rgb10a2Unorm` | rg: view-space shading normal, spheremap; b, a reserved, 0 |
| 1 | G1 | `Rgba8UnormSrgb` | rgb: linear base colour (stored as sRGB); a: 8-bit lighting-record index |
| 2 | G2 | `Rgba8Unorm` | roughness, metal or scattering amount, sqrt(emission), material AO |
| 3 | G3 | `R16Uint` | bits 9..15 and 2..8: 7+7 bit octahedral geometric normal (x high); bits 0..1: engine flags (bit 0 scattering model, bit 1 reserved) |

To write the targets, a producer registers or borrows a feature whose
`FeaturePass` names `PassSlot::OpaqueGbuffer`, with `color_format` `Rgb10a2Unorm`
and `additional_color_formats` `[Rgba8UnormSrgb, Rgba8Unorm, R16Uint]`
(`gbuffer::GBUFFER_FORMATS`), concatenates `gbuffer::GBUFFER_WGSL` into its
source, and returns `scene_gbuffer_encode(surface, view_from_scene)` from its
fragment entry point. The module declares no bindings: each producer passes its
own camera rotation. The lighting side calls
`scene_gbuffer_decode(g0, g1, g2, g3, scene_from_view)` on loaded texels.
`SceneGbufferSurface` holds `shading_normal` (scene space, after decals, unit),
`geometric_normal` (scene space, unit), `base_colour` (linear), `roughness`,
`metal` (metal, or with flag bit 0 the per-pixel scattering amount, the surface
then having no metal), `emission` (0..1), `ao` (material AO; screen AO stays a
lighting input), `flags` (bits 0..1 stored) and `lighting_record` (0..255).
`SceneGbufferTargets` has `@location(0..3)` members `g0`, `g1`, `g2` and `g3`.
Producers pass the rotation of `FrameUniform::view` as `view_from_scene`; the
lighting pass decodes with its transpose.

Depth: the slot does not own depth; the depth prepass clears and writes it. A
producer that declares a `depth_format` in the slot gets the scene depth attached
read-only when it is complete before Opaque (the depth prepass ran and Opaque
loads it); it tests against it, `Equal` for exact forward winners, and writes
nothing. Without a completed depth the request fails with
`FrameError::AttachmentContract` naming the `OpaqueGbuffer` entry. The stock producers
(`gbuffer_writes`: terrain, opaque, masked and relief meshes, impostor cards) are
the scene's own terrain and object programs with their fragment entries replaced,
drawn over the camera selection the prepass and Opaque draw.

Encoding. View space is the camera's right-handed view space (it looks down -z),
so camera-facing normals have z > 0. G0 stores the Lambert azimuthal equal-area
(spheremap) projection about +z, `enc = n.xy / sqrt(8 n.z + 8) + 0.5`, computed in
the equivalent radius form `normalize(n.xy) * sqrt((1 - n.z) / 8) + 0.5` so that
it is defined at the singular point n.z = -1, which encodes as (1, 0.5) and
decodes to (0, 0, -1). The decode is `f = min(|4 enc - 2|^2, 4)`,
`n = ((4 enc - 2) sqrt(1 - f / 4), 1 - f / 2)`. Measured quantisation of the
shading normal: at most 0.25 degrees for camera-facing normals, 0.35 for
z >= -0.5, 0.7 for z >= -0.9, and 6 degrees anywhere including at z = -1, on
Metal. lavapipe and RADV round some stores to the neighbouring code, so the GPU
probe holds each stored normal to the worst decode within one code of the CPU
mirror's code, or the band bound if that is larger. The
geometric normal's 7+7 bit octahedral code is within 1.9 degrees. Base colour is
within 0.0045 in linear units, roughness, metal and AO within half a code, and
emission within 1/255 through its square-root transfer. Every target clears to
zero and a covered pixel never stores G3 = 0: the corner code (0, 0) with no
flags is written as (127, 127), which decodes to the same normal (0, 0, -1). So
G3 = 0 means no producer wrote the pixel.

`FeatureTarget::Gbuffer(GbufferTarget::G0..G3)` exposes each target for capture
readback; `Readback` reads `Rgb10a2Unorm` and `R16Uint` natively. The CPU tests
check the quantisation through a Rust mirror of the WGSL and validate the module
with naga; the GPU tests draw a small terrain scene with and without the path,
and a producer that writes known surfaces through `scene_gbuffer_encode`.

The scene itself is a producer. When the frame observes the G-buffer and Opaque
loads the completed prepass depth (`DepthMode::Reuse`), the prepass metadata carries
G-buffer pipelines (four targets), and the scene feature draws the camera's selection
into the slot through the same walk as the colour pass, in the unshadowed and the
shadowed scene. Other observation depth modes declare no stock producer; the stock
scene leaves the cleared targets untouched. Each fragment entry stores the material its forward
entry would light, before any lighting: terrain stores `terrain_material`, objects
`shade_material`, and impostor cards `scene_card_gbuffer_material`, which is derived
from the frozen `scene_card_shade` by counted rewrites. Scattering materials set flag
bit 0 and store their per-pixel amount in G2.y with metal 0. Every object entry stores
its material's lighting record in G1.a; terrain stores record 0. For scattering
materials whose mask is not packed, forward lights both the metalness and the amount that
`scene_mask_response` returns, and the G-buffer keeps only the amount. Frames
without the depth prepass have no stock producer and store nothing unless a
registered synthetic producer writes texels.

### Lighting records

A lighting record holds the constant material values that shading reads besides
the surface's own channels: the back-scattering shape, colour and mode and the
transmission amount (WGSL `SceneLightingRecord` in `shader/environment.wgsl`:
`scattering_shape`, `scattering_colour`, `scattering_mode`, `transmission`). The
renderer builds one table per uploaded material set (`gbuffer::records::LightingRecordTable`,
extended by appends): equal records share an index and index 0 is the neutral standard
model, no scattering and no transmission. Every material uniform carries its index in
`scattering_mode.z`, which producers read with `scene_material_lighting_record_index()`
and store as `SceneGbufferSurface.lighting_record`; G1.a holds it as `index / 255`.
Eight bits address 255 records besides record 0. A scene with more refuses the lighting
pass before any pass records, with `FrameError::FeaturePreparation { feature: "opaque
G-buffer" }` and the reason `opaque path needs N material records, limit is 255`; indices
are never truncated. Forward drawing is unaffected by the count.

### Deferred lighting

With a G-buffer path the renderer also runs `PassSlot::OpaqueLighting`, one
full-screen pass after the G-buffer slot and before Opaque, on the scene depth the
depth prepass completed. A frame without the prepass has no producers, so its
observation lights nothing. The stock scene features
(plain, resident and shadowed) draw it with the same forward pipelines, frame group,
shadow set and screen-space set as their Opaque draws: the program is the frame's
forward object source with `gbuffer.wgsl` and `shader/gbuffer_lighting.wgsl` appended,
so every shadow, SSAO receiver, reflection and receiver-payload variant and every
specialisation constant is the forward one. Its inputs are set 1, bindings 16 to 22: the
four targets, the scene depth, the record table and a camera block (inverse
view-projection and inverse view rotation). The inverse projection centres clip depth
on one, combining its depth and translation columns in f64 before the f32 upload to
avoid cancellation near the far plane. Per pixel it reconstructs the position once
from depth, decodes the surface, and evaluates exactly the forward mesh shading after
decals: `scene_surface_lighting` (sun with the cascades and the selected disk highlight,
SH or hemisphere ambient, material and screen AO through the selected occlusion rules,
environment lookup and BRDF factor), then `scene_record_scattering` and
`scene_record_transmission` from the record, emission, and height fog and exposure once.
Forward meshes and impostor cards call the same record functions. Pixels whose G3 is 0
are discarded, so they keep the targets' clear; every lighting function still runs there
so that the shadow lookups' derivatives stay in uniform control flow.

Outputs, in the scene colour format at the scene extent: the exposed radiance (alpha 1
where covered) and, when reflections are installed, the replaceable environment
contribution and replacement factor that Opaque's second and third colour targets carry
for SSR. In observation they are diagnostic targets cleared to zero every frame
(`LightingTarget::Colour`, `Environment`, `Factor`), read only by captures as
`FeatureTarget::DeferredLighting(target)`, so the visible frame stays forward. Shadow
receiver-plane derivatives come from neighbouring pixels in the full-screen pass (the
receiver payload, when present, is read per pixel instead), so they differ from forward
at depth and normal discontinuities.

### Rendering from the G-buffer

`SceneOpaquePath::GBUFFER_RENDER`, `experiment(Render, AllStock, Compact,
MaterialOwnedDepth)`, makes the visible frame come from the G-buffer. It needs the
metadata installation through screen-space AO, reflections or depth fitting. Without
it the frame is refused with
`FrameError::FeaturePreparation { feature: "opaque G-buffer" }` and the reason
`rendering from the G-buffer needs the depth prepass`. It refuses a multisampled
frame. The checked scene depth must be the frame-owned depth target at its canonical
mip and layer. A foreign depth attachment returns a named error before stock scene
preparation, including on devices without stencil support. Forward and observation
retain their existing checked-attachment rules. The frame is:

1. `DepthNormalPrepass` clears and writes depth, G0-G3, the canonical normal and
   coverage target, optional velocity, and optional SSR material in one terrain/object
   geometry walk. Depth winners mark stencil when shared depth has a stencil plane.
   Masked and relief surfaces use the material producer's coverage and fragment depth.
2. The depth pyramid and SSAO consume those metadata targets. Same-frame depth fitting
   submits this producer and the first depth level before adopting its fit and drawing
   fitted shadow casters. The continuation retains the producer's physical resources,
   velocity attachment and pending colour-draw publication. Camera selection, colour draw
   counters and effect bindings publish only after the suffix succeeds. A refused suffix
   preserves the previous completed frame's draw statistics and leaves admitted player work
   unconsumed; the next frame starts a fresh producer publication.
3. `OpaqueLighting` takes Opaque's colour target and, with reflections, its environment
   and factor products, including their clears. No diagnostic lighting targets are
   allocated and Render has no separate `OpaqueGbuffer` geometry walk.
4. `Opaque` loads those targets, depth and velocity, and draws the sky, celestial meshes
   and dynamic meshes. It does not draw terrain or stored objects again.
5. Water, transparency, SSR and post use the existing suffix. The after-opaque snapshot
   copies lit colour; post-water final SSR depth remains a separate product.

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
G3 and velocity contain the moved geometry. A fresh ordinary retry preserves that motion;
retry and settling products match an uninterrupted control in both cases. Declaring one
scene's resident write keys while encoding a different dirty scene fails capture admission
with an undeclared resident key before submission; both scenes keep their completed products.
The [public prefix-attachment regressions](tests/stock_scene_retry/prefix_identity.rs) complete
an additional caller pass in the stock prefix, then change only its attachment backing or only
its load contract in the same-frame suffix. They require `AttachmentView(resource)` or
`AttachmentContract(entry)`, respectively, before caller preparation or submission, with
completed products, draw statistics and temporal history unchanged.

The [public camera-only TAA regressions](tests/stock_camera_taa.rs) pair resident TAA with a
fused G-buffer depth prefix and plain SSAO pipelines. They check that the prefix and prepared
stock suffix declare no scene-velocity colour writes, while camera history and jitter advance
across two submitted frames. The retained pipeline-switch case first completes a
velocity-enabled frame, then switches the same scene and renderer to plain pipelines. It
requires both plans to omit velocity writes while the velocity allocation stays resident and
TAA history continues.

[The same-frame capture fixture](src/shadows/tests/sdsm/depth_reuse/gbuffer/retained_retry.rs)
covers program-admission and caller-preparation refusal after a physical prefix. It
checks one submission on refusal and two on success before reading products, preserves
completed statistics and temporal state, and compares retry and settling with a control.

[The mixed rigid/skinned fixture](src/shadows/tests/sdsm/depth_reuse/gbuffer/skinned_retry.rs)
checks a resident pose-descriptor switch and root movement across a submitted prefix
and refused colour suffix. Its non-velocity path compares colour and G3 with an
uninterrupted control and an unchanged completion, with separately visible changes
for rigid and weighted geometry.

The ordered producer outputs are G0-G3, optional `Rg16Float` velocity,
`Rgba16Float` canonical normals, and optional `Rgba16Float` SSR material. Their
aligned attachment charge is 34, 38, 42 or 46 bytes with five to seven attachments.
Admission checks this composition against enabled device limits before allocation.
Render supports Front receivers at zero normal bias. An Opposing reservation or
nonzero shadow normal bias returns a named admission error before retained state
changes. Front uses G3's geometric normal and reconstructed scene depth. It retains
quad displacements within the normal's 7+7-bit octahedral quantization cone only when
both texels of the derivative pair are covered and carry the same geometric-normal
code. Material flags do not affect that comparison. Other displacements use the
current pixel's receiver-plane solve. The compatibility bound is
`abs(dot(normal, delta)) <= sqrt(18)/127 * length(delta)`; it conservatively contains
tangents after G3 rounding and normalization. Parallel surfaces with the same code
can still be accepted inside that cone. The tests in `src/gbuffer/front_tests.rs`
cover the bound across octahedral folds and axes, reject an outside displacement or
missing pair provenance, preserve the other coordinate of each derivative pair, and
check the plane solve with both projection types.
Both displacements feed the existing projected-gradient solver, without receiver-payload
attachments. Forward remains the default; observation keeps its separate read-only
`OpaqueGbuffer` pass.

Differences from Forward: the G-buffer quantisation above; scattering materials that also have a per-pixel
metalness keep only the scattering amount; shadow receiver-plane derivatives come from
screen neighbours, so shadow edges can differ by a texel at depth and normal
discontinuities; impostor cards are lit through the mesh path, without the card shading's
separate back-face shadow lookup for transmission (the far-side visibility uses the
receiver's `-normal` lookup instead); and a masked surface's scene colour alpha is 1
where forward wrote its coverage alpha.

`RendererConfiguration::opaque_admission_report` lists opaque material families and
candidate-layout colour attachment byte costs, attachment limits, and lighting-record
counts for a prepared scene. Callers invoke it explicitly; it is independent of
`SceneOpaquePath` and creates no renderer resources. The report uses typed
material-family classification, counts the distinct lighting records besides record 0
over the scene's whole material table, enforces Compact's 255 and Wide's 65,535 records
with `OpaqueAdmissionError::RecordOverflow`, and refuses sample counts other than one
with a typed error.

## Composing with the stock scene

A caller outside the crate can draw its own passes around the stock scene and read what the stock scene produced. `compositor::prepare_stock_scene` takes the inputs `encode_scene` takes and returns a `StockSceneFeature`, an opaque `PreparedFeature` with `declaration()`, `plan()` and `frame()`. Clone the plan's passes and views, append the caller's own, save the declaration and frame uniform, then take `stock.take_offscreen_operations()`. Keep that owned work alive through encoding and pass `offscreen.as_ref().map(|work| work as &dyn FrameOperations)` as the head of `FrameRenderer::encode_frame_with`, with `&(uniform, None)` as its input. Borrow the stock feature mutably in a `BorrowedFeature` beside the caller's features for that call. The offscreen work is separate from `plan()` and can be taken only once; the executor records admitted camera, blit and copy work before the stock passes in the same frame. `encode_scene` follows this path with no caller additions. The feature owns the pipelines it selected and holds the scene exclusively until it is dropped, with no borrow of the renderer.

Retaining or taking admitted offscreen work does not execute it. `encode_frame` supplies no head, and a `None` head in `encode_frame_with` records the stock draws without refreshing their offscreen inputs. Forward any returned work on the preparation encoder before the stock passes, or abandon the frame. The omission characterizations in `src/draw/tests/program_frame/offscreen/composition/omission.rs` cover both leaving work in the feature and taking it without forwarding; the parent test keeps the forwarded pixel, pass-order and single-submission control.

A frame with screen-space metadata can also split off its depth and normal prepass. `FrameRenderer::stock_depth_prefix` takes the scene and the pipelines the suffix draws with, as `prepare_stock_scene` does, and returns a `StockDepthPrefix`; it refuses a renderer that has no metadata. It decides whether the colour draws write velocity before it plans, so a renderer that renders from the G-buffer under TAA lists the velocity target on its first frame as on any later one. `plan()` lists exactly the entries the prefix completes, `PassEntry::new(PassSlot::DepthNormalPrepass)` and `PassEntry::new(PassSlot::DepthPyramid(0))`, with their views. On a renderer that renders from the G-buffer the producer is fused into the prepass: the entries are the same and the prepass entry attaches the G-buffer targets and any velocity target, whose views the plan then lists. `encode` runs the prefix and appends the caller's own passes, views and features to it before the whole list is validated. A caller feature in a later entry reads the original device depth by declaring the prepass depth attachment of that plan, and reads the lit colour by declaring `scene color`; `PrepareContext::resource` gives it the views and `PrepareContext::completed_in_prefix` tells it which entries the prefix completed. `FeatureTarget::Depth` holds linear metres and is a different resource from the device depth.

`FrameRenderer::capture_texture_keys` and `capture_effect_keys` give the write keys a capture needs when the prefix is submitted alone and the suffix prepared afterwards. A role stays a label: a read of an earlier resource must be declared and is checked like any other.

Caller attachment bindings include the current view's retained-content permission. Changing that permission on the same texture resolves the bindings again. A `Load` without an earlier producer requires `AttachmentView::with_retained_contents()` on the current frame's view; a previously cached retained view cannot authorize it. Refusal occurs before feature preparation or submission. The two permission transitions are covered in `src/compositor/frozen_tests/retained_permission.rs`.

The public composition regressions in `tests/stock_composition.rs` and its `stock_composition` children cover sampled stock depth and lit colour, prefix completion and retry behavior, and retained operations. The operation test checks one callback across prefix and suffix recording, no submission before the caller submits once, and exact red RGB through a readback copied in that encoder. The retry tests retain their distinct-backing and same-backing load-contract refusal controls; `stock_camera_taa.rs` keeps the warm velocity-to-plain regression.

## Game passes

A game that draws with its own render-program programs adds frame features through the renderer configuration, with no second renderer or scheduler. `RendererConfiguration::game_passes` is a `GamePasses` inventory of `GameFeatureFactory` entries. The renderer calls each factory's `build` when it allocates a generation and again after every `reconfigure`, and registers the feature it returns through `FrameRenderer::register`. An empty inventory, the default, leaves every frame as the stock frame.

A factory declares its targets with `TargetDeclaration`: a name, a renderable colour format, `TargetExtent::Frame` or `TargetExtent::Fixed`, array layers, and `TargetLifetime::PerFrame` or `Persistent`. A persistent layer is readable in later frames, and across renderer replacement while its declaration is unchanged, once a *submitted* frame has left contents in it; a cancelled frame leaves the contents and that bookkeeping as they were. A read of a layer nothing wrote is `FrameError::MissingProducer` naming the target at planning.

The targets belong to the renderer. `ConfiguredFrameRenderer::game_targets()` gives the live generation's textures and written layers. A renderer built from a clone of the inventory makes targets of its own, so two renderers never share or rebind a texture. A replacement builds its targets privately from the live ones and keeps every backing whose declaration (every field, with the extent it resolves to) is identical; the renderer holds the new targets only when the whole generation is built, so a replacement that fails, in a factory or anywhere later, leaves the live textures, written layers, generation and budget as they were.

The targets count against the allocation budget. A kept backing is charged once across generations; a backing a replacement makes anew, such as a frame-sized target after a resize, is charged beside the old one until the old generation retires (`AllocationSet::backing_texture`).

A layer is recorded as written when the *last* attachment that wrote that exact texture in a submitted frame stored its contents; a last write that discarded them leaves the layer unwritten. Only the declared texture counts: a stock attachment with the same name is another texture and never marks it, and a pass or read that would stand a declared target in for a same-named attachment of the frame is refused at planning with `FrameError::AttachmentHazard`. A same-frame fitted capture lists each game target's texture and each persistent target's publication in its suffix writes, so a game's passes run inside it, and a capture that fails publishes none of the game's bookkeeping.

```rust,ignore
let passes = GamePasses::new().with(Arc::new(MyGame::new()));
let mut settings = RendererConfiguration::legacy(size, format);
settings.game_passes = passes;
settings.terrain_draws = TerrainDraws::Suppressed; // optional: the game draws the terrain
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
```

`GamePlace::BeforeOpaque` passes run after the G-buffer and its lighting, `AfterOpaque` before the contour IDs and opaque snapshots, `AfterTransparent` before the reflection and screen-space tail. The first game pass to write the frame colour or depth clears it; later game passes and Opaque load what was left, so stock objects compose with the game's output by depth. `PassTarget::of` also works for stock entries (`Opaque`, `OpaqueGbuffer`, `DepthNormalPrepass`, `ShadowCasters(n)`) through `PassTarget::in_plan`.

`GameBuild::shadows`, a `FrameShadowHandle`, lends the stock shadow cascades of the frame in flight. The shadowed route publishes them when the frame's cascades are final (after any same-frame depth fit and refresh, before the features prepare), and only when the renderer has game features. `current()` returns `FrameShadows`: the cascade depth array view the stock casters wrote this frame, the comparison sampler the receivers use with it, and each cascade's `view_projection`, `split_distance` and `depth_bias`, with the selection rule, near distance, view-depth vector, blend, fade and normal bias the stock receivers apply, all from the receiver uniform the frame uploads. It refuses with `FrameInputError::OutsideFrame` when no frame is being prepared, and with `NotPrepared` when the frame in flight published nothing (an unshadowed route, shadows off, or a fit with no receivers); a later frame never receives an earlier frame's cascades. The view and sampler belong to the frame: read them in `prepare` and `draw`, never write through them and never keep them, because the next frame writes the same texture and a renderer replacement frees it. A game pass declares no read of the view: every game place runs after the shadow casters, and `GamePassSpec::reading` refuses a stock resource such as `shadow depth` with `FrameError::AttachmentView`. Tests: `src/shadows/tests/game_shadows.rs`, `tests/game_passes/shadow_cascades.rs`.

`RendererConfiguration::terrain_draws = TerrainDraws::Suppressed` keeps the scene's terrain resident, streamed and updated but skips every stock terrain draw: opaque, G-buffer, depth prefix, metadata and shadow casters. The default `Stock` is byte-identical to a build without the option. A frame with game features is planned without the stock plan cache. Switching the choice between frames refreshes every retained shadow cascade on the next submitted frame, whatever the refresh intervals say. Tests: `tests/game_passes.rs`, `src/compositor/game/tests.rs`, the capture tests in `src/shadows/tests/sdsm/game_targets.rs`, the Observation and Render route tests in `src/shadows/tests/suppressed_routes.rs` and the forward-order tests.

### Stock ambient occlusion for game passes

`GameBuild::occlusion`, a `FrameOcclusionHandle`, lends the screen-space ambient occlusion of the frame in flight. Every route publishes it as it begins encoding the frame, before any feature plans, and `current()` refuses as the shadow handle does: `FrameInputError::OutsideFrame` outside a frame, `NotPrepared` before the frame has published. The two handles share that one error type. `Ok(None)` means the renderer computes no screen-space ambient occlusion. Otherwise `FrameOcclusion` holds the last filter stage's texture and view: red is visibility in `[0, 1]` (1 unoccluded), `OcclusionDepth` says whether green holds nothing, linear view depth in metres (gathering receivers, `Rg32Float`) or depth over the far distance (scalable kernel, `Rg16Float`), and `stock_receivers` says whether the stock receivers sample it. The texture can be smaller than the frame; sample it at the normalised screen position.

A pass that samples the occlusion declares it with `GamePassSpec::sampling_frame([FrameOcclusion::read()])`. The read attaches nothing; frame validation orders the pass after the occlusion filter, and the declaration makes the frame compute the occlusion even when no stock receiver samples it. The SSAO stages come before `GamePlace::BeforeOpaque` in the forward order. Tests: `tests/game_passes/lighting.rs` (unshadowed frame without occlusion, frame with occlusion) and the outside-frame test in `src/compositor/game/lent.rs`.

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
after every Opaque occurrence and before `AfterOpaqueSnapshot`. On Forward,
Opaque draws the scene's opaque surfaces. With `GBUFFER_RENDER`, Opaque follows
`OpaqueGbuffer` and `OpaqueLighting` and draws the remaining forward-only
surfaces, so the ID pass sees the completed scene depth on either path. The
observation path uses the same ordering. Frozen composition refuses a contour
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
