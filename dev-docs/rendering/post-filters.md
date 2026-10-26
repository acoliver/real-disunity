# Post-filter derivation

The bloom and FXAA bodies in `crates/render-post/src/post_filters.wgsl` implement the mathematical specification below. They retain the existing texture bindings, specialization constants, stage identities and attachment formats.

The algorithm references are Timothy Lottes' [FXAA white paper](https://developer.download.nvidia.com/assets/gamedev/files/sdk/11/FXAA_WhitePaper.pdf) for luminance-based edge detection and directional filtering, and Jorge Jimenez's [Next Generation Post Processing in Call of Duty: Advanced Warfare](https://www.iryoku.com/next-generation-post-processing-in-call-of-duty-advanced-warfare/) presentation at SIGGRAPH 2014 for multi-resolution bloom. These references identify concepts, not imported implementation code or a license grant for sample code. This implementation uses its own filter arrangement and constants.

## Bloom

The prefilter sanitizes each RGB channel to finite, nonnegative values. For brightness `b = max(R,G,B)`, threshold `t` and knee radius `k = t * knee`, define hard excess `max(b-t,0)`. When `k > 0`, integrating a linear transition ramp gives `clamp(b-t+k,0,2k)^2 / (4k)`. Use the larger excess, divided by brightness, to scale RGB. Zero brightness produces zero RGB. Intermediate alpha is one.

Reduction samples a three-by-three footprint at source-texel offsets around the normalized destination pixel center. Its one-dimensional coefficients are `[1,2,1]`. The two-dimensional outer product sums to `4 * 4 = 16`, so division by 16 normalizes it. Repeated half-resolution passes extend the footprint without using a 13-tap kernel.

Reconstruction applies the same separable tent to the lower-resolution image, with offsets measured in lower-resolution texels. Average that result with the current high-resolution level. Both kernels and the final average preserve constant radiance. Integer loads clamp at image boundaries, including odd extents and single-texel levels.

## Separable Gaussian bloom

`BloomKernel::SeparablePyramid` is an alternative to the binomial and tent pyramid above. It uses the same prefilter.

Level zero has the half-resolution extent `ceil(width/2) x ceil(height/2)`. Each later level halves again with ceiling division. The first pass averages the prefiltered values of the four source texels under each level-zero texel. Later passes average four texels of the previous level. Integer loads clamp at borders.

For a level with radius `r > 0`, the discrete weights are `w(i) = exp(-i^2 / (2 sigma^2))` for `i = 0..r` with `sigma = r / 3`, normalized so `w(0) + 2 * sum(w(1..r)) = 1`. Radius zero is the identity. Adjacent positive taps `i` and `i+1` are folded into one bilinear fetch at offset `i + w(i+1) / (w(i) + w(i+1))` with weight `w(i) + w(i+1)`. An unpaired last tap keeps its own offset and weight. Each fetch is mirrored about the centre. Bilinear interpolation between two texels reproduces the two discrete weights exactly, so folding does not change the convolution away from borders.

Levels are blurred from the coarsest to the finest. The horizontal pass convolves the level with its weights. The vertical pass convolves the horizontal result, multiplies it by the level tint and weight, and adds a bilinear sample of the next coarser level's result, if any. The tone stage samples the level-zero result bilinearly at each output pixel and multiplies it by `Bloom::intensity`. The CPU computes the weights; the shader reads them from a storage buffer.

## FXAA

Use `(R + 2G + B) / 4` as a display-linear luminance proxy. The coefficients favor green and sum to one. A three-by-three neighborhood supplies minimum and maximum luminance. Skip filtering when contrast is at most `max(1/64, maximum/10)`.

Compute a gradient from right minus left and bottom minus top, using cross-axis coefficients `[1,2,1]`. Rotate the gradient by a right angle. Normalize this tangent by its largest absolute component. A zero gradient preserves the original pixel.

Sample bilinearly at tangent offsets of plus and minus 1.5 texels. Give the original RGB weight one half and each side sample weight one quarter. The sum is one. Blend that result with the original RGB using the existing FXAA strength and preserve original alpha. The luma proxy, contrast cutoff and sample reach are choices of this implementation, not a claim to reproduce an FXAA quality preset.

## Verification

`filter_reference.rs` contains a test-only scalar reference. `filter_reference_gpu_tests.rs` compares complete output bytes for bloom extraction, reduction, reconstruction and FXAA, including clamped borders, odd FXAA extents, strength endpoints and a single-texel image. Byte-encoded fixtures assert distance from quantization half-integers. `filter_hand_computed_gpu_tests.rs` adds hand-computed odd-size bloom reconstruction values and channel-varying FXAA cases below, at and above both contrast cutoffs, with horizontal and vertical tangents. Those expectations are exactly representable in binary16 and use complete byte equality. Existing post-chain, exposure, bloom energy and edge-behavior tests remain independently applicable.
