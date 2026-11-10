# Screen-space reflection composition

SSR consumes the existing opaque color snapshot as linear HDR radiance, before tone mapping or sRGB encoding. The snapshot carries the radiometric scale used by geometry. In the stock HDR path, post owns exposure and `prepare_uniform` sets geometry exposure to one. A custom geometry-owned exposure path must keep incident radiance and the published environment on the same scale. RGB in the trace and its roughness mips is premultiplied by confidence; alpha stores confidence. Filtering interpolates both together.

`CaptureOutput::Specular` exposes the existing post-decal pixel-centre opaque/masked SSR material plane. RGB stores linear F0 and alpha stores the material roughness channel before the lighting roughness remap and lower clamp. Under `SceneRoughnessRemap::LinearAlpha`, alpha retains the authored GGX alpha channel rather than perceptual roughness; authored zero remains zero. The plane requires SSR, precedes water and transparent surfaces, and has one sample regardless of forward MSAA. `products.specular` contains tightly packed little-endian RGBA16Float bytes in top-left row-major order, with uncovered pixels `(0, 0, 0, 1)`. Capture does not enable SSR, and accumulation is rejected.

`FrameRenderer::set_reflection_environment(device, maps)` accepts submitted `PreparedEnvironmentMaps`, installs the prefiltered cube and integrated BRDF LUT, and selects environment replacement. The configured renderer exposes the same operation. Installed water shares these maps and their BRDF convention. `set_water_environment(maps)` installs only the water maps. Retained scenes use `set_scene_environment(device, maps, consumers)`: Water changes only water, Reflections changes only SSR, and Both uses the shared reflection setter. Consumer changes first clear the previous bindings, so unselected consumers retain empty maps. Resizing retains the map views and selected composition algorithm.

Opaque forward shading evaluates the split-sum factor `F0 * A + B`. It publishes the actual environment-specular contribution, including exposure and camera fog transmittance, as `FrameProduct::forward_specular(samples)`. `FrameProduct::reflection_factor(samples)` publishes the split-sum factor times camera transmittance. The latter does not apply exposure again because incident SSR radiance already includes it. Both products use per-sample RGBA16Float storage and are declared additional targets of the existing opaque pass.

The post-water traversal hierarchy stores positive linear view depth. Every coarse cell is the minimum of its valid 2x2 children, including partial edge bins, so it bounds the nearest surface in its footprint. Grid traversal uses this bound to decide whether to refine a cell before skipping it. Both full-resolution and reflection-output grid domains use the same hierarchy. In the half-resolution domain, the same grid walk refines contacted leaves through the full-resolution children crossed by the projected ray, in ray order. Each child uses the perspective depth and thickness test; a leaf without a child intersection does not end the walk. The first intersected child supplies its pixel and depth to opaque ownership, target-normal rejection and confidence. Grid confidence uses the non-grid thickness convention: the positive entry-depth residual fades from full confidence at half the allowed thickness to zero at its outer edge. Literal depth crossings have zero residual. This confidence multiplies RGB and alpha together. Averaged occlusion footprints and filtered radiance are separate products and do not supply traversal bounds.

Water publishes its depth-tested normal and coverage flag, sample offsets within the pixel, scalar F0 and perceptual roughness, and environment-specular contribution. Environment alpha carries camera transmittance. The trace reconstructs a covered water receiver from its published sample offsets and its own forward sample depth. Both perspective and orthographic projections are supported. Offsets use RGBA16Float material channels, with their storage precision; no sample pattern is assumed. Resolve selects metadata independently for each sample. Opaque traversal targets must still match the immutable opaque snapshot.

For confidence `c`, resolve adds the delta

```
filtered_premultiplied_radiance * factor - c * published_environment_specular
```

The roughness cutoff fade multiplies both terms and therefore also scales effective confidence. Zero confidence leaves the shaded result unchanged. Full confidence replaces the environment lobe without changing diffuse, direct light or emission.

