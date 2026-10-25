# Ambient occlusion and scene metadata

`SceneSsao` selects sampling and reconstruction independently. Radius and bias are authored in millimetres and converted once to view-space metres. Strength uses micro-units. The horizon angle limits directions relative to the receiving normal.

`SceneSsaoKernel::Hemisphere` retains the full-resolution projected hemisphere sampler. `SceneSsaoKernel::Spiral` samples a deterministic golden-angle screen-space pattern with squared radial spacing and pixel-dependent rotation. Sample distance chooses among averaged metadata footprints. Samples outside the screen, outside the radius, behind the receiving normal or without geometry coverage contribute no occlusion. Both policies return visibility in `[0, 1]`, with one meaning unoccluded. AO affects ambient lighting only.

`SceneSsaoFilter::DepthAwareBox` retains the existing five-by-five depth-aware tent. `SceneSsaoFilter::SeparableBilateral` runs horizontal and vertical nine-tap Gaussian passes. Taps are spaced two pixels apart. The Gaussian standard deviation is `4 / 1.5` taps, and the depth weight is `exp(-7 * abs(depth_difference_metres))`. Samples at least one metre apart are excluded. Each pass divides by its accepted weight sum, preserving constant visibility without an added brightness multiplier. Texture boundaries replicate their edge texel.

## Shared source, distinct reductions

`scene_metadata` owns source textures and geometry pipeline preparation. The normal and material prepasses use the forward geometry shader's deformation, material normals and alpha-coverage predicates, including foliage animation and transitions. Normals are in view space, with alpha zero for uncovered pixels. Depth linearization produces positive view depth, not radial camera distance.

The source prepass uses pixel-centre coverage at both 1x and 4x forward MSAA. It is deliberately not an MSAA coverage resolve. Source metadata consumers share this sampling domain, while resolved forward-depth products retain their separate coverage contract.

The conservative `R32Float` hierarchy remains a minimum reduction for reflection traversal. Shadow depth statistics read full-resolution linear depth and normal coverage. A checked same-frame prefix supplies these products to the remaining screen passes without another geometry prepass or full-resolution linearization. Neither consumer reads the averaged AO hierarchy.

Spiral sampling adds a separate `FrameProduct::occlusion_metadata(level)` hierarchy with `AverageDepthNormalMeters` meaning. RGB stores the averaged view-normal vector and alpha stores average positive view depth in metres. Zero alpha denotes no covered input. Covered inputs are averaged over a two-by-two footprint with edge replication at odd extents. Normal vectors are not renormalized after reduction, preserving the coherence of the averaged directions. The hierarchy stops at level six or 1x1, whichever comes first; sampling binds levels zero, two, four and six, clamped to the final level. This half-float product has half-float depth precision and range; it must not be used as conservative traversal depth or shadow statistics.

The existing pass names and stages remain unchanged for the hemisphere and box policies. Only the explicit new policies add averaged-metadata or horizontal-filter passes. Configured renderer allocation budgets include all new textures, and transactional replacement retires their resources with the rest of the frame configuration.
