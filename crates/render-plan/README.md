# Render planning

`worldrunner-render-plan` is the CPU-only scene and document boundary for Real
Disunity. It also extracts native `world-simulation` sprites, tiles, cameras and
UI into scene contributions (the `simulation` and `ui` features).

- `RenderScene` owns validated geometry, materials, textures, optional terrain,
  and an optional environmental profile. No profile means unlit albedo plus
  emission, without lighting or grading. It does not resolve paths or load assets.
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

There is no built-in content adapter registry, inventory interpreter, source
terrain/clutter table, format-specific decoder, or bundled content. Producers must
supply engine artifacts or construct scenes through the validated public API.

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
but only one resolve. `render-scene` maps authored TAA to `PostResolve::Taa` with that history weight (at most 0.99). Bloom and exposure precede the selected tone curve (Reinhard or
fitted ACES), followed by FXAA. MSAA and FXAA are allowed together: MSAA resolves
HDR coverage before metering and FXAA filters the display-linear result.
Contrast and saturation remain unsupported.
