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

This crate runs SPIR-V only. On macOS the engine uses the Linux build's data, because the macOS build of the same Steam build ships no shader with a Vulkan platform (the Linux and Windows builds ship 171), and loading the macOS build's shaders fails with `PlayerError::NoSpirvPrograms` ("no SPIR-V programs; Metal programs unsupported").

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

## Player structs

A player variant describes a constant buffer's structs beside its members. `constants::pack_layout` packs the one shape that was measured and refuses every other. `unity-import player-shader-structs` repeats the measurement; it places each struct with `BufferLayout::struct_placement`, the code that packs, and compares the result with the bound uniform block's SPIR-V type tree (`Offset`, `ArrayStride`, `MatrixStride` and nested struct types).

Measured on the Old World player (Unity 6000.3.15f1), counting each bound constant buffer of each variant, as `player-shader-layouts` does:

| Question | Answer | Buffers or members it covers |
| --- | --- | --- |
| Where does a struct instance start? | The struct's `index` equals the `Offset` of the block member that holds it. That member is the block's only member, at `Offset` 0, so `index` is 0 | 5,605 of 5,605 buffers |
| How does `index` relate to the buffer's member list? | It does not index it. Every one of these buffers has no members of its own, so a position in the member list, an ordinal among structs and a byte offset all give 0 and cannot be told apart | 5,605 of 5,605 buffers have zero members and one struct |
| Are a struct member's offsets from the struct's start or from the buffer's start? | From the start of an element. The elements are 2 or 256 in number and share one member list, and all 10,931 members end within the struct's size, which an offset from the buffer's start could not do for element 1. Each offset is an `Offset` of the SPIR-V struct type | 10,931 of 10,931 members |
| What is the stride of a struct array? | The struct's `size`. It is the block array's `ArrayStride` and the SPIR-V struct type's span, and the array size is the block array's length. The buffer's size is the array size times the struct size and the block's span | 5,605 of 5,605 buffers |
| Can struct members collide with the buffer's own members? | Not observable: no bound buffer has both. 0 of 5,605 has any member of its own | 0 of 5,605 buffers |
| In what order are the members listed? | Not the type's order in 544 buffers. The offsets are the same set, so members are found by offset, not by position | 5,061 of 5,605 buffers in the type's order |

The rule explains all 5,605 buffers and all 10,931 struct members: 3,808 float4 members, 4,124 float 4x4 matrices and 2,999 int4 members. A matrix is not a SPIR-V matrix type there: it is an array of four float4 at stride 16, the same four rows of 16 bytes that the packer writes for a top-level matrix. The int4 is a `uint4` in the module; the packer writes the 32 bits of an `i32`.

Names. Of 10 distinct struct member names and 3 distinct struct names, none is a material property name, none is another buffer's top-level member name and none is a struct name. 9 of the 10 member names are another buffer's top-level member name, or a material property name, followed by one and the same suffix. So a struct member is named by its own name, not by the struct's name plus the member's name; nothing in the player says which property name the engine itself fills these members from.

What the packer does with this:

- The buffer must have one struct, no members of its own, and that struct's `index` must be 0. The struct then starts at byte 0.
- Element `e` of the struct array is at `e * size`. A member is at its offset within the element. A struct that is not an array (array size 0 or 1) is one element; that case was not seen in the player.
- A property named by the member's own name supplies it. A vector, matrix or integer-vector array property gives one value per element, so element `e` takes item `e`. A single value is element 0's. Elements the array does not reach, and members no layer names, stay zero; a missing member is listed once in `PackedConstants::missing` however many elements the struct array has.
- A member inside a struct is one element's value, so it is not an array itself. Float vectors of one to four components, float matrices of four columns and integer scalars are written as at the top level. Integer vectors of four components (`PropertyValue::IntVector`, `IntVectorArray`) are written inside structs only, where they were measured; at the top level they stay `UnsupportedValue`.
- Named errors: `UnsupportedStruct` for an editor-style struct, a struct in a buffer that has members or another struct, and a struct whose `index` is not 0; `NegativeSize` for a negative array size or struct size; `MemberOutsideStruct` for a member that does not fit within the struct's size; `OutOfBounds` for elements that run past the buffer; `ArrayTooLong`, `WrongKind` and `UnsupportedValue` as for top-level members.

