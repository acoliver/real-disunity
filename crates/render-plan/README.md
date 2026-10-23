# Render planning

`worldrunner-render-plan` is the CPU-only scene and document boundary for Real
Disunity. The package name is retained for application API compatibility.

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
  schema is `real-disunity.render-contract.v3`, version 3. Discovery selects
  `urn:real-disunity:capability:render-contract:v3` exactly. Other schema families
  are rejected; there is no implicit migration or alias. Its `EffectsRecipe`
  remains mandatory, including sky and SSAO fields. Omitting these authored
  fields fails deserialization. Version 3 adds required SSAO `sample_count` and
  `bias_millimeters`; version 2 documents and discovery bindings are rejected.
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
terrain/clutter table, proprietary decoder, or bundled content. Producers must
supply canonical artifacts or construct scenes through the validated public API.
