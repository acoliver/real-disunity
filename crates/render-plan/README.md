# Render planning

`worldrunner-render-plan` is the CPU-only scene and document boundary for Real
Disunity. It also extracts native `world-simulation` sprites, tiles, cameras and
UI into scene contributions (the `simulation` and `ui` features).

- `RenderScene` owns validated geometry, materials, textures, optional terrain,
  and an optional environmental profile. No profile means unlit albedo plus
  emission, without lighting or grading. It does not resolve paths or load assets.
- `SceneResources` clones share texture and material tables. `with_materials`
  replaces materials and validates the complete result without copying textures.
  `try_into_tables` returns both original vectors only when both tables are
  uniquely owned, preserving table allocations and texture payload addresses.
  Otherwise it returns the resource value unchanged. Callers can append to the
  owned vectors and validate them again with `SceneResources::new`.
- `SceneDirection` validates a unit vector quantized to integer micro-components.
  A scene sun points from a surface toward the light in right-handed Z-up
  coordinates: `[east, north, elevation]`. An authored document sun uses XYZ
  components under that document's caller-specified `CoordinateBasis` instead.
  The application composition factory must convert that direction to the scene
  basis when constructing `SceneSun`; the direction type performs no basis
  conversion. JSON directions contain three integers, never a quaternion.
- `ContractDocument` describes caller-authored recipes and exact routes. Its
  schema is `real-disunity.render-contract.v4`, version 4. Discovery selects
  `urn:real-disunity:capability:render-contract:v4` exactly. Other schema families
  and versions are rejected, with no conversion or alias. Its `EffectsRecipe` is
  mandatory, including sky, SSAO (`sample_count`, `bias_millimeters`) and water
  (`height_millimeters`, linear `color_micro`, per-metre `absorption_micro`,
  `roughness_micro`) fields. Omitting an authored field fails deserialization.
  Runtime `RenderScene` profile optionality does not change the document contract. There is no stock document-to-scene
  interpreter; the application composition factory chooses the runtime profile
  and which authored requests it implements.
- Contract and implementation registries bind caller-supplied document,
  component, capability, payload, and scene factories. Planning retains the
  validated scene and verified publication provenance.
- Capability helpers decode canonical engine artifacts, packages, exact bytes,
  and bounded U8/U16 grids. They never infer routes from content.
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

## Governed payload preparation and composition

`activate_render_payload_contract` binds the authored handlers and adapters
without requiring scene factories. `prepare_discovered_render_world_payloads`
then checks that discovery matches the publication's unique bare render-contract
binding ID and content address and the activated document. Both discovered entry
points reject a mismatch with `GovernedContractSubstitution`. Preparation checks
authored route and asset closure, runtime preparation, contribution limits and
payload adaptation. It returns
`PreparedRenderWorldPayloads`, retaining the contract binding ID, content address,
publication identities, root payload and owned adapted contributions. No route
scene factory or document composer runs during preparation. The low-level
`prepare_render_world_payloads` deliberately accepts a caller-supplied contract
and has no discovery binding, matching `build_render_world_plan`.

`payloads_for_route(kind, route_key)` selects by exact component schema or
capability URI, independently of shared payload contracts. Each contribution
retains its route, claimant, handler descriptor and payload contract. A declared
idle route returns an empty iterator; an undeclared route returns
`AuthoredRouteMismatch` with its kind and exact name. Selection does not choose
one payload when several match. The consumer must enforce its join cardinality
and reject missing, ambiguous or incorrectly typed inputs.

For rendering, use `activate_render_world_contract`, or call
`ActivatedRenderPayloadContract::into_render_contract` on the original payload
activation. Admission requires the document composer and every declared route's
scene factory, including idle routes. `compose_render_world_plan` consumes the
prepared result and invokes those factories through the existing composition
path. The activation must be the same instance that prepared the payloads;
a separate activation, even of equal authored declarations, is rejected with
`AuthoredRouteMismatch` naming the prepared executable contract.

`DocumentSceneInput::payloads_for_route` gives the document composer the same
route-qualified adapted inputs alongside the root and proven scene contributions.
A composer can therefore join separately decoded resources before constructing
one scene. A route factory may explicitly return
`SceneContribution::NonRendering` for metadata or other join-only inputs. This
outcome retains its claimant and adapted payload. Omitting the factory still
returns `MissingSceneFactory`. Every `RenderWorldPlan` requires a validated
scene; preparation never manufactures one or substitutes an empty scene.

There is no built-in content adapter registry, inventory interpreter, source
terrain/clutter table, format-specific decoder, or bundled content. Producers must
supply engine artifacts or construct scenes through the validated public API.

## Terrain normal inputs

Terrain source texel columns and rows increase layer U and V without a vertical
flip. Normal channels decode as `2 * value - 1`. Appearance rotation is inverted
on each layer normal before blending into the common east/north material frame.
Page callbacks supply already-unrotated directions. See the public
[`LayerSample::normal`](../render-scene/src/terrain/pages/composition.rs) and
[`TerrainPageSource::sample`](../render-scene/src/terrain/pages/scene.rs) contracts
and the [terrain page contract](../../dev-docs/rendering/terrain-pages.md).

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

The optional `ssr` effects recipe carries `enabled`, `max_steps`,
`thickness_millimeters`, `roughness_cutoff_micro` and `resolution_scale_micro`.
All quantities are required within a present recipe. A v4 document may omit the
whole recipe. `SceneEffects::with_ssr`
installs the corresponding validated runtime policy; factories explicitly copy
`EffectsRecipe::ssr()` when composing runtime effects.

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

The optional `hdr_post` recipe may be omitted from a v4 document. A present recipe requires `adaptive_exposure`, `bloom`
and `fxaa_subpixel_micro`, with explicit null for disabled options. Factories
adopt it with `SceneEffects::with_hdr_post`. All nested quantities are required.
Adaptive exposure defines a 256-bin log2 luminance range in milli-stops, a
retained low/high percentile interval, middle-grey key, exposure bounds and
separate exponential approach rates per second. Rates use milli-units; exposure,
percentiles and key use millionths. Capture policies select fixed delta in
microseconds or an absolute fixed exposure. Fixed exposure bypasses adaptation.

Bloom defines linear threshold, fractional soft knee, intensity and one to eight
pyramid levels. Existing runtime `ScenePostStage::Bloom` selects knee 0.5 and five
levels. Only one bloom is accepted; duplicate controls and vignette fail
with typed errors. FXAA or TAA may be requested through the recipe or runtime AA choice,
but only one resolve. `SceneAntiAlias::Taa { history_micro, clip_scale_micro }` takes a history weight and a clip-box scale in micro-units (`clip_scale_micro` within `500_000..=2_000_000`, 1_000_000 by default through `SceneAntiAlias::taa`); TAA has no serialized document route, only this programmatic recipe. `render-scene` maps authored TAA to `PostResolve::Taa` with those values (history weight at most 0.99). Bloom and exposure precede the selected tone curve (Reinhard or
fitted ACES), followed by FXAA. MSAA and FXAA are allowed together: MSAA resolves
HDR coverage before metering and FXAA filters the display-linear result.
Contrast and saturation remain unsupported.

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