## What it does

- **Layouts.** `layout::ProgramLayout::build` merges the stages' resources into one pipeline layout and checks them against Unity's parameters, in either form. An unsupported parameter, a parameter outside the binding table, a resource without a parameter, a parameter of another kind than its resource, a binding for other stages and stages that declare one binding differently are named errors. A parameter the entry point does not use is reported, not bound.
- **Constants.** `constants::pack_layout` (through `pack_constant_buffer` or `pack_variant_buffer`) writes vectors, matrices and arrays at Unity's byte offsets, a register of 16 bytes apart, sizes the buffer for the shader's needs and reports missing properties, which stay zero. A player variant's members are found by name. A member kind other than float and int, a matrix with fewer than four columns, an integer vector outside a struct, a negative size and a member outside the buffer are named errors, and so is a struct of a shape that has not been measured (see [Player structs](#player-structs)). `StageProgram::constant_buffer_targets` and `texture_targets` give the prepared set and binding each constant buffer and texture fills, for the stage's own bindings.
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
- **Target signature.** A `TargetSignature` is one color format (none for an offscreen depth-only target), an optional depth format and a sample count. `TargetSignature::new` refuses what the runtime does not draw into, by name: more than one sample (`UnsupportedTarget::Multisampled`), other than one color attachment (`ColorAttachments`), a depth format with a stencil aspect (`Stencil`) and a depth attachment without depth (`NotDepth`). The pipeline cache key includes the signature.
- **Arena.** `ProgramRuntime::arena_allocations(bytes)` is the storage the renderer reserves through its configuration, so the renderer's budget refuses an arena that does not fit. `open_arena` refuses a reservation smaller than the runtime's budget (`ArenaTooSmall`), so everything the runtime admits lies within storage the renderer counted once. It also refuses a runtime that already holds allocations made before any arena (`ArenaAfterAllocations`), because the reservation could not cover them and would be given back while they are live; open the first arena before allocating. A `ProgramArena` belongs to the runtime that opened it (`ForeignArena`).
- **Admission.** `admit_draws` takes the frame's encoder, the arena, the signature and the packets. It prepares every packet and then makes the frame retain the arena through `Publications::retain_lease`, which checks the frame's authority (`LeaseError::ForeignAuthority`) and that the arena is still live (`LeaseError::Expired`). It returns an `AdmittedPrograms`, the only way to obtain one, or nothing at all.
- **Lifetime.** The frame keeps the arena's `FrameOwner` leased until the GPU has completed the submission. `ProgramArena::retire` fails with `Busy` while an unsubmitted or in-flight frame holds it and with `Expired` afterwards. Cancelling or dropping the prepared frame releases the lease and submits nothing. The prepared draws hold clones of the wgpu objects they use, which wgpu keeps alive while a recorded command needs them. The runtime refuses `release_buffer`, `release_texture` and `release_target` with `Busy` while a frame is being recorded or is prepared and unsubmitted, so no submission that could use the resource is still to come. It keeps a released resource's bytes in its budget until the newest submission made at the release has completed. Nothing unsubmitted can use the resource at that moment and the release excludes new frames while it records the serial, so that serial is at or after the last submission that used the resource. **One retention rule.** The reservations of the resources a call uses are held by a `FrameOwner` that the frame retains when the call records into it, and every recording entry point follows that rule through the same mechanism. `draw` holds the buffers and textures it binds, its vertex and index buffers and the target it draws into. `clear_target` holds the target. `dispatch` holds the buffers and textures it binds. `read_buffer` holds the buffer and the readback staging buffer. `read_target` holds the target and the staging buffer. `write_buffer` holds the buffer and the staging buffer of the write. Prepared draws follow the rule when a consumer retains them on the frame (`AdmittedPrograms::retain`, `PreparedProgramDraw::retain`). The holds matter when the runtime is dropped with a frame still unsubmitted: the frame keeps the bytes counted, in the renderer's arena reservation, until its outcome, so work recorded into a frame cannot outlive the accounting of what it touches. A frame that is cancelled or dropped gives the bytes back at once and a submitted one when the GPU has completed it. A pending readback and a dropped readback hold the same: the staging bytes stay counted until the frame completes, not until the caller resolves or drops the read. Dropping the draw detaches the owner (`FrameOwner::detach`): a frame that retained it can still be submitted and keeps the bytes counted until it is cancelled, dropped or completed on the GPU, whatever happened to the draw and the runtime. A frame that never retained the draw holds nothing, so cancelling a frame returns the bytes at once. A consumer that records a prepared draw must retain it on that frame.
- **Checks at the consumer.** `AdmittedPrograms::check_target` compares the draws' signature with the pass's (`TargetMismatch`), and `retain` repeats the lease check for the frame that finally encodes them.

