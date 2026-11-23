# Render planning

`worldrunner-render-plan` is the CPU-only scene boundary for Real
Disunity. With the `simulation` feature it also extracts native `world-simulation`
mesh instances into retained scene objects and holds the scene textures that
sprites draw with. Sprite, tile, camera and UI extraction lives in
`world-simulation`; `worldrunner-render-scene` draws its plan.

- `RenderScene` owns validated geometry, materials, textures, optional terrain,
  and an optional environmental profile. No profile means unlit albedo plus
  emission, without lighting or grading. It does not resolve paths or load assets.
- `SceneResources` clones share texture and material tables. `with_materials`
  replaces materials and validates the complete result without copying textures.
  `try_into_tables` returns both original vectors only when both tables are
  uniquely owned, preserving table allocations and texture payload addresses.
  Otherwise it returns the resource value unchanged. Callers can append to the
  owned vectors and validate them again with `SceneResources::new`.
- `SceneTexture::Released` stands in for a texture whose source bytes were
  dropped. It holds a `SceneTextureDescriptor`: the texel format, extent and mip
  count of a texel grid; the block format, colour space, extent and mip count of a
  validated `DDS` container; or the format, size, mip count, preparation request and
  face order (+X, -X, +Y, -Y, +Z, -Z; face-major, finest mip first) of a specular
  cube. Every descriptor also records the exact source length in bytes. A
  descriptor is validated like the source it replaces, so a released table keeps
  dense texture handles, material channel admission and fade-threshold admission.
  `SceneResources::with_textures` swaps the texture table, shares the material
  table and validates the result. `logical_texture_bytes` returns the checked
  source-payload total of every texture, released or not. Adoption, append and
  replacement reject aggregate overflow; failed mutations preserve the table and
  total. Incremental updates account only for the changed textures, and the total
  query does not scan the table. `retained_texture_source_bytes` sums
  only the payload bytes that table value still reaches, so it is zero after every
  texture is released. Both are table arithmetic: another table value that shares
  or clones the sources keeps its own bytes, and neither number measures process
  memory. Operations that need source bytes report the texture as released.
  `worldrunner-render-scene` validates a table and converts it with
  `release_texture_sources`, because only its DDS reader can establish container facts.
- `SceneDirection` validates a unit vector quantized to integer micro-components.
  A scene sun points from a surface toward the light in right-handed Z-up
  coordinates: `[east, north, elevation]`. The direction type performs no basis
  conversion. JSON directions contain three integers, never a quaternion.
- `world_schema::scene_vector_from_world`, `SceneTransform::from_world_schema`,
  and `SceneVertex::from_world_schema` share the checked basis change from schema
  Y-up XYZ to scene Z-up `(x, -z, y)`. Producers quantize units before crossing
  this boundary. Positions, normals, and quaternion axes must cross together.

## CPU object picking

`RenderScene::pick(ScreenRay, objects, include)` tests world-space metre rays
against `(ObjectId, SceneObject)` placements referencing the scene's resident
mesh and material tables. `ObjectId` is the shared retained identity type.
The predicate can implement inclusion or exclusion sets without allocating a
query-specific object list. Nonfinite and zero-direction rays return `None`;
other directions are normalized, so `PickHit::distance_meters` is a distance
from the supplied origin. Camera screen rays start on the near plane.

Each mesh LOD caches its local bounds at construction. A query transforms the
LOD-zero bounds to a world AABB, rejects nonintersecting placements, then tests
indexed triangles with a double-precision ray in local space. It uses the same
model matrix as rasterization, including quaternion normalization and float
unit conversion. Exact distance ties choose the lowest `ObjectId`, then the
lowest triangle index. Single-sided materials reject back faces; LOD material
slots override the placement material. Misses return `None`.

