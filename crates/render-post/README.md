# Configured post and output

`PostChain<F: StageFactory>` is a retained, **linear** post/output service. It is not a general render graph and does not submit commands or acquire surfaces.

- `FrameConfig` specifies the scene extent, color format and sample count. `FrameTargets` retains the resolved color texture and optional multisample texture. Use `scene()` and `resolve()` together for the scene pass; subsequent stages always bind `resolved()`.
- `Resources::new` allocates opaque, explicitly sized caller-authored buffers. Move these into the chain for exposure/history/config ownership. Neither resize nor reconfiguration clears or reallocates them. The provider defines their binding layouts and initialization bytes; the service does not infer or reinterpret an exposure ABI.
- `StageFactory::build` creates bind groups and returns an ordered `StageSet`. It is called on construction, resize and reconfiguration, **not** once per frame. Move sized textures and buffers into `StageSet::resources`; pipeline and sampler handles can remain in the factory. Failed resize/reconfiguration leaves the previous target set and schedule intact.
- `Stage` describes a clear, buffer copy, compute dispatch or fullscreen draw. Bind groups retain the provider's explicit layouts and ascending group order. An optional stage can be absent or `enabled: false`. The provider is responsible for supplying a valid bypass/input when an effect is disabled. The service validates one final enabled output and bounded compute dispatches before accepting a schedule.
- `encode` executes the configured order against a supplied surface or offscreen output view. It does not choose exposure, bloom, tone curves, LUTs, adaptation rules or antialiasing. Providers must declare those stages and dependencies explicitly.
- `CopyOutput` is the built-in **no-effects** factory: nearest texel copy, including scaling and destination attachment transfer/channel encoding. It does not introduce a display look.
- `color_texture` allocates common intermediate/output textures. `Readback` retains bounded staging storage, verifies the source contract, strips padding and returns native bytes (RGBA/BGRA/HDR are not silently converted). Mapping has a shared 30-second deadline and unmaps on returned errors.

Minimal copy-only presentation:

```rust,ignore
let mut output = PostChain::new(
    device,
    FrameConfig::new([width, height], scene_format, samples),
    Resources::default(),
    CopyOutput::new(device, surface_format),
)?;
// Render using output.targets().scene() and output.targets().resolve().
output.encode(encoder, surface_view);
output.resize(device, [new_width, new_height])?;
```

For an authored effect, implement `StageFactory`; compile custom shaders with `fullscreen_pipeline` / `compute_pipeline` and construct explicit bind groups in `build`. For example, return histogram clear, histogram dispatch, exposure dispatch, bloom draws, then an output draw. A diagnostic exposure override is an in-order `CopyBuffer` stage, not a queue write racing the compute schedule. Updating per-frame uniforms remains a caller responsibility, before submission. An output format change requires a correspondingly configured factory/pipeline; device/surface lifecycle and capture publication remain host responsibilities.

The required GPU tests exercise exact copy, HDR/MSAA resolve, nonaligned readback, optional output changes, disabled-stage preservation, history across resize/reconfiguration and typed invalid configurations. They use native PRIMARY backends by default and honor `WGPU_BACKEND`; adapter absence fails rather than skips.

## Ordered host captures

For a capture inside an existing frame encoder, construct `Readback` with the **actual source format**, extent, sample count 1, and an explicit byte ceiling. Call `encode(encoder, source)` at the capture point, submit that encoder, then call `read_encoded(device, submission)`; `None` waits for all submitted work. `read(device, queue, source)` remains the convenience copy/submit/read path. A mutable borrow prevents overlapping maps through this owner. The caller must use the creating device and submit encoded copies before reading.

Supported native formats are RGBA8/BGRA8 linear and sRGB, RGBA16Float, R32Float and RGBA32Float. Wrong-format, multisampled, array, non-D2, wrong-size and missing-COPY_SRC sources are rejected before copy encoding. Floating-point data is never interpreted or converted: NaN payloads, infinities and signed zeros survive byte-for-byte. `read_encoded_padded` plus `padded_bytes_per_row` retains the GPU copy layout for diagnostic exporters that consume explicit row pitch; ordinary `read_encoded` strips padding.

