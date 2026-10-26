# Retained frame executor

Every frame the stock renderer draws goes through one executor and one submission. This page describes how a host encodes frames, overlaps them, registers its own features and reads GPU pass timings.

## Layouts and documents

The packed vertex stride is 40 bytes and the instance stride is 80 bytes. Use `geometry::VERTEX_STRIDE`, `geometry::INSTANCE_STRIDE` and the engine packing helpers rather than hand-written layouts. The material uniform is 112 bytes. Scene coordinates are Z-up metres on the GPU, converted from integer millimetres at the CPU boundary.

The render-contract loader in `render-plan` accepts only `schema: "real-disunity.render-contract.v4"` with `schema_version: 4`. Any other version is rejected, and there is no conversion path. Optional effect recipes are validated even when their weights are zero. The field ranges are documented on the recipe types in `crates/render-plan`.

Shadow `extent_millimeters` values are light-space coverage-box extents. Camera far-split cascades are a separate policy, constructed with `SceneShadowCascades::camera_splits(distances_millimeters, blend_band_micro)`. Distances must increase, exceed the camera near plane and not exceed its far plane.

## One executor and one submission

`render-post::PostChain` owns geometry pass opening and post-stage execution. `render-scene::compositor::FrameRenderer` installs the stock scene, shadow, snapshot and effect features on that executor. The convenience APIs `OffscreenTarget::render`, `capture_scene` and `capture_scene_shadowed` use the same executor; there is no separate offscreen drawing loop. `CopyOutput` is the stage factory for the no-look output profile. A procedural byte-comparison regression compares direct drawing, convenience capture and encoder-only composition with copy output.

For a custom host, `compositor::encode_resident` appends a previously prepared resident draw without submitting. It preserves the legacy exposed sRGB path and assumes the host has already set frame uniforms and flushed edits. Use `compositor::encode_scene` for staged frame uploads and camera-aware effects, or `ShadowedScene::encode_with` for shadow composition. Finish with `FrameRenderer::encode_output` and append capture copies to that same encoder. Submit once, then present an acquired surface image immediately. Explicit capture reads wait only for the supplied submission.

For overlapping frames reserve a `FrameAdmission` slot, select it on the scene and renderer, encode, and submit in preparation order through the same admission authority. Reuse a slot only after completion. Successful feature uploads are committed, so successful encoders must be submitted rather than abandoned. Resize builds replacement resources before publishing them; zero-size surfaces suspend acquisition instead of allocating empty targets.

For example, this legacy-profile host fragment encodes two admitted frames without waiting for either submission. `context`, `scene`, `pipelines`, `renderer`, `target` and the two `FrameUniform` values are initialized through their public constructors before this fragment. This legacy path writes directly to the supplied target attachments, so no separate post-output copy is needed. The target uses independent per-slot readbacks. All queue work for this host must use the same authority; standalone capture conveniences must not submit concurrently outside it.

```rust
use worldrunner::scene::gpu::{compositor, wgpu};
use worldrunner_render_gpu::flight::FrameAdmission;

let flight = FrameAdmission::new(2)?;
let mut tickets = Vec::new();
for frame in frames {
    let admission = flight.try_acquire()?;
    let slot = admission.slot();
    scene.select_frame_slot(slot);
    renderer.select_frame_slot(slot);
    let mut encoder = context.device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compositor::encode_scene(
        &mut renderer, context.device(), &mut encoder,
        (&mut scene, &pipelines),
        (&target.color_attachment(), &target.depth_attachment()), frame,
    )?;
    target.encode_readback_slot(slot, &mut encoder)?;
    tickets.push((slot, flight.submit(
        admission, context.queue(), [encoder.finish()], || {},
    )));
    context.device().poll(wgpu::PollType::Poll)?;
}
for (slot, ticket) in tickets {
    let image = target.read_encoded_slot(&context, slot, ticket.submission())?;
    consume(image);
}
flight.retire();
```

`FrameAdmission` is exported by `worldrunner-render-gpu::flight` (add that direct public dependency for this host API). In an interactive loop, handle `AdmissionError::WouldBlock` by returning to event handling/polling, rather than waiting for the whole queue. The capture reads above are explicit submission-scoped waits for a capture host. For HDR/effect capture, use `FrameRenderer::hdr` with camera-aware `encode_scene` or `ShadowedScene::encode_with`, and keep exposure at the HDR profile's required value. `encode_resident` and the convenience captures preserve the legacy exposed path and do not opt into those effects.

The legacy profile retains exposed sRGB output. The HDR profile writes unexposed `Rgba16Float` radiance and applies exposure and tone mapping once in post before the final output transfer. Selecting HDR therefore changes colour behavior explicitly. It supports 1x or 4x forward samples; the optional screen-space metadata prepass is single-sample. Ordinary cross-feature transparent sorting and temporal AA remain unsupported and return typed errors.

## External features

