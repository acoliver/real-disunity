# Desktop and windowless scene host

`ViewerOptions` configures the scene host, including `RetainedHost`. Presentation
cadence controls when a frame is requested; surface present mode controls how the
window system presents it.

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

## Selected adapter

Set `adapter_callback: Some(AdapterCallback::new(...))` to observe an
`AdapterIdentity` containing the selected adapter's name and `AdapterClass`.
Classes map directly from wgpu's device type: `Integrated`, `Discrete`, `Virtual`,
`Software` (CPU adapters), or `Other`. No classification is inferred from a name.
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
its frame and published scene generation. Polling, capture-only work and queued
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
counts and retained GPU pass timing are not yet exposed. These omissions are
not represented by zero-valued category fields. The host still requests no GPU
timestamp capability and allocates no timing queries by default.