`BufferReadback` provides the same allocation ceiling, ordered encode, shared 30-second deadline and guaranteed unmap-on-return for opaque diagnostic buffers. Its configured byte count must be nonzero and copy-aligned; encoding copies exactly that count from source offset zero. The provider, not the SDK, interprets exposure/statistics layouts. Capture publication, metadata, float decoding and surface BGRA-to-RGBA packing remain host responsibilities.

## Stock HDR effects

`ToneMapOutput::with_effects` installs optional adaptive exposure, bloom and
FXAA. An empty policy uses the unchanged fixed-exposure shader. Slot-aware
factories build one immutable schedule per admitted slot. Exposure history
survives resize, while sized filter targets rebuild transactionally.

Metering sanitizes each channel, clamps log2 luminance into 256 bins, and reduces
the fractionally trimmed low/high percentile interval using bin centres.
Adaptation is `previous + (target - previous) * (1 - exp(-speed * delta))`,
starting from exposure one. Up and down rates are independent. Before encoding,
call `prepare_effects` with the selected slot and explicit delta/reset. Its
upload is an encoder-ordered copy, never a queue write. A fixed capture delta
overrides host timing; fixed capture exposure bypasses adaptation. One queue-ordered carry buffer advances history in submission order. Each
reduction publishes its result into a separate slot buffer before tone mapping,
so later frames cannot change that frame's result. Submit successful encoders
in preparation order; do not reuse a slot until its submission completes. A
reset applies once at the requested frame boundary. The effective delta must
be finite and within 0..=10 seconds; invalid preparation returns
`PostError::FrameDelta` before allocation or encoder mutation. A valid fixed
capture delta overrides even invalid host timing. Clearing an installed policy
with empty settings removes all effects and restores the fixed schedule.

Bloom prefilters unexposed radiance with a quadratic soft knee, downsamples
with a normalized nine-tap binomial kernel, and upsamples with the same
separable 3x3 tent. Each upsample averages its local and coarser levels,
preserving constant radiance instead of multiplying brightness by pyramid
depth. Bloom uses Rgba16Float intermediates and adds before the selected tone curve.
NaN, infinities and negative channels contribute zero to metering and bloom.
Positive infinity remains a white highlight in direct output but does not
spread into neighbours or enter histogram arithmetic. Finite half-float
highlights remain eligible. This extends the existing output special-value
policy to finite statistical/filter inputs without changing direct tone mapping.

