# Rendering features

The renderer is built on [wgpu](https://wgpu.rs) and runs on Vulkan, Metal and DX12. A game never talks to the GPU directly. It describes what to draw, either as native world state that the engine extracts each frame (2D) or as a `RenderScene` (3D), and the renderer validates the description before allocating anything. An unsupported or out-of-range setting fails with a typed error at that point, before the first frame.

Each section below links to the developer page with the exact contract, for when you need units, ranges or ordering guarantees.

## 2D: sprites, tiles and UI

2D games draw from the native world. Each frame, the engine extracts sprites, tiles, UI text and the active camera, and draws them in the scene's color target.

- **Sprites and tiles** take a texture region, pivot, pixels-per-unit, linear RGBA tint, flips and a sorting layer and order. Sorting is stable: equal keys keep their submission order. Screen-space UI draws after world sprites with its own sorting.
- **Cameras** are orthographic. The enabled camera with the highest depth is drawn. Solid Color and Skybox clear to the background color, including alpha; a skybox material is not drawn. Depth Only and Nothing need a camera below them and are rejected.
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

A renderer is built with one frame profile. The **legacy profile** shades straight to an sRGB target with one sample, which is enough for unlit 2D. The **HDR profile** renders 16-bit float radiance with 1x, 2x, 4x or 8x MSAA, then runs a post chain before writing the display image. These effects need the HDR profile:

- **Exposure**: fixed, or adaptive from a luminance histogram with separate rates for brightening and darkening. Capture modes can fix the exposure or the time step so output is reproducible.
- **Bloom**: a threshold with a soft knee, up to eight pyramid levels, using either a tent-filter pyramid or a separable Gaussian pyramid.
- **Tone mapping**: Reinhard.
- **FXAA** and **SMAA 1x**, which can be combined with MSAA. See [choosing antialiasing](#choosing-antialiasing).
- **Upscaling**: render at 50% to 100% of the output size and reconstruct with FSR 1 EASU or bilinear filtering, with optional RCAS sharpening.
- **SSAO**: hemisphere or spiral sampling with a depth-aware filter. It darkens ambient light only.
- **Screen-space reflections** on opaque surfaces and water, blended with the environment reflection by confidence and roughness.

Temporal antialiasing (see below), vignette, filmic tone curves, color grading beyond exposure, and sorted alpha-blended 3D geometry are not implemented. Requesting one fails with a typed error.

### Choosing antialiasing

One `AntiAliasing` value holds the choice: an `MsaaSamples` count (`X1`, `X2`, `X4` or `X8`) and a `PostResolve` (`None`, `Fxaa`, `Smaa` or `Cmaa2`) and optional CAS sharpening. Pass it to `FrameRenderer::hdr_antialiased`.

| Choice | Cost | What it smooths |
| --- | --- | --- |
| MSAA | Memory and fill rate grow with the count. | Geometry edges, using coverage samples. Shader aliasing and thin alpha-tested detail stay. |
| FXAA | One full-screen pass after tone mapping. | Edges by contrast, including shader and texture aliasing, with some softening. |
| SMAA 1x | Three full-screen passes after tone mapping, plus two small intermediate targets. | Edges by contrast, blended by the shape of the edge. Near-horizontal, near-vertical and diagonal lines come out cleaner and thin lines stay crisper than with FXAA. |
| CMAA2 | A buffer clear, three compute passes and one full-screen pass after tone mapping, plus about 32 MB of storage at 1080p. | Edges by contrast, blended by shape, with extra care for text and thin detail. Needs compute shaders. |
| CAS (sharpening) | One full-screen pass after the resolve, plus one `Rgba16Float` intermediate. About 0.4 to 0.5 ms at 1920x1080 on a Radeon 780M. | Nothing is smoothed. It restores local contrast that FXAA, SMAA or a soft texture filter removed. |

MSAA 4x with no post resolve is the usual starting point. Add FXAA when the scene has specular or alpha-tested aliasing that coverage samples do not reach. 1x and 4x work on every adapter. 2x and 8x depend on the adapter and format: a count the adapter cannot render for `Rgba16Float` color and `Depth32Float` depth fails with `PostError::SampleCount` before any target is allocated. It is never reduced to a nearby count. Mesa lavapipe supports 1x, 4x and 8x and rejects 2x.

Prefer SMAA over FXAA when the image is dominated by long high-contrast edges (UI-like geometry, wireframe, fences, power lines, text in the world) and the extra passes fit the frame budget. SMAA reads clean stair-step shapes, so it works best on the geometry edges that remain after MSAA, or with no MSAA at all. It does not smooth texture or shader aliasing inside a surface, and it leaves a perfectly straight axis-aligned edge alone because there is no stair step to resolve. FXAA is cheaper and touches more of the image, which suits noisy specular detail. Use `PostResolve::smaa(SmaaPreset::Medium)` as a starting point. `High` adds diagonal and corner handling and `Ultra` searches further and lowers the edge threshold. `SmaaSettings` exposes the threshold, search distances, corner rounding and luma or color edge detection for custom tuning.

Choose CMAA2 (`PostResolve::cmaa2(Cmaa2Preset::High)`) when the scene mixes long edges with text or thin detail that must stay crisp and the adapter runs compute shaders. It is a port of Intel's CMAA2 2.3 single-sample path and works on the image after tone mapping, like FXAA and SMAA. The four presets differ only in the edge threshold (0.15, 0.10, 0.07 and 0.05, with `High` the default), and `Cmaa2Settings::with_extra_sharpness` keeps more small detail. On a Radeon 780M at 1920x1080 the CMAA2 passes took about 3.5 ms, SMAA 1.9 to 2.4 ms and FXAA 1.1 ms, so it is the most expensive of the three. Its MSAA path is not ported; the post chain receives the resolved image, so that path is not needed here. The shader is Apache-2.0 licensed; see the [post crate README](../crates/render-post/README.md#cmaa2) for the deviations and the license.

FXAA, SMAA and CMAA2 are alternatives, because `resolve` holds one choice. If the scene effects already author FXAA, leave `resolve` at `None`; setting both is rejected. TAA will be a new `PostResolve` variant.

Contrast adaptive sharpening is a separate setting, `AntiAliasing::sharpen`, and works with any resolve: `AntiAliasing::new(MsaaSamples::X4, PostResolve::fxaa(1.0)).with_sharpen(CasSettings::new(0.5))`. It is AMD FidelityFX CAS in its no-scaling form. For each pixel it compares the pixel with its four neighbours and sharpens where there is headroom to black and white, so flat areas and edges that already reach black or white are left alone. `sharpness` runs from 0 (the reference default, least ringing, still sharpens) to 1 (strongest). It is off by default and then costs nothing. Use it after FXAA or SMAA, which soften fine detail, or at native resolution when the image looks soft. It runs at the render extent before any upscale. RCAS, the sharpening pass of the FSR 1 upscale, is a different stage: it runs at the output extent after reconstruction, is set with `UpscaleSettings::sharpness` in stops and exists only when an upscale is configured. Use RCAS to recover detail lost by upscaling and CAS to offset the softness of the antialiasing resolve. Both can be on, but check that the combination does not over-sharpen.

### Temporal antialiasing (in progress)

Temporal antialiasing arrives in stages, tracked in issue 256. This stage adds the inputs and no visible change. `AntiAliasing::with_jitter(ProjectionJitter::default())` shifts the scene projection by a sub-pixel offset each frame, taken from a Halton(2,3) sequence of 8 frames (any length from 1 to 64). The offsets stay inside the pixel, so a jittered frame differs from an unjittered one by less than one pixel at every edge. The renderer also remembers the unjittered view-projection matrix and jitter of the last frame that was submitted, so a later resolve can reproject history.

Jitter is off unless you request it, and then every output is byte-identical to a build without it. The resolve that accumulates the jittered frames, `PostResolve::Taa`, is not part of this release. Do not enable jitter on its own for display: each frame lands on a different sub-pixel position, so edges shimmer. The setting exists for tests and for the resolve that will follow. History resets on the first frame, on `FrameRenderer::reset_temporal_history` (call it on a camera cut), on resize, when the jitter setting changes and after a cancelled or failed frame. Shadow cascades and shadow casters never receive the jitter. Details are in the [render-scene README](../crates/render-scene/README.md#temporal-antialiasing-in-progress).

Contracts: [frame executor](../dev-docs/rendering/frame-executor.md), [post filters](../dev-docs/rendering/post-filters.md), [ambient occlusion](../dev-docs/rendering/ambient-occlusion.md), [reflections](../dev-docs/rendering/reflections.md), and the [render-post README](../crates/render-post/README.md) for exposure, bloom and upscaling settings.

## Windows, headless rendering and capture

`scene-viewer` presents a scene in a desktop window with up to three frames in flight, handles resize and minimize, and recovers from a lost surface. The same renderer can render headless to an offscreen target and read the pixels back, which is how the GPU tests check exact output bytes. A host can also schedule its own render passes alongside the stock ones, or read back GPU timings for every pass.

Contracts: [rendering services](../dev-docs/rendering/services.md), [frame executor](../dev-docs/rendering/frame-executor.md), [resource contracts](../dev-docs/rendering/resource-contracts.md).
