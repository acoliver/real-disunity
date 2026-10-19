# Render planning

`worldrunner-render-plan` is the CPU-only scene and document boundary for Real
Disunity. The package name is retained for application API compatibility.

- `RenderScene` owns validated geometry, materials, textures, optional terrain,
  and explicit effects. It does not resolve paths or load assets.
- `ContractDocument` describes caller-authored recipes and exact routes. Its
  schema is `real-disunity.render-contract.v1`, version 1. Discovery selects
  `urn:real-disunity:capability:render-contract:v1` exactly. Other schema families
  are rejected; there is no implicit migration or alias.
- Contract and implementation registries bind caller-supplied document,
  component, capability, payload, and scene factories. Planning retains the
  validated scene and verified publication provenance.
- Capability helpers decode canonical engine artifacts, packages, exact bytes,
  and bounded U8/U16 grids. They never infer routes from content.
- `world_schema` preserves the shared CPU world-schema vocabulary.

There is no built-in content adapter registry, inventory interpreter, source
terrain/clutter table, proprietary decoder, or bundled content. Producers must
supply canonical artifacts or construct scenes through the validated public API.