`ToneMap::Reinhard` retains `x/(1+x)`. `ToneMap::Filmic` applies
`clamp(x*(a*x+b)/(x*(c*x+d)+e),0,1)` with `(a,b,c,d,e) =
(2.51,0.03,2.43,0.59,0.14)`, the fitted ACES approximation published by
[Krzysztof Narkowicz (2016)](https://knarkowicz.wordpress.com/2016/01/06/aces-filmic-tone-mapping-curve/).
This implements the published equation without importing shader code. It is a
component-wise fit, without ACES color-space transforms. `aces.rs` owns the
coefficients used by both shader generation and CPU references, including the
unclamped bloom bright-extraction curve. Bloom is added before exposure, then
the selected curve runs before the attachment's single sRGB encoding. Negative
radiance and NaN map to zero; positive infinity maps to one. Zero exposure maps
all radiance to zero. `ToneMap::evaluate` provides the display-linear CPU reference;
`ToneMapOutput::evaluate` retains its existing Reinhard semantics.

FXAA uses `(R + 2G + B) / 4` as its luminance proxy. `HdrPostSettings::resolve` is
`PostResolve::Fxaa(FxaaSettings { strength, quality })` to enable it. `strength` is
the filter strength. `quality: FxaaQuality` sets the relative
`edge_threshold`, absolute `minimum_threshold`, `subpixel_blend` strength multiplier,
and `luminance` transfer. Strength and each numeric quality setting must be finite
and within `0..=1`; a zero strength validates but schedules no pass. Filtering skips contrast at or below
`max(minimum_threshold, maximum * edge_threshold)`. Defaults are `0.1`, `1/64`,
`1.0`, and `FxaaLuminance::Linear`, preserving the existing bytes and division by ten.
`FxaaLuminance::Srgb` applies the sRGB OETF to the scalar linear luminance proxy
before contrast and gradient calculations. It does not encode RGB or reinterpret
the source texture: the chain always supplies display-linear RGB.

A Sobel-derived tangent selects bilinear samples 1.5 texels away along its
major axis, with center weight 1/2 and side weights 1/4. The product of strength
and `subpixel_blend` blends that result with the original pixel. Zero gradient
or zero blend preserves the original. An Rgba32Float LDR intermediate preserves
flat-region precision. Only the final sRGB attachment applies output transfer;
alpha is preserved. The filter uses explicit integer loads and bilinear
arithmetic, without implicit derivatives in divergent control flow. See the
[post-filter derivation](../../dev-docs/rendering/post-filters.md) for the base
kernel; the quality controls above extend its fixed contrast cutoff and strength.

## Choosing antialiasing

`AntiAliasing { msaa: MsaaSamples, resolve: PostResolve }` is the one setting for
antialiasing. `MsaaSamples` has the variants `X1`, `X2`, `X4` and `X8`;
`MsaaSamples::try_from(u32)` rejects every other number with
`PostError::SampleCount`. `AntiAliasing::frame_config` builds the `FrameConfig`
for the scene attachments, and `PostResolve` goes into `HdrPostSettings::resolve`.
`PostResolve` is `#[non_exhaustive]`: match it with a wildcard arm, because
further resolves are added as variants. Its variants are `None`, `Fxaa` and `Smaa`
(see [SMAA 1x](#smaa-1x)); one resolve is selected, never two. Prefer SMAA when long
high-contrast geometry edges dominate and three passes fit the budget, and FXAA as the
cheaper single pass for noisy specular detail.

`FrameConfig::validate` admits the sample counts every device guarantees (1 and 4).
`FrameConfig::validate_device` also admits 2 and 8 when the device enabled
`TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`, and `FrameConfig::validate_adapter`
checks the adapter's reported sample counts and resolve support for the color and
depth formats. Run `validate_adapter` before creating a chain with 2 or 8 samples;
wgpu otherwise reports an unsupported count as a device validation error. An
unsupported count is always an error and is never reduced.

### SMAA 1x

`PostResolve::Smaa(SmaaSettings)` selects SMAA 1x instead of FXAA; `HdrPostSettings::resolve`
holds one resolve, so the two cannot run together. `PostResolve::smaa(SmaaPreset::High)`
builds a preset. `SmaaSettings::LOW`, `MEDIUM`, `HIGH` and `ULTRA` equal the reference
`SMAA_PRESET_*` values:

| Preset | `threshold` | `max_search_steps` | `max_search_steps_diag` | `corner_rounding` |
| --- | --- | --- | --- | --- |
| Low | 0.15 | 4 | off | off |
| Medium | 0.1 | 8 | off | off |
| High | 0.1 | 16 | 8 | 25 |
| Ultra | 0.05 | 32 | 16 | 25 |

Custom values are validated: `threshold` is above 0 and at most 0.5, `max_search_steps`
is 0 through 112, `max_search_steps_diag` is `None` or 0 through 20, and `corner_rounding`
is `None` or 0 through 100. `None` skips diagonal or corner handling, as the reference does
for Low and Medium. `SmaaEdgeDetection::Luma` (default) compares Rec. 709 luma and
`SmaaEdgeDetection::Color` compares the largest channel difference.

The stage runs where FXAA runs, on the display-referred image at the render extent after tone
mapping and before any upscale. It schedules `smaa_edges` (Rg8Unorm), `smaa_weights`
(Rgba8Unorm) and `smaa_blend` as separate stage identities, each with its own timing slot.
The two intermediate targets and the lookup buffers belong to the `StageSet`, so resize and
reconfiguration rebuild them with the rest of the schedule. Edge decisions use sRGB-encoded
values because the reference thresholds assume gamma-space input; the blend itself reads and
writes linear color. All four channels are blended, so alpha follows the same weights as
color and stays within the range of the pixels it mixes. A constant image is unchanged.

The area (160x560 RG8) and search (64x16 R8) lookup tables are generated at run time by
`src/smaa_lut.rs`, a Rust port of the reference `AreaTex.py` and `SearchTex.py`, and bound as
storage buffers (the stage builder has no queue for texture uploads). The orthogonal half of the
area table is cached for the process; the diagonal half is generated only when
`max_search_steps_diag` is set. Tests compare the output with 64-bit digests of the reference
`AreaTex.h` and `SearchTex.h`, so the bytes are identical to the published tables. An opt-in test compares byte for byte with the downloaded headers: set `SMAA_REFERENCE_DIR` to a directory holding `AreaTex.h` and `SearchTex.h` from `iryoku/smaa` (not committed) and run `cargo test -p worldrunner-render-post smaa_lut -- --include-ignored`.

SMAA reads clean stair-step shapes. A straight axis-aligned edge is already sharp and is left
unchanged; blending happens at steps, diagonals, corners and line ends. A one-pixel line is
softened at its ends and kept along its length. Compared with FXAA it preserves thin geometry
better and does not blur texture detail, but it does not smooth shader or texture aliasing.
Choose it when geometry edges dominate and the three passes fit the budget. Choose FXAA for a
single cheap pass over noisy specular detail.

Differences from the reference shader: the color input is Rgba32Float and is read with point
loads, so neighborhood blending mixes two loaded texels where the reference uses one bilinear
fetch; area lookups use explicit bilinear arithmetic; and predication, reprojection and
subsample indexing (SMAA S2x, T2x, 4x) are not implemented.

## Reduced internal resolution

`FrameConfig::size` is the internal scene extent. Construct a configuration with
its presentation extent, then call `with_render_extent([render_width, render_height])`.
`output_extent()` returns the presentation extent. Both dimensions must be
nonzero, within device limits, and the output must contain the render extent.
`RenderScale::new` accepts 500000 through 1000000 millionths (0.5 through 1.0).
Its `extent` method rounds to nearest, with halfway values rounded up.

`SpatialUpscale` is a retained `StageFactory` for display-referred inputs.
Select `UpscaleMode::Bilinear` or `UpscaleMode::Easu`. Set `sharpness` to
`Some(stops)` for RCAS, from zero (maximum sharpening) through two stops.
`None` is the exact off value and omits the RCAS pass. With RCAS enabled, the
reconstruction pass writes an Rgba16Float intermediate: its display-referred
values need no more precision, and on an integrated GPU at 1600x900 output it
measured about half the reconstruction and RCAS time of an Rgba32Float
intermediate. The WGSL port uses full
precision reciprocals and square roots, with clamped border loads. EASU clamps
each reconstructed RGB channel to the nearest four source samples. Alpha uses
bilinear interpolation and passes unchanged through RCAS.

For HDR, set `HdrPostSettings::upscale` and provide the two extents in
`FrameConfig`. Exposure and bloom run at the render extent before tone mapping.
FXAA runs after tone mapping at the render extent, then EASU/bilinear and RCAS
run before final attachment transfer and format conversion. At equal extents,
the HDR path omits reconstruction and RCAS, preserving the native pass count.
Use `PostChain::resize_extents` to replace both extents and their dependent
bindings transactionally. `resize` changes the internal extent; an explicit
presentation extent remains unchanged. Reconfiguration rebuilds stage bindings.

### Third-party license

`src/upscale.wgsl` ports EASU and RCAS from
[AMD FidelityFX Super Resolution 1, ffx_fsr1.h](https://github.com/GPUOpen-Effects/FidelityFX-FSR).
Copyright (c) 2021 Advanced Micro Devices, Inc. All rights reserved.
Licensed under the MIT License. The full copyright, permission and warranty
notice is preserved in the shader file.

`src/smaa.wgsl` ports SMAA 1x from
[SMAA.hlsl](https://github.com/iryoku/smaa/blob/master/SMAA.hlsl), and `src/smaa_lut.rs` ports
the lookup-texture generators `Scripts/AreaTex.py` and `Scripts/SearchTex.py` from the same
repository. Copyright (C) 2013 Jorge Jimenez, Jose I. Echevarria, Belen Masia, Fernando
Navarro and Diego Gutierrez. Licensed under the MIT License. The full copyright, permission
and warranty notice is preserved in both files. The port was written from the reference
source. No code was copied from Bevy's WGSL port.

### Numerical checks and informational timing

The scalar f64 reference and f32 GPU kernels are compared with an absolute
per-channel tolerance of 0.0001. Reconstruction is compared without RCAS. RCAS is
compared against the half-float reconstruction it reads, captured by rendering
the same reconstruction into an Rgba16Float target, so the check does not depend
on the driver's rounding for that store. HDR composition allows one 8-bit code step
for attachment transfer and quantization. Fixtures cover constant fields,
vertical and 45-degree edges, a steeper rasterized edge, full-range and
mid-range checkerboards, single-pixel inputs, and odd extents. The EASU
nearest-four clamp bounds each result; arbitrary rasterized slopes can have
small reversals within those bounds.

Run the informational timing test on the selected Vulkan device:

```sh
cargo test -p worldrunner-render-post gpu_post_chain_scale_timings_are_informational -- --nocapture
```

It reports three post-chain GPU samples for each of 0.5, 0.77 and 1.0 scale at
320x180 output. Samples include tone mapping and enabled reconstruction/RCAS.
The first sample can include driver warm-up. These timings have no performance
threshold and exclude scene rendering. Linux CI selects Mesa lavapipe with
`VK_ICD_FILENAMES` and `WGPU_BACKEND=vulkan`.

## Separable Gaussian bloom

Set `HdrPostSettings::bloom_kernel` to `BloomKernel::SeparablePyramid` to
select Gaussian bloom. `BloomKernel::Tent` remains the default and retains the
existing binomial reduction and tent reconstruction. `bloom: None` schedules no bloom passes for either
kernel. The shared `Bloom` parameters control threshold, quadratic knee, level
count (1 through 8), and final intensity. The Gaussian option supplies eight
`GaussianLevel` entries; only the requested prefix is read. Each entry specifies
an integer radius (0 through 64 texels), RGB tint and scalar weight. Tint and
weight must be finite and in 0 through 64. Radius zero is an identity blur.

```rust
use worldrunner_render_post::{Bloom, BloomKernel, GaussianLevel, HdrPostSettings};
let settings = HdrPostSettings {
    bloom: Some(Bloom { threshold: 1.0, knee: 0.5, intensity: 0.7, levels: 5, policies: Default::default() }),
    bloom_kernel: BloomKernel::SeparablePyramid([GaussianLevel {
        radius: 44,
        tint: [1.0, 0.9, 0.8],
        weight: 0.2,
    }; 8]),
    ..HdrPostSettings::default()
};
settings.validate().unwrap();
```

The first pass applies the bright pass independently to four source texels and
averages them into a ceiling-half-resolution target. Later levels average 2x2
blocks with edge clamping, so the last odd row and column remain represented.
Each extent is `ceil(previous / 2)` on each axis. By default, a requested level
after 1x1 is rejected by the factory before target allocation. Set
`bloom.policies.terminal = BloomTerminal::Repeat` to retain the requested level
count on small extents. Small images can use `gaussian_extents` for default
validation or `gaussian_extents_with_terminal(size, count, terminal)` to inspect
the selected policy's extents.

After all downsample passes, levels execute from coarsest to finest. A horizontal
Gaussian blur writes a scratch target. The vertical blur writes back into that
level's original downsample target, applies its tint and weight, and adds the
bilinearly upsampled result from the next coarser level. The terminal vertical
pass has no coarser contribution. The final half-resolution image is bilinearly
upsampled during tone mapping and multiplied by `Bloom::intensity`. Alpha comes
from the original scene. Unit tints and weights summing to one preserve a
constant field's post-bright-pass radiance. Other weights deliberately change
energy.

The CPU generates a normalized discrete Gaussian with `sigma = radius / 3`.
Adjacent positive taps are folded into one bilinear sample with a fractional
offset and mirrored around the centre. Each axis requires
`1 + 2 * ceil(radius / 2)` filtered samples per pixel. Folded taps live in an
immutable storage buffer and are read directly by the shader. Each level owns
two RGBA16Float textures and a tap buffer per admitted slot. The original
level target is reused after its downsample consumers have executed. Resize and
reconfiguration use the existing transactional `PostChain` replacement path.

Every pass has a stable timestamp identity: `gaussian_bright` at level zero,
`gaussian_downsample` at subsequent levels, and `gaussian_horizontal` /
`gaussian_vertical` at each logical level. Enable the existing chain timing API
to collect these individually. The explicit synthetic probe renders a
1600x900 frame with radii 44, 32, 20, 12 and 6, discards five warm-up samples
and reports per-pass medians over twenty samples:

```sh
WGPU_BACKEND=vulkan cargo test -p worldrunner-render-post --lib \
  gaussian_bloom_stage_timestamps -- --ignored --nocapture --test-threads=1
```

The shader reads taps directly from the storage buffer. Passing the tap array to
a helper by value and indexing it dynamically produced the same output but cost
about 236 ms per frame on an RDNA3 integrated GPU, against about 1.2 ms for direct
access, so that form is not used.

Numerical tests compare each odd-extent pyramid level to a discrete CPU
convolution, including centre/edge impulses, radius 0, 1, 4, 44 and 64, threshold,
knee, tint and weights. The GPU tolerance is `0.004 * abs(reference) + 0.0001`,
accounting for half-float stores and hardware bilinear interpolation. CPU folded
weights preserve normalization and discrete convolution within `1e-6` for every
supported radius.

### Authored pyramid policies

`Bloom::policies` configures the same separable pyramid through `BloomPolicies`.
Use `BloomPolicies::default()` when migrating existing `Bloom` literals. Defaults
retain per-source-texel soft-knee extraction, box reduction, radius/3 kernels,
vertical tint, all levels blurred and output level zero. Their stage identities
and arithmetic order remain unchanged. Nondefault policies require
`BloomKernel::SeparablePyramid`; the tent path rejects them during validation.

- `BrightExtraction::SoftKnee` retains the max-channel quadratic knee.
  `ToneCurve(ToneCurveBright)` sanitizes and clamps RGB to `maximum`, computes
  luminance with `luminance` weights, and evaluates
  `s = max(T(exposure * exposure_scale * luminance) - T(threshold), 0) * contrast`.
  It returns `rgb * s / max(max_channel(rgb), epsilon)`. `BrightCurve` selects
  ACES filmic `x*(2.51*x+0.03)/(x*(2.43*x+0.59)+0.14)` or Reinhard `x/(1+x)`.
  These curves are evaluated without an additional output clamp. Maximum and
  epsilon must be finite in `(0, 65504]`; exposure scale and contrast must be
  finite in `[0, 65504]`. Luminance coefficients must be finite in `[0, 1]` and sum
  to one within `0.0001`. Exposure remains a caller-owned GPU value. The tone
  curve maps `Bloom::threshold` into its output range, so with this extraction
  the threshold may be any finite value in `[0, 65504]`. The soft knee compares
  colour values directly and keeps the `[0, 64]` threshold range.
- `BloomReduction::BoxPerTexel` is the legacy mode: extraction runs
  independently on four integer source texels before averaging. Later levels
  average four integer texels. `QuarterTexelAfterAverage` averages four bilinear
  clamp-to-edge samples at destination centre plus/minus one quarter **source**
  texel on each axis, then extracts brightness on the first reduction. Every
  later reduction uses those same positions. Destination centres map through
  normalized coordinates using the ceiling-half target extent, including odd
  source extents.
- `GaussianKernelRule::RadiusThird` retains the original radius and sigma.
  `AxisClampedPairs` computes each axis independently: `r = min(radius,
  max(axis_extent, 1))`, `N = ceil(r/2)`, support `2*N`, and
  `sigma = (2*N+0.5)/3`. Radius zero remains identity. Symmetric taps normalize
  to one and adjacent taps fold into bilinear pairs. Horizontal and vertical
  passes bind their own immutable storage weights for this rule. The public
  `parameters` and `weights` methods expose the CPU calculation.
- `BloomLevelSelection` separates `first_blurred` and `output` from
  `Bloom::levels`, the number of reductions. Indices begin at zero for half
  resolution. Validation requires `first_blurred <= output < levels`; levels
  below `first_blurred` remain bright-only reductions. For six reductions with
  a bright-only half-resolution level and quarter-resolution output, select
  `{ first_blurred: 1, output: 1 }`. Coarser blurred levels still fold additively.
- `BloomTerminal::Reject` is the default and rejects any requested level after
  the first 1x1 reduction. `Repeat` keeps all remaining logical levels at 1x1.
  Each is reduced, blurred, tinted, weighted and folded independently through
  the existing pyramid. Selection indices still address logical levels, and
  `AxisClampedPairs` clamps each radius to the 1-texel axis before pairing.
  For six levels, a 1x1 source produces six 1x1 levels; 17x9 produces 9x5,
  5x3, 3x2, 2x1, 1x1, 1x1; 32x32 produces 16x16, 8x8, 4x4, 2x2, 1x1, 1x1.
  This option applies to both `BloomPyramid::append` and stock HDR bloom.
- `BloomTintPass::Vertical` retains the original tint placement.
  `Horizontal` applies RGB tint before the horizontal half-float store; weight
  remains in the vertical pass before additive folding. Moving tint changes
  intermediate half-float rounding, so the two placements can differ slightly.

### Bloom in a caller schedule

`BloomPyramid { bloom, levels }.append(device, set, scene, exposure)` appends
only the existing bloom passes and retains their textures and weight buffers
in the caller's `StageSet::resources`. It returns an RGBA16Float output view at
`selection.output`. The output is folded bloom **before final intensity**; the
caller multiplies by `Bloom::intensity` in its own composite. No exposure,
tone-mapping, anti-aliasing or composite stage is scheduled by this builder.

The scene must be a single-sample, single-mip, single-layer, filterable D2 color
texture view with `TEXTURE_BINDING` usage, covering the complete texture. For
`ToneCurve`, pass `Some(&exposure_buffer)`: its first four bytes are a native
f32 nonnegative exposure and its usage includes `STORAGE`. Additional trailing
bytes are permitted. `SoftKnee` accepts `None`. Invalid settings, extents and
missing/incompatible exposure buffers return `PostError` before appending any
stages or resources. The caller must supply matching-device handles and a view
of the complete texture.

Schedule exposure writes or copies before appending bloom stages, then append
the caller's composite using the returned view. Keep per-frame updates ordered
with the host's frame admission and submission contract. Updating the bound
buffer changes extraction on the next execution without rebuilding pipelines.
The stock HDR path supplies its admitted-slot exposure when adaptation is
installed, or its configured fixed exposure otherwise.

Synthetic GPU references exercise both extraction curves, exposure changes
between frames, both reduction modes, even and odd extents, independently
clamped axis kernels, tint placement, six reductions with a bright-only first
level, and a caller-owned exposure-copy and composite schedule. Each RGB channel
allows `0.006 * abs(reference) + 0.0002` for accumulated half-float stores and
bilinear interpolation. The existing default-policy comparisons retain their
original tighter tolerance.
