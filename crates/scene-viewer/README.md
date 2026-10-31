# Desktop and windowless scene host

`ViewerOptions` configures the scene host, including `RetainedHost`. Presentation
cadence controls when a frame is requested; surface present mode controls how the
window system presents it.

## Opaque capture attribution

Retained recipes accept `CaptureOutput::Albedo`, `SunVisibility`, `DirectDiffuse`,
`AmbientDiffuse`, `EnvironmentSpecular` and `SunSpecular`. Read them through the
matching `RetainedCaptureProducts` accessors (`direct_diffuse()`, for example).
All return linear `RGBA16Float` decoded images, top-left first, at native raster
extent. RGB albedo is unitless reflectance after textures, terrain-layer blending,
contact surfaces and projected decals. Sun visibility is strength-adjusted
unitless directional visibility in [0, 1], replicated in RGB. Alpha is opaque
coverage; uncovered pixels are zero. These products precede exposure, fog and
post-processing and exclude water, transparency, sky and dynamic sprite/UI draws.
They use a single pixel-centre sample even when forward colour uses MSAA.
Recipes must have one unjittered sample, no supersampling or advancing clock, and
no native RCAS accumulation.

Capture-only opaque pipeline permutations replay the existing material shaders
and directional map without advancing frame state. Depth-fitted captures encode
these replays in the fitted drawing transaction, preserving its selected terrain
pages and bindings. This avoids adding targets to ordinary draws and keeps
material and shadow calculations shared with lit colour.
Each requested product charges its RGBA16Float colour and Depth32Float replay
targets (12 bytes per raster pixel), readbacks and decoded storage against the
shared capture-recipe reservation (256 MiB by default). Descriptor planning and live admission
reject requests exceeding that pool with `CaptureRequestError::RecipeBudget`;
planning wraps it in `ViewerError::Capture`. Live admission also accounts for
outstanding reservations. With no attribution request, no replay targets or pipelines are
created and existing presentation allocation totals remain unchanged.
`DirectDiffuse` is the sun diffuse lobe after directional shadow visibility and
any diffuse AO selected by the specular convention. `LambertianPrepared` uses
albedo times (1 - metalness), N dot L, and sun irradiance; the default convention
also applies Fresnel energy conservation and division by pi. `AmbientDiffuse`
contains hemisphere or SH diffuse after material and screen AO. `EnvironmentSpecular`
is the forward environment reflection radiance, including its split-sum factor and
specular AO, before SSR replacement. `SunSpecular` contains the directional specular
lobe, or the clipped disk when configured. Radiance uses the same linear scene units
as `LinearHdr`, without camera exposure.

The four radiance products reconstruct opaque surface lighting within RGBA16Float
rounding. Mesh emission (including packed-atlas emission), masked backlighting and
translucency are excluded. Terrain has none of those terms. Neither mesh nor terrain
surface lighting currently adds local lights. Fog, exposure, SSR replacement,
water, transparency, sky and post-processing are outside this sum. Compare against
`LinearHdr` only where those operations are absent, with unit exposure and one
forward sample. Unlit surfaces publish albedo as ambient and zero other radiance.

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

## Retained scenes from a world

`edits_from_world_changes` turns one drain of `World` changes into a `Vec<SceneObjectEdit>`,
using a `MeshResolver` from `worldrunner_render_plan::simulation::mesh_instances` and an
`ObjectIdMap`. A resolver that only maps handles builds default scene objects with the
instance's tint. A resolver that overrides `resolve_template` returns
`MeshTemplate::Objects`, the scene objects the host draws for the instance with every field it
sets (LOD distance scale, fade, tint, shadow flag). Extraction replaces only each object's
transform with the instance's world transform.

A host whose own placement policy computes the scene transform (quantized saved orientation,
axis conventions, per-entity scale) returns `MeshTemplate::Placed` instead. Extraction returns
those objects unchanged, transform included, and does not read the world transform. The journal
still decides which objects are extracted again, so the host writes every input of its
placement to the `World`.

One world object may resolve to several scene objects, such as LOD levels or impostor cards.
`ObjectIdMap::from_initial` takes one world handle per retained scene object in the initial
scene's order and gives a handle that appears several times its ids in that order; `get_all`
returns them. Edits add, replace and remove all of an object's ids together. When the resolver
returns more objects than the object has ids, the extra ones are added under fresh ids; when it
returns fewer, the surplus ids are removed. Ids are never reused.

`MeshExtractOptions::with_millimeters_per_unit` sets the world unit (default 1000.0, meters) for
`edits_from_world_changes_with` and `extract_mesh_instances_with`. It applies to translation
only; scale stays a ratio. Whole world units are exact up to 2^24 because positions are `f32`.

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

Desktop and windowless retained hosts reserve the configured capture-recipe pool in
the initial inventory, reducing the headroom available to renderer targets even
when no capture is requested. This host-wide pool is counted once across renderer
generations. Admitting a recipe reserves storage without replacing targets.
Executing accumulated recipes can replace targets for their raster footprint or
RCAS selection; restoring ordinary presentation uses the same admission policy.
Ordinary desktop and live presentation do not reserve that pool. Attachment
replacement creates targets while previous generations remain alive, so the full
replacement peak must fit.

### Capture pool policy

`ViewerOptions::capture_pool` defaults to `CapturePoolPolicy::Fixed(256 * 1024 * 1024)`.
Choose a positive fixed capacity or `CapturePoolPolicy::Derived { recipes,
max_extent, headroom_bytes, granularity_bytes, cap_bytes }`. Supply a nonempty
recipe set and a nonzero maximum physical surface extent. At host creation, the
largest declared recipe's storage requirement at `max_extent` gets the same
checked headroom, rounding and optional cap as the presentation budget. Recipe
storage includes staging, decoded products and accumulation storage. The initial
frame samples and AO format determine the derived capacity.

The pool stays constant for the host's lifetime, including physical resize,
effect reconfiguration and temporary supersampled raster extents. Replacement
inventories count the same shared CPU descriptor once, while target allocations
count every unretired generation. The pool reserves descriptor bytes without
eagerly allocating its capacity. `RetainedFrameStatistics::capture_pool_bytes`
reports that constant capacity.

A declaration sizes capacity, rather than restricting which requests are legal.
Concurrent requests must fit together. Derived-policy queued requests reserve
the larger of current-extent storage and storage at `max_extent`, so resizing
within that extent cannot overcommit the pool. The recipe's own `max_bytes` is
checked only at the current extent, not at the hypothetical maximum. Execution
updates each reservation to its frozen extent and checks `max_bytes` again;
active and undelivered results keep their charge until delivery. Requests above
the declared extent are accepted only if their storage fits the constant pool.
Growth beyond the declared extent or the recipe's own byte limit after admission
can still fail execution with `ViewerError::Driver`, as with a fixed pool.

Requests exceeding the pool or their own `max_bytes` fail synchronously with
`CaptureRequestError::RecipeBudget { required, limit }`; planning returns the same
error wrapped in `ViewerError::Capture`. Runtime reservation failures remain
`ViewerError::Driver`. A low cap allows host startup but rejects recipes that
exceed it. Zero capacities, maximum-extent dimensions, granularity or caps, and
empty recipe declarations fail option validation. Storage arithmetic overflow
fails with `CaptureRequestError::InvalidRecipe`.

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
A derived plan describes startup sizing with the supplied effects. When planning
replacement effects for an existing host, pass `CapturePoolPolicy::Fixed` with
that host's `capture_pool_bytes` statistic to preserve its lifetime capacity.

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