The only consumer so far is `render-scene`'s opaque pass, see [its README](../render-scene/README.md#player-program-draws). The first slice covers one single-sample color attachment with `Depth32Float` and nothing else.

## Offscreen work through the checked operations

Unity player offscreen work is a camera rendering into a render texture, `Graphics.Blit` with a material pass and `Graphics.CopyTexture`. This crate turns a frame's records of it into `worldrunner_render_post::contracts::TypedOperation`s that run the player's own programs. It has no recorder of its own: the operations go to the one executor, which checks hazards, budgets, cancellation and timing, and the frame's timing and statistics list them. A second scheduler writing into the encoder before `encode_scene` is excluded by the one-renderer rule.

### Path of a frame

```text
host adapter       maps the world export to an OffscreenFrame (`play`'s `offscreen` module; this crate does not read world-simulation)
OffscreenState::build(runtime, arena, frame)       the one entry point: resolution, packing, versions
   -> OffscreenOperations { operations, later_reads, .. }    typed operations in call order, before the scene
OffscreenOperations::admit(encoder)                ResourceRegistry::compile_operations_with_reads
   -> AdmittedOffscreen
SceneGpu::admit_offscreen(admitted)                next to admit_programs
FrameEncoding::encode_scene(..)                    encodes the operations with the frame's pass timer, then the stock scene
frame.finish().submit()                            the frame's one submission
```

`render-scene`'s `encode_scene` already calls `FrameRenderer::encode_frame` after it has encoded other preparation work into the same encoder (terrain geometry, sky, terrain pages). The offscreen operations are encoded in the same place, before the stock operations. The pass timer lives in the post chain (`TimingState`), so the contract gets one entry point that encodes a compiled list with the frame's timer (`TimingState::encode_checked`). The passes then appear in `FrameTiming::passes` as `PassId::Operation`.

### Mapping to the contract

| Record | Operation | Resource uses |
| --- | --- | --- |
| All uniform data of the frame | One `TypedWork::Upload` into a budgeted constants buffer, first in the list | Writes the buffer range; every draw reads its slice as `BufferUseKind::Uniform` |
| `CameraRender` | `TypedWork::Draws { pass, draws }`, one `CheckedDraw` per sub-mesh | The pass clears or loads the render texture and its depth as the camera says (`AttachmentLoad`, `StoreOp::Store`); textures the materials sample are `ResourceUse::read(.., Sampled)` in `pass.uses`; vertex and index buffers are `BufferUse::read(.., Vertex/Index)` |
| `Blit` with a material | `TypedWork::Fullscreen` with the material's pass | The target is an attachment loaded with its prior contents, the source is read as `Sampled` |
| `Blit` without a material | `TypedWork::CopyTexture` when the sizes and formats agree | Copy read and write |
| `CopyTexture` | `TypedWork::CopyTexture` with element and mip in the `TextureRange` of each view | Copy read of the source range, copy write of the destination range |
| Scene draws that sample a result | Not an operation: declared with `ResourceRegistry::compile_operations_with_reads` | Sampled reads of the ranges the host names in `OffscreenFrame::later_reads` |

