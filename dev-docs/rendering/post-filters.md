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

## Tone mapping

`ToneMap::Filmic` uses Krzysztof Narkowicz's [ACES Filmic Tone Mapping Curve](https://knarkowicz.wordpress.com/2016/01/06/aces-filmic-tone-mapping-curve/), published January 6, 2016. For each exposed RGB channel `x`, the fitted display curve is

`f(x) = clamp(x * (2.51*x + 0.03) / (x * (2.43*x + 0.59) + 0.14), 0, 1)`.

Bloom is composed into sanitized radiance before exposure multiplication and evaluation of the curve. Negative and NaN radiance map to zero. Positive infinity maps to one at positive exposure; zero exposure maps it to zero. This component-wise fit does not include the full ACES color-space transforms. The unclamped rational fit tends to `2.51/2.43`; the clamped display curve tends to one. `aces.rs` shares coefficients between the CPU reference and generated WGSL. Bloom extraction retains its existing unclamped fit.

## FXAA

Use `Y = (R + 2G + B) / 4` as a display-linear luminance proxy. The coefficients favor green and sum to one. `FxaaQuality::luminance` defaults to `FxaaLuminance::Linear`. Selecting `FxaaLuminance::Srgb` instead applies the scalar sRGB OETF to `Y` before contrast and gradient calculations: `12.92*Y` for `Y <= 0.0031308`, otherwise `1.055*Y^(1/2.4) - 0.055`. RGB samples remain display-linear in either mode; the final sRGB attachment performs the output encoding once.

A three-by-three neighborhood supplies minimum and maximum luminance in the selected domain. Skip filtering when contrast is at most `max(minimum_threshold, maximum * edge_threshold)`. The absolute `minimum_threshold` defaults to `1/64`; the relative `edge_threshold` defaults to `0.1`. At that relative default the shader retains division by ten, preserving the original `max(1/64, maximum/10)` arithmetic. The absolute threshold is measured in the selected luminance domain.

Compute a gradient from right minus left and bottom minus top, using cross-axis coefficients `[1,2,1]`. Rotate the gradient by a right angle. Normalize this tangent by its largest absolute component. A zero gradient preserves the original pixel.

Sample bilinearly at tangent offsets of plus and minus 1.5 texels. Give the original RGB weight one half and each side sample weight one quarter. The sum is one. Blend that result with the original RGB using the `PostResolve::Fxaa` strength multiplied by `FxaaQuality::subpixel_blend`, which defaults to one, and preserve original alpha. The multiplier scales this kernel's existing blend; it does not add a separate subpixel estimator. All three numeric quality controls must be finite and in `[0,1]`. Quality settings travel with the strength inside `PostResolve::Fxaa`, so they cannot enable FXAA by themselves, and a zero strength schedules no pass. The luma proxy, contrast cutoff and sample reach are choices of this implementation, not a claim to reproduce an FXAA quality preset.

## Verification

`filter_reference.rs` contains a test-only scalar reference. `filter_reference_gpu_tests.rs` compares complete output bytes for bloom extraction, reduction, reconstruction and FXAA, including clamped borders, odd FXAA extents, strength endpoints and a single-texel image. Byte-encoded fixtures assert distance from quantization half-integers. `filter_hand_computed_gpu_tests.rs` adds hand-computed odd-size bloom reconstruction values and channel-varying FXAA cases below, at and above both contrast cutoffs, with horizontal and vertical tangents. Those expectations are exactly representable in binary16 and use complete byte equality. Existing post-chain, exposure, bloom energy and edge-behavior tests remain independently applicable.

`display_policy_gpu_tests.rs` checks the Filmic tone paths, transfer, alpha and disabled-effect stage identities. Its ramp paints the resolved texture directly at configured sample counts one and four, so that loop covers post-resolve tone processing, not MSAA resolution. Existing resolve tests cover the resolve operation.

## Temporal antialiasing

TAA runs before exposure metering, bloom and tone mapping, unlike FXAA, SMAA and CMAA2, which read the tone-mapped image. The stages are `taa resolve`, `taa history` and `taa depth history`, then the usual order. The resolve rebuilds the pixel with a Gaussian (sigma 0.47 pixel) over the 3x3 jittered samples around it, where the sample at pixel `q` lies at `q + 0.5 - jitter`. It reprojects history with the closest depth of the 3x3 neighborhood (per-sample minimum under MSAA), clips the history to the mean plus or minus `clip_scale` standard deviations of the neighborhood in YCoCg of luminance-compressed color along the ray toward the box center, and blends with weight `1 - history_weight` for the current color, each side weighted by `1 / (1 + luma)`. `SceneAntiAlias::Taa { history_micro, clip_scale_micro }` authors both settings in micro-units; `SceneAntiAlias::taa(history_micro)` uses the default `clip_scale` of 1. The render-plan bounds `500_000..=2_000_000` equal the `0.5..=2` range of `TaaSettings::validate`, which a render-scene test cross-checks. References: Brian Karis, "High Quality Temporal Supersampling" (SIGGRAPH 2014) and Marco Salvi, "An Excursion in Temporal Supersampling" (GDC 2016).
