# Format-neutral shader program runtime

`worldrunner-render-program` builds layouts and pipelines for shader programs, binds their resources and records draws and dispatches with wgpu. It sits in the Graphics layer. Its root modules describe programs in engine terms and name no shader file format. A default-off `unity` module converts Unity player programs into those descriptions, and the runtime executes the result.

## Neutral model

A caller states each stage's code, the resources and constants the stage declares, and the fixed-function state of a draw. Nothing below mentions a source format.

| Concept | Types | What they say |
| --- | --- | --- |
| Stage module | `module::StageModule`, `StageSource::{SpirV, Wgsl}`, `StageKind` | Code, entry point and stage. SPIR-V keeps naga's Y flip, which turns its clip space into the one wgpu draws with. WGSL is used as written. |
| Declarations | `module::StageDeclarations` | The `ResourceLayout` and the `BoundConstantLayout`s of one stage. |
| Resource layout | `binding::{BindingAddress, ResourceBinding, ResourceKind, BindingSource, PropertyId}` | Set, binding, visibility and kind: uniform or storage buffer (with access and a minimum size), texture (dimension, sample type, multisampled) and sampler (filtering, non-filtering or comparison). |
| Constant layout | `constants::{ConstantLayout, Member, ScalarKind, ArrayLayout, MatrixLayout}`, `pack_constants` | Named members with kind, rows, columns, array count and stride, byte offset and matrix line stride and order. |
| Render state | `state::{RenderState, RasterState, DepthState, ColorState, StencilState}` | Cull, front face, depth test, write and bias, one entry per colour target (blend and write mask) and stencil. |
| Variants | `variant::{FeatureId, FeatureSet, Variants, select_variant, UnknownFeatures}` | A program per exact feature set. |
| Property values | `properties::{PropertySet, PropertyValue}`, `runtime::{Resource, BoundResource, BufferHandle, TextureHandle, TargetHandle}` | Floats, integers, unsigned integers, booleans, vectors, matrices, arrays and resource handles, looked up by name in layers where the first layer with a name wins. |

Execution consumes only these values. `layout::ProgramLayout::build` checks each stage's declarations against the resources its entry point uses, `constants::pack_constants` fills a buffer, `ProgramRuntime::graphics_program` and `compute_program` build the wgpu layouts, and `draw`, `prepare_draw`, `admit_draws` and `dispatch` record the work.

- **Layouts.** A resource the entry point uses must be declared at its address, as the kind the shader has and visible to the stage that uses it. A binding declared twice, a declaration of another kind, a declaration the stage cannot see and stages that declare one binding differently are named `LayoutError`s. A declaration the entry point does not use is listed in `ProgramLayout::unused` and gets no slot. A slot is as large as the larger of the shader's need and the declared minimum size, and its visibility is the declared one.
- **Constants.** Offsets, array strides and matrix line strides come from the layout, so no packing rule of a shader language is built in. A vector writes its leading columns. `PropertyValue::Matrix` holds element `(row, column)` at index `column * 4 + row`, and the layout's storage order places the logical values. An unsigned member takes `Uint` or a non-negative `Int`, and a boolean member takes `Bool` or an `Int` of 0 or 1. A member that is missing from the layers stays zero and is listed in `PackedConstants::missing`. A member outside the buffer, a value of another kind, an array longer than its member and a member shape the packer does not write are named `ConstantError`s.
- **Variants.** `Variants` matches the enabled features, restricted to the ones its programs differ by, against an exact set. `UnknownFeatures::Fail` names an enabled feature that no program differs by, and `Ignore` leaves it out.
- **State.** One colour target is drawn. Stencil, depth bias, alpha to coverage, several colour targets and an unwritten colour target are representable and refused by name (`ProgramError::UnsupportedState`) when a draw is prepared. A state without depth tests nothing and writes no depth.
- **Draws.** `ProgramRuntime::draw` records non-indexed or indexed triangles with a vertex layout the caller gives, into a colour target with an optional depth target. `read_target` records a copy of the pixels. Render and compute pipelines and shader modules are cached by content, and the pipeline key includes the stage sources, the layout, the whole state and the target signature.
- **Compute.** `ProgramRuntime::dispatch` records a kernel; `read_buffer` records a copy of a storage buffer.
- **Samplers.** Every sampler binding gets one fixed sampler: linear filtering, clamped to the edge. A comparison sampler is a named `UnsupportedSampler` error at layout creation.