This query uses undeformed LOD-zero geometry. It does not sample texture alpha,
shader wind, fade dithering, billboard replacements, terrain or water. Transform
scale is positive and uniform, as required by `SceneTransform`; non-uniform
scaling must already be in resident vertex positions. It allocates no spatial
index or per-query triangle storage. Worst-case work is O(objects * triangles),
with O(objects) bounds rejection when all objects miss. The `measure_pick_cost`
test prints timings for 1000 objects with 100 triangles each, both all-hit bounds
and all-rejected bounds, with no machine-dependent timing assertion.

## Terrain normal inputs

Terrain source texel columns and rows increase layer U and V without a vertical
flip. Normal channels decode as `2 * value - 1`. Appearance rotation is inverted
on each layer normal before blending into the common east/north material frame.
Page callbacks supply already-unrotated directions. See the public
[`LayerSample::normal`](../render-scene/src/terrain/pages/composition.rs) and
[`TerrainPageSource::sample`](../render-scene/src/terrain/pages/scene.rs) contracts
and the [terrain page contract](../../dev-docs/rendering/terrain-pages.md).

## Material scalar channels

`SceneMaterial::with_channels(SceneMaterialChannels)` selects `TextureChannel::R`,
`G`, `B` or `A` for roughness, metalness, ambient occlusion and self-illumination.
The default is G/B/R/R. Roughness and metalness share the roughness-metallic map;
the other two selectors address their respective maps. Scalar factors still
multiply the selected values. Translucency masks reinterpret selected metalness.

Packed impostors use `SceneImpostor::with_material_channels` independently of
mesh sampling. Their default is R/G/G/R: roughness and metalness/translucency
from RT, then occlusion and emission from the ambient-occlusion map. Material-mode
impostors and fixed mesh LODs use `SceneMaterial::channels` instead. Decals reject
non-default material selectors with `UnsupportedMaterialChannels` and keep their
separate R/B roughness/metalness layout. Ordinary terrain palettes also reject
non-default selectors with that typed error and keep G/B sampling. Layered terrain
keeps its independent `TerrainLayerChannelMap` contract.

Scalars may share a stored channel, with their factors applied independently.
For example, R8 and BC4 can provide R for both roughness and metalness.
`TextureChannel::try_from(u32)` rejects indices outside 0..4 with
`InvalidTextureChannel`. Resource admission rejects absent stored channels with
`MissingTextureChannel`, including BC4's non-R lanes and BC5's B/A lanes. DDS
header parsing reads metadata without copying or decoding blocks; malformed scalar
map headers return `InvalidMaterialTextureFormat`. Scene closure also checks any
packed-impostor material used on a fixed mesh against its mesh-channel selectors.
Unsupported encoded container types remain the renderer's texture-admission errors.

## Reducible textures

