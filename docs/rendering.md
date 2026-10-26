# Rendering features

The renderer is built on [wgpu](https://wgpu.rs) and runs on Vulkan, Metal and DX12. A game never talks to the GPU directly. It describes what to draw, either as native world state that the engine extracts each frame (2D) or as a `RenderScene` (3D), and the renderer validates the description before allocating anything. An unsupported or out-of-range setting fails with a typed error at that point, before the first frame.

Each section below links to the developer page with the exact contract, for when you need units, ranges or ordering guarantees.

## 2D: sprites, tiles and UI

2D games draw from the native world. Each frame, the engine extracts sprites, tiles, UI text and the active camera, and draws them in the scene's color target.

- **Sprites and tiles** take a texture region, pivot, pixels-per-unit, linear RGBA tint, flips and a sorting layer and order. Sorting is stable: equal keys keep their submission order. Screen-space UI draws after world sprites with its own sorting.
- **Cameras** are orthographic. The enabled camera with the highest depth is drawn, and its clear color, including alpha, is kept.
- **Texture sampling** is set per texture: point, bilinear or trilinear filtering, and repeat, clamp or mirror wrapping on each axis independently.
- **Lit sprites** use global 2D lights that multiply sprite color per sorting layer, following URP's behavior for global lights. Point, spot and freeform lights and 2D shadows are not implemented.
- **UI text** is laid out natively from a Canvas and RectTransforms, rasterized from a TrueType font the game supplies, and drawn as sprites.

Contracts: [native sprites](../dev-docs/simulation/sprites.md), [2D textures and lighting](../dev-docs/rendering/2d.md), [UI layout and text](../dev-docs/simulation/ui.md).

## 3D scenes

A `RenderScene` holds meshes, materials, textures, object placements, optional terrain and an optional lighting environment. Scene coordinates are right-handed and Z-up, authored in integer millimetres.

- **Materials** are metallic-roughness, opaque or alpha-masked, with optional baked or vertex ambient occlusion, tint, wind animation and thin-surface transmission.
- **Meshes** can have several LOD levels with distance ceilings, and any level can be a camera-facing billboard card. Objects can be added, moved, hidden and removed every frame without re-uploading the scene.
- **Terrain** is a rectangular heightfield with blended materials. Large terrains can stream adaptive material pages based on screen-space detail.
- **Shadows** come from one directional light with one to four cascades, as fixed coverage boxes or camera-distance splits. Far cascades can refresh less often than the near one to save GPU time.
- **Water** bodies are rectangles, basin heightfields or authored meshes, with refraction, absorption, animated or texture-driven normals and flow, and environment reflections.
- **Sky** can be a procedural gradient with a sun disc, or an equirectangular dome texture, plus textured discs and meshes for moons and planets. Height fog applies to geometry, sky and water.
- **Image-based lighting** prefilters an environment cube map and a BRDF lookup table on the GPU for reflections.

Contracts: [scene materials and water](../dev-docs/rendering/scene-materials.md), [terrain material pages](../dev-docs/rendering/terrain-pages.md), [atmosphere and fog](../dev-docs/rendering/atmosphere.md), and the [render-scene README](../crates/render-scene/README.md) for LOD, instance editing and shadows.

## Frame profiles and post-processing

A renderer is built with one frame profile. The **legacy profile** shades straight to an sRGB target with one sample, which is enough for unlit 2D. The **HDR profile** renders 16-bit float radiance with 1x or 4x MSAA, then runs a post chain before writing the display image. These effects need the HDR profile:

- **Exposure**: fixed, or adaptive from a luminance histogram with separate rates for brightening and darkening. Capture modes can fix the exposure or the time step so output is reproducible.
- **Bloom**: a threshold with a soft knee, up to eight pyramid levels, using either a tent-filter pyramid or a separable Gaussian pyramid.
- **Tone mapping**: Reinhard.
- **FXAA**, which can be combined with MSAA.
- **Upscaling**: render at 50% to 100% of the output size and reconstruct with FSR 1 EASU or bilinear filtering, with optional RCAS sharpening.
- **SSAO**: hemisphere or spiral sampling with a depth-aware filter. It darkens ambient light only.
- **Screen-space reflections** on opaque surfaces and water, blended with the environment reflection by confidence and roughness.

Temporal antialiasing, vignette, filmic tone curves, color grading beyond exposure, and sorted alpha-blended 3D geometry are not implemented. Requesting one fails with a typed error.

Contracts: [frame executor](../dev-docs/rendering/frame-executor.md), [post filters](../dev-docs/rendering/post-filters.md), [ambient occlusion](../dev-docs/rendering/ambient-occlusion.md), [reflections](../dev-docs/rendering/reflections.md), and the [render-post README](../crates/render-post/README.md) for exposure, bloom and upscaling settings.

## Windows, headless rendering and capture

`scene-viewer` presents a scene in a desktop window with up to three frames in flight, handles resize and minimize, and recovers from a lost surface. The same renderer can render headless to an offscreen target and read the pixels back, which is how the GPU tests check exact output bytes. A host can also schedule its own render passes alongside the stock ones, or read back GPU timings for every pass.

Contracts: [rendering services](../dev-docs/rendering/services.md), [frame executor](../dev-docs/rendering/frame-executor.md), [resource contracts](../dev-docs/rendering/resource-contracts.md).
