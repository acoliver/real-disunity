# Desktop and windowless scene host

`ViewerOptions` configures the scene host, including `RetainedHost`. Presentation
cadence controls when a frame is requested; surface present mode controls how the
window system presents it.

## Retained run activity

On macOS, retained desktop and windowless runs hold an `NSProcessInfo` activity
with exactly `UserInitiated | LatencyCritical` to prevent App Nap while backgrounded
or occluded. Desktop activity starts before event-loop creation and lasts through
shutdown and renderer cleanup. Windowless activity starts before renderer startup
and stays active between `frame` and `poll` calls until shutdown or drop. Startup
failures, runtime failures and cancellation release the token after cleanup.
Other platforms hold no platform activity. `UserInitiated` disables idle system
sleep. No separate power-assertion API is called; display sleep and occluded-window
presentation policy are unchanged.

Set `ViewerOptions::activity_reason` to a nonempty string using `.into()`, for example
`"Capturing a scene".into()`. `ActivityReason::default()` supplies
`"Retained scene execution"`. Empty reasons return `ViewerError::InvalidOptions`
before activity or renderer startup. The reason is used only by retained runs.

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
## Retained reflection settings

A `RetainedUpdate::Settings` that changes only screen-space reflection parameters,
such as distance, steps or thickness after a resize, reconfigures the frame renderer
and keeps the resident scene. Turning reflections on or off changes the receiver
pipelines, so that update prepares a new resident scene in the background. Changes to
SSAO, water, atmosphere or sky also prepare a new resident scene.


`RetainedUpdate::Objects(Vec<SceneObjectEdit>)` applies many `Add { id, object }`,
`Replace { id, object }` and `Remove(id)` edits as one atomic update that takes one
preparation slot. The host validates every edit first, then applies them to its object
table in place, so the cost grows with the size of the batch, not of the scene. The
scene's object list is rebuilt from the table only when the whole scene is needed: before a
settings, resources or terrain update. Edits apply in order:
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

Set `present_mode` to `PresentMode::Fifo`, `FifoRelaxed`, `Mailbox`, `Immediate`,
`AutoVsync` or `AutoNoVsync`. The examples use FIFO and
`desired_maximum_frame_latency: 2`, the host's default presentation behavior.
Automatic modes delegate concrete mode selection to wgpu and do not need to
appear in the surface's advertised modes. For an explicitly selected concrete
mode, startup returns `ViewerError::UnsupportedPresentMode` before configuring
the surface if it does not advertise that mode. Concrete modes have no fallback.

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
host does not select a second adapter. Return `Ok(Some(FrameSettings {
frame_profile, presentation_budget, shadows }))` to choose the initial frame profile
(including samples and render scale) and presentation sizing policy, plus the initial directional shadow settings. These are
the same fields and types that `ViewerOptions` carries, with no engine preset
names. `Ok(None)` or no callback preserves the supplied options.

Use `RetainedHost::with_initial_content(options, supplier)` when content depends on
that adapter or the physical surface size. The one-shot supplier receives
`InitialContext { adapter: &AdapterIdentity, surface_extent: [u32; 2] }` and returns
`InitialContent { scene, settings: Option<FrameSettings> }`. Desktop startup uses
the window's physical inner size at surface creation, regardless of the logical
window size or render scale. Windowless startup uses the configured `options.size`.
A zero dimension is preserved for a minimized desktop surface; rendering stays
suspended until a nonzero resize. Windowless sizes still require nonzero dimensions.
The supplied extent initializes `RetainedHost::size()` without a separate resize
event for that same size. Later different physical sizes still deliver
`RetainedInput::Resized`, including suspension and restoration.
The supplier replaces `adapter_callback`; specifying both fails with
`ViewerError::InvalidOptions`.
The selected scene is validated and its object/decal IDs, readiness, capture
features and capture pool are derived before negotiating device features or
allocating presentation and resident resources. No temporary scene is uploaded.
Supplier errors are `ViewerError::InitialContent`; validation uses the existing
typed scene and option errors. The supplier is consumed even when startup fails.

Before startup, a supplier host has no scene: object queries are empty, `view()`
returns `Ok(None)`, and all scene edits return `InitialSceneUnavailable`.
Generation-zero capture requests queue within the configured admission bound;
recipe features, page dependencies and pool reservations are checked against the
selected content and settings before device creation. Input and exit controls
remain available. Concrete-scene construction keeps its synchronous validation
and pre-start editing behavior.