Versions are per frame. Every render texture, array and source texture is imported with initial version 0 (a new wgpu texture is zero-filled, so version 0 is truthful), and each write takes the next version of its mip and layer. Each frame builds a new `ResourceRegistry`, so the registry never accumulates retired textures. A runtime-owned texture is imported with `import_leased` and the arena's lease, so a retired arena fails the compile and the encode by name, and the encode retains the lease on the frame.

### Contract extensions in render-post

| Extension | Why Old World needs it |
| --- | --- |
| `CheckedDraw::rebind` | A draw is a pipeline, bind groups and geometry. `CheckedDraw::with_geometry` creates the pipeline every time. Splat cameras draw hundreds of sub-meshes with a few programs, so the pipeline is created once per program and state and the other draws rebind it, with the layouts and vertex ABI checked against it |
| `TypedWork::Fullscreen` vertex streams | The player's blit vertex stages read position and texture coordinates from vertex buffers. `Fullscreen` drew three vertices with no vertex buffer, which a pipeline whose vertex stage has inputs cannot use |
| `ResourceRegistry::compile_operations_with_reads` | A scene draw that samples a render texture is recorded by the stock pass, which is not an operation. The host declares its reads, and the same version rules that check operations check them: a read before any write, a read of a stale version and a read of a range a later operation writes are errors |
| `TimingState::encode_checked` | The frame's pass timer is inside the post chain. A host outside render-post had no way to give `CompiledOperations::encode` the frame's timer |

Each has its own test in `crates/render-post`; see the [render-post README](../render-post/README.md).

### Resolution

`ProgramShader` is the decoded `unity_serialized::shader::Shader` and the decompressed chunks of its Vulkan platform. `ProgramShader::resolve(selector, keywords)` walks the first sub-shader's passes, picks the pass, picks the variant, decodes its record and parameters, prepares its stages together with `spirv_prep::prepare_stages` and builds a `GraphicsProgram`. The walk is the one `unity-import`'s `player_variants` does (this crate does not depend on `unity-import`).

- **Pass selector.** `PassSelector::Index(n)` is the n-th pass of the sub-shader, for `Graphics.Blit`. `PassSelector::Tag { key, values }` is the first pass whose tag `key` equals the first of `values` that any pass has, for camera draws. Keys are compared without regard to ASCII case and values exactly; the pass's own tags are searched after the render state's. The crate names no tag key and no value; the caller gives both. No match is `ResolveError::NoPass`, never another pass.
- **Keywords.** The material's keywords and the enabled global keywords are mapped to the shader's keyword indices by name. `variant::select_variant` then matches them exactly, as `select_player_sub_program` does, but only among the variants of the Vulkan platform. A keyword the shader does not list cannot change a variant and is ignored. No match is `ResolveError::NoVariant`.
- **Cache.** A prepared variant is cached by the content of its record (the SPIR-V, the entry points and the parameters), so two shaders or passes with the same variant share one program. Render pipelines are cached by program, state, target signature, vertex layout and bind group layouts.

### Property layers and samplers

A `PropertyLayer` holds numeric values and texture inputs by name. A draw reads four layers, highest first: the caller's per-draw values, the renderer's property block, the material and the globals. For a camera draw the camera's values (view and projection, which the caller names however its shaders do) sit in the per-draw layer, below the renderer's own per-draw values. This crate names no Unity built-in. A constant buffer member or texture no layer has keeps the packer's behaviour (zero) and is listed in `OffscreenOperations::missing`; a texture no layer has is `OffscreenError::MissingTexture`.

