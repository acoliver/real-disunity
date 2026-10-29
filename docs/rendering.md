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

Vignette, filmic tone curves, color grading beyond exposure, and sorted alpha-blended 3D geometry are not implemented. Requesting one fails with a typed error.

### Choosing antialiasing

One `AntiAliasing` value holds the choice: an `MsaaSamples` count (`X1`, `X2`, `X4` or `X8`) and a `PostResolve` (`None`, `Fxaa`, `Smaa`, `Cmaa2` or `Taa`) and optional CAS sharpening. Pass it to `FrameRenderer::hdr_antialiased`.

| Choice | Cost | What it smooths |
| --- | --- | --- |
| MSAA | Memory and fill rate grow with the count. | Geometry edges, using coverage samples. Shader aliasing and thin alpha-tested detail stay. |
| FXAA | One full-screen pass after tone mapping. | Edges by contrast, including shader and texture aliasing, with some softening. |
| SMAA 1x | Three full-screen passes after tone mapping, plus two small intermediate targets. | Edges by contrast, blended by the shape of the edge. Near-horizontal, near-vertical and diagonal lines come out cleaner and thin lines stay crisper than with FXAA. |
| CMAA2 | A buffer clear, three compute passes and one full-screen pass after tone mapping, plus about 32 MB of storage at 1080p. | Edges by contrast, blended by shape, with extra care for text and thin detail. Needs compute shaders. |
| TAA | One full-screen pass and one texture copy before exposure metering, one `Rgba16Float` history texture, sub-pixel camera jitter and a depth contract (see [Temporal antialiasing](#temporal-antialiasing)). | Accumulates many jittered frames, so it smooths geometry, shader and texture aliasing and thin lines over time. Moving objects soften and can ghost slightly. |
| CAS (sharpening) | One full-screen pass after the resolve, plus one `Rgba16Float` intermediate. About 0.4 to 0.5 ms at 1920x1080 on a Radeon 780M. | Nothing is smoothed. It restores local contrast that FXAA, SMAA or a soft texture filter removed. |

MSAA 4x with no post resolve is the usual starting point. Add FXAA when the scene has specular or alpha-tested aliasing that coverage samples do not reach. 1x and 4x work on every adapter. 2x and 8x depend on the adapter and format: a count the adapter cannot render for `Rgba16Float` color and `Depth32Float` depth fails with `PostError::SampleCount` before any target is allocated. It is never reduced to a nearby count. Mesa lavapipe supports 1x, 4x and 8x and rejects 2x.

Prefer SMAA over FXAA when the image is dominated by long high-contrast edges (UI-like geometry, wireframe, fences, power lines, text in the world) and the extra passes fit the frame budget. SMAA reads clean stair-step shapes, so it works best on the geometry edges that remain after MSAA, or with no MSAA at all. It does not smooth texture or shader aliasing inside a surface, and it leaves a perfectly straight axis-aligned edge alone because there is no stair step to resolve. FXAA is cheaper and touches more of the image, which suits noisy specular detail. Use `PostResolve::smaa(SmaaPreset::Medium)` as a starting point. `High` adds diagonal and corner handling and `Ultra` searches further and lowers the edge threshold. `SmaaSettings` exposes the threshold, search distances, corner rounding and luma or color edge detection for custom tuning.

Choose CMAA2 (`PostResolve::cmaa2(Cmaa2Preset::High)`) when the scene mixes long edges with text or thin detail that must stay crisp and the adapter runs compute shaders. It is a port of Intel's CMAA2 2.3 single-sample path and works on the image after tone mapping, like FXAA and SMAA. The four presets differ only in the edge threshold (0.15, 0.10, 0.07 and 0.05, with `High` the default), and `Cmaa2Settings::with_extra_sharpness` keeps more small detail. On a Radeon 780M at 1920x1080 the CMAA2 passes took about 3.5 ms, SMAA 1.9 to 2.4 ms and FXAA 1.1 ms, so it is the most expensive of the three. Its MSAA path is not ported; the post chain receives the resolved image, so that path is not needed here. The shader is Apache-2.0 licensed; see the [post crate README](../crates/render-post/README.md#cmaa2) for the deviations and the license.

FXAA, SMAA and CMAA2 are alternatives, because `resolve` holds one choice. If the scene effects already author FXAA, leave `resolve` at `None`; setting both is rejected. `PostResolve::taa` is a fourth alternative and is described below.

Contrast adaptive sharpening is a separate setting, `AntiAliasing::sharpen`, and works with any resolve: `AntiAliasing::new(MsaaSamples::X4, PostResolve::fxaa(1.0)).with_sharpen(CasSettings::new(0.5))`. It is AMD FidelityFX CAS in its no-scaling form. For each pixel it compares the pixel with its four neighbours and sharpens where there is headroom to black and white, so flat areas and edges that already reach black or white are left alone. `sharpness` runs from 0 (the reference default, least ringing, still sharpens) to 1 (strongest). It is off by default and then costs nothing. Use it after FXAA or SMAA, which soften fine detail, or at native resolution when the image looks soft. It runs at the render extent before any upscale. RCAS, the sharpening pass of the FSR 1 upscale, is a different stage: it runs at the output extent after reconstruction, is set with `UpscaleSettings::sharpness` in stops and exists only when an upscale is configured. Use RCAS to recover detail lost by upscaling and CAS to offset the softness of the antialiasing resolve. Both can be on, but check that the combination does not over-sharpen.

### Temporal antialiasing

`PostResolve::taa(0.9)` or `PostResolve::taa_with(TaaSettings { .. })` accumulates frames. Each frame the projection moves by a sub-pixel offset from a Halton(2,3) sequence. The resolve rebuilds the pixel from the jittered samples around it, reprojects the previous result with the camera motion and a depth, clips that history to the colors of the current neighborhood and blends the two. Over about ten frames a static edge converges toward a supersampled one. The technique follows Brian Karis, "High Quality Temporal Supersampling" (SIGGRAPH 2014).

Settings, both finite and range checked by `TaaSettings::validate`:

| Field | Range | Default | Effect |
| --- | --- | --- | --- |
| `history_weight` | 0.0 to 0.99 | 0.9 | Share of the reprojected history in the blend. Higher is smoother and slower to follow change. |
| `clip_scale` | 0.5 to 2.0 | 1.0 | Scale of the min/max color box of the 3x3 neighborhood, about its centre. Smaller rejects history sooner and ghosts less. |

The scene recipe `SceneAntiAlias::Taa { history_micro, clip_scale_micro }` maps to the same resolve: `history_micro` is `history_weight` in micro-units (0 to 990_000) and `clip_scale_micro` is `clip_scale` in micro-units (`500_000..=2_000_000`, validated when the effects are built). `SceneAntiAlias::taa(history_micro)` keeps the engine default clip scale of `1_000_000`. The recipe is programmatic only; the JSON v4 `hdr_post` block has no TAA field. Jitter is automatic: a TAA resolve selects `ProjectionJitter::default()` unless `AntiAliasing::with_jitter` chose another length, and `FrameRenderer::set_projection_jitter(None)` returns an error while TAA is resident. TAA is one resolve, so it excludes FXAA, SMAA and CMAA2.

The pass runs before exposure metering, bloom and tone mapping, on the scene-referred `Rgba16Float` image. History therefore holds no exposure, so an adaptive exposure change cannot ghost, and metering and bloom see the accumulated image instead of a shimmering jittered one. The color box uses luminance-compressed values so a bright pixel cannot dominate the clip, and the blend is linear because luminance weights pulled every edge toward its darker side. The current frame is reconstructed from the sample nearest the pixel centre, so the jittered frames accumulate to a box-filtered pixel instead of a blurred one. MSAA works with TAA: the hardware resolve runs first, and with 4x or 8x samples the depth used for reprojection is the per-sample minimum. A multisampled frame is not jittered (`ProjectionJitter::offset_pixels_for` returns zero for more than one sample): its pixels are already box-filtered area averages, and accumulating those at shifted positions converges to the box convolved with the jitter distribution, a tent twice as wide as a pixel. Resampling the shifted frame at `p + 0.5 + jitter` with Catmull-Rom before accumulating did not recover the box (static 4x error 0.00153 against 0.00046 for MSAA alone). TAA on MSAA input therefore stabilises over time and adds no spatial supersampling; a static 4x image equals the MSAA image.

Depth contract: draw the scene into `FrameRenderer::targets().depth()`. The resolve reads that texture, and `encode_scene` refuses another depth attachment while TAA is resident. Call `FrameRenderer::reset_temporal_history()` on a camera cut. History also resets on the first frame, on resize, when the jitter changes and after a cancelled or failed frame. A reset frame equals the first frame of a fresh renderer.

Limits of this release:

- Wind and instance motion are produced; billboards and impostors are not yet. Scenes drawn with `ScenePipelines::with_taa_velocity` write the residual of an object's wind deformation and of its own transform change (opaque, masked and relief) into the velocity target, and the resolve adds it to the camera reprojection. Each batch keeps a previous-transform buffer beside its instance buffer while a pass draws velocity pipelines: a frame that edits a slot (`set_instance_transform`) first writes that slot's last published transform there, a slot left alone for one frame converges to previous equal to current, and new or reused slots start equal to current. A cancelled frame leaves the buffer as the last published frame had it. Velocity pipelines bind it as a third per-instance vertex buffer at locations 12 to 15 (instance layout and stride; only the matrix columns are read); shadow casters and non-TAA pipelines keep two buffers. A camera-turned billboard or impostor card has no transform of its own to follow, so it still reprojects from the camera alone and relies on the neighborhood clip. Terrain, water, decals, dynamic quads, sky and celestial meshes keep a zero residual. Shadow receivers built with `ShadowedScene::with_taa_velocity` write the same residual (caster depth draws have no colour target). With SSR on, the reflection forward draws write it as a fourth colour target beside the three reflection targets when the device allows 28 bytes per sample (`max_color_attachment_bytes_per_sample`, 32 by default); on a device with a lower limit they keep the camera reprojection. A renderer whose SSAO or SSR pipelines are not velocity variants (SSAO without SSR on a `with_ssao` scene) also keeps the camera reprojection. With 4x MSAA the velocity attachment stays multisampled and the resolve reads the minimum-depth sample; the gain from wind velocity is small there because MSAA is not jittered.
- Sky and other pixels at depth 1.0 reproject approximately.
- SSAO and SSR run on the jittered matrix. Measured on lavapipe at 64x64 with a fixed camera over 16 frames (frames 8 to 15, 8-bit sRGB values on a 0 to 1 scale), this adds no visible flicker and no bias at 1x or 4x. Frame-to-frame change of the resolved image is 0.00073 with SSAO against 0.00072 without it, and 0.00059 with SSR against 0.00057 without it. At 4x the jitter is zero, so the output is static (0.0). SSAO moves by 0.00047 and SSR by 0.00011 between TAA on and off, against feature contributions of 0.0093 and 0.0068, so the prepass and the reflection rays keep the jittered matrix. The tests are in `crates/render-scene/src/draw/tests/taa_screen_space_tests.rs`. The scene is one creased floor and card, so a thin or high-frequency receiver can still shimmer; no such case was measured.
- History is read with a Catmull-Rom filter (nine bilinear taps, negative lobes clamped to zero) and blended with a motion-adaptive weight: the history weight is scaled by `clamp(1 - k * offset, 0, 1)`, where `offset` is the pixel's reprojection displacement in pixels, with k = 1.5 for jittered 1x frames and k = 16 for multisampled frames. A static pixel keeps the full weight. The trade-off is that a moving pixel supersamples less: at 1x, TAA in motion is only 0.73x to 0.88x the error of a history-free frame, because the jitter accumulation needs frames at the same scene position, and a multisampled image in motion is the MSAA image with history only on pixels that move less than 1/16 px per frame. Measurements showed that the earlier loss did not grow with the number of resampled frames (the error was at its plateau from the first moving frame); the likely cause is that edge pixels take the closest neighbor's motion and thin features resample poorly, errors the weight avoids by trusting history less where there is motion (not isolated further).
- A multisampled frame is not jittered, so an explicit `AntiAliasing::with_jitter` sequence on a multisampled target has no effect: `ProjectionJitter::offset_pixels_for` returns zero for more than one sample.
- There is no depth history, so a pixel uncovered by motion is detected only by the color clip.

Measured on lavapipe at 64x64 against a 4x supersampled reference, after 32 static frames: mean error 0.0020 (1x) and 0.00046 (4x MSAA, unjittered) on 8-bit sRGB values, against 0.0044 for one unantialiased frame and 0.00046 for 4x MSAA alone. In motion (16 frames, +41 mm per frame, about 0.27 px on the near object), the mean error over frames 4 to 15 at 1x is 0.00230 for TAA against 0.00260 when history is reset every frame (0.88x, 0.0031 against 0.0033 on the last frame); on the -29 mm path it is 0.00185 against 0.00254 (0.73x). At 4x the TAA last frame measures 0.00036 (+41 mm) and 0.00043 (-29 mm), equal to 4x MSAA alone, from 0.0027 before the motion weight (7.7x MSAA alone). Diagnosis at 4x before the change: history weight 0.9 gave 0.0025, 0.5 gave 0.0009 and 0.0 equalled MSAA alone (0.0004), and the error was flat from the first moving frame. A second camera path moving the other way keeps the edge within half a pixel at 1x and 4x. The tests are in `crates/render-scene/src/compositor/taa_end_to_end_tests.rs`. The history copy is a texture copy shared by all frame slots, about 0.4 ms at 1920x1080 on a Radeon 780M against 0.9 ms for a fullscreen copy pass, and a test reuses slot 0 every frame to check the history does not depend on slot order. Details of the passes are in the [post crate README](../crates/render-post/README.md#temporal-antialiasing) and [render-scene README](../crates/render-scene/README.md#temporal-antialiasing).

## Windows, headless rendering and capture

`scene-viewer` presents a scene in a desktop window with up to three frames in flight, handles resize and minimize, and recovers from a lost surface. The same renderer can render headless to an offscreen target and read the pixels back, which is how the GPU tests check exact output bytes. A host can also schedule its own render passes alongside the stock ones, or read back GPU timings for every pass.

Contracts: [rendering services](../dev-docs/rendering/services.md), [frame executor](../dev-docs/rendering/frame-executor.md), [resource contracts](../dev-docs/rendering/resource-contracts.md).