Consumers needing texture generations, subresource access declarations, MRT or a different authored pass order can use the [shared resource and raster contracts](resource-contracts.md). Stock geometry uses the same raster executor and retains its existing outputs and timing identities.

Run the headless public example:

```sh
cargo run -p worldrunner --features scene --example frame-feature
```

`crates/worldrunner/examples/frame-feature.rs` depends only on public facade APIs. It registers a custom `FrameFeature`, draws into `opaque`, declares immutable opaque colour/depth snapshot reads, and samples the colour snapshot in a later pass. Its integration test runs the same example headlessly and checks output and snapshot bytes. The facade exposes the feature contracts through `worldrunner::scene::gpu::executor`.

Declare every pass and sampled resource before preparation. Build pipelines through `BuildContext`; use `PrepareContext::resource` for checked snapshot views. Stage uploads during `prepare` and publish resident state only in `commit`. The executor opens passes and owns attachments. Features receive no queue and cannot submit or change attachments. Snapshot consumers require the HDR profile. External features must provide matching metadata passes to participate as SSR receivers.

## Asynchronous pass timings

Timing defaults to disabled. Before device creation, call `GpuContext::from_adapter_with_timing` with `TimingMode::Optional` or `Required`. Generic device hosts can instead put `TIMESTAMP_QUERY` in `DeviceRequirements::optional_features` or `required_features`. Required unsupported capability fails device validation. Optional unavailable capability remains usable but `configure_timing` reports `TimingSupport::Unsupported`; it never fabricates zero durations.

For explicit negotiation, request timing as optional when rendering must work without it, or required when the application cannot proceed without measurements:

```rust
use worldrunner_render_gpu::device::{DeviceRequirements, request_device};
use worldrunner::scene::gpu::wgpu;

let optional = DeviceRequirements {
    optional_features: wgpu::Features::TIMESTAMP_QUERY,
    ..Default::default()
};
let (device, queue, negotiated) = request_device(&adapter, &optional).await?;
println!("missing optional features: {:?}", negotiated.unavailable_optional);
// Alternatively, for a separate device request with a hard requirement:
let required = DeviceRequirements {
    required_features: wgpu::Features::TIMESTAMP_QUERY,
    ..Default::default()
};
// request_device(&adapter, &required).await? fails before device creation
// when the adapter lacks the required capability.
```

Configure the retained renderer with the device, queue and timing mode. Call `begin_timing(frame_id)` before geometry. `encode_output` automatically resolves timing after its final output stage; geometry-only hosts call `finish_timing` explicitly. Direct `PostChain` users bracket geometry, `encode_resolve` and post encoding with `begin_timing`/`finish_timing`. If encoding returns an error after beginning timing, call `finish_timing` on that attempted encoder before another begin, then discard the failed encoder and reconfigure timing to replace its unread ring before another begin. Otherwise its unsubmitted readback callback can disconnect. Never submit a failed encoding attempt. Successfully encoded frames must still be submitted in preparation order. The host submits the encoder and drives normal nonblocking device polling. `take_timings` never polls or waits and returns only completed frames in encoding order.

Every executor render and compute pass has a pair of pass-boundary timestamp writes, including each shadow cascade, snapshot, screen effect, MSAA resolve and post stage. These need `TIMESTAMP_QUERY`, not `TIMESTAMP_QUERY_INSIDE_PASSES`; no inside-pass writes are used. Each result carries a frame ID, a typed geometry slot or provider-declared post-stage ID, and `duration_ns` as an `Option<f64>`. A reversed query pair has `None`, retains its pass identity, and increments `unavailable_timing_passes`. Durations are timestamp differences multiplied by the queue timestamp period. Stage IDs contain a semantic name and logical level, independent of execution position. Providers must declare distinct IDs for timed stages and retain them across resize or reconfiguration. Bloom downsample levels start at 1; upsample levels identify their destination, starting at 0. Copies and buffer clears are not render or compute passes and do not produce pass timings.

The ring retains at most three unread frames, each with at most 1024 pass pairs. Each slot allocates two 16,384-byte buffers and one 2048-entry query set. When unread or pending slots fill the ring, new timing admissions are declined and `dropped_timing_frames` increases; rendering still encodes and submits. A frame exceeding the pass bound is dropped whole with the same counter. Reversed query pairs are counted separately as unavailable passes and never produce a numeric duration. The portable API does not expose the hardware counter width, so reversed pairs cannot be unwrapped reliably. Valid pairs require end >= begin and a duration shorter than one counter cycle; the latter is a host/device operating constraint, not something inferred from the two values. Readbacks use encoder-associated map-on-submit callbacks, so callers need no separate submission hook or wait. Drain results regularly. Reconfiguring timing replaces its ring and discards any old unread results; do so only between frames.

Native recovery tests exercise resize, lost/outdated transitions and zero-size resume. Linux/X11 execution belongs to the software-Vulkan CI lane. Timestamp-query tests support both available and unavailable adapters, since software drivers and some Windows devices lack the capability.
