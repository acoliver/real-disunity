# Running Unity player shader programs

`worldrunner-render-program` draws and dispatches the shader programs that Unity ships in a player build, with wgpu. It sits in the Graphics layer. This slice proves that one pass, a vertex and fragment program or a compute kernel, can be selected, laid out, fed its constants, textures and state, and executed on a real adapter with predicted results.

## Inputs

The crate takes data that the importer has already prepared. It does not read bundles or translate shaders.

| Input | Type | Source |
| --- | --- | --- |
| Prepared SPIR-V words and entry point | `module::StageModule` | The shader preparation in `unity-import` |
| Binding table | `module::ModuleBinding` | Mirrors `unity-import`'s `ResourceBinding`: set, Unity binding, prepared binding and, for a split combined sampler, the sampler's binding |
| Parameters | `unity_serialized::shader_program::ProgramParameters` | The serialized sub program |
| Keyword variants | `unity_serialized::shader_program::Program` | `variant::select_sub_program` picks the sub program whose keyword set equals the enabled keywords, and `select_player_sub_program` does the same across platform lists |
| Fixed-function state | `unity_serialized::shader_state::ShaderState` | `state::resolve_state` reads each value from its fixed number or from the first property layer that names it |
| Properties | `properties::PropertySet` | Layers searched in order; the first layer with a name wins |

## What it does

- **Layouts.** `layout::ProgramLayout::build` merges the stages' resources into one pipeline layout and checks them against Unity's parameters. An unsupported parameter, a resource without a parameter, a parameter of another kind than its resource and stages that declare one binding differently are named errors. A parameter the entry point does not use is reported, not bound.
- **Constants.** `constants::pack_constant_buffer` writes vectors, matrices and arrays at Unity's byte offsets, a register of 16 bytes apart, sizes the buffer for the shader's needs and reports missing properties, which stay zero. A negative size, a parameter outside the buffer, a struct member and a value kind the packer does not write are named errors. `StageProgram::constant_buffer_targets` and `texture_targets` give the prepared set and binding each parameter fills.
- **Variants.** An exact keyword match selects the sub program. A missing combination is a named error.
- **State.** Cull, `ZTest`, `ZWrite`, color mask and blend for render target 0 resolve from fixed or property-controlled values. A property no layer has keeps the pass's fixed value and is listed in `ResolvedState::defaulted`. Color and alpha blend resolve separately. Unknown values and state this slice does not draw (stencil that is not off, alpha to coverage, depth bias, conservative rasterisation, depth clip off) are named errors. A draw has one color attachment, so the blend state of render targets 1 to 7 is not read, whether or not the pass sets `separate_blend`.
- **Draws.** `ProgramRuntime::draw` records non-indexed or indexed triangles with a vertex layout the caller gives, into a color target with an optional depth target. `read_target` records a copy of the pixels. Render and compute pipelines and shader modules are cached by content.
- **Compute.** `ProgramRuntime::dispatch` records a kernel with Unity-style bindings; `read_buffer` records a copy of a storage buffer.

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
- **Release.** `release_buffer`, `release_texture` and `release_target` fail with `ProgramError::Busy` while a frame is being recorded. They keep the bytes in the budget until the work submitted so far has completed, as `render-gpu`'s `Configuration::replace` does for retired generations; the next allocation, or `collect`, frees them. A pending readback holds its staging bytes until it is resolved or dropped.
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
- **Lifetime.** The frame keeps the arena's `FrameOwner` leased until the GPU has completed the submission. `ProgramArena::retire` fails with `Busy` while an unsubmitted or in-flight frame holds it and with `Expired` afterwards. Cancelling or dropping the prepared frame releases the lease and submits nothing. The prepared draws hold clones of the wgpu objects they use, which wgpu keeps alive while a recorded command needs them. The runtime refuses `release_buffer` while a frame is being recorded and keeps released bytes in its budget until the submissions made so far have completed, so a buffer an admitted draw uses stays counted until the GPU is done with it.
- **Checks at the consumer.** `AdmittedPrograms::check_target` compares the draws' signature with the pass's (`TargetMismatch`), and `retain` repeats the lease check for the frame that finally encodes them.

The only consumer so far is `render-scene`'s opaque pass, see [its README](../render-scene/README.md#player-program-draws). The first slice covers one single-sample color attachment with `Depth32Float` and nothing else.

## Measured conventions

The tests in `tests/program/conventions.rs` draw into an 8x8 target and compare bytes. They pin what a Unity Vulkan module needs:

- The module's position output is used unflipped. Positive clip Y lands in the bottom rows of the image.
- The depth range that survives clipping is 0 to w, as in Vulkan; the nearer of two triangles wins under `LessEqual`, and z of -0.5 or 1.5 is clipped.
- `wgpu::FrontFace::Cw` marks a triangle that is clockwise in the image, with row 0 at the top, as a front face. Cull Back drops the others and Cull Front drops the front faces. The draw request carries the winding, so a caller states it.

`tests/program/pass.rs` and `drawn_state.rs` draw a textured pass with a tint from a constant buffer, the multiply and add keyword variants, a split combined sampler, blend off against `SrcAlpha`/`OneMinusSrcAlpha`, and cull Back against Off for a back-facing quad. Pixels are predicted in closed form from the texels, tint and offset, independent of the shader, and compared with the 8-bit bound from `worldrunner_render_gpu::tolerance_test_support` (0.625 steps). The alpha-blend test adds 0.5 step because lavapipe's fixed-point blend measured 0.8 steps from the prediction; the Radeon stayed within the shared bound.

## Not done

- **Unity sampler state.** Every sampler binding gets one fixed sampler: linear filtering, clamped to the edge. The packed sampler state in the serialized parameters is not decoded, so wrap, filter and anisotropy settings are ignored.
- **Multisampling, stencil and multiple render targets.** Targets are single-sample with one color attachment and an optional depth attachment. Stencil state and a stencil depth format are named errors, and so are multisampled and multiple-attachment signatures.
- **Player-variant parameter blobs.** `Program::parameter_blob_indices` are not decoded, so parameters come from the sub programs' own `ProgramParameters`.
- **Frame planning and composition beyond one pass.** The runtime still owns no pass order and no attachments. `render-scene` takes admitted draws into its stock opaque pass for one single-sample color attachment and `Depth32Float`. It refuses scenes with temporal reconstruction, screen-space passes or multisampling by name. Camera render textures, sampling a draw's target in a later pass, compute between passes, transparent ordering, the depth-fitted capture continuation and `render-plan` descriptions of program work are not done. The standalone target that `create_render_target` makes remains the caller's to create and release.
- **Budget timing of write staging.** The staging buffer of a recorded `write_buffer` is admitted when it is recorded and released when the call returns, not when the GPU finishes the copy.
- **Calling shader preparation.** `shader_prep` lives in `unity-import`, which is in the Tool layer, so a Graphics-layer runtime cannot call it. Callers supply the prepared words and binding table.

## Tests

```sh
cargo test -p worldrunner-render-program -- --test-threads=1
```

`tests/program/packet.rs` covers preparation and recording into a caller's pass, and `arena.rs` covers admission, authority, lifetime and budget refusals. The frame tests in `render-scene` run with `cargo test -p worldrunner-render-scene --lib -- program_frame --test-threads=1`.

The draw and dispatch tests need a Vulkan adapter and use the one wgpu selects. They obtain the device through `render-gpu` (`headless_instance`, `request_device`), build a `SubmissionAuthority` and submit every frame through it. The first test to create a runtime prints the adapter's name to stderr; run with `--nocapture` to see it. Set `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json` to run them on lavapipe.