A `TextureInput` may carry a `SamplerState` (wrap U, wrap V and a filter mode). `SamplerCache` creates one `CheckedSampler` per state. The state of a texture input overrides the state of the texture, and a texture with neither keeps the fixed sampler the crate always used, so existing tests do not change. A split combined sampler takes the state of the texture it was split from.

### Render textures

A `RenderTexture` has a caller id, a size, a `GraphicsFormat` code, depth bits, a mip count, an sRGB flag and a generation. `OffscreenState` creates it through the runtime, so it counts against the runtime's budget and the renderer's arena, and creates it again when the generation or any of the other fields change. The old texture is released when the runtime is not recording, and a release that finds a frame being recorded is retried at the next build. The `GraphicsFormat` codes are the engine's own managed `GraphicsFormat` values; the table has 4 (`Rgba8UnormSrgb`), 5 (`R8Unorm`), 8 (`Rgba8Unorm`), 45 (`R16Float`), 48 (`Rgba16Float`) and 52 (`Rgba32Float`), and any other is `OffscreenError::UnsupportedFormat`. Depth is 16 bits (`Depth16Unorm`), 24 (`Depth24Plus`) or 32 (`Depth32Float`); any other count is a named error and stencil is not drawn.

GraphicsFormat 0 (`None`) with depth bits is a depth-only render texture: its storage is one depth texture with the spec's layers and mips, and that depth is the texture that `OffscreenState::texture_handle`, copies, readbacks and sampled reads use. A camera renders into it in a `RasterPass` with no color attachment (`colors` is empty) and the pipelines have no color target; render-post's checks accept such a pass, so the contract needed no extension. `TargetSignature::depth_only` is the crate-internal signature of that pass, and `TargetSignature::color` is `None` for it. `TargetSignature::new` still refuses anything but one color attachment. A blit with a material needs a color attachment to draw over and refuses a depth-only target with `OffscreenError::NoColorAttachment`. `CopyTexture` between two depth-only textures of the same format is a texture copy, and a blit with a material samples the depth as a non-filterable float through a point sampler (the rule for textures the device cannot filter). A readback copies into a buffer, which `Depth24Plus` does not allow, so `read_back` of it is `OffscreenError::UnreadableFormat`; `Depth32Float` reads back. GraphicsFormat 0 with no depth bits is `OffscreenError::UnsupportedFormat { code: 0 }`.

A pass declares its textures as filterable floats. A texture whose format the device cannot filter, such as `Rgba32Float` without `FLOAT32_FILTERABLE`, is bound as a non-filterable float and read through a point sampler that keeps the texture's wrap modes (clamp to the edge when it has no sampler state), so such a read returns the nearest texel instead of failing the contract's sample type check. Depth textures are read this way too. With the feature the device filters it and the declared filtering applies.

### Camera draws

Renderers are selected by the camera's culling mask against the renderer's layer and drawn in the order of their material's render queue; equal queues keep input order. There is no front-to-back sorting. One `CheckedDraw` is built per sub-mesh from the resolved program, the packed constants and the bindings. Vertex buffers follow the variant's vertex input table: each (source channel, target) pair takes the mesh channel `source` and feeds shader location `target`. A source channel the mesh lacks is `OffscreenError::MissingChannel`. Culling is the pass's, with clockwise front faces as the other draws here; mirrored transforms are not detected.

### Blits

A blit with a material binds its source under the main texture property name `OffscreenFrame::main_texture` (default `_MainTex`, the name the public blit API uses) and, when a constant buffer member of that name plus `_TexelSize` exists, a vector of the texel width, height, width and height of the source's mip 0 (`1/w, 1/h, w, h`). The vertex stage reads a quad of two triangles in clip space. A blit without a material is a texture copy when the size and format agree, else `OffscreenError::BlitNeedsMaterial`.

### Copies

`CopyTexture` checks the source and destination before the executor does: the mip and the element exist, the sizes at those mips are equal and the formats are equal up to sRGB. Anything else is `OffscreenError::CopyMismatch` (size or format) or `OffscreenError::NoSuchSubresource`.

