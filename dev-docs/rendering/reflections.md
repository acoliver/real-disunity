# Screen-space reflection composition

SSR consumes the existing opaque color snapshot as linear HDR radiance, before tone mapping or sRGB encoding. The snapshot carries the radiometric scale used by geometry. In the stock HDR path, post owns exposure and `prepare_uniform` sets geometry exposure to one. A custom geometry-owned exposure path must keep incident radiance and the published environment on the same scale. RGB in the trace and its roughness mips is premultiplied by confidence; alpha stores confidence. Filtering interpolates both together.

`CaptureOutput::Specular` exposes the existing post-decal pixel-centre opaque/masked SSR material plane. RGB stores linear F0 and alpha stores the material roughness channel before the lighting roughness remap and lower clamp. Under `SceneRoughnessRemap::LinearAlpha`, alpha retains the authored GGX alpha channel rather than perceptual roughness; authored zero remains zero. The plane requires SSR, precedes water and transparent surfaces, and has one sample regardless of forward MSAA. `products.specular` contains tightly packed little-endian RGBA16Float bytes in top-left row-major order, with uncovered pixels `(0, 0, 0, 1)`. Capture does not enable SSR, and accumulation is rejected.

`FrameRenderer::set_reflection_environment(device, maps)` accepts submitted `PreparedEnvironmentMaps`, installs the prefiltered cube and integrated BRDF LUT, and selects environment replacement. The configured renderer exposes the same operation. Installed water shares these maps. Resizing retains the map views and selected composition algorithm.

Opaque forward shading evaluates the split-sum factor `F0 * A + B`. It publishes the actual environment-specular contribution, including exposure and camera fog transmittance, as `FrameProduct::forward_specular(samples)`. `FrameProduct::reflection_factor(samples)` publishes the split-sum factor times camera transmittance. The latter does not apply exposure again because incident SSR radiance already includes it. Both products use per-sample RGBA16Float storage and are declared additional targets of the existing opaque pass.

Water publishes its depth-tested normal and coverage flag, sample offsets within the pixel, scalar F0 and perceptual roughness, and environment-specular contribution. Environment alpha carries camera transmittance. The trace reconstructs a covered water receiver from its published sample offsets and its own forward sample depth. Both perspective and orthographic projections are supported. Offsets use RGBA16Float material channels, with their storage precision; no sample pattern is assumed. Resolve selects metadata independently for each sample. Opaque traversal targets must still match the immutable opaque snapshot.

For confidence `c`, resolve adds the delta

```
filtered_premultiplied_radiance * factor - c * published_environment_specular
```

The roughness cutoff fade multiplies both terms and therefore also scales effective confidence. Zero confidence leaves the shaded result unchanged. Full confidence replaces the environment lobe without changing diffuse, direct light or emission.

Each MSAA sample has a horizontal lane in the existing SSR radiance targets. Ceil-divided mip reduction and bilinear reconstruction stay inside that lane, including partial edge bins. Sample positions are not assumed or hard-coded. Construction and resize reject a packed width above the device texture limit with `FrameError::SsrSettings`; the maximum frame width is the limit divided by sample count at full resolution, or twice that at half resolution. Reduction is a recursive equal-child filter, not an original-pixel area average at unequal partial footprints. Water-covered target pixels are conservatively rejected, and opaque receiver resolve retains its pixel-level shoreline exclusion.

`set_reflection_composition(Composition::Additive)` explicitly selects the existing additive algorithm for opaque receivers. Existing callers without installed reflection maps retain that algorithm. Water receivers use environment replacement because their forward shader already contains the environment lobe.
