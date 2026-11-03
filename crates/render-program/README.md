# Running Unity player shader programs

`worldrunner-render-program` draws and dispatches the shader programs that Unity ships in a player build, with wgpu. It sits in the Graphics layer. This slice proves that one pass, a vertex and fragment program or a compute kernel, can be selected, laid out, fed its constants, textures and state, and executed on a real adapter with predicted results.

## Inputs

The crate takes data that the importer has already prepared. It does not read bundles or translate shaders.

| Input | Type | Source |
| --- | --- | --- |
| Prepared SPIR-V words and entry point | `module::StageModule` | The shader preparation in `unity-import` |
| Binding table | `module::ResourceBinding` | `unity_serialized::spirv_prep::ResourceBinding`, re-exported: set, Unity binding, prepared binding and, for a split combined sampler, the sampler's binding. Every stage of a variant has the same table, from `spirv_prep::prepare_stages` |
| Parameters | `module::StageParameters` | Either `unity_serialized::shader_program::ProgramParameters`, the editor-style sub program's parameters, or `unity_serialized::shader_parameters::VariantParameters`, which a Unity 6 player stores for each variant. `StageProgram::new` takes either. See below |
| Keyword variants | `unity_serialized::shader_program::Program` | `variant::select_sub_program` picks the sub program whose keyword set equals the enabled keywords, and `select_player_sub_program` does the same across platform lists. `Program::variant_blobs` and `unity_serialized::shader_parameters::platform_parameters` pair a player variant with its `VariantParameters` |
| Fixed-function state | `unity_serialized::shader_state::ShaderState` | `state::resolve_state` reads each value from its fixed number or from the first property layer that names it |
| Properties | `properties::PropertySet` | Layers searched in order; the first layer with a name wins |

## Two forms of parameters

An editor-style sub program describes one stage with a name index for every property and a Unity binding for every resource, in no descriptor set. A player variant describes the whole variant: constant buffers with named members (kind, rows, columns, matrix flag, array size, byte offset) and a typed binding list whose descriptor slots carry the binding, the set and the stages that use the resource. Both reduce to the same internal lists, so layouts, constant buffers and targets have one implementation:

- `source::Source` (crate-private) turns either form into the resources a stage declares, its constant buffers and its textures. Layouts (`declare.rs`, `layout.rs`) and targets (`targets.rs`) read only those lists.
- `buffers::BufferLayout` is a constant buffer's members in one form: a `ParamName` (an index into the pass's names, or the name itself), a `Kind`, rows, columns, matrix flag, array size and byte offset. `constants::pack_layout` packs it. `pack_constant_buffer` (editor-style) and `pack_variant_buffer` (player) convert and call it.
- The binding table is `unity_serialized::spirv_prep::ResourceBinding`. A player binding names a set and a binding in Unity's module, before the sampler split, and `entry` finds the table entry with that set and Unity binding. Editor-style bindings name no set, so the Unity binding must be unique across sets (`LayoutError::AmbiguousBinding` otherwise).
- A texture binding declares the image at the entry's prepared binding. When the entry's combined image sampler was split, the split sampler is declared too. A texture's sampler slot and a sampler binding declare a sampler at the entry's split sampler binding, or else at its own prepared binding. This is why a texture that names its own slot as its sampler, a texture with no sampler slot and a combined image sampler that was split all lay out alike.

### Player bindings

| Binding | Declared as | Notes |
| --- | --- | --- |
| Texture, dimension 4, 6, 8, 10 | 2D, 3D, cube, 2D array texture | Measured against the SPIR-V of the Old World player. Not multisampled |
| Texture, dimension 5 or 11 | `LayoutError::MultisampledTexture` | Measured as a multisampled 2D and a multisampled 2D array image (6 and 3 bindings in the Old World player). This renderer has no multisampled textures |
| Texture, any other dimension | `LayoutError::UnsupportedParameter` | |
| Constant buffer | Uniform buffer | Its layout is the `ConstantBuffer` of the same name; `LayoutError::UndescribedBuffer` if there is none |
| Buffer block | Storage buffer, read-only or not as the module says | |
| Storage image | `LayoutError::UnsupportedParameter` | One instance in the player |
| Sampler | Filtering or comparison sampler as the module says | The packed state is not decoded |

The stage mask is checked against the module's stage. Bit 0 is the vertex stage and bit 1 the fragment stage:

- A binding that names other stages only is neither bound nor reported unused, and an unsupported binding among them does not fail the stage.
- A resource the module uses that the parameters bind for other stages only is `LayoutError::StageMismatch`.
- A slot's visibility is the stages in its mask plus the module's own stage.
- A binding that names the stage but that the entry point does not use is reported in `ProgramLayout::unused` and gets no slot.
- A compute stage has no bit in the mask, because none has been seen, so the mask is not consulted for it.

## What it does