Each MSAA sample has a horizontal lane in the existing SSR radiance targets. Ceil-divided mip reduction and bilinear reconstruction stay inside that lane, including partial edge bins. Sample positions are not assumed or hard-coded. Construction and resize reject a packed width above the device texture limit with `FrameError::SsrSettings`; the maximum frame width is the limit divided by sample count at full resolution, or twice that at half resolution. Reduction is a recursive equal-child filter, not an original-pixel area average at unequal partial footprints. Water-covered target pixels are conservatively rejected, and opaque receiver resolve retains its pixel-level shoreline exclusion.
## Receiver-compatible reconstruction

Resolve classifies raw trace receivers before mixing their radiance. Opaque
descriptors unproject the opaque depth at its full-resolution pixel centre,
independently of the trace-grid centre. Water descriptors use the published
normal and sample offset with that sample's raw forward depth. Water resolve binds the scheduled scene depth in
place of the reduced final-depth input; its opaque shoreline check reduces those
raw samples conservatively and rejects any pixel with water coverage in any lane.
AO has a separate binding and never supplies a receiver normal.

Two receivers must have the same owner, normalized-normal dot product at least
0.9, and symmetric tangent-plane residual at most
`max(0.01 metres, 0.001 * min(view_depth_a, view_depth_b))`. The residual is the
larger absolute projection of their position difference onto either geometric
plane normal. Each plane uses the nearest same-owner depth neighbour on each
pixel axis, avoiding a farther silhouette or depth step when a nearer neighbour
exists. If an axis has no covered neighbour, the published shading normal is the
remaining local plane estimate. Where depth samples span a plane, shading
normals govern angular rejection only; normal-map perturbations do not tilt the
depth-continuity plane. These local estimates approximate surface continuity
rather than mesh identity.
Sufficiently close, coplanar objects of the same receiver type can share support.

For nonzero roughness at LOD at most one, resolve integrates raw trace texels
with a separable tent of radius `clamp(2 * exp2(lod), 2, 4)`. Beyond LOD one the
tent stays at radius four and blends into the roughness mip kernel with
`smoothstep(1, 2, lod)`. At and above LOD two only the mip kernel contributes;
there is no cap on its support. Perfect mirrors use receiver-gated level-zero
bilinear reconstruction. Compatible alpha-zero misses remain in the denominator
of every kernel. Premultiplied RGB and alpha always share the same weights.

For each requested mip level, resolve verifies every raw receiver contributing
to its four bilinear coarse taps. Homogeneous support samples the existing mip
texture, including its storage rounding. A rejected receiver triggers a raw
gather over those same reduction trees. Each leaf's weight is the product of
reciprocal valid-child counts along its tree, multiplied by the coarse bilinear
weight. Rejected receivers contribute neither pair nor denominator. Floor-dropped
subtrees remain excluded; ceil partial bins retain equal-child weights. Each LOD
is normalized independently before fractional-LOD interpolation.

The direct check and fallback each visit up to `4^level` raw receivers per
coarse tap, including on the homogeneous path. At full image sizes the resulting
fragment workload can trigger a GPU hang and abort later command buffers. An
unexecuted SDSM reduction can then read back four zero words, which depth fitting
correctly rejects as malformed evidence. This implementation has no
receiver-compatibility summary pyramid or resolution-independent work bound;
full native mip reconstruction is not safe for production-sized rough surfaces.
The full-size SDSM/SSR GPU regression exercises this failure without private
content. Full native mips retain their logical extents and two-axis sample
packing. Resolve requires
11 sampled textures for full mips, or `levels + 10` for separate levels, plus one
when application AO is enabled. Unsupported device budgets fail before allocation.


`set_reflection_composition(Composition::Additive)` explicitly selects the existing additive algorithm for opaque receivers. Existing callers without installed reflection maps retain that algorithm. Water receivers use environment replacement because their forward shader already contains the environment lobe.

The sky background and celestial mesh composition share the opaque pass and select colour targets from that pass's reflection and velocity variant. Their SSR environment and factor targets have empty write masks, preserving the cleared zero RGB and unit alpha. Sky pixels have no normal coverage and contribute no reflections. Sky and celestial composition velocity also preserve the cleared zero residual for camera-only TAA reprojection. Celestial visibility geometry renders in its separate radiance and depth pass.
