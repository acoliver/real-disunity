# Canonical rendering services

The engine's reusable GPU mechanisms live in four independent service crates. Scene rendering (`render-scene`) and window hosting (`scene-viewer`) are built on these services and do not duplicate them. Application code using the `worldrunner` scene/viewer facade does not need direct GPU dependencies or copies of the SDK shaders.

## Ownership and entry points

| Service | SDK-owned mechanisms | Caller-owned decisions |
| --- | --- | --- |
| `worldrunner-render-gpu` | Explicit device negotiation; validated texture upload and packed mip slicing; device-local retained texture storage; explicit material bindings; color attachments; surface configuration/acquisition and capture unpacking | Required capabilities, content decoding, binding ABI, cache identity, window/event lifecycle, surface recreation |
| `worldrunner-render-lighting` | Validated environment preparation; explicit light direction/frame conversion; hemisphere/polynomial ambient evaluation; shared reflectance kernels and WGSL | Radiometric convention, coefficients, material model, roughness guards, visibility algorithm, exposure ownership |
| `worldrunner-render-shadow` | Budgeted depth maps/readback; depth pipeline and receiver layout construction; far-depth clear/store; validated directional configuration, fitting and shared sampling kernels | Coverage and cascade policy, shader inputs/vertex ABI, scene transforms, bias/filter values, map resolution and memory budget |
| `worldrunner-render-post` | Retained linear stage scheduling, resolved/MSAA frame targets, explicit retained resources, copy-only output, ordered texture/buffer readback | Effect shaders/order, buffer ABI and initialization, metering/exposure policy, tone mapping, diagnostics interpretation, command submission and capture publication |

The service crates do not own application assets, scene documents, filesystem publication, or a global render graph. Important dependencies are passed explicitly as devices, queues, descriptors, resources and caller-authored factories.

## GPU/device and resource contracts

`device::DeviceRequirements` separates required features/limits from optional features and an optional all-or-nothing limits profile. `negotiate` and `request_device` return typed failures for unmet requirements; required capabilities are never silently lowered. Unsupported optional choices are observable in `NegotiatedDevice`.

`device::headless_instance` selects native PRIMARY backends by default and honors diagnostic environment overrides. Applications must handle adapter absence as an error when rendering is required; the service does not choose a platform-specific API unconditionally.

`texture::TextureUpload` validates borrowed layers and mip layouts before GPU allocation/upload. `texture::packed_mips` provides the shared packed-layout interpretation, including block-compressed edge mips. `CanonicalTextureStore` is explicitly device-context scoped: a retained object is not transferable to an unrelated device merely because its content identity matches.

`resource::PreparedMaterial` consumes explicit `MaterialDescriptor` / `MaterialBindings`; binding indices and caller layouts remain authoritative. No implicit binding renumbering is introduced.

`surface::Presenter`, `SurfacePolicy` and `AcquiredFrame` separate presentation mechanics from host recovery. Zero-area suspension, configure/recreate actions and validation errors remain distinct. Native surface extent and bounded scene-attachment extent are different contracts; the host must not silently substitute one for the other.

## Viewer frame pacing

Admission and completion progress never block the event thread. Backend drawable acquisition can wait for a presentable image through Metal `nextDrawable`, Vulkan `acquire_next_image`, or the DX12 frame-latency wait. Present mode and frames-in-flight settings bound that wait according to the backend's presentation policy.

## Lighting, shadows and exposure

`EnvironmentConfig::prepare` validates `LightingFrame`, direct and optional independent shadow directions, radiance, ambient inputs and `ExposureOwnership`. The coefficient transform must be orthonormal; reflections are permitted, scale and shear are not. Direction semantics are explicit rather than inferred. Polynomial coefficients retain the documented `[1,x,y,z,xy,xz,yz,x²-y²,z²]` order and the caller's normalization/convolution convention.

`ExposureOwnership::SceneLinear` supplies a scene multiplier. `PostProcess` leaves scene exposure at one and retains authored stops as provenance; callers must avoid applying both exposure paths. Shared CPU kernels and `lighting_wgsl!` expose the same mathematical conventions without selecting an application-specific look.

Shadow clients configure coverage, fitting and sampling explicitly using `DirectionalShadowConfig`, `DirectionalFit`, receiver descriptors and the shared `shadow_wgsl!` kernels. `DepthMapBudget` distinguishes allocation limits from readback limits. `begin_depth_pass` clears to far depth and stores even when there are no draws, preventing stale occluders. A disabled diagnostic readback is not a reason to reject otherwise valid depth-map rendering.

## Post/output and ordered capture

`PostChain<F: StageFactory>` is a retained linear schedule, not a render graph. `FrameConfig` specifies extent, color format and sample count; `FrameTargets::scene()` and `resolve()` belong to the scene pass, and subsequent stages consume `resolved()`.

`StageFactory::build` runs at construction/reconfiguration/resize, not once per frame. Explicit fixed resources survive target resizing. Failed reconfiguration leaves the previous accepted schedule intact. `StageSet` describes enabled operations and one final enabled output; optional effects need a caller-defined valid bypass. The chain encodes but does not submit or acquire surfaces.

`CopyOutput` performs a no-effects texel copy with the destination attachment's transfer/channel behavior. It does not add tone mapping or clamp floating-point values into an implicit display range. Color format and multisample resolve support are validated independently; sample-count support alone does not prove a format can resolve.

For in-frame captures, use `Readback::encode` at the intended point in the frame encoder, submit, then `read_encoded`. The configured format/extent and byte ceiling must describe the actual source. Native HDR/float bytes, including special floating-point bit patterns, are not silently converted. `BufferReadback` similarly captures opaque, aligned caller-defined bytes. Hosts retain decoding, channel packing and publication responsibilities. See [the post-service API guide](../../crates/render-post/README.md) for retained resources, copy-only presentation and capture details.

## Actual SDK consumers

- `render-scene/src/device.rs` delegates device acquisition to `render-gpu`; texture layout/upload and prepared-material binding paths use the same service.
- `render-scene/src/frame.rs` prepares shared environment configuration. Composed scene shaders include canonical lighting and post WGSL. Shadow resources, fitting and sampling use `render-shadow`.
- `render-scene/src/offscreen.rs` uses canonical post readback and row-unpadding mechanics.
- `scene-viewer/src/renderer.rs` uses the canonical surface presenter. `scene-viewer/src/presentation.rs` owns a `PostChain<CopyOutput>` instead of a separate presentation shader.

Scene-specific coordinate conversions and shader bindings remain adapters at these boundaries. They are not a second low-level rendering service. The scene/viewer facade remains the application-facing integration boundary.

These are 0.1 APIs with no stability promise to external callers. Scene `upload_*` helpers return `Result`, and prepared materials are built from explicit descriptors. Applications should integrate through the `worldrunner` scene/viewer facade.

## Verification and CI coverage

`cargo xtask ci-cpu` validates CPU layers and the facade's no-default-feature graph. `cargo xtask ci-gpu` selects every registered non-managed workspace package, including all four services, the scene renderer, and viewer. It runs policy, fail-closed source-size, format, warning-denying Clippy, five explicit complexity lints, check/build, tests including ignored tests, and doctests. Missing adapters are failures. The GPU-required profile is not a substitute for the separate managed-runtime CI profile.

Keep new packages registered in `crates/xtask/src/layers.rs`; unknown members are rejected rather than silently omitted. Validate an application's actual default and viewer dependency graphs as well as its source boundary: a declared path or revision alone does not prove which implementation is reachable.