- **Layouts.** `layout::ProgramLayout::build` merges the stages' resources into one pipeline layout and checks them against Unity's parameters, in either form. An unsupported parameter, a parameter outside the binding table, a resource without a parameter, a parameter of another kind than its resource, a binding for other stages and stages that declare one binding differently are named errors. A parameter the entry point does not use is reported, not bound.
- **Constants.** `constants::pack_layout` (through `pack_constant_buffer` or `pack_variant_buffer`) writes vectors, matrices and arrays at Unity's byte offsets, a register of 16 bytes apart, sizes the buffer for the shader's needs and reports missing properties, which stay zero. A player variant's members are found by name. A member kind other than float and int, a matrix with fewer than four columns, a negative size, a member outside the buffer and a struct are named errors. `StageProgram::constant_buffer_targets` and `texture_targets` give the prepared set and binding each constant buffer and texture fills, for the stage's own bindings.
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

## Measured conventions

The tests in `tests/program/conventions.rs` draw into an 8x8 target and compare bytes. They pin what a Unity Vulkan module needs:

- The module's position output is used unflipped. Positive clip Y lands in the bottom rows of the image.
- The depth range that survives clipping is 0 to w, as in Vulkan; the nearer of two triangles wins under `LessEqual`, and z of -0.5 or 1.5 is clipped.
- `wgpu::FrontFace::Cw` marks a triangle that is clockwise in the image, with row 0 at the top, as a front face. Cull Back drops the others and Cull Front drops the front faces. The draw request carries the winding, so a caller states it.

`tests/program/pass.rs` and `drawn_state.rs` draw a textured pass with a tint from a constant buffer, the multiply and add keyword variants, a split combined sampler, blend off against `SrcAlpha`/`OneMinusSrcAlpha`, and cull Back against Off for a back-facing quad. Pixels are predicted in closed form from the texels, tint and offset, independent of the shader, and compared with the 8-bit bound from `worldrunner_render_gpu::tolerance_test_support` (0.625 steps). The alpha-blend test adds 0.5 step because lavapipe's fixed-point blend measured 0.8 steps from the prediction; the Radeon stayed within the shared bound.

## Not done

- **Unity sampler state.** Every sampler binding gets one fixed sampler: linear filtering, clamped to the edge. The packed sampler state in the serialized parameters is not decoded, so wrap, filter and anisotropy settings are ignored.
- **Multisampling, stencil and multiple render targets.** Targets are single-sample with one color attachment and an optional depth attachment. Stencil state is a named error.
- **Multisampled textures, storage images and compute masks.** Dimension codes 5 and 11 are multisampled images, which are named errors because targets and textures are single-sample. A storage image is a named error. A compute stage's bit in the stage mask has not been seen, so compute ignores the mask.
- **Player structs.** A constant buffer with structs is `ConstantError::UnsupportedStruct`. 5,605 of the constant buffer bindings in the Old World player's Vulkan variants have structs, and how a struct's offset, size and members combine with the buffer's own members and with property names has not been measured.
- **Player member kinds and matrix shapes.** Only float (0) and int (1) member kinds and matrices of four columns are written. Every one of the 201,382 members of the Old World player's bound buffers is one of those (float scalars, vectors and 4x4 matrices, with or without an array, and int scalars), so a bool, unsigned, narrower matrix or int vector is a named error that the player checked never reaches.
- **Prepared numbering across stages.** The stages of a variant must come from `spirv_prep::prepare_stages`, which gives every stage one binding table. Modules prepared one at a time by `prepare` number the same Unity binding differently when they split different samplers, and `ProgramLayout::build` then reports `LayoutError::Conflict`: in the Old World player that was 1,350 of 10,768 two-stage variants, and with the stages prepared together none of them is, and all 10,768 share one numbering. `prepare_stages` fails with `PrepareError::BindingClass` when stages declare one Unity binding as different classes of resource; no variant in that player does. The table lists every binding any stage declares, so a stage's table can hold entries its module does not declare: a parameter that names such a binding for the stage is reported in `ProgramLayout::unused`, not as `LayoutError::UnboundSlot`.
- **Frame planning and composition.** The runtime records where it is told to and owns no pass order or attachments. Having `render-plan`, `render-scene` and `render-post` own the order of passes and the attachments a program draws into is the next step; this crate does not do it yet, and no scene or material system calls the runtime. A target here is a standalone color and depth texture pair that the caller creates and releases.
- **Budget timing of write staging.** The staging buffer of a recorded `write_buffer` is admitted when it is recorded and released when the call returns, not when the GPU finishes the copy.
- **Calling shader preparation.** `unity_serialized::spirv_prep::prepare_stages` is in the Foundation layer, which this crate may depend on, but the runtime does not call it. Callers supply the prepared words and `PreparedModule::bindings` of each stage, prepared together with the variant's other stages. `unity-import player-shader-layouts` does this for every Vulkan variant of a player.

## Tests

```sh
cargo test -p worldrunner-render-program -- --test-threads=1
```

The draw, dispatch and layout tests run once with editor-style parameters and once with a player variant's, from equivalent hand-built descriptions, and must agree. The draw and dispatch tests need a Vulkan adapter and use the one wgpu selects. They obtain the device through `render-gpu` (`headless_instance`, `request_device`), build a `SubmissionAuthority` and submit every frame through it. The first test to create a runtime prints the adapter's name to stderr; run with `--nocapture` to see it. Set `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json` to run them on lavapipe.
