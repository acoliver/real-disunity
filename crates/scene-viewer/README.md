# Desktop and windowless scene host

`ViewerOptions` configures the scene host, including `RetainedHost`. Presentation
cadence controls when a frame is requested; surface present mode controls how the
window system presents it.

For measurement runs, call `host.with_window_on_top(true)` after either
`RetainedHost` constructor to keep the desktop window above ordinary windows
with `winit::window::WindowLevel::AlwaysOnTop`. This option is off by default.

## Short sounds

`RetainedHost::audio()` and `RetainedEventHost::audio()` expose the same
`AudioService`. Call `start_device()` explicitly before playback; construction
never opens a device. Load a bounded PCM WAV with `load_wav`, then use `play`
with a clip handle and gain, `stop`, `set_master_gain` and `set_mute`, or typed
`AudioCommand` controls. Natural endings deliver `RetainedEvent::AudioFinished`
through the driver event hook, including while presentation is paused. Stop and
shutdown cancel voices without a natural-ending notification.

`configure_bus`, `play_on_bus` and `replace_ducking_rules` expose the same mixer
bus/ducking controls through both retained host interfaces. Typed commands use
`ConfigureBus`, `PlayOnBus` and `DuckingRules`; existing `Play` and `Stop` commands
remain valid and unrouted playback bypasses buses. Bus and rule changes can be
configured before device startup, but fail after shutdown. Routed admission uses
the same device-status and pending-completion limits as ordinary playback.
Configuration errors, including unknown bus names and unsupported envelope names,
come from the shared CPU mixer. Audio playback commands do not cross the C ABI;
there is no ABI record or revision change. See the
[bus and envelope contract](../world-simulation/README.md#option-buses-and-ducking).

`pause_mask`, `pause(bits, ExistingVoices::{Continue, Suspend})` and `resume(bits)`
forward to the same mixer under its existing lock. These controls and their
`AudioCommand::{PauseMask, Pause, Resume}` forms work before startup and reject
`Closed`; no second reason mask lives in the service. Device cleanup and null-device
setup cancel voices while preserving held reasons. Accepted output-rate changes
also preserve reasons but require empty voice and completion slots.
The read-only query returns `Result<u32, AudioServiceError>`; its typed command
returns `AudioReply::PauseMask`.

`play_with_options` and `AudioCommand::PlayWithOptions` use the same status and
pending-completion capacity checks as ordinary playback. `PlaybackOptions::new(gain)`
is pause-eligible; setting `pause_eligible: false` admits independent UI, music or
other caller-selected sounds on any existing bus. Eligibility is never inferred
from a bus name or the host clock. Suspended voices retain capacity and cursor
and cannot finish until resumed; ineligible voices continue under explicit null
host-time advancement. The mixer defines the [reason and detector semantics](../world-simulation/README.md#pause-reasons-and-voice-eligibility).
Pause and resume affect subsequent mixing only. They cannot withdraw samples
already queued in the physical output ring; no instantaneous speaker silence is
promised.

`status()` distinguishes `Ready`, `Closed` and `Unavailable` reasons: not started,
null device, no device, open failure, unsupported configuration and device loss.
Transitions deliver `AudioStatusChanged`; startup failures also provide
`device_error()`. Physical playback is rejected unless the device is ready.
The default mono/stereo device must provide f32, i16 or u16 output. Device errors
stop the mixer and cancel pending voices. Drivers choose what to do next.

Windowless initialization always selects the null device and refuses physical
device startup. Its status remains `Unavailable(NullDevice)` even while simulated
voices run. `advance_time` renders and discards samples at 48 kHz, carrying
fractional frames between calls. `frame` or `poll` delivers the resulting endings;
wall time alone cannot complete a simulated voice.

A mixer worker fills a preallocated 2048-frame atomic stereo ring in 128-frame
blocks. The cpal data callback only reads samples, converts to the output type
and advances atomic cursors. It allocates nothing, takes no lock and calls no
user hook. Underruns output silence without completing unconsumed audio. Endings
are delivered only after the callback consumes their final block, which indicates
handoff to the OS rather than audible speaker completion. Stop, gain and mute
changes affect subsequent mixing; already queued samples can remain for up to
2048 frames, in addition to device latency. The host polls device events at a
5 ms wakeup interval independently of frame cadence. Shutdown stops the stream,
joins the worker and closes admission before notifying the driver.

The only direct audio dependency is `cpal` 0.15.3, licensed Apache-2.0,
with default features disabled. Linux requires ALSA development libraries
(`libasound2-dev` and `pkg-config` on Debian/Ubuntu). The repository's lavapipe
package list does not install ALSA, and the hub's `rd-gate` image provisioning
is outside this repository. Verify that image's packages before requesting its
gate; changing protected CI configuration requires Andrew.

For an explicit audible check later, run
`cargo run --locked -p worldrunner-scene-viewer --example tone`. It plays one
second of 440 Hz at gain 0.15. Tests never run this example or open a device.
See [the mixer contract](../world-simulation/README.md#short-clip-audio).

## Native window focus

Both `RetainedHost::focus_window()` and `RetainedEventHost::focus_window()`
forward one immediate request to the session's native window, including during
presentation pauses. They return `WindowControlError::NoWindow` before native
window creation and `WindowControlError::Unsupported` in windowless sessions.
Cursor controls retain `CursorError::Unavailable` when there is no native window.
A successful request does not guarantee that the window system grants focus.
Focus changes arrive only as ordinary `RetainedInput::Focused(bool)` events;
the request generates no input. The hosts and error type are also exported by
`worldrunner::scene::viewer`. See the
[window control contract](../../dev-docs/rendering/frame-executor.md#retained-native-window-controls).

## Host timers

Both retained host interfaces provide `timer_at`, `timer_every`, `timer_now` and
`cancel_timer`. Drivers receive `RetainedEvent::Timer { handle }` even during
presentation pauses. The service bounds live timers at `MAX_HOST_TIMERS` (256)
and returns typed `TimerError` admission errors. Repeating timers skip missed
intervals. Windowless sessions freeze the host clock at initialization; use
`WindowlessRetained::advance_time` before `frame` or `poll` to advance timers
and update ticks. Shader time remains independent. See the
[delivery and clock contract](../../dev-docs/rendering/frame-executor.md#retained-host-timers).

## Picking the presented scene

`RetainedHost::pick([x, y], include)` and `RetainedEventHost::pick` query the
last presented frame on the event thread. Coordinates are continuous physical
pixels from the top-left window corner, matching `RetainedInput::PointerMoved`.
Do not multiply them by the display scale. `include: FnMut(ObjectId) -> bool`
can consult the game's selectable-ID set. A hit returns `RetainedPickHit` with
`generation` and a render-plan `PickHit` containing the object ID, distance from
the near-plane ray origin in metres, scene-space point, triangle and barycentrics.
These types are re-exported through `worldrunner::scene::viewer`.

The host freezes the resolved camera, physical viewport, resident resources and
object placements only after a successful presentation submission, before the
`Presented` callback. Accepted edits and renderer readiness cannot change that
snapshot. A move, removal, resource replacement, camera edit or physical resize
becomes pickable only with its presented frame. Misses, excluded objects,
nonfinite/out-of-viewport pixels and queries before the first frame return `None`.
The snapshot remains available during presentation pauses. Submission follows the
host's existing presentation contract and does not assert GPU completion or OS
scanout. Windowless ordinary submissions publish the same snapshot; an accumulated
capture publishes its frozen unjittered view when the capture completes, without
emitting `Presented`. Desktop capture-only work leaves the displayed snapshot alone.

Snapshots share meshes, textures, materials and unchanged object tables through
`Arc`. A published object edit copies the previous table only if a displayed
snapshot still references it. Accepting an edit keeps the existing batch-sized
cost. There is no GPU readback, physics world or spatial acceleration structure.
Picking uses undeformed resident LOD-zero triangles and material/LOD-slot sidedness.
Texture alpha, shader wind, distance-fade dithering, billboard replacement, terrain,
water and UI are outside this geometric query. This is suitable for object selection,
but does not reproduce every shaded pixel. The retained object API has no separate
visibility flag: removing an object removes its pickable geometry at presentation.
See the [CPU query contract](../render-plan/README.md#cpu-object-picking).


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

## Retained screen-space UI

Convert an existing `worldrunner_world_simulation::ui::contribution::UiRenderPlan` with
`render_scene::simulation::UiOverlay::new`, then submit
`RetainedUpdate::Ui(Arc::new(overlay))`. For textured graphics, use
`UiOverlay::with_textures(plan, &textures)` with a map from the graphic's texture
identity to the existing `SceneTexture` resource. Missing image identities are
errors. Text supplies its own linear glyph coverage.

This replaces text and graphics together in one ordered generation, without
preparing or uploading unrelated scene resources. Empty text and graphic lists
clear it. Resource, terrain and settings replacements preserve the accepted UI.
Capture observations include submitted draw counts in `scene.passes.ui`;
fully clipped draws are excluded.

Positions and clip bounds are physical pixels from the lower-left corner. The
shared compositor draws solid, translucent and textured quads after tone mapping
and post filters, using straight-alpha blending without scene depth or lighting.
Text and graphics sort together by `RectLayout::draw_order()`: Canvas layer,
Canvas order, then hierarchy preorder. Later surfaces cover earlier ones, as in
`UiRouter` hit testing. Graphics precede text on the same object.

Every contribution clips to the extraction viewport and its object's explicit
`CanvasRenderer::clip_rect`, measured in root-canvas coordinates. Bounds round
inward to whole-pixel scissors, including for rotated element geometry. Scissors
also intersect the current output extent. Re-extract when the viewport changes;
existing contributions keep their pixel coordinates on resize. Soft clipping and
stencil/pop-material masks are not implemented.

Only overlay text is accepted; camera/world text canvases remain on the scene
path. Graphics already projected to screen pixels are accepted; world graphics
are rejected. Linear HDR and opaque-term capture products exclude this
display-space UI. Built-player retained-driver UI export is not connected yet.

## Retained camera updates

`RetainedUpdate::Camera { pose, lens, convention }` preserves temporal history.
For a teleport, view reset or scene switch, submit
`RetainedUpdate::camera_with_continuity(pose, lens, convention, CameraContinuity::Cut)`.
`CameraContinuity::Continuous` has the same behavior as the ordinary `Camera`
variant and is the continuity default. Both forms validate and publish the camera
in the ordered retained-update queue.

`RetainedUpdate::camera_with_shadow_policy(pose, lens, convention, continuity,
Some(policy))` adds directional-shadow reception to that same transaction.
`policy` is `CameraShadowPolicy::Enabled` or `CameraShadowPolicy::Disabled`.
Disabling suppresses cascade caster passes and receiver sampling of cached shadow
maps. The maps stay cached, and caster changes made while disabled remain pending
for refresh when shadows resume.
`None`, `Camera`, `CameraWithContinuity` and frames without a camera update preserve
the currently published policy. The policy takes effect only after its generation is
published on an admitted retained frame or accumulated-recipe sample, and remains
effective across resident rebuilds. Enabling requires a startup-selected
shadowed resident. Enabling on a Plain scene returns `UnsupportedScene` and
rejects the entire camera transaction.
Disabling on Plain is accepted as a no-resource request and publishes with the
camera generation without installing shadow resources. The viewer reads no world
camera itself: `play` publishes the presented camera's `renderShadows` through this
policy (see `crates/play/README.md`).

A cut discards TAA and temporal-upscaler history, restarts projection jitter at
index zero, and replaces previous camera, jitter and animation time with the new
frame's values for velocity producers. It also resets shadow depth-fitting
evidence, contraction filters and retained cascade maps. Exposure adaptation is
preserved across cuts; the separate `reset_exposure()` API requests an explicit
restart. SSAO and SSR use current-frame products and have no separate temporal history.
The reset happens before encoding the first submitted frame with the cut view.
Multiple published cuts before that submission collapse into one reset. An
abandoned or cancelled frame does not consume the cut; a later continuous
update does not erase it.

`RetainedFrameStatistics::temporal_history_reset` and
`RetainedCaptureObservation::temporal_history_reset` report
`TemporalHistoryReset::CameraCut` for the affected submission, otherwise `None`.
Capture observations freeze this field with the source frame's scene statistics;
accumulated recipes report their first sample after warmup.

## Retained shadow light

Submit `RetainedUpdate::Settings` with effects updated through
`SceneEffects::with_shadow_direction` to move the shadow light without replacing
resident geometry. `None` follows the direct sun. Direct lighting, sky and the
celestial sun are unchanged. Interactive frames and same-frame captures use the
same shadow direction, including when depth fitting is enabled.

## Retained reflection settings

A `RetainedUpdate::Settings` that changes only screen-space reflection parameters,
such as distance, steps or thickness after a resize, reconfigures the frame renderer
and keeps the resident scene. Turning reflections on or off changes the receiver
pipelines, so that update prepares a new resident scene in the background. Changes to
SSAO, water, atmosphere or sky also prepare a new resident scene.

## Batch object updates

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
`edits_from_world_changes_with` and `extract_mesh_instances`. It applies to translation
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

## Game rendering

`ViewerOptions::game` carries a `GameRendering` into every renderer the viewer builds: the `GamePasses` inventory from `worldrunner-render-scene`. The viewer registers the inventoried factories when it allocates the renderer and again after every reconfiguration, so a game's persistent targets survive replacement while their declarations are unchanged. The targets belong to the viewer's renderer: a replacement the factory refuses leaves them as they were. The default adds nothing and frames are the stock frames. A factory that draws the scene's terrain surface or water itself declares it with `GameFeatureFactory::replaces_stock`, and the renderers the viewer builds then skip the stock draws of it. The viewer does not know what the game draws.

```rust,ignore
let mut options = ViewerOptions::default();
options.game = GameRendering {
    passes: GamePasses::new().with(Arc::new(MyGame::new())),
};
```

Tests: `presentation::game_tests` in this crate and `tests/game_passes.rs` in `render-scene`.

## Window dimensions

`size` is the initial request, with each dimension in `1..=4096`.
`window_size_unit: WindowSizeUnit::Logical` preserves logical-pixel creation.
Select `WindowSizeUnit::Physical` to request exact physical backing dimensions,
independent of the display scale. The window system may constrain either request.
Resize events and retained host sizes always use physical pixels. Windowless hosts
have no display scale, so both units use the supplied pixel dimensions directly.

`window_mode` selects the desktop placement. `WindowMode::Windowed` (the default)
opens a decorated window at `size`. `WindowMode::Maximized` maximizes it, and
`WindowMode::BorderlessFullscreen` covers the monitor the window opens on at
desktop resolution. `size` remains the restore size in every mode. The platform
reports the resulting surface through ordinary resize events, so reconfiguration
and the presentation budget follow the actual physical size. Retained drivers
change the mode at run time with `set_window_mode` on `RetainedHost` or
`RetainedEventHost`, for example from an Alt+Enter binding, and read the current
monitor's physical size and scale with `display`. The same `DisplayInfo` reaches
the initial content supplier as `InitialContext::display`, before the first frame,
so an application can choose its render scale from output pixels. Both return
`WindowControlError::NoWindow` before native startup. Windowless hosts reject any
mode other than `Windowed` at startup and return `WindowControlError::Unsupported`
from both calls.

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
Ordinary desktop presentation does not reserve that pool. Attachment
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

## Texture residency tiers

Set `ViewerOptions::texture_tiers` to `Some(TextureTierPolicy { budget_fraction_micro,
max_dropped_levels })` to reduce declared scene textures at upload. `None` preserves
the authored chains. The fraction is in millionths of the selected adapter's
device-local memory. Unknown adapter memory preserves the authored chains.

Only handles in `SceneResources::reducible_textures()` participate. The scene
validates that these handles have no fixed-level or authored-size consumers.
The uploader chooses one global count of top mip levels to drop, using the
smallest count that fits the budget. Each texture retains a base of at least
4x4; cubes retain their authored chains. The remaining authored mip data uploads
directly, without resampling. If the maximum reduction still exceeds the budget,
the maximum is used and the reported choice has `fits: false`.

The choice includes incoming fixed-size scene textures and the device census at
preparation time. During replacement, previous textures remain counted until their
resident generation retires. This is a texture sizing policy, separate from
presentation allocation admission. Allocations created after the choice and driver
overhead are outside its estimate. With a policy enabled, retained texture
replacements and appends containing textures prepare a complete replacement scene,
so the global tier and its byte totals are recalculated together. Other resource
edits keep their in-place upload path.

`RetainedFrameStatistics::texture_tier` reports the resident scene's choice:
`dropped_levels`, `authored_bytes`, `kept_bytes`, and `saved_bytes()`. Byte counts
cover the declared reducible textures. The field is `None` without a policy.
Queued scene preparation does not replace the current resident's choice.

## Selected adapter

Set `adapter_callback: Some(AdapterCallback::new(...))` to observe an
`AdapterIdentity` containing the selected adapter's name, `AdapterClass` and
`backend: wgpu::Backend` (for example `Metal`, `Vulkan`, `Dx12` or `Gl`).
Classes map directly from wgpu's device type: `Integrated`, `Discrete`, `Virtual`,
`Software` (CPU adapters), or `Other`. No classification is inferred from a name.
The backend reports the selected graphics API independently of hardware class.
`memory` is the adapter's `AdapterMemory` where the platform publishes it: on Linux,
DRM drivers with `mem_info_*` files in sysfs (amdgpu) give device-local, host-visible
and GTT totals. It is `None` elsewhere and for software adapters, so a host that
chooses settings from it must handle the unknown case.
`GpuContext::adapter_identity()` exposes the same identity in the scene renderer.

The configured callback or initial content supplier runs once if startup reaches
resolution, after desktop surface compatibility and backend census checks. The
callback runs on the calling thread. Resolution precedes GPU scene preparation.
The viewer creates a device afterward on owned paths and uses an existing device
on shared paths. It does not select a second adapter.
Return `Ok(Some(FrameSettings { frame_profile, presentation_budget, shadows }))`
to choose the initial frame profile (including samples and render scale),
presentation sizing policy and directional shadow settings. These fields have
the same types as in `ViewerOptions`. `Ok(None)` or no callback preserves the
supplied options. A callback error aborts startup as `ViewerError::Driver`.

Use `RetainedHost::with_initial_content(options, supplier)` when content depends on
that adapter or the physical surface size. The one-shot supplier receives
`InitialContext { adapter: &AdapterIdentity, surface_extent: [u32; 2], display:
Option<DisplayInfo>, presentable_backends: Option<&[wgpu::Backend]> }` and returns
`InitialContent { scene, settings: Option<FrameSettings> }`. `display` is `None` for
windowless startup and when the platform cannot identify the monitor.

Desktop startup supplies `Some` for `presentable_backends` even without monitor
information. The list contains distinct backends under the effective instance
backend mask, ordered by the first enumerated adapter observed to support the
same surface for each backend. `None` means unknown, including windowless startup.
The census guarantees neither device creation nor support for scene features.
Before calling the supplier, the desktop census returns
`ViewerError::SelectedBackendDisallowed { backend, enabled }` if the mask excludes
the selected backend, or `ViewerError::SelectedBackendMissing { backend }` if the
backend is allowed but the census observed no adapter on it supporting the surface.

Desktop startup uses the window's physical inner size at surface creation,
regardless of the logical window size or render scale. Windowless startup uses
the configured `options.size`.
A zero dimension is preserved for a minimized desktop surface; rendering stays
suspended until a nonzero resize. Windowless sizes still require nonzero dimensions.
The supplied extent initializes `RetainedHost::size()` without a separate resize
event for that same size. Later different physical sizes still deliver
`RetainedInput::Resized`, including suspension and restoration.
The supplier replaces `adapter_callback`; specifying both fails with
`ViewerError::InvalidOptions`.
The selected scene is validated and its object/decal IDs, readiness, capture
features and capture pool are derived before allocating presentation and resident
resources. Owned paths then negotiate device features; shared paths check the
existing device's features. No temporary scene is uploaded.
Supplier errors are `ViewerError::InitialContent`; validation uses the existing
typed scene and option errors. The supplier is consumed when invoked and is never
retried, even if it or a later startup step fails.

Before startup, a supplier host has no scene: object queries are empty, `view()`
returns `Ok(None)`, and all scene edits return `InitialSceneUnavailable`.
Generation-zero capture requests queue within the configured admission bound;
recipe features, page dependencies and pool reservations are checked against the
selected content and settings before presentation allocation, and before device
creation on owned paths. Input and exit controls remain available. Concrete-scene
construction keeps its synchronous validation and pre-start editing behavior.

Desktop and windowless retained startup apply the decision before presentation
target allocation, scene preparation or residency. They use the existing option,
selected-adapter capability and presentation-budget validation, with the same
`ViewerError::InvalidOptions` and `ViewerError::Configuration` errors. No quality
reduction or budget fallback is performed. Every prequeued retained settings
generation is revalidated against the resolved profile before presentation
allocation, and before device creation on owned paths; incompatible effects fail
as `ViewerError::UnsupportedScene`, even if a later generation removes them.
Derived capture-pool capacity uses the resolved sample
count and then remains fixed for the run. `RetainedHost::options()`
exposes the effective settings from the first driver update.

## Retained frame statistics

`RetainedHost::statistics()` and `RetainedEventHost::statistics()` return the
latest `RetainedFrameStatistics`. It is absent before the first ordinary
submission and replaced before each `Presented` callback. A snapshot identifies
its renderer submission frame and published accepted scene generation. Frame IDs
share the capture/timing sequence; capture-only subframes can leave gaps between
ordinary snapshots. `presentation_budget_bytes` reports the
effective presentation admission budget, including the admitted replacement peak.
`resident_bytes` is the device's GPU memory census at submission, by resource class
(textures, meshes, instances, shadow maps and the rest); classes the engine does not count
yet read zero. `upload_staging` counts the device's reusable upload staging since startup:
chunks created and held, uploads and uploaded bytes. Steady frames reuse chunks, so
`chunks_created` stops growing once the frames in flight have settled. Polling, capture-only work and queued
CPU edits do not replace it. Storage is one snapshot, with no event backlog or
GPU readback. Submission does not imply GPU completion.

`scene.passes.opaque_objects` and `scene.passes.prepass_objects` count the
indexed object draws the main camera's colour pass and depth/normal prepass
recorded, split into `meshes` and `impostors`, with their instance counts and
whether the pass drew gathered (`Compacted`) or per-range (`Ranged`) instances.
Material slots and transition complements each count as a draw; hidden and
culled placements do not. Decals shade receivers inline,
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

Captures request a [forced cascade refresh](src/renderer.rs) when directional
shadows are enabled, including [every rendered recipe sample](src/renderer/capture/advanced.rs).
Their frozen `observation.scene.passes.shadow_cascades` counters describe that
capture submission. Measure map reuse with `statistics()` from successive ordinary
submissions under the configured [refresh policy](../render-shadow/src/refresh.rs);
its default intervals request a refresh every frame. Zero caster draws alone do
not establish that receiver sampling is disabled.

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

`scene.terrain_geometry` is absent before the first owning submission and when
there is neither streaming terrain nor a nonstreaming terrain draw. `Static`
identifies one nonstreaming resident mesh. `Streaming` reports desired leaves,
resident coverage at that detail or finer (`drawn_at_detail`), coarser resident
ancestors (`drawn_coarse`), total residents and unresolved desired leaves.
Those coverage counters precede view culling; `drawn.count` counts submissions
and can be zero while selected and resident leaves remain nonzero. The selector
owns both split-error and hysteretic merge decisions, independently of replacement
admission budgets. All leaves cover
the grid, including offscreen coarse coverage. Finest-stride leaves satisfy the
policy even if their projected error exceeds the threshold.

Replacement builds and uploads synchronously within each frame's budgets. There
is no worker queue. `unresolved_leaves` counts selected leaves not yet built and
uploaded at their selected detail, including splits and merges blocked by resident,
retired-allocation or upload limits. Fine leaves retained inside the hysteresis
band are selected and complete; fine coverage awaiting a selected merge is
incomplete. Submitted uploads ordered before draws are resident for that frame;
this does not assert GPU completion. Retired allocations are excluded.
`drawn_coarse` counts resident ancestors before culling, not covered descendants.
`complete` requires every selected leaf at its selected detail and no unresolved
work. Budget-limited coverage remains incomplete even when it stops changing.

`drawn` contains the count and a stable FNV-1a-128 fingerprint of sorted submitted
sample windows after view-frustum culling, encoded as little-endian u32 minimum
x/y, maximum x/y and stride. These are the engine's rectangular quadtree
identities. The fixed-size fingerprint
avoids truncating large selections and keeps snapshots bounded. Equal fingerprints
and counts support frame-selection comparison, with the usual noncryptographic
hash collision limitation; they do not identify terrain content or a camera.

Both completed capture outcomes retain their source `frame` and an `observation`
with its accepted `generation` and full `scene: SceneStatistics`, including pass
draws (including camera object draws), decals, resource bytes, terrain pages and installed
effects and terrain geometry. Page and geometry readiness remain distinct fields
in `observation.scene`.
The snapshot is frozen at source submission, before resource-upload statistics
are consumed by ordinary presentation, and never reconstructed from the latest
presentation when GPU readback completes. Accumulated recipes
report the first rendered sample after any warmup frames, including that sample's
readiness. Recipe products use owned boxed storage. Cancellation carries no frame
observation. Timing retains its typed disabled, pending, unavailable or complete
status; unavailable durations are never represented as zero.

## Moving-frame CPU measurement

The `moving_frame_cpu` example renders a synthetic outdoor scene (terrain, 3000 props over
41 materials, a shadowed sun with three camera-split cascades and the HDR bloom and FXAA
chain) through a windowless retained host while the camera walks a circle and moves every
frame. It reports the median, 90th percentile, minimum and maximum of the time inside host
calls for each frame, from the driver's camera update to submit, and of the timed thread's
on-CPU time. The wait for the GPU happens between frames, outside both figures. Shadow view
selection runs on short-lived worker threads, whose CPU time is in neither figure. Build it in
release mode, because debug assertions add work that a release build does not do. On Linux
without a GPU, select lavapipe; its GPU time means nothing, and the CPU figures stay valid:

```text
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json WGPU_BACKEND=vulkan \
  cargo run --locked --release -p worldrunner-scene-viewer --example moving_frame_cpu -- \
  --frames 300 --warmup 60 --objects 3000 --in-flight 1 --size 1280x720
```

`--in-flight 1`, the default, admits a frame only
after the previous one has finished, so the software rasterizer does not compete with the timed
thread. The example uses no game assets.

`--clutter N` adds N pieces of clutter to the scene, spread over eight large instance batches.
With `--edits E` the driver also edits E of them every frame, rounded up to a multiple of three,
as a game that streams clutter around the camera does: a third are removed, a third are added
near the camera and a third are recycled to a new spot near it. The edits go through one
`RetainedUpdate::Objects` in the driver's update, so the timed span includes publishing them to
the host's tables, the pick snapshot and the renderer's instance batches:

```text
cargo run --locked --release -p worldrunner-scene-viewer --example moving_frame_cpu -- \
  --frames 300 --warmup 100 --clutter 20000 --edits 300
```

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
multisample resolve uses `Resolve`. Only executed passes appear. Output retains its
authored post stage names.
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

## Scene antialiasing

Retained HDR hosts take their post resolve from `SceneEffects::anti_alias()`.
Set `SceneEffectsParts::anti_alias` to
`SceneAntiAlias::smaa(SceneSmaaPreset::Ultra)` for SMAA 1x with luma edges;
`SceneAntiAlias::Smaa(SceneSmaaSettings { .. })` also supports color edges and
custom quantities. No `FrameProfile` override is needed. Scene construction
validates the integer controls before GPU preparation, and SMAA selects no
projection jitter. The timing sample includes `smaa_edges`, `smaa_weights` and
`smaa_blend` as separate stage identities. See the
[scene resolve contract](../render-scene/README.md#scene-authored-smaa).

## Native-resolution sharpening

Use `FrameProfile::HdrNativeRcas { samples, tone_map, rcas }` for interactive
HDR frames. A retained HDR capture can override it with
`RetainedCaptureRecipe::with_native_rcas(NativeRcasSettings { stops, domain })`;
stops must be finite in `0..=2`. The override applies only to that recipe,
and ordinary frames restore the profile setting. The default is off.
See [the post contract](../render-post/README.md#native-resolution-rcas) for
domain transfer, reciprocal and quantization behavior.

## A device another user shares

`ViewerStart::new` makes the event loop before the window. `ViewerStart::adapter` chooses the adapter once, without a surface and without a device, so a caller can size its requirements to what the adapter offers. `ViewerStart::device(options, requirements)` then makes the one device, for the viewer's needs combined with the caller's, and returns it with its `SubmissionAuthority`; a later call returns the same device. `RetainedHost::run(start, driver)` runs the window on that device and checks that the adapter can present to the window. A start that made no device leaves the viewer to make its own.

`RetainedHost::admit_offscreen(operations)` hands the caller's format-neutral `render_program::offscreen::PendingOffscreen`, built on that device, to the next frame the viewer renders. The frame admits them on its own recording and runs them before its first pass in its one submission, so the caller submits and waits for nothing. An accumulated recipe's first admitted warmup or sample consumes the packet once; later samples do not replay it. A later call replaces operations no frame took. An operation the contract refuses fails the run with `ViewerError::Offscreen`.

`RetainedHost::show_game_screen` offers a single-sample `Rgba8UnormSrgb` texture
on the shared device. Export that same texture as `GAME_SCREEN` from the operations
that write it. An export of another texture under that name does not write the
offered screen: the frame drops the export, runs the operations and leaves the
offered screen as it was; the same holds while no screen is offered. The
viewer samples it in the same frame and draws the UI over it. With no current
writer, the viewer uses the last submitted contents of that allocation. A host
that filled the texture in an earlier submission can declare that when it calls
`GameScreen::new`. An unwritten or withdrawn screen selects the stock scene.

Accumulated capture samples (supersampling, temporal samples, jitter or native
RCAS) offer the screen and carry the queued operations like an ordinary frame.
The first admitted warm-up or sample runs the operations once. Later samples
show the submitted screen contents. When the requested view needs a terrain page
or geometry upload, the screen frame records it in the same submission. A warm-up
records no capture sample. A frame that shows the screen can supply its display
colour and, when configured, linear HDR. Depth, normals, ambient occlusion, SSR
confidence and the specular plane require the stock scene. Albedo, sun visibility,
and diffuse and specular lighting terms also require the stock scene, even when
requested on their own. A capture request containing any of these outputs returns
`ViewerError::UnsupportedScene("capture of frame attachments while the game screen replaces the stock scene")`
before the frame submits or takes the queued operations, while a screen with
contents is offered or the queued operations export the offered screen. The
submission serial, the request and the queued operations stay as they were.

Offscreen work cannot accompany shadow depth fitting. The viewer rejects it
whenever shadow depth fitting is configured, including interactive
`QuantizedPerBound` fitting, and returns
`ViewerError::UnsupportedScene("player offscreen work with same-frame depth fitting")`
before the fitted path renders or submits work. This refusal leaves the submission
serial unchanged. Plain frames and shadowed frames without depth fitting accept
offscreen work through the frame's recording.

The check uses the published generation after ready retained updates have been
applied, respecting capture holds and readiness dependencies. A refusal preserves
the pending offscreen operations. A later ready settings update that disables
fitting lets a compatible frame consume those operations without readmission.
If preparing an update submits GPU work, the call returns pending without taking
the operations; a later call checks compatibility before rendering.

`SharedGpu` is the same instance, adapter and device without a window: `SharedGpu::headless` makes one, and `RetainedHost::windowless(driver, Some(gpu))` runs a windowless session on its device, refusing a scene feature the device lacks as a window does.