### Vertex source channels in the Old World player

`unity-import player-shader-channels <player data directory>` counts, through the engine's decoders, how many variants read each source channel of their vertex input table (`BindChannels`: pairs of a source channel and a shader input). Over the 171 shaders with a Vulkan platform and their 10,773 variants (none unreadable):

| Source channel | Variants that read it | Of 10,773 |
| --- | --- | --- |
| 0 | 10,002 | 92.9% |
| 1 | 7,801 | 72.4% |
| 4 | 8,593 | 79.8% |
| 3 | 4,010 | 37.2% |
| 2 | 3,368 | 31.3% |
| 5 | 159 | 1.5% |
| 6, 7, 9, 10, 11 | 2 each | |
| 8 | 7 | |
| 12, 13 | 1 each | |

Channels 0 to 4 matter: 0, 1 and 4 are read by most variants, 2 and 3 by a third. Channel 5 is read by 159 variants and every channel from 6 on by at most 7. The shader inputs fed are 13 to 24; inputs 13 to 17 take 10,002, 8,647, 7,913, 4,037 and 3,328 variants and the rest at most 13. Tables hold 0 to 6 pairs (771 variants none, 3,876 three, 3,315 five) and 2 variants hold 12. No variant reads a source channel or feeds an input twice, and in all 10,773 the source mask is exactly the bits of the pairs' source channels, so the mask adds nothing a caller needs. The numbers are the codes the table stores; this crate maps them to mesh channels only through `MeshChannel::source`, so a host adapter decides what each code is. `tests/program/offscreen/camera.rs` draws with codes 0 and 4 and refuses a missing channel by name; the other codes take the same path.

## Measured conventions

The tests in `tests/program/conventions.rs` draw into an 8x8 target and compare bytes. They pin what a Unity Vulkan module needs:

- The module's position output is used unflipped. Positive clip Y lands in the bottom rows of the image.
- The depth range that survives clipping is 0 to w, as in Vulkan; the nearer of two triangles wins under `LessEqual`, and z of -0.5 or 1.5 is clipped.
- `wgpu::FrontFace::Cw` marks a triangle that is clockwise in the image, with row 0 at the top, as a front face. Cull Back drops the others and Cull Front drops the front faces. The draw request carries the winding, so a caller states it.

`tests/program/pass.rs` and `drawn_state.rs` draw a textured pass with a tint from a constant buffer, the multiply and add keyword variants, a split combined sampler, blend off against `SrcAlpha`/`OneMinusSrcAlpha`, and cull Back against Off for a back-facing quad. Pixels are predicted in closed form from the texels, tint and offset, independent of the shader, and compared with the 8-bit bound from `worldrunner_render_gpu::tolerance_test_support` (0.625 steps). The alpha-blend test adds 0.5 step because lavapipe's fixed-point blend measured 0.8 steps from the prediction; the Radeon stayed within the shared bound.

## Not done

