# Desktop and windowless scene host

`ViewerOptions` configures the scene host, including `RetainedHost`. Presentation
cadence controls when a frame is requested; surface present mode controls how the
window system presents it.

## Retained shadow light

Submit `RetainedUpdate::Settings` with effects updated through
`SceneEffects::with_shadow_direction` to move the shadow light without replacing
resident geometry. `None` follows the direct sun. Direct lighting, sky and the
celestial sun are unchanged. Interactive frames and same-frame captures use the
same shadow direction, including when depth fitting is enabled.

## Batch object updates

`RetainedUpdate::Objects(Vec<SceneObjectEdit>)` applies many `Add { id, object }`,
`Replace { id, object }` and `Remove(id)` edits as one atomic update. The host
clones its object table once, validates and applies the edits in order, rebuilds
the scene once and takes one preparation slot, so a game can edit hundreds of
objects per frame without reaching the preparation limit. Edits apply in order:
the last edit to an ID wins, and an `Add` after a `Remove` of the same ID is
accepted. Adding an ID that is present, or replacing or removing an unknown ID,
fails the whole batch. A failing batch leaves the host unchanged and the error
names the edit (`object edit 3: unknown object ID`). An empty batch is accepted
and takes no slot. The single-object variants keep their behavior.

## Presentation settings

Set `present_mode` explicitly to `PresentMode::Fifo`, `FifoRelaxed`, `Mailbox` or
`Immediate`. The examples use FIFO and `desired_maximum_frame_latency: 2`, the
host's default presentation behavior. Automatic fallback modes (`AutoVsync` and
`AutoNoVsync`) are rejected as `ViewerError::InvalidOptions`. If the selected
surface does not advertise the requested mode, startup returns
`ViewerError::UnsupportedPresentMode` before configuring the surface. There is no
fallback to a different mode.

`desired_maximum_frame_latency` must be positive. It is a wgpu hint, which the
backend can clamp to its supported range. It is independent of
`frames_in_flight`, which bounds admitted engine submissions in `1..=3`.
Windowless hosts validate both presentation fields but ignore them because they
have no surface.

## Window dimensions

`size` is the initial request, with each dimension in `1..=4096`.
`window_size_unit: WindowSizeUnit::Logical` preserves logical-pixel creation.
Select `WindowSizeUnit::Physical` to request exact physical backing dimensions,
independent of the display scale. The window system may constrain either request.
Resize events and retained host sizes always use physical pixels. Windowless hosts
have no display scale, so both units use the supplied pixel dimensions directly.

## Presentation allocation budget

Set `presentation_budget: PresentationBudgetPolicy::default()` for a fixed 1 GiB
limit, or `PresentationBudget(bytes).into()` for an explicit fixed byte limit.
`PresentationBudgetPolicy::Derived { headroom_bytes, granularity_bytes, cap_bytes }`
computes the complete live descriptor inventory at startup and every replacement,
adds headroom and rounds up to the positive byte granularity. `cap_bytes: Some(n)`
clamps that result to `n`; an inventory above the cap retains typed budget rejection.
The peak includes the current and all unretired generations, counting shared
storage once. Overflow returns a typed configuration error. Zero fixed limits,
zero granularity and zero caps return
`ViewerError::InvalidOptions` before window or GPU startup. This limit covers the
presentation allocation inventory, including CPU capture storage, rather than
all scene resources or total process memory.

Creation, resize, capture recipes and retained settings changes use the selected
policy. Rejections remain typed as `ViewerError::Configuration(ConfigurationError::Budget {
required, budget })`. The required bytes include renderer generations that
coexist during replacement. No rendering quality is reduced to fit the budget.

Desktop and windowless retained hosts reserve the 256 MiB capture-recipe pool in
the initial inventory, reducing the headroom available to renderer targets even
when no capture is requested. This host-wide pool is counted once across renderer
generations. Admitting a recipe reserves storage without replacing targets.
Executing accumulated recipes can replace targets for their raster footprint or
RCAS selection; restoring ordinary presentation uses the same admission policy.
Ordinary desktop and live presentation do not reserve that pool. Attachment
replacement creates targets while previous generations remain alive, so the full
replacement peak must fit.

`plan_presentation` returns an `AllocationSet` without a device or backing storage.
Pass the physical extent, viewer settings, effective scene effects, selected device
limits and features, and `PresentationPlanning` with the capture mode
(`PresentationCapture::Ordinary`, `Retained`, or `Recipe(&recipe)`) and retained
effect history. Set `retained_effect_history` to false at startup; for replacements
use `ConfiguredFrameRenderer::has_effect_history()`. History persists after disabling
post-processing or restoring ordinary presentation after a native-RCAS recipe.
Recipe plans include their supersampled raster footprint and native RCAS override. Call `validate(limits)` to obtain descriptor bytes. To plan a
replacement peak, extend one inventory with each live generation before validating;
shared entries are counted once. The planner does not allocate or reduce quality.

## Selected adapter