`tests/native.rs` drives this interface with WGSL stages and no Unity record, and runs without the `unity` feature.

## The native API

A Rust game registers programs and makes materials without building stages, layouts and bindings by hand. The calls are on `ProgramRuntime` and take no Unity type. [Drawing with your own shader programs](../../docs/render-program-native.md) is the guide, with a complete example that `cargo run -p worldrunner-render-program --example native_material` runs and that CI compiles.

| Call | What it does |
| --- | --- |
| `register_program(ProgramDesc) -> ProgramHandle` | Parses and validates each variant's stages (`StageDesc::wgsl` or `StageDesc::spirv`), checks declarations, builds layouts, checks the state is drawable and derives the property schema. A failed registration publishes no handle. |
| `create_material(ProgramHandle, PropertySet) -> MaterialHandle` | A material with initial properties and no feature enabled. |
| `set_property(material, name, PropertyValue)` | Typed values and `Resource` handles for buffers and textures. An unknown property is `UnknownProperty` and a value of another kind is `WrongPropertyKind`, both naming the property. A refused call leaves the material as it was. |
| `set_features(material, &[&str])` | Replaces the enabled features and checks that a variant has exactly that set. An unknown feature is `VariantError::UnknownFeature` and an unmatched set is `NoMatch`. A refused call keeps the old features. |
| `prepare_material_draw(frame, &MaterialDraw) -> MaterialPacket` | Selects the variant, packs the constants into buffers the packet and the frame share, and takes the resources the properties name. The packet is a snapshot: later edits and `release_material` do not change it. |
| `record_material(frame, target, &MaterialPacket)` and `draw_material(frame, target, &MaterialDraw)` | Record into a pass over a render target through the same path as `draw`. |
| `admit_material_draws(frame, &ProgramArena, TargetSignature, &[MaterialPacket]) -> AdmittedPrograms` | The frame-join path of `admit_draws`, with each packet's instance range. |

- **Layers.** A draw's `overrides` come first and the material's properties second. A name found in the first layer wins.
- **Schema.** A program's property names and the kind of each come from its constant members and from the buffer and texture bindings that name a property. Each value is checked by packing it into a probe of its member, so the packer stays the one authority on what a member takes. Two declarations that give one name different kinds make registration fail with `PropertyConflict`.
- **Constants.** Every constant layout of the selected variant is packed with `pack_constants`, from the layers, into a buffer of its own for each prepared draw. Both stages may declare the same layout at one address and must declare it identically. A member with no value is zero and is listed by `MaterialPacket::missing`.
- **Retention and budget.** The buffers are admitted against the runtime's budget like any other allocation, and so under the open arena. They are not in the handle table. The packet and the frame that prepared it share their reservations, and the frame keeps them counted until it is cancelled, dropped or completed, so the retention rule below covers them with no release call.
- **Instances.** `MaterialDraw::instances` is the range of instances drawn. An instance-rate vertex buffer must hold as many strides as the range's end, and a backwards range is `InstanceRange`. The plain `DrawPacket` and `DrawRequest` still draw one instance.
- **Handles.** A handle carries the number of the runtime that made it, so another runtime refuses it as `UnknownHandle`. A material keeps its program alive.

`tests/material/` drives the API without the `unity` feature: a full-target triangle of colour `[0.25, 0.5, 0.75, 1]` with codes predicted as `round(255 * c)` under the shared quantization bound, a feature-selected variant that doubles red, property edits against a packet prepared earlier, an asymmetric scene with depth that pins the WGSL clip convention, instance ranges, texture and buffer properties, SPIR-V stages, registration refusals, and packets in a caller's frame and arena.

## The Unity adapter

