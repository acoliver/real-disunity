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
further resolves are added as variants. Its variants are `None`, `Fxaa`, `Smaa`
(see [SMAA 1x](#smaa-1x)) and `Cmaa2` (see [CMAA2](#cmaa2)); one resolve is selected, never
two. Prefer SMAA when long high-contrast geometry edges dominate and three passes fit the
budget, FXAA as the cheaper single pass for noisy specular detail, and CMAA2 when the
adapter has compute shaders and the image has text or thin detail that must stay crisp.
Sharpening is a separate, optional field, `AntiAliasing::sharpen: Option<CasSettings>`, that
composes with any resolve (see [Contrast adaptive sharpening](#contrast-adaptive-sharpening)).
`AntiAliasing::new` leaves it `None`; `with_sharpen` sets it.

`AntiAliasing::jitter: Option<ProjectionJitter>` shifts the projection by a sub-pixel offset.
`ProjectionJitter::new(length)` is a Halton(2,3) sequence of 1 to 64 frames
(`ProjectionJitter::default()` has 8). `offset_pixels(frame_index)` gives the sub-pixel
offset in output pixels, strictly inside (-0.5, 0.5) with +y down, and
`clip_offset(frame_index, size)` the matching clip-space translation. The projection shift moves
the scene by +offset, so the pixel at `q` holds the scene at `q + 0.5 - offset`. `AntiAliasing::new`
leaves jitter off. `PostResolve::Taa` turns it on with the default sequence
(`PostResolve::effective_jitter`); see [Temporal antialiasing](#temporal-antialiasing).

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
`AreaTex.h` and `SearchTex.h`, so the bytes are identical to the published tables. The digests were computed from the published headers when the generator was ported; the headers themselves are not committed.

SMAA reads clean stair-step shapes. A straight axis-aligned edge is already sharp and is left
unchanged; blending happens at steps, diagonals, corners and line ends. A one-pixel line is
softened at its ends and kept along its length. Compared with FXAA it preserves thin geometry
better and does not blur texture detail, but it does not smooth shader or texture aliasing.
Choose it when geometry edges dominate and the three passes fit the budget. Choose FXAA for a
single cheap pass over noisy specular detail.

Differences from the reference shader: area lookups use explicit bilinear arithmetic, and
predication, reprojection and subsample indexing (SMAA S2x, T2x, 4x) are not implemented.
Edge detection reads the color input with point loads and neighborhood blending uses one
hardware bilinear fetch per side, as the reference does.

The display-referred image between tone mapping and SMAA is `Rgba16Float`. Values in 0..1
have an 11-bit mantissa at half precision, finer than the 8-bit output, and the format is
filterable, which the bilinear blend fetch needs. FXAA keeps its `Rgba32Float` intermediate
and its flat-region precision contract. `PostResolve::display_format` is the one place that
chooses the format. A resolve followed by an upscale still writes `Rgba32Float` between the
resolve and the upscale.

Measured with the ignored probe `chain::aa_tests::smaa_gpu_tests::smaa_and_fxaa_stage_timestamps` on an
AMD Radeon 780M (RADV, Mesa 26.0.8) at 1920x1080, median of 5 timestamp-query samples per
stage, on a synthetic frame of diagonal edges. The first column is the `Rgba32Float`
intermediate with two point-loaded texels in the blend, the second the `Rgba16Float`
intermediate with bilinear blending:

| Stage (ms) | SMAA high before | SMAA high after |
| --- | --- | --- |
| tone | 2.168 | 1.152 |
| `smaa_edges` | 0.892 | 0.547 |
| `smaa_weights` | 1.223 | 1.184 |
| `smaa_blend` | 1.210 | 0.571 |
| total | 5.493 | 3.454 |

The other presets moved the same way (total, before to after): low 4.989 to 2.953, medium
4.951 to 3.124, ultra 5.674 to 3.259. FXAA measured 3.387 before and 3.102 after with no code
change, and no AA measured 0.366 and 0.408, which bounds the run-to-run noise at about 0.3 ms.
The weights pass reads only `Rg8Unorm` edges and is unchanged.

### CMAA2

`PostResolve::Cmaa2(Cmaa2Settings)` selects Conservative Morphological Anti-Aliasing 2.3 instead
of FXAA or SMAA; `HdrPostSettings::resolve` holds one resolve. `PostResolve::cmaa2(Cmaa2Preset::High)`
builds a preset. `Cmaa2Settings::LOW`, `MEDIUM`, `HIGH` and `ULTRA` equal the reference
`CMAA2_STATIC_QUALITY_PRESET` levels. They differ only in the edge threshold:

| Preset | `edge_threshold` |
| --- | --- |
| Low | 0.15 |
| Medium | 0.10 |
| High (default) | 0.07 |
| Ultra | 0.05 |

`Cmaa2Settings::edge_threshold` accepts values above 0 and up to 0.5; others fail with
`PostError::UnsupportedProfile`. `with_extra_sharpness` turns on the reference
`CMAA2_EXTRA_SHARPNESS` option, which keeps more text and small detail and smooths less.
Edges are found on the square root of the display-linear color as a luma approximation, as the
reference does for linear input.

The stage runs where FXAA and SMAA run: on the display-referred image at the render extent
after tone mapping and before any upscale. The image is `Rgba16Float` with storage binding. It
schedules `cmaa2_clear` (a buffer clear of the counters), the compute passes `cmaa2_edges`,
`cmaa2_candidate_args` and `cmaa2_candidates`, and the full-screen `cmaa2_present` as separate
stage identities, each with its own timing slot. It needs compute shaders and storage buffers in
the compute stage and in the fragment stage (read-only).

Deviations from the reference `CMAA2.hlsl` (the list is also at the top of `src/cmaa2.wgsl`):

- Only the single-sample path is ported. The MSAA path (`CMAA2_SUPPORT_MSAA`) is not.
- `ComputeDispatchArgsCS` becomes the one-thread pass `cmaa2_candidate_args`, because the
  candidate count is final only after the edge pass. It writes indirect dispatch arguments from
  the counter into a 12-byte `INDIRECT` buffer, and `cmaa2_candidates` uses
  `dispatch_workgroups_indirect` (`Operation::ComputeIndirect`). Lists longer than 4096
  workgroups wrap to further rows. A buffer clear zeroes the counters instead of the reference
  pass.
- WGSL has no `R8Uint` storage texture and storage textures have no `InterlockedExchange`.
  Edges are one `u32` per 2x2 quad (4 bits per pixel) in a storage buffer, and the candidate and
  blend-item lists and the atomic list heads are storage buffers. Positions are not packed into
  14 bits, so the packing does not limit extents to 16384 pixels.
- The reference writes blended colors into the input image in place, in a compute pass
  (`DeferredColorApply2x2CS`) that visits each touched quad through a location list. The input
  here is a sampled texture, so `cmaa2_present` writes the output instead: every fragment reads
  its pixel, walks the blend list of its 2x2 quad (the list heads are per quad, as in the
  reference) and averages the items for its own pixel, or returns the input pixel unchanged when
  there are none. The result and the alpha of the original pixel match the reference apply
  pass. There is no working copy of the image, no apply pass and no location list. The edge
  pass resets every quad head, not only the heads of quads with edges, so the fragment pass
  never reads items of an earlier frame.
- Samples outside the image clamp to the border instead of reading as zero.
- A full work list drops the item instead of writing past the end. The lists hold a
  fraction of the pixel count, as in the reference.
- Luma edge detection, `CMAA2_COLLECT_EXPAND_BLEND_ITEMS` and
  `CMAA2_DEFERRED_APPLY_THREADGROUP_SWAP` are enabled, and colors in the lists use the
  `R11G11B10` E4 packing (non-HDR range, clamped to 1.99).

At 1920x1080 the CMAA2 storage is about 14.6 MB of buffers (edges and list heads 2.1 MB each,
candidates 2.1 MB, blend items 8.3 MB). There is no second image. These are counted in the post
allocation report. Without a CMAA2 resolve none of it is allocated.

Measured with the ignored probe `chain::aa_tests::smaa_gpu_tests::smaa_and_fxaa_stage_timestamps` on an
AMD Radeon 780M (RADV) at 1920x1080, median of 5 timestamp-query samples per stage, with the
total including the `tone` stage (about 0.9 to 1.0 ms, the same work for every resolve except
FXAA, whose `Rgba32Float` intermediate makes `tone` about 2 ms). Times are in ms. The "disc"
frame is a sparse disc edge and the "stripes" frame is dense stripes. Run-to-run noise is about
0.1 to 0.3 ms.

| Resolve | AA stages, stripes | Total, stripes | Total, disc |
| --- | --- | --- | --- |
| none | none | 0.375 | 0.344 |
| FXAA | `fxaa` 0.930 | 3.068 | 2.816 |
| SMAA low | edges 0.542, weights 0.770, blend 0.441 | 2.760 | 2.017 |
| SMAA medium | edges 0.543, weights 0.788, blend 0.516 | 2.932 | 2.101 |
| SMAA high | edges 0.543, weights 1.120, blend 0.441 | 3.053 | 2.081 |
| SMAA ultra | edges 0.543, weights 1.173, blend 0.430 | 3.144 | 1.981 |
| CMAA2 low | edges 0.450, candidates 0.582, present 0.540 | 2.613 | 1.801 |
| CMAA2 medium | edges 0.413, candidates 0.546, present 0.478 | 2.433 | 1.735 |
| CMAA2 high | edges 0.429, candidates 0.522, present 0.454 | 2.374 | 1.727 |
| CMAA2 ultra | edges 0.413, candidates 0.518, present 0.473 | 2.383 | 1.746 |

On this GPU CMAA2 high now costs less than SMAA high and FXAA in total: 2.37 ms against 3.05 ms
(SMAA) and 3.07 ms (FXAA) on stripes, and 1.73 ms against 2.08 ms and 2.82 ms on the disc. The
FXAA total is mostly its wider `tone` intermediate. Counting only the AA passes, CMAA2 high is
1.40 ms on stripes and 0.73 ms on the disc, SMAA high is 2.10 and 1.06 ms, and the single `fxaa`
pass is 0.93 and 0.84 ms: CMAA2 is cheaper than SMAA on both frames, and cheaper than the FXAA
pass only on the sparse frame. The disc edge pass costs 0.30 ms, the same as `smaa_edges` at
0.32 ms, which is close to the time to read the 16.6 MB image once. The extra cost on the stripes
frame is the candidate pass (0.52 ms, atomics and the line search) and the present pass
(0.45 ms against 0.38 ms on the disc). The preset changes the time by less than the noise, because
the fixed-size dispatches run the same number of threads whatever the edge count.

How the time got here, on the stripes frame at the high preset (total with `tone`): indirect
dispatch of the candidate and apply passes gave 3.88 ms (disc 2.71 ms); folding the apply pass
into the present pass and dropping the full-image working copy gave 2.37 ms (disc 1.73 ms). The
edge pass fell from 1.20 to 0.43 ms mostly because it no longer writes four `Rgba16Float`
texels per thread. Edges were already stored packed, one `u32` per quad. One experiment was
rejected: sharing the quad lumas through workgroup memory so each edge thread loads 4 texels
instead of 8 (one more barrier) changed the stripes edge pass from 0.429 to 0.418 ms and the
disc from 0.305 to 0.295 ms, within noise, so it was reverted to keep the pass simple. The probe
skips the measurement and passes on adapters without `TIMESTAMP_QUERY`, such as lavapipe.

### Contrast adaptive sharpening

`CasSettings { sharpness }` selects AMD FidelityFX CAS in its sharpen-only form. It is not a
`PostResolve` variant: it does not remove aliasing, it restores local contrast that a resolve or
a soft texture filter took away, so it follows whichever resolve is selected. Set it with
`AntiAliasing::with_sharpen(CasSettings::new(0.5))` or `HdrPostSettings::sharpen`.
`sharpness` must be finite and within 0 through 1, or `PostError::UnsupportedProfile("CAS settings")`
is returned. Sharpness 0 is the reference default with the least ringing and still sharpens;
1 is the strongest setting. `None` schedules nothing: the stage list, intermediates and output
bytes are those of a build without the field.

The stage runs after the resolve (FXAA, SMAA, CMAA2, or tone mapping when the resolve is `None`) and
before any upscale, at the render extent, as one fullscreen pass with the stage identity `cas`
and its own timing slot. Its input is the display-referred image between tone mapping and the
output, so the same intermediate rules apply: a resolve that feeds CAS writes an `Rgba16Float`
texture, and so does tone mapping when the resolve is `None`. The FXAA, SMAA and CMAA2 intermediates
are unchanged when CAS is off. With an upscale, CAS writes an `Rgba16Float` texture that the
upscale reads. The allocation budget counts that one extra intermediate.

For each pixel the filter reads itself and its four neighbours. The green channel gives the local
minimum and maximum; the headroom to black or white divided by the maximum, square-rooted, is the
amount of sharpening. Where contrast already reaches 0 or 1 the amount is 0, so flat areas and
saturated edges are untouched. The amount scales a negative lobe between -1/8 (sharpness 0) and
-1/5 (sharpness 1) on the four neighbours, and the result is normalized by `1 + 4w` and saturated.
Every output channel stays within the 3x3 neighbourhood range on a soft edge, a constant image is
unchanged at every sharpness, and alpha is copied.

How it differs from RCAS: RCAS is the optional second pass of the FSR 1 upscale. It runs at the
output extent after EASU, takes its strength in stops from 0 (strongest) through 2, and limits
ringing from the minimum and maximum of each color channel. It also supports an opt-in native-resolution pass. CAS
runs without scaling, at the render extent, with a 0 through 1 sharpness and a contrast
adaptive amount driven by headroom, so it works with MSAA, FXAA or SMAA at native resolution. With
an upscale configured, CAS sharpens before reconstruction and RCAS after it; use both only if the
combined result is not over-sharpened. Choose the filter and its domain explicitly when restoring detail after reconstruction or a
soft resolve at native resolution.

Differences from the reference header: only the `noScaling` path of `CasFilter` is ported, as a
fragment pass. Taps outside the image clamp to the edge texel. The default quality path is used
(green-channel weights, no `CAS_BETTER_DIAGONALS`) with exact reciprocals and square roots in
place of the `APrx` approximations, and the 16-bit packed form is not ported. The shader file
keeps the AMD notice.

`src/cas_reference.rs` is a scalar f64 reference of the same kernel. The pass alone, writing an
`Rgba32Float` target from f32 input images with odd extents, is compared with it at an absolute
per-channel bound of 0.00001, a few f32 ulps. Chain tests then check each resolve, resize, the
upscale ordering and the 8-bit sRGB result.

Measured with the ignored probe `chain::aa_tests::smaa_gpu_tests::smaa_and_fxaa_stage_timestamps` on an
AMD Radeon 780M (RADV, Mesa 26.0.8) at 1920x1080, median of 5 timestamp-query samples, on a
synthetic frame of diagonal edges, sharpness 0.5:

| Configuration | `cas` stage (ms) | Total (ms) |
| --- | --- | --- |
| tone mapping, then CAS | 0.417 | 1.678 |
| FXAA, then CAS | 0.492 | 4.618 |
| SMAA medium, then CAS | 0.490 | 4.644 |

The CAS pass costs about 0.4 to 0.5 ms. The run-to-run noise on this adapter is about 0.3 ms
(the FXAA and SMAA blend passes moved by more than the CAS pass between runs), so read the
stage figure as a bound rather than a precise cost.

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
the HDR path omits reconstruction and upscale RCAS. Native RCAS is separately opt-in,
so the default preserves the native pass count and output bytes.
Use `PostChain::resize_extents` to replace both extents and their dependent
bindings transactionally. `resize` changes the internal extent; an explicit
presentation extent remains unchanged. Reconfiguration rebuilds stage bindings.

### Native-resolution RCAS

`HdrPostSettings::native_rcas: Option<NativeRcasSettings>` schedules one `rcas`
fullscreen pass when the render and output extents are equal, after tone mapping,
the selected resolve and CAS. `None` is the default and allocates no extra texture
or stage. `NativeRcasSettings { stops, domain }` requires finite stops within
`0..=2`: zero is strongest, two is weakest and neither means off. Invalid settings
fail validation before allocation. At reduced render extents this setting is
inactive; `UpscaleSettings::sharpness` retains its display-linear behavior.

`RcasDomain::DisplayLinear` applies the existing RCAS equations directly to
RGB. `RcasDomain::EncodedOutput` applies the output transfer to each RGB tap
before the same kernel: sRGB outputs use the piecewise sRGB OETF, while linear
UNORM and float outputs use identity transfer. Alpha is copied without transfer
or filtering. Border loads clamp to the edge. Both domains use full-precision
f32 division, including `1 / (1 + 4*lobe)`, rather than the reference's approximate
reciprocal. No input tap is quantized to bytes.

The final native RCAS input is `Rgba32Float`, one extra texture per admitted slot.
For sRGB encoded-domain filtering the shader inverses the transfer on its result
so the attachment's OETF encodes it once before quantization. Values saturate to
`[0,1]` for the encoded sRGB output. UNORM conversion rounds to the nearest code;
transfer implementation error and rounding at code boundaries permit a difference
of one 8-bit code from the scalar f64 reference, including ties. The tiny known-edge
GPU test checks that bound on each channel. The direct display-linear float test
uses absolute error `0.00001` for f32 arithmetic; it has no half-float store.

Interactive viewer frames select `FrameProfile::HdrNativeRcas { samples, tone_map,
rcas }`. Retained captures select `RetainedCaptureRecipe::with_native_rcas(rcas)`,
which validates the same settings and requires an HDR frame profile. Each capture
sample runs at full raster resolution without reconstruction; the recipe override
is restored to the interactive profile afterward. Supersampling and temporal
accumulation combine the sharpened samples through the existing capture path.
Feature readbacks retain their existing non-accumulated recipe restriction.

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

`src/cmaa2.wgsl` is a modified port to WGSL of
[CMAA2.hlsl](https://github.com/GameTechDev/CMAA2) (Conservative Morphological
Anti-Aliasing 2.3, author Filip Strugar). Copyright (c) 2018, Intel Corporation. Licensed
under the Apache License, Version 2.0. The full license text is in
[LICENSE-APACHE-2.0-CMAA2.txt](LICENSE-APACHE-2.0-CMAA2.txt). The shader file carries the
copyright and license header verbatim, a statement that it was modified, and the list of
changes. The upstream repository has no NOTICE file.

`src/cas.wgsl` ports the sharpen-only path of
[AMD FidelityFX CAS, ffx_cas.h](https://github.com/GPUOpen-Effects/FidelityFX-CAS) (ffx-cas/ffx_cas.h).
Copyright (c) 2017-2019 Advanced Micro Devices, Inc. All rights reserved.
Licensed under the MIT License. The full copyright, permission and warranty
notice is preserved in the shader file.

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

## Temporal antialiasing

`PostResolve::Taa(TaaSettings)` (the scene recipe `SceneAntiAlias::Taa { history_micro, clip_scale_micro }` authors both fields; `history_weight` 0.0 to 0.99, default 0.9; `clip_scale` 0.5 to 2.0,
default 1.0) adds three stages, `taa resolve`, `taa history` and `taa depth history`, ahead of the luminance histogram, so
exposure metering, bloom and the tone stage read the TAA output. `taa.wgsl` follows Karis 2014 (color box and
compressed-luma clip); the current frame uses the nearest jittered sample (1x; a multisampled
frame is not jittered, so the sample is the MSAA pixel and a static 4x image equals MSAA alone,
0.00046 against 0.00161 with jitter at 64x64; resampling the shifted frame gave 0.00153), the box is the
3x3 min/max, the blend is linear and history is read with Catmull-Rom; the history weight is scaled by `clamp(1 - k * offset_px, 0, 1)` (a motion-adaptive blend factor, as in Karis 2014 and later TAA talks), k = 1.5 at 1x and 16 on multisampled frames, so static pixels keep the full weight (static 1x 0.00201 against 0.00436 aliased, static 4x equal to MSAA alone) and in motion 4x TAA equals MSAA alone (0.00036 and 0.00043 against 0.00036 and 0.00043) while 1x TAA is 0.88x and 0.73x the history-reset error (mean over frames, +41 and -29 mm per frame); the cost is less supersampling of moving pixels. An explicit `with_jitter` sequence on a multisampled target is ignored; `taa_reference.rs` is the f64 reference the GPU tests compare against.

`taa resolve` reads the resolved scene color, the frame depth (`texture_depth_2d`, or
`texture_depth_multisampled_2d` with a per-sample minimum when MSAA is on), the history texture and
the per-slot uniform block, and writes the TAA image. `taa history` copies that image into the
history texture with `Operation::CopyTexture` (`copy_texture_to_texture`, encoded outside a render
pass, so it has no timing slot). One `Rgba16Float` history texture per render size is shared by all
frame-slot stage sets, so the history is the previous frame whichever slot ran it and however many
frames are in flight. A resize replaces it, and settings without TAA release it. The shader ignores
the history while the block says it is invalid, so no clear is needed.

`taa resolve` also binds an object-motion residual at `@binding(5) velocity` (`taa_gpu::Inputs::velocity`
with `velocity_samples`). The format is `Rg16Float`; the value is the object-only residual in
previous-frame uv units, `uv(P_prev * world_prev) - uv(P_prev * world_now)`, where `world_prev` is the
same surface with the previous transform and animation time. `history_uv` adds the residual of the
closest-depth texel to the camera reprojection (Karis dilation: depth and velocity come from the same
texel, and from the minimum-depth sample when the target is multisampled, where the binding is
`texture_multisampled_2d` and its sample count must equal the depth's). Zero means camera
reprojection alone, so a cleared target needs no special case. With `velocity: None` the resolve binds
a 1x1 zero texture of the same sample count, which is what a scene without velocity pipelines gets:
object passes of `render-scene` write the wind residual only when built with `with_taa_velocity`
(`ShadowedScene::with_taa_velocity` for shadow receivers; SSR scenes write it when the colour budget fits), and
output is unchanged from a build without the binding. The motion-adaptive
weight reads the total displacement including the residual. The reference and GPU tests for this are in
`taa_gpu_tests/velocity_tests.rs`.

Measured at 1920x1080 on a Radeon 780M with `smaa_and_fxaa_stage_timestamps` (median of 5, stripes
and disc scenes). The fullscreen `taa history` pass took 0.87 to 0.97 ms of a 2.99 to 3.18 ms timed
TAA path (`taa resolve` 1.81 to 1.89 ms, `taa history`, `tone` 0.31 to 0.33 ms). The texture copy has
no timestamp, so the probe also times 24 frames in one submission without queries: the whole chain
(scene paint, TAA and tone) fell from 3.95 and 3.86 ms per frame to 3.37 and 3.39 ms, so the copy
costs about 0.4 ms against 0.9 ms for the pass, a saving of 0.5 to 0.6 ms per frame. Sampling the
resolve output into a second history target and swapping the two needs bind groups that change per
frame, which slot-static stage sets cannot hold, so it was not tried.

`TaaUniform` is the per-frame block, `TAA_UNIFORM_BYTES` = 96 bytes, little-endian, one uniform
buffer per frame slot uploaded by `ToneMapOutput::prepare_taa`:

| Bytes | Content |
| --- | --- |
| 0..64 | `reprojection`, column-major `mat4x4<f32>`: previous unjittered view-projection times the inverse current one, computed in f64 by the host |
| 64..72 | `jitter_pixels` x, y (+y down) |
| 72..76 | `history_valid`: 1.0 when the history texture holds the previous frame, else 0.0 |
| 76..80 | zero |
| 80..88 | `depth_terms` `a`, `b` (two `f32`): linear view depth of device depth `z` is `b / (z - a)`; `b` = 0 disables the disocclusion test |
| 88..96 | zero |

An all-zero block means no history: the stage returns the reconstructed current color. The block is
built by `FrameRenderer` from the camera data, so this crate needs no matrix library.

History lifecycle: the first frame, `reset_temporal_history`, a resize, a jitter change and a
cancelled or failed transaction give `history_valid` = 0. The host must draw into
`FrameTargets::depth()`.

### TAA depth history

The 96 bytes are the 80-byte block of the reprojection, jitter and `history_valid`, then `depth_terms` `(a, b)` at bytes 80..88 and 8 bytes of padding. The linear view depth of device depth `z` is `b / (z - a)` with `a = far / (far - near)` and `b = -near * far / (far - near)`; `b == 0` disables the disocclusion test. `render-scene` reads the terms off the unjittered view-projection (the z row minus `a` times the w row is the translation lane), so no near or far plane travels with `TemporalView`.

`taa resolve` writes a second target, the linear depth of the closest-depth texel (`R32Float`, `taa_gpu::DEPTH_FORMAT`), through `StageTarget::Pair`, and `taa depth history` copies it into the depth history with `Operation::CopyTexture`, as the color path does; the frame depth itself cannot be copied (it may be multisampled and has no `COPY_SRC` guarantee). The depth history has the lifecycle of the color history: one texture per render size shared by all frame slots, created and replaced together with the color history, released by `release()` when TAA is no longer requested, and reset only through `history_valid`. The resolve reads it at `@binding(6)` with `textureLoad`.

Measured at 1920x1080 on a Radeon 780M with the same probe (stripes and disc scenes, median of 5): `taa resolve` takes 1.99 and 2.01 ms with the depth history against 2.04 and 1.93 ms before it, and the untimed whole chain takes 3.99 and 4.18 ms per frame against 3.78 and 3.77 ms, so the second target and the depth copy cost 0.2 to 0.4 ms per frame.