Set `adapter_callback: Some(AdapterCallback::new(...))` to observe an
`AdapterIdentity` containing the selected adapter's name, `AdapterClass` and
`backend: wgpu::Backend` (for example `Metal`, `Vulkan`, `Dx12` or `Gl`).
Classes map directly from wgpu's device type: `Integrated`, `Discrete`, `Virtual`,
`Software` (CPU adapters), or `Other`. No classification is inferred from a name.
The backend reports the selected graphics API independently of hardware class.
`GpuContext::adapter_identity()` exposes the same identity in the scene renderer.

The callback runs once per renderer startup, on the calling thread, before device
creation and GPU scene preparation. Desktop selection accounts for surface
compatibility. The selected adapter is then consumed to create the device; the
host does not select a second adapter. Returning `Err(String)` aborts startup as
`ViewerError::Driver`. Capture shared caller state to record a quality decision
for subsequent retained scene edits; the callback does not change the initial
scene or presentation settings.

## Retained frame statistics

`RetainedHost::statistics()` and `RetainedEventHost::statistics()` return the
latest `RetainedFrameStatistics`. It is absent before the first ordinary
submission and replaced before each `Presented` callback. A snapshot identifies
its frame and published scene generation. `presentation_budget_bytes` reports the
effective presentation admission budget, including the admitted replacement peak. Polling, capture-only work and queued
CPU edits do not replace it. Storage is one snapshot, with no event backlog or
GPU readback. Submission does not imply GPU completion.

`scene.meshes` and `scene.impostors` count main-camera indexed draws and their
submitted instance counts. Material slots and transition complements each count
as a draw; hidden and culled placements do not. Decals shade receivers inline,
without separate draw calls. `decals_visible` is the selected volume count after
frustum culling, priority selection and the 32-volume shader limit;
`decals_total` includes all authored volumes in that submitted frame.

`textures` covers the authored texture table and `mesh_buffers` covers vertex,
index and transition payloads. Resident bytes exclude driver overhead, staging
padding, instance buffers, internal textures and terrain resources. Uploaded
bytes are reported once per resident scene generation, including initial
residency; an unchanged subsequent frame reports zero uploaded bytes.

Terrain geometry/material page statistics, water and sky draws, all-pass draw
counts are not yet exposed. These omissions are
not represented by zero-valued category fields. The host still requests no GPU
timestamp capability and allocates no timing queries by default.

## GPU pass timing

Set `gpu_timing: true` to request GPU timestamps. The default is `false`, which
requests no timestamp features, allocates no query sets or readback buffers, and
leaves rendering commands and pixels unchanged. The host uses the existing
`PassTimer` and the stock compositor's pass-level timestamp writes. Only
`TIMESTAMP_QUERY` is required; inside-encoder and inside-pass writes are not used.

`RetainedHost::gpu_timing()` and `RetainedEventHost::gpu_timing()` expose
`Disabled`, `Pending`, `Unavailable(GpuTimingUnavailable)`, or `Complete(FrameTiming)`.
Startup continues when timing is unavailable so an optional observation cannot
prevent a game from rendering. Missing features produce `Unavailable(MissingFeatures(...))`,
with no zero-valued measurements substituted. A backend that advertises timestamps but
returns all-zero or reversed counters reports `Unavailable(InvalidTimestampPair { frame_id, pass_id })`.
A map or callback failure returns `ViewerError::Timing` instead of hiding the error.

A completed sample identifies its source `frame_id`, independent of the latest
submission statistics. Capture recipe timings identify each rendered subframe; later
subframes have distinct IDs, while the capture result retains its initial source ID.
Each pass retains its existing `PassId`: geometry uses `PassSlot` (shadows,
metadata/prepass, AO, opaque, SSR and water), output stages use `StageId`, and
multisample resolve uses `Resolve`. Only executed passes appear. Legacy presentation
is the `post copy output` stage. HDR output retains its authored post stage names.
Depth fitting adds `sdsm receiver initialize` and `sdsm receiver reduce` stages at
level zero to the same geometry/output sample, including same-frame captures.
Durations are nanoseconds using the queue timestamp period. All-zero or reversed
pairs yield `None`; wgpu does not expose the counter modulus. Equal nonzero counters
yield zero nanoseconds because a pass can be shorter than timer resolution. The host reports that sample as typed unavailable
instead of presenting it as a completed measurement.

The ring holds three unread frames, at most 1024 pass pairs per frame. When full,
a new measurement is dropped while rendering continues. Mapping is asynchronous;
normal nonblocking device polls deliver completions. Reading retains only the
newest completed sample and never waits or consumes it. Resize and settings
changes preserve the timing ring and its unread samples.

## Native-resolution sharpening

Use `FrameProfile::HdrNativeRcas { samples, tone_map, rcas }` for interactive
HDR frames. A retained HDR capture can override it with
`RetainedCaptureRecipe::with_native_rcas(NativeRcasSettings { stops, domain })`;
stops must be finite in `0..=2`. The override applies only to that recipe,
and ordinary frames restore the profile setting. The default is off.
See [the post contract](../render-post/README.md#native-resolution-rcas) for
domain transfer, reciprocal and quantization behavior.