`RenderScene::with_reducible_textures` declares the textures whose top mip levels
the renderer may drop at upload to fit adapter memory (GitHub #376). None is
reducible by default, so a texture keeps its authored chain until its producer
opts in. `SceneResources::reducible_textures` returns the declaration, which
survives placement edits and `with_textures`. Validation refuses a handle outside
the table, and refuses by name a declared handle that the scene also binds through
a consumer reading level 0 or the authored size: the environment cube and BRDF
table, the sky dome, celestial discs, water normal, flow and wind maps, material
wind fields, fade threshold textures, impostor relief and border maps, terrain
palette maps, layer sources and colorization. Impostor material atlas maps also
stay authored so dropping levels cannot invalidate their frame grid. UI overlay
textures are a separate dynamic table and never use scene handles.

## Material anisotropy

`SceneMaterial::with_anisotropy` accepts a validated `TextureAnisotropy` and applies
it to all resident material channels, including impostor auxiliary maps. The
default is 1. Construction accepts only 1, 2, 4, 8 and 16. Resident material
samplers use linear minification, magnification and mip filtering with repeat
wrapping regardless of per-texture `TextureSampler` settings. Materials with
equal levels share a sampler; shared textures do not force equal material levels.
Dynamic quads and sprites continue to use their texture filter/wrap settings
without anisotropy. `TerrainLayeredSource::anisotropy` selects the same validated
levels for source images; 1 preserves isotropic sampling bytes. Higher levels use
portable shader taps along the major footprint axis with explicit trilinear LOD,
bounded by the footprint ratio and authored level. Hardware source samplers stay
at 1 to avoid backend-dependent anisotropy. The same setting governs cached-page
reads, with at most 16 manual trilinear taps and per-tap gutter-safe page lookup.
See [texture sampling](../../dev-docs/rendering/2d.md#texture-sampling) for device
clamping and [terrain pages](../../dev-docs/rendering/terrain-pages.md) for the
terrain sampling limitations.

## Independent shadow light

`SceneEffects::with_shadow_direction(Some(direction))` selects a validated
`SceneDirection` toward the shadow light in scene Z-up coordinates. The default
is `None`, which follows `SceneSun::direction`. Passing `None` restores that
behavior. Direct irradiance, sky lighting and the celestial sun keep the direct
sun direction. Zero or non-unit integer directions return
`SceneValidationError::DirectionNotNormalized`; non-finite components cannot be
represented. Both hemispheres are supported, matching the lighting service.

## Screen-space reflections

The optional `SceneSsr` policy carries `enabled`, `max_steps`,
`thickness_millimeters`, `roughness_cutoff_micro` and `resolution_scale_micro`.
`SceneEffects::with_ssr` installs the validated policy; omitting it leaves
reflections off.

The HDR compositor traces the immutable after-opaque colour snapshot against
single-sample prepass depth and shading normals. A separate matching geometry
pass records material F0 and roughness, including metallic-roughness textures
and terrain blends. Reflections affect only the environment-specular lobe;
direct specular and diffuse ambient are unchanged. Reflection textures carry
premultiplied confidence and use up to six ceil-divided blur levels. No
temporal history is retained. This is a screen-space approximation with a
100-metre maximum ray length and no offscreen reflection data. Composition with
installed environment maps and water receivers is described in
[screen-space reflection composition](../../dev-docs/rendering/reflections.md).

## HDR post processing

The optional `SceneHdrPost` policy requires `adaptive_exposure`, `bloom`
and `fxaa_subpixel_micro`, with explicit null for disabled options. Callers
adopt it with `SceneEffects::with_hdr_post`. All nested quantities are required.
Adaptive exposure defines a 256-bin log2 luminance range in milli-stops, a
retained low/high percentile interval, middle-grey key, exposure bounds and
separate exponential approach rates per second. Rates use milli-units; exposure,
percentiles and key use millionths. Capture policies select fixed delta in
microseconds or an absolute fixed exposure. Fixed exposure bypasses adaptation.

Bloom defines linear threshold, fractional soft knee, intensity and one to eight
pyramid levels. Existing runtime `ScenePostStage::Bloom` selects knee 0.5 and five
levels. Only one bloom is accepted; duplicate controls and vignette fail
with typed errors. FXAA may be requested through `hdr_post` or `SceneAntiAlias`;
TAA and SMAA use `SceneAntiAlias` programmatically. Only one resolve is accepted. `SceneAntiAlias::Taa { history_micro, clip_scale_micro }` takes a history weight and a clip-box scale in micro-units (`clip_scale_micro` within `500_000..=2_000_000`, 1_000_000 by default through `SceneAntiAlias::taa`); TAA has no serialized field, only this programmatic setting. `render-scene` maps authored TAA to `PostResolve::Taa` with those values (history weight at most 0.99). Bloom and exposure precede the selected tone curve (Reinhard or
fitted ACES), followed by FXAA or SMAA when selected. MSAA can accompany either:
MSAA resolves HDR coverage before metering and the spatial resolve filters the
display-linear result.
Contrast and saturation remain unsupported.

`SceneAntiAlias::smaa(SceneSmaaPreset::Ultra)` selects SMAA 1x with luma edges.
`SceneSmaaSettings::{LOW, MEDIUM, HIGH, ULTRA}` provide the four presets;
`SceneAntiAlias::Smaa(settings)` accepts custom settings with
`SceneSmaaEdgeDetection::{Luma, Color}`. `threshold_micro` is in `1..=500_000`,
`max_search_steps` in `0..=112`, `max_search_steps_diag` is `None` or `0..=20`,
and `corner_rounding` is `None` or `0..=100`. Optional zero enables the feature
with zero steps or rounding; `None` disables it. `validate` returns
`SceneValidationError::RangeViolation` naming the field and its bounds.
Effects construction validates the same quantities before adopting the policy.
These integer scene types carry no GPU dependency. `render-scene` translates them
into the existing three-pass post resolve without automatic projection jitter.
There is no serialized SMAA field. CMAA2 is not exposed by `SceneAntiAlias`.

## Shared specular environment

`SceneEffects::with_environment` selects an optional `SceneEnvironment` by texture
handles. Its cube is `SceneTexture::SpecularCube`, using the existing
`SceneSpecularCube`: linear BC6H unsigned-float radiance with a complete mip chain,
or linear RGBA16Float radiance with an explicitly sized mip chain. Faces are
+X, -X, +Y, -Y, +Z, -Z, with finest-to-coarsest levels per face.

The split-sum LUT is a square, single-mip linear RG16Float or RGBA16Float texel
grid. RG stores A,B at x = view cosine, y = roughness. Its roughness and visibility
convention must match the scene. Consumers select water, reflection fallback, or
both, and must be enabled. Scene construction rejects missing handles, wrong
resource kinds, LUT formats and mismatched conventions with `SceneValidationError`.
`SceneEnvironment::new` defaults `specular_intensity_micro` to `1_000_000`.
The accepted range is `0..=64_000_000`; larger values return
`SceneValidationError::EnvironmentIntensity`. This linear multiplier scales only
the cube-derived lobe, once per selected consumer. Diffuse SH, direct light,
BRDF factors, scene-hit radiance and source payloads remain unchanged. Water
retains its independent per-body environment amplitude.
Retained settings updates replace or remove the environment without uploading the
scene again. Omitting it preserves the stock empty-environment rendering.

## Water surface sampling

`SceneWaterSettings::normal_anisotropy` uses `TextureAnisotropy`, accepting only
1, 2, 4, 8 or 16. `STOCK` selects 1. Enable `normal_mips` to use derivative-selected
normal sampling with the authored ratio. Without normal mips, the explicit
mip-zero bilinear path is unchanged. Water uses the scene's `direct_diffuse`
convention, including `SceneDirectDiffuse::LambertianPrepared` for prepared sun
colours. Per-body `sun_disk` controls remain independent of the scene surface disk.

`SceneSsao.receiver` selects nearest filtered-AO lookup by default, or a depth-aware four-tap gather. Both enum choices are valid with every supported kernel/filter pair. See [ambient occlusion](../../dev-docs/rendering/ambient-occlusion.md#full-resolution-receivers) for metre weighting and composition.

## Post-processing admission

`worldrunner_render_plan::post` owns `RenderScale`, `UpscaleMode`,
`UpscaleSettings`, `PostContractError` and `MAX_GAUSSIAN_RADIUS`. These CPU contracts
are shared with `render-post` without a GPU dependency or forwarding aliases.
`RenderScale::new` accepts integer millionths in `500_000..=1_000_000`;
`extent` rejects zero dimensions and rounds to nearest, with ties upward, using
64-bit integer arithmetic. `UpscaleSettings::validate` accepts absent RCAS or
finite sharpness in `0..=2` stops. The Gaussian support radius bound is 64 texels.
Admission failures use `UnsupportedProfile(&'static str)` or `EmptyExtent`.
The CPU-consumer integration tests cover the contracts, and `cargo xtask policy`
rejects GPU dependencies anywhere in this crate's transitive closure.