Desktop and windowless retained startup apply the decision before presentation
target allocation, scene preparation or residency. They use the existing option,
selected-adapter capability and presentation-budget validation, with the same
`ViewerError::InvalidOptions` and `ViewerError::Configuration` errors. No quality
reduction or budget fallback is performed. Every prequeued retained settings
generation is revalidated against the resolved profile before device creation;
incompatible effects fail as `ViewerError::UnsupportedScene`, even if a later
generation removes them. Derived capture-pool capacity uses the resolved sample
count and then remains fixed for the run. `RetainedHost::options()`
exposes the effective settings from the first driver update. Returning `Err(String)`
aborts startup as `ViewerError::Driver` before device creation.

## Retained frame statistics

`RetainedHost::statistics()` and `RetainedEventHost::statistics()` return the
latest `RetainedFrameStatistics`. It is absent before the first ordinary
submission and replaced before each `Presented` callback. A snapshot identifies
its renderer submission frame and published accepted scene generation. Frame IDs
share the capture/timing sequence; capture-only subframes can leave gaps between
ordinary snapshots. `presentation_budget_bytes` reports the
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

`scene.passes` reports CPU-encoded calls and instances for main terrain,
transparent water, background sky including discs, and four shadow cascades.
Skipped cascades report zero for that frame. Metadata, water masks, dynamic
quads and separate celestial mesh passes are excluded.

`scene.effects.shadows` reports installation of the directional shadow feature
and its mesh and terrain receiver bindings. Cached cascades remain installed
and bound even when `scene.passes.shadow_cascades` reports no redraws.
`scene.effects.ssao` reports installation of screen-space AO resources, whether
AO has nonzero intensity, whether its depth/normal prepass ran, and which
receiver pipelines sample it. Reflection-only neutral AO resources are installed
but disabled. Receiver bindings come from the encoded receiver paths, independent
of visibility or pixel coverage; absent terrain is unbound. The resident
composition adapter reports its own unshadowed bindings. Raw `SceneGpu` draws
publish none of these observations, preserving the previous submitted snapshot.

`scene.terrain_pages` reports requested selected leaves, exact residents used by
the draw table, and pending leaves. Ancestor fallback pages do not count as exact
residency. Unpaged terrain reports zero page counts. The counts remain tied to
the submitted frame even while subsequent page uploads settle.

`scene.terrain_geometry` is absent without a main terrain draw and before the
first owning submission. `Static` identifies one nonstreaming resident mesh.
`Streaming` reports the streaming selector's desired leaves, leaves drawn at
that detail or finer, coarser drawn resident ancestors, resident draw leaves and
unresolved desired leaves. The selector owns both split-error and hysteretic
merge decisions, independently of replacement admission budgets. All leaves cover
the grid, including offscreen coarse coverage. Finest-stride leaves satisfy the
policy even if their projected error exceeds the threshold.

Replacement builds and uploads synchronously within each frame's budgets. There
is no worker queue. `unresolved_leaves` counts selected leaves not yet built and
uploaded at their selected detail, including splits and merges blocked by resident,
retired-allocation or upload limits. Fine leaves retained inside the hysteresis
band are selected and complete; fine coverage awaiting a selected merge is
incomplete. Submitted uploads ordered before draws are resident for that frame;
this does not assert GPU completion. Retired allocations are excluded.
`drawn_coarse` counts actual ancestor draws, not their covered descendants.
`complete` requires every selected leaf at its selected detail and no unresolved
work. Budget-limited coverage remains incomplete even when it stops changing.

`drawn` contains the count and a stable FNV-1a-128 fingerprint of all sorted drawn
sample windows, encoded as little-endian u32 minimum x/y, maximum x/y and stride.
These are the engine's rectangular quadtree identities. The fixed-size fingerprint
avoids truncating large selections and keeps snapshots bounded. Equal fingerprints
and counts support frame-selection comparison, with the usual noncryptographic
hash collision limitation; they do not identify terrain content or a camera.

Both completed capture outcomes retain their source `frame` and an `observation`
with its accepted `generation` and full `scene: SceneStatistics`, including pass
draws, meshes, impostors, decals, resource bytes, terrain pages and installed
effects and terrain geometry. Page and geometry readiness remain distinct fields
in `observation.scene`.
The snapshot is frozen at source submission, before resource-upload statistics
are consumed by ordinary presentation, and never reconstructed from the latest
presentation when GPU readback completes. Accumulated recipes
report the first rendered sample after any warmup frames, including that sample's
readiness. Recipe products use owned boxed storage. Cancellation carries no frame
observation. Timing retains its typed disabled, pending, unavailable or complete
status; unavailable durations are never represented as zero.

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