`render_program::unity` exists with the `unity` feature, which is off by default and enables the `worldrunner-unity-serialized` dependency. It is the only code in the crate that names that crate. The neutral types mean the same thing with the feature on or off. Everything the adapter converts is data the importer has already prepared. The crate does not read bundles or translate shaders.

| Unity input | Adapter module | Converted to |
| --- | --- | --- |
| Prepared SPIR-V words, entry point and binding table (`ModuleBinding`) | `unity::module::{StageModule, StageProgram}` | A neutral `StageModule` and, when a layout is built, `StageDeclarations` with each parameter placed at its prepared set and binding. A split combined sampler becomes a texture and a sampler binding. |
| `ProgramParameters` | `unity::layout::ProgramLayout` | Resource declarations, each completed from the module's own binding when Unity's class matches it. A class that does not match is Unity's `Mismatch`. Loose vectors and matrices, buffer and constant buffer arrays and unknown texture dimensions are named `UnsupportedParameter` errors. |
| `ConstantBuffer` and the pass's name table | `unity::constants::pack_constant_buffer` | A `ConstantLayout` with 16-byte registers for vectors, array elements and matrix lines, packed by `pack_constants`. A matrix of dimension `n` writes `n` registers from the property's floats in the order given. |
| `Program`, `SubProgram`, `PlayerSubProgram` and keyword indices | `unity::variant` | `select_sub_program` and `select_player_sub_program` choose by exact keyword set through the neutral `select_variant`. A keyword the stage does not declare is ignored. |
| `ShaderState` and property layers | `unity::state::resolve_state` | A resolved target-zero state, which `RenderState::neutral(front_face)` turns into a neutral `RenderState`. State this slice does not draw is a named `StateError`. |
| `DrawRequest`, `DrawPacket`, `DispatchRequest` with Unity's types | `unity::runtime::ProgramRuntime` | A wrapper that dereferences to the neutral runtime and converts stages, programs and states before calling it. Resources, frames, readbacks, arenas and retention are the neutral runtime's own. |

Unity's clip-space and matrix conventions, its target-zero blend behaviour and its policy on unknown keywords are measured compatibility behaviour and stay as they were. The existing suites route through the adapter by import path only.

## The boundary check

`cargo xtask policy` includes a check of this crate's source, in `crates/xtask/src/policy/boundary.rs`. Outside `src/unity/`, no path, import, re-export, `extern crate`, attribute or macro input may name `worldrunner_unity_serialized`, however it is renamed or grouped. No module may name the `unity` module, so a root module can neither expose an adapter type nor re-export the adapter, and `lib.rs` declares the module once behind `#[cfg(feature = "unity")]`. The manifest may not rename the dependency.

The check reads the workspace manifests too. A workspace crate's normal or build dependency on `worldrunner-render-program` may enable the `unity` feature only if the crate is `worldrunner-play` or `worldrunner-unity-import`. Enabling it means listing `unity` in `features`, listing a feature that implies `unity`, or leaving `default-features` on while `unity` is in this crate's default set. The rule reads the manifest as `cargo metadata` resolves it, so it holds for workspace-inherited dependencies, renamed ones and target-specific tables, which count as normal dependencies. A dev-dependency may enable `unity` in any crate: cargo applies its features to that crate's own tests and examples only, which is how `render-scene` runs its program-frame tests. The rule keeps that dev-only use from reaching a production build through feature unification. `crates/xtask/src/policy/boundary/tests.rs` holds the negative fixtures for each form.

`cargo check -p worldrunner-render-program --no-default-features` builds the neutral runtime, and `cargo tree -p worldrunner-render-program --no-default-features -e normal,build` lists no `worldrunner-unity-serialized`.

## Driving it through the submission authority

The runtime shares the caller's `wgpu::Device` and `worldrunner_render_gpu::submission::SubmissionAuthority`. It has no queue of its own and never submits. `draw`, `dispatch`, `clear_target`, `write_buffer`, `read_buffer` and `read_target` record into an encoder, and each refuses an encoder that did not come from the authority's current frame (`ProgramError::Transaction`). The caller decides what a frame holds and submits it once:

```rust
let mut runtime = ProgramRuntime::new(device.clone(), authority.clone(), AllocationBudget(256 << 20));
let mut frame = authority.begin(&device)?;      // SubmitError::Busy or DeviceLost, as everywhere else
runtime.clear_target(&mut frame, target, [0.0; 4], 1.0)?;
runtime.draw(&mut frame, &request)?;
let pending = runtime.read_target(&mut frame, target)?;
let submission = frame.finish().submit()?;       // the one submission
let image = pending.resolve(submission)?;        // waits for that submission, bounded
```

Several runtimes and other renderer crates can record into the same frame; `tests/program/frames.rs` records a dispatch and a clear from two runtimes into one frame and checks that `submitted_serial` rises by exactly one.

- **Construction.** `ProgramRuntime::new(device, authority, budget)`. The authority's queue is used only to upload initial texture contents through `render-gpu`'s `TextureUpload`, the use `SubmissionAuthority::queue` documents. Buffers are created with `render-gpu`'s `upload_buffer`.
- **Per-draw constants.** Create a constant buffer with `create_buffer`, or record `write_buffer`, which stages the bytes in a copy-source buffer and copies them into the destination inside the frame, so the write has a place in the frame's order.
- **Readback.** `read_buffer` and `read_target` record the copy into a staging buffer and return a `PendingBufferRead` or `PendingTargetRead`. `resolve` takes the `SubmissionIndex` that `submit` returned, waits for that submission with a 30 s bound, maps the staging buffer and returns the bytes.
- **Budget.** Buffers, textures, render targets and their depth attachments, readback staging buffers and the staging of a recorded write are preflighted with `AllocationSet::validate_budget` on top of everything live. Over the budget is `ProgramError::OverBudget`, and nothing is created. Pipelines, shader modules, bind groups and the sampler are not counted, because an `AllocationSet` describes buffer, texture and readback storage only. `live_bytes` reports the total.
- **Release.** `release_buffer`, `release_texture` and `release_target` fail with `ProgramError::Busy` while a frame is being recorded. They keep the bytes in the budget until the work submitted so far has completed, as `render-gpu`'s `Configuration::replace` does for retired generations; the next allocation, or `collect`, frees them. A pending readback holds its staging bytes until it is resolved or dropped and the frame that recorded the copy has completed or been cancelled.
- **Errors from the authority.** `SubmitError` values keep their names: `ProgramError::DeviceLost`, `Busy`, `Lease` and `Transaction`. Every creation and recording call checks for device loss first.

## Joining a renderer frame

A draw can also be recorded into a render pass that another crate opened. `worldrunner-render-scene` uses this to draw player programs inside its stock opaque pass, after its own opaque geometry. Both share the pass's color target, depth buffer and the frame's one submission. This crate prepares the draws and owns their lifetime. `render-scene` owns the pass.

```rust
// Once: the renderer reserves the arena, so its total counts the bytes.
let reservation = renderer.reserve(&context, ProgramRuntime::arena_allocations(ARENA))?;
let mut runtime = ProgramRuntime::new(device.clone(), authority.clone(), AllocationBudget(ARENA));
let mut arena = runtime.open_arena(reservation)?;

// Each frame: admit the draws, hand them to the scene, encode, submit once.
let mut frame = authority.begin(&device)?;
let admitted = runtime.admit_draws(&frame, &arena, signature, &packets)?;
scene_gpu.admit_programs(admitted);
renderer.encoding().encode_scene(&device, &mut frame, (&mut scene_gpu, &pipelines), attachments, uniform)?;
frame.finish().submit()?;
```