- **Unity sampler state.** A texture input carries wrap U, wrap V and a filter mode, and the crate keeps one sampler per state. The packed sampler state in the serialized parameters is still not decoded, so a texture with no state gets the fixed sampler (linear filtering, clamped to the edge), and anisotropy, mip bias and a separate wrap W are not carried. A host adapter maps a texture's wrap and filter settings onto `SamplerState`.
- **Multisampling, stencil and multiple render targets.** Targets are single-sample with one color attachment and an optional depth attachment, or a depth attachment alone for an offscreen depth-only render texture. Stencil state and a stencil depth format are named errors, and so are multisampled and multiple-attachment signatures.
- **Multisampled textures, storage images and compute masks.** Dimension codes 5 and 11 are multisampled images, which are named errors because targets and textures are single-sample. A storage image is a named error. A compute stage's bit in the stage mask has not been seen, so compute ignores the mask.
- **Player struct shapes beyond the measured one.** A struct that starts anywhere but byte 0, a buffer with a struct and members, a buffer with several structs, and editor-style structs are `ConstantError::UnsupportedStruct`, because the player has none: all 5,605 buffers with structs are one struct array at byte 0 and nothing else. Whether `index` is a byte offset, a position or an ordinal is therefore open. Structs inside structs and arrays inside structs are not seen (the SPIR-V struct types hold none) and are refused as unsupported values. Which property the engine fills the struct members from (the per-draw instance data) is not decided here: a caller supplies them under the members' own names.
- **Player member kinds and matrix shapes.** Only float (0) and int (1) member kinds and matrices of four columns are written. Every one of the 201,382 members of the Old World player's bound buffers is one of those (float scalars, vectors and 4x4 matrices, with or without an array, and int scalars), and so are the 10,931 members of its structs, which add int4. So a bool, unsigned, narrower matrix or int vector at the top level is a named error that the player checked never reaches.
- **Prepared numbering across stages.** The stages of a variant must come from `spirv_prep::prepare_stages`, which gives every stage one binding table. Modules prepared one at a time by `prepare` number the same Unity binding differently when they split different samplers, and `ProgramLayout::build` then reports `LayoutError::Conflict`: in the Old World player that was 1,350 of 10,768 two-stage variants, and with the stages prepared together none of them is, and all 10,768 share one numbering. `prepare_stages` fails with `PrepareError::BindingClass` when stages declare one Unity binding as different classes of resource; no variant in that player does. The table lists every binding any stage declares, so a stage's table can hold entries its module does not declare: a parameter that names such a binding for the stage is reported in `ProgramLayout::unused`, not as `LayoutError::UnboundSlot`.
- **Frame planning and composition.** The runtime owns no pass order and no scene attachments. `render-scene` takes admitted draws into its stock opaque pass for one single-sample color attachment and `Depth32Float`, and encodes admitted offscreen operations before its own operations. It refuses scenes with temporal reconstruction, screen-space passes or multisampling for draws by name, and refuses offscreen operations on its resident and shadowed paths. Compute between passes, transparent ordering, the depth-fitted capture continuation and `render-plan` descriptions of program work are not done. The standalone target that `create_render_target` makes remains the caller's to create and release.
- **Offscreen work.**
  - Camera renders are drawn in render queue order without front-to-back or back-to-front sorting, and a draw's culling is the pass's. Mirrored transforms do not flip the winding. Stencil, multisampled, cube and 3D render textures are not covered. A camera or a blit draws into mip 0 of element 0; other mips and elements are reached by `CopyTexture`, which the array test does.
  - The GraphicsFormat table has 4, 5, 8, 45, 48 and 52, format 0 with depth bits as a depth-only target, and the depth sizes 16, 24 and 32; any other code is `OffscreenError::UnsupportedFormat`. Formats Old World asks for beyond these are not known from the unity-import report.
  - A blit without a material only copies. A scaling blit without a material is `BlitNeedsMaterial`.
  - `later_reads` names the ranges a scene draw samples; the host must name them, because a scene draw that samples a render texture is not an operation.
  - The host adapter that maps the world export onto `OffscreenFrame`, builds the `ProgramShader` of each shader and supplies the built-in per-draw and per-camera values is the `offscreen` module of `play`, see [its README](../play/README.md#offscreen-work-from-a-player). The built-ins it leaves missing show up in `OffscreenOperations::missing`.
  - Property blocks, per-instance arrays and instancing of a renderer's many sub-meshes are built one draw each; draws are not batched.
- **Budget timing of write staging.** The staging buffer of a recorded `write_buffer` is admitted when it is recorded and released when the call returns, not when the GPU finishes the copy.
- **Calling shader preparation.** `unity_serialized::spirv_prep::prepare_stages` is in the Foundation layer, which this crate may depend on, but the runtime does not call it. Callers supply the prepared words and `PreparedModule::bindings` of each stage, prepared together with the variant's other stages. `unity-import player-shader-layouts` does this for every Vulkan variant of a player.

## Tests

```sh
cargo test -p worldrunner-render-program -- --test-threads=1
```

`tests/program/packet.rs` covers preparation and recording into a caller's pass, and `arena.rs` covers admission, authority, lifetime and budget refusals. The frame tests in `render-scene` run with `cargo test -p worldrunner-render-scene --lib -- program_frame --test-threads=1`.

The draw and dispatch tests need a Vulkan adapter and use the one wgpu selects. They obtain the device through `render-gpu` (`headless_instance`, `request_device`), build a `SubmissionAuthority` and submit every frame through it. The first test to create a runtime prints the adapter's name to stderr; run with `--nocapture` to see it. Set `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json` to run them on lavapipe.

`tests/program/player_structs.rs` packs struct arrays at the byte level from hand-built player descriptions, including every named error. `tests/program/player_struct_block.rs` builds a uniform block of an array of structs with naga from WGSL, predicts its layout by hand, reads the `Offset` and `ArrayStride` decorations naga wrote back from the SPIR-V, compares the packed bytes with them and runs a compute kernel on the GPU that reads each element.

The draw, dispatch and layout tests run once with editor-style parameters and once with a player variant's, from equivalent hand-built descriptions, and must agree. The draw and dispatch tests need a Vulkan adapter and use the one wgpu selects. They obtain the device through `render-gpu` (`headless_instance`, `request_device`), build a `SubmissionAuthority` and submit every frame through it. The first test to create a runtime prints the adapter's name to stderr; run with `--nocapture` to see it. Set `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json` to run them on lavapipe.

### Offscreen tests

`tests/program/offscreen/` uses tiny player-shaped shaders built with naga from WGSL (`src/synthetic.rs`, behind the `test-support` feature, which `render-scene` also enables for its own tests). Each has Unity-shaped `VariantParameters` and a vertex input table; the camera shader has two passes tagged with LightMode values and two keyword variants. Predicted pixels come from the placements, clear colors, tints and inversions, never from the output:

- `resolve.rs`: pass by index and by tag list, exact keyword match, no fallback, missing pass and missing variant named, variants cached by content.
- `camera.rs`: two renderers filtered by layer over a clear color, render queue order, depth, a camera that does not clear, missing vertex channel and missing texture named, missing property listed.
- `layers.rs` and `sampler.rs`: the four layers in priority order and the camera below the renderer; repeat, clamp and mirror against the fixed sampler at predicted texels; a texture input's state over the texture's.
- `blit.rs` and `copy.rs`: a material pass by index, the texel size vector, the main texture name, a plain copy, element and mip copies, and the named errors for sizes, formats, subresources, unsupported formats and depths.
- `formats.rs`: cameras and blits into `R16Float` and `Rgba32Float` render textures read back at predicted half-float and float values, and a 2x2 float texture blitted up to 8x8 through the blit pass's filtering sampler: nearest texels when the device cannot filter the format, bilinear blends for `Rgba16Float`.
- `depth_only.rs`: a camera into depth-only textures of 32 bits (depth read back as exact floats), and of 24 bits (depth sampled by a blit, and carried by a `CopyTexture`), the pass with no color attachment, and the named errors for a blit into a depth-only target, a `Depth24Plus` readback and format 0 without depth.
- `lifetime.rs`: contents kept between frames, re-creation on a new generation or size with the old storage released, the budget, and a cancelled frame that moves no counter and leaves the arena free.
- `render-scene`'s `draw/tests/program_frame/offscreen.rs`: a camera into a render texture, a blit with a material pass, a copy into an array element and a scene draw sampling that element, with a readback after each stage, one submission and the passes at the head of the frame's timing. Its device requests timestamp queries and the test fails if there are none.

```sh
cargo test -p worldrunner-render-program --test program -- offscreen --test-threads=1
cargo test -p worldrunner-render-scene --lib -- program_frame --test-threads=1
```
