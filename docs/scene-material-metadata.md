# Scene materials and frame metadata

Scene inputs remain owned CPU data. TextureHandle, MaterialHandle and MeshHandle address the existing resource tables. Rendering does not resolve paths or decode application content.

SceneMaterial::with_features attaches typed tint, baked/vertex occlusion, wind and layer descriptions without rebuilding the base material. Layer weights and tint/occlusion factors use millionths. Layer references must address the same material table, and nested layers are rejected. SceneObject carries an independent instance tint. SceneMeshLod::with_card extends its existing billboard policy and rejects conflicting orientation policies.

Positions and card sizes use millimetres in the right-handed Z-up scene frame. Wind direction uses SceneDirection in that same frame, amplitude uses millimetres, period uses milliseconds, and phase is a fraction of a cycle in millionths. These descriptions perform no implicit basis changes. The existing view transform owns the rendering basis conversion.

SceneWaterBody replaces the rectangular plane name. It retains elevation, rectangle bounds, scattering colour, absorption per metre and roughness. All optical ranges are checked at scene ingress. SceneEffects::with_water_settings describes an authored mesh, normal map, flow map and refraction displacement through the same resource tables. Settings require a body. A missing refraction bound (`None`) preserves stock procedural refraction. Every authored bound (`Some`), including zero, is rejected by stock rendering until displacement limiting is implemented.

## Rendering support

This change establishes typed inputs for subsequent pipeline work. Stock rendering accepts absent descriptors and neutral material features. Nonneutral tint/AO, wind, layers, card dimensions/transitions and non-stock water settings return explicit unsupported-input errors before scene uploads. Runtime placement insertion also rejects unsupported tint. No shader approximation is selected silently.

The existing mesh and billboard vertex transform is reused by forward, shadow and metadata passes. Their masked fragments call one alpha-coverage predicate. Terrain remains opaque. Water writes surface depth but does not yet produce normal/material reflection receivers. Dynamic transparent quads remain outside the opaque metadata prepass. Adding a producer requires matching its deformation, coverage and numerical products, rather than substituting metadata from a different geometry path.

## Shared products

FrameProduct describes an existing executor ResourceRef, numerical meaning, texture format and sampling domain. It does not allocate textures or mint another resource identity. SSAO prepass, reflection material and water-depth snapshot declarations use these descriptions. Shared source validation requires matching extent and sampling, D2 single-layer textures and sampled-texture usage.

| Product | Representation | Coverage and units |
| --- | --- | --- |
| Prepass depth | Depth32Float | Device depth, cleared to one |
| Prepass normal | Rgba16Float | View-space unit normal RGB, coverage alpha; uncovered alpha zero |
| Reflection material | Rgba16Float | Linear F0 RGB, perceptual roughness alpha |
| Conservative depth | R32Float | Nearest positive view depth in metres |
| Averaged depth/normal | Rgba16Float | Separate declared reduction meaning, not a traversal input |
| Environment specular | Rgba16Float | Linear HDR replaceable specular radiance, not total shaded colour |

The last two meanings are descriptions for explicit producer integration; stock rendering does not allocate or publish those products. Pixel-centre prepass coverage remains distinct from forward MSAA coverage. Per-sample source products permit only 1x and 4x. Forward depth snapshots declare conservative resolution with the actual 1x or 4x source count and one stored sample; pixel-centre hierarchy depth retains its separate sampling policy. Reduced depth products must be single-sample. Formats with sRGB transfer functions are rejected for metadata and specular radiance.

All stock resource names, frame stages, allocation/publication ownership and rendering outputs remain unchanged. Metadata declarations acquire no independent publication lifecycle. Cancellation uses the existing prepared-frame transaction and never submits its encoded writes.