- **Preparation and recording.** `prepare_draw` takes a `DrawPacket`, which is a `DrawRequest` without its target, and a `TargetSignature`. It builds the pipeline for the signature's formats and sample count, the bind groups and the buffer references, and returns a `PreparedProgramDraw`. `PreparedProgramDraw::record` only sets state and draws, so it cannot fail inside a pass. `draw` is `prepare_draw` followed by `record` into a pass over the target.
- **Target signature.** A `TargetSignature` is one color format, an optional depth format and a sample count. `TargetSignature::new` refuses what the runtime does not draw into, by name: more than one sample (`UnsupportedTarget::Multisampled`), other than one color attachment (`ColorAttachments`), a depth format with a stencil aspect (`Stencil`) and a depth attachment without depth (`NotDepth`). The pipeline cache key includes the signature.
- **Arena.** `ProgramRuntime::arena_allocations(bytes)` is the storage the renderer reserves through its configuration, so the renderer's budget refuses an arena that does not fit. `open_arena` refuses a reservation smaller than the runtime's budget (`ArenaTooSmall`), so everything the runtime admits lies within storage the renderer counted once. It also refuses a runtime that already holds allocations made before any arena (`ArenaAfterAllocations`), because the reservation could not cover them and would be given back while they are live; open the first arena before allocating. A `ProgramArena` belongs to the runtime that opened it (`ForeignArena`).
- **Admission.** `admit_draws` takes the frame's encoder, the arena, the signature and the packets. It prepares every packet and then makes the frame retain the arena through `Publications::retain_lease`, which checks the frame's authority (`LeaseError::ForeignAuthority`) and that the arena is still live (`LeaseError::Expired`). It returns an `AdmittedPrograms`, the only way to obtain one, or nothing at all.
- **Lifetime.** The frame keeps the arena's `FrameOwner` leased until the GPU has completed the submission. `ProgramArena::retire` fails with `Busy` while an unsubmitted or in-flight frame holds it and with `Expired` afterwards. Cancelling or dropping the prepared frame releases the lease and submits nothing. The prepared draws hold clones of the wgpu objects they use, which wgpu keeps alive while a recorded command needs them. The runtime refuses `release_buffer`, `release_texture` and `release_target` with `Busy` while a frame is being recorded or is prepared and unsubmitted, so no submission that could use the resource is still to come. It keeps a released resource's bytes in its budget until the newest submission made at the release has completed. Nothing unsubmitted can use the resource at that moment and the release excludes new frames while it records the serial, so that serial is at or after the last submission that used the resource. **One retention rule.** The reservations of the resources a call uses are held by a `FrameOwner` that the frame retains when the call records into it, and every recording entry point follows that rule through the same mechanism. `draw` holds the buffers and textures it binds, its vertex and index buffers and the target it draws into. `clear_target` holds the target. `dispatch` holds the buffers and textures it binds. `read_buffer` holds the buffer and the readback staging buffer. `read_target` holds the target and the staging buffer. `write_buffer` holds the buffer and the staging buffer of the write. Prepared draws follow the rule when a consumer retains them on the frame (`AdmittedPrograms::retain`, `PreparedProgramDraw::retain`). The holds matter when the runtime is dropped with a frame still unsubmitted: the frame keeps the bytes counted, in the renderer's arena reservation, until its outcome, so work recorded into a frame cannot outlive the accounting of what it touches. A frame that is cancelled or dropped gives the bytes back at once and a submitted one when the GPU has completed it. A pending readback and a dropped readback hold the same: the staging bytes stay counted until the frame completes, not until the caller resolves or drops the read. Dropping the draw detaches the owner (`FrameOwner::detach`): a frame that retained it can still be submitted and keeps the bytes counted until it is cancelled, dropped or completed on the GPU, whatever happened to the draw and the runtime. A frame that never retained the draw holds nothing, so cancelling a frame returns the bytes at once. A consumer that records a prepared draw must retain it on that frame.
- **Checks at the consumer.** `AdmittedPrograms::check_target` compares the draws' signature with the pass's (`TargetMismatch`), and `retain` repeats the lease check for the frame that finally encodes them.

The only consumer so far is `render-scene`'s opaque pass, see [its README](../render-scene/README.md#player-program-draws). The first slice covers one single-sample color attachment with `Depth32Float` and nothing else.

## Measured conventions

The tests in `tests/program/conventions.rs` draw into an 8x8 target through the adapter and compare bytes. They pin what a Unity Vulkan module needs, which holds for any SPIR-V stage:

- The module's position output is used unflipped. Positive clip Y lands in the bottom rows of the image.
- The depth range that survives clipping is 0 to w, as in Vulkan; the nearer of two triangles wins under `LessEqual`, and z of -0.5 or 1.5 is clipped.
- `wgpu::FrontFace::Cw` marks a triangle that is clockwise in the image, with row 0 at the top, as a front face. Cull Back drops the others and Cull Front drops the front faces. The winding is `RasterState::front_face` in the neutral state and a field of the adapter's draw request, so a caller states it.

`tests/program/pass.rs` and `drawn_state.rs` draw a textured pass with a tint from a constant buffer, the multiply and add keyword variants, a split combined sampler, blend off against `SrcAlpha`/`OneMinusSrcAlpha`, and cull Back against Off for a back-facing quad. Pixels are predicted in closed form from the texels, tint and offset, independent of the shader, and compared with the 8-bit bound from `worldrunner_render_gpu::tolerance_test_support` (0.625 steps). The alpha-blend test adds 0.5 step because lavapipe's fixed-point blend measured 0.8 steps from the prediction; the Radeon stayed within the shared bound.

## Not done

- **Engine values.** `BindingSource` names a property or nothing, and there is no value provider. A native game sets the matrices, times and environment resources its shaders read as ordinary properties. Binding object, camera and light matrices, time and shadow or environment resources by semantic is not done.
- **Releasing programs.** A material can be released; a registered program lives as long as the runtime or a material that uses it.
- **Per-variant state.** A program has one render state for all its variants.
- **Struct arrays and specialization.** A constant layout holds scalar, vector and matrix members. Struct arrays and WGSL override constants are not represented.
- **Unity sampler state.** Every sampler binding gets one fixed sampler: linear filtering, clamped to the edge. The packed sampler state in the serialized parameters is not decoded, so wrap, filter and anisotropy settings are ignored.
- **Multisampling, stencil and multiple render targets.** Targets are single-sample with one colour attachment and an optional depth attachment. Stencil state and a stencil depth format are named errors, and so are multisampled and multiple-attachment signatures.
- **Player-variant parameter blobs.** `Program::parameter_blob_indices` are not decoded, so the adapter takes parameters from the sub programs' own `ProgramParameters`.
- **Frame planning and composition beyond one pass.** The runtime still owns no pass order and no attachments. `render-scene` takes admitted draws into its stock opaque pass for one single-sample colour attachment and `Depth32Float`. It refuses scenes with temporal reconstruction, screen-space passes or multisampling by name. Camera render textures, sampling a draw's target in a later pass, compute between passes, transparent ordering, the depth-fitted capture continuation and `render-plan` descriptions of program work are not done. The standalone target that `create_render_target` makes remains the caller's to create and release.
- **Shader preparation.** `shader_prep` lives in `unity-import`, which is in the Tool layer, so a Graphics-layer runtime cannot call it. Callers supply the prepared words and binding table.

## Tests

The suites that build Unity records (`program` and `variant`) run only with the `unity` feature:

```sh
cargo test -p worldrunner-render-program --features unity -- --test-threads=1
```

Without the feature, `cargo test -p worldrunner-render-program` runs the unit tests, `tests/native.rs` and `tests/material/`, builds `examples/native_material.rs` and runs the guide's code blocks as doctests. `cargo xtask test` enables every feature.

`tests/program/packet.rs` covers preparation and recording into a caller's pass, and `arena.rs` covers admission, authority, lifetime and budget refusals. The frame tests in `render-scene` enable the feature as a dev-dependency and run with `cargo test -p worldrunner-render-scene --lib -- program_frame --test-threads=1`.

The draw and dispatch tests need a Vulkan adapter and use the one wgpu selects. They obtain the device through `render-gpu` (`headless_instance`, `request_device`), build a `SubmissionAuthority` and submit every frame through it. The first test to create a runtime prints the adapter's name to stderr; run with `--nocapture` to see it. Set `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json` to run them on lavapipe.
