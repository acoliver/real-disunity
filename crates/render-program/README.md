# Format-neutral shader program runtime

`worldrunner-render-program` builds layouts and pipelines for shader programs, binds their resources and records draws and dispatches with wgpu. It sits in the Graphics layer. Its root modules describe programs in engine terms and name no shader file format. A default-off `unity` module converts Unity player programs into those descriptions, and the runtime executes the result.

## Neutral model

A caller states each stage's code, the resources and constants the stage declares, and the fixed-function state of a draw. Nothing below mentions a source format.

| Concept | Types | What they say |
| --- | --- | --- |
| Stage module | `module::StageModule`, `StageSource::{SpirV, Wgsl}`, `StageKind` | Code, entry point and stage. SPIR-V keeps naga's Y flip, which turns its clip space into the one wgpu draws with. WGSL is used as written. |
| Declarations | `module::StageDeclarations` | The `ResourceLayout` and the `BoundConstantLayout`s of one stage. |
| Resource layout | `binding::{BindingAddress, ResourceBinding, ResourceKind, BindingSource, PropertyId}` | Set, binding, visibility and kind: uniform or storage buffer (with access and a minimum size), texture (dimension, sample type, multisampled) and sampler (filtering, non-filtering or comparison). |
| Constant layout | `constants::{ConstantLayout, Member, ScalarKind, ArrayLayout, MatrixLayout}`, `pack_constants` | Named members with kind, rows, columns, array count and stride, byte offset and matrix line stride and order. A signed integer member is a scalar or a vector of four components, and an array of either. |
| Render state | `state::{RenderState, RasterState, DepthState, ColorState, StencilState}` | Cull, front face, depth test, write and bias, one entry per colour target (blend and write mask) and stencil. |
| Variants | `variant::{FeatureId, FeatureSet, Variants, select_variant, UnknownFeatures}` | A program per exact feature set. |
| Property values | `properties::{PropertySet, PropertyValue}`, `runtime::{Resource, BoundResource, BufferHandle, TextureHandle, TargetHandle}` | Floats, integers, unsigned integers, booleans, vectors, integer vectors, matrices, arrays and resource handles, looked up by name in layers where the first layer with a name wins. |

Execution consumes only these values. `layout::ProgramLayout::build` checks each stage's declarations against the resources its entry point uses, `constants::pack_constants` fills a buffer, `ProgramRuntime::graphics_program` and `compute_program` build the wgpu layouts, and `draw`, `prepare_draw`, `admit_draws` and `dispatch` record the work.

- **Layouts.** A resource the entry point uses must be declared at its address, as the kind the shader has and visible to the stage that uses it. A binding declared twice, a declaration of another kind, a declaration the stage cannot see and stages that declare one binding differently are named `LayoutError`s. A declaration the entry point does not use is listed in `ProgramLayout::unused` and gets no slot. A slot is as large as the larger of the shader's need and the declared minimum size, and its visibility is the declared one.
- **Constants.** Offsets, array strides and matrix line strides come from the layout, so no packing rule of a shader language is built in. A vector writes its leading columns. `PropertyValue::Matrix` holds element `(row, column)` at index `column * 4 + row`, and the layout's storage order places the logical values. An unsigned member takes `Uint` or a non-negative `Int`, and a boolean member takes `Bool` or an `Int` of 0 or 1. A signed integer member takes `Int` (when it has one component), `IntVector`, which writes its leading `cols` components, or an `IntVectorArray`, one element per array element. An unsigned or boolean member of an array takes one value, which is element 0's. A member that is missing from the layers stays zero and is listed in `PackedConstants::missing`. A member outside the buffer, a value of another kind, an array longer than its member and a member shape the packer does not write are named `ConstantError`s.
- **Variants.** `Variants` matches the enabled features, restricted to the ones its programs differ by, against an exact set. `UnknownFeatures::Fail` names an enabled feature that no program differs by, and `Ignore` leaves it out.
- **State.** A draw through `draw`, `prepare_draw` or a material writes one colour target. Stencil, depth bias, alpha to coverage, several colour targets and an unwritten colour target are representable and refused by name (`ProgramError::UnsupportedState`) when such a draw is prepared. A draw over lent resources (below) checks the state against its signature with `RenderState::check_targets`, which takes one `Some` colour state per attachment, with any write mask. A state without depth tests nothing and writes no depth.
- **Draws.** `ProgramRuntime::draw` records non-indexed or indexed triangles with a vertex layout the caller gives, into a colour target with an optional depth target. `read_target` records a copy of the pixels. Render and compute pipelines and shader modules are cached by content, and the pipeline key includes the stage sources, the layout, the identity of each lent layout, the whole state and the target signature.
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
| Prepared SPIR-V words, entry point and binding table (`unity_serialized::spirv_prep::ResourceBinding`, re-exported as `unity::module::ResourceBinding`) | `unity::module::{StageModule, StageProgram}` | A neutral `StageModule` and, when a layout is built, `StageDeclarations` with each parameter placed at its prepared set and binding. A split combined sampler becomes a texture and a sampler binding. Every stage of a variant has the same table, from `spirv_prep::prepare_stages`. |
| `ProgramParameters` or `VariantParameters` (`unity::module::StageParameters`) | `unity::layout::ProgramLayout` | Resource declarations, each completed from the module's own binding when Unity's class matches it. A class that does not match is Unity's `Mismatch`. Loose vectors and matrices, buffer and constant buffer arrays and unknown texture dimensions are named `UnsupportedParameter` errors. See [Two forms of parameters](#two-forms-of-parameters). |
| `ConstantBuffer` of either form and the pass's name table | `unity::constants::{pack_constant_buffer, pack_variant_buffer, pack_layout}` | A `ConstantLayout` with 16-byte registers for vectors, array elements and matrix lines, packed by `pack_constants`. A matrix of dimension `n` writes `n` registers from the property's floats in the order given. A player buffer's struct array becomes members whose elements are the struct's size apart. See [Player structs](#player-structs). |
| `StageProgram` | `unity::targets` | The prepared set and binding that each constant buffer and texture of the stage fills: a `BufferLayout` and a `ParamName`. |
| `Program`, `SubProgram`, `PlayerSubProgram` and keyword indices | `unity::variant` | `select_sub_program` and `select_player_sub_program` choose by exact keyword set through the neutral `select_variant`. A keyword the stage does not declare is ignored. |
| `ShaderState` and property layers | `unity::state::resolve_state` | A resolved target-zero state, which `RenderState::neutral(front_face)` turns into a neutral `RenderState`. State this slice does not draw is a named `StateError`. |
| `DrawRequest`, `DrawPacket`, `DispatchRequest` with Unity's types | `unity::runtime::ProgramRuntime` | A wrapper that dereferences to the neutral runtime and converts stages, programs and states before calling it. Resources, frames, readbacks, arenas and retention are the neutral runtime's own. |

Unity's clip-space and matrix conventions, its target-zero blend behaviour and its policy on unknown keywords are measured compatibility behaviour and stay as they were. The existing suites route through the adapter by import path only.

### Two forms of parameters

An editor-style sub program describes one stage with a name index for every property and a Unity binding for every resource, in no descriptor set. A player variant describes the whole variant: constant buffers with named members (kind, rows, columns, matrix flag, array size, byte offset) and a typed binding list whose descriptor slots carry the binding, the set and the stages that use the resource. Both reduce to the same internal lists, so layouts, constant buffers and targets have one implementation:

- `unity::source::Source` (private) turns either form into the resources a stage declares, its constant buffers and its textures. Layouts (`unity/declare.rs`, `unity/layout.rs`) and targets (`unity/targets.rs`) read only those lists.
- `unity::buffers::BufferLayout` is a constant buffer's members in one form: a `ParamName` (an index into the pass's names, or the name itself), a `Kind`, rows, columns, matrix flag, array size and byte offset. `unity::constants::pack_layout` converts it to a neutral `ConstantLayout` and packs it. `pack_constant_buffer` (editor-style) and `pack_variant_buffer` (player) build the `BufferLayout` and call it.
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

### Player structs

A player variant describes a constant buffer's structs beside its members. `unity::constants::pack_layout` packs the one shape that was measured and refuses every other. `unity-import player-shader-structs` repeats the measurement; it places each struct with `unity::buffers::BufferLayout::struct_placement`, the code that packs, and compares the result with the bound uniform block's SPIR-V type tree (`Offset`, `ArrayStride`, `MatrixStride` and nested struct types).

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
- A member inside a struct is one element's value, so it is not an array itself. Float vectors of one to four components, float matrices of four columns and integer scalars are written as at the top level. Integer vectors of four components (`PropertyValue::IntVector`, `IntVectorArray`) are written inside structs only, where they were measured; at the top level they stay `UnsupportedValue`. The adapter gives the neutral packer one member for each struct member, with the struct's size as the array stride, so the neutral packer knows nothing of structs.
- Named errors: `UnsupportedStruct` for an editor-style struct, a struct in a buffer that has members or another struct, and a struct whose `index` is not 0; `NegativeSize` for a negative array size or struct size; `MemberOutsideStruct` for a member that does not fit within the struct's size; `OutOfBounds` for elements that run past the buffer; `ArrayTooLong`, `WrongKind` and `UnsupportedValue` as for top-level members.

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

## Lending a renderer's resources

A renderer that already owns buffers, textures and samplers can have a program draw with them in place. `prepare_imported_draw(&ImportedDraw, TargetSignature)` returns a `PreparedProgramDraw` like `prepare_draw` does, over:

- **Imports by semantic.** A binding declared with `BindingSource::Engine(EngineSemantic::named(..))` is filled by an `Import` of a buffer, a texture with its view or a sampler. Two declarations of one semantic at different addresses, or a stage pair that disagrees on it, are a `LayoutError::Conflict`. A binding with no import is `MissingImport`, an import no binding takes is `UnusedImport` and one semantic imported twice is `DuplicateImport`, each naming the semantic. The runtime checks an imported buffer against the shader's need, an imported texture's format against the binding's sample type and that the binding is a single-sample 2D texture, a resource of the wrong kind as `WrongResource`, and takes the sampler as given, so a sampler binding can be filtered, clamped or anisotropic as its owner made it. Bindings declared by property name are still filled from `BoundResource`s of the runtime's own handles in the same draw.
- **Borrowed geometry.** `ImportedVertexInput`, `ImportedIndices` and `ImportedGeometry` carry the wgpu buffers themselves. Ranges are checked against the buffers' sizes by the rules of `range.rs`, and an instance-rate buffer must hold as many strides as the end of `ImportedDraw::instances`, so an overrun is `VertexRangePastBuffer` before anything is recorded.
- **Attachments.** `TargetSignature::attachments(&colors, depth, samples)` describes one to `MAX_COLOR_ATTACHMENTS` colour formats and a sample count of 1, 2, 4, 8 or 16. The state gives each attachment a blend and a write mask. An empty mask leaves an attachment as it was, which is how a material that writes location zero shares a pass with attachments that belong to other materials. `TargetSignature::new` still describes the one single-sample attachment of the plain draw.
- **External sets.** `ProgramRuntime::graphics_program_in(vertex, fragment, &[ExternalSet])` lends whole bind group sets that the owner of the pass binds once. `ExternalSet { set, layout, bindings }` carries the renderer's `wgpu::BindGroupLayout`, which goes into the pipeline layout as it is, and a `ResourceLayout` of what the shader reads from the set. Each resource a stage uses at a lent set must be listed in `bindings` at its address with the same class (`ResourceKind::same_class`: buffer kind and access, texture dimension, multisampling and sample type, sampler class, ignoring the filterable flag). A resource that is not listed is `LayoutError::UndeclaredResource` naming the shader's variable, and one listed as another class is `LayoutError::Mismatch`. Listed bindings the shader does not use are allowed, and a stage's own declarations at a lent set are ignored. wgpu checks the rest, such as visibility, when it creates the pipeline, and its refusal is `ProgramError::Gpu`. A prepared draw builds no bind group for a lent set and `record` does not bind it. `PreparedProgramDraw::external_sets()` and `GraphicsProgram::external_sets()` list the sets the pass owner must have bound before recording. The pipeline cache key includes each lent layout's identity, so programs with equal stages and different lent layouts never share a pipeline, while clones of one layout do. A `BoundResource` at a lent set, an import of an engine semantic that a lent binding names, and a set lent twice are `ProgramError::ExternalSetConflict { set }`. [The guide](../../docs/render-program-native.md#sets-the-pass-owner-binds) has a compiled example, and `tests/lent/external.rs` draws two programs over one pass-bound group.
- **Stats.** `ProgramRuntime::stats()` returns `RuntimeStats { modules_built, pipelines_built, bind_groups_built, sets_prepared, draws_prepared }`. Each count only rises and is incremented where the object is created: a shader module in the module cache, a pipeline in the render and compute pipeline caches, a bind group where a program's set is bound, and a draw where a prepared draw is made (`prepare_draw`, materials, admissions and `prepare_imported_draw`). A cache hit or a call refused before it creates the object counts nothing. `sets_prepared` is zero until a call prepares reusable sets. A cost test takes a snapshot before a frame and compares it with one after; `tests/lent/external.rs` predicts the exact counts of a sequence of draws.
- **Retention.** The runtime counts none of the lent resources: their owner has its own accounting or none, and a second count would charge them twice. `ImportedDraw::owners` are `Keepalive` values, `Arc<dyn Any + Send + Sync>`, that the draw holds for as long as the draw or a frame that called `PreparedProgramDraw::retain` lives, through the `FrameOwner` that holds a runtime's own reservations. A cancelled frame gives them back at once and a submitted one when the GPU has completed it. A lender whose resources must outlast a frame that is still being recorded or is in flight passes the object that owns them as an owner.

## Measured conventions

The tests in `tests/program/conventions.rs` draw into an 8x8 target through the adapter and compare bytes. They pin what a Unity Vulkan module needs, which holds for any SPIR-V stage:

- The module's position output is used unflipped. Positive clip Y lands in the bottom rows of the image.
- The depth range that survives clipping is 0 to w, as in Vulkan; the nearer of two triangles wins under `LessEqual`, and z of -0.5 or 1.5 is clipped.
- `wgpu::FrontFace::Cw` marks a triangle that is clockwise in the image, with row 0 at the top, as a front face. Cull Back drops the others and Cull Front drops the front faces. The winding is `RasterState::front_face` in the neutral state and a field of the adapter's draw request, so a caller states it.

`tests/program/pass.rs` and `drawn_state.rs` draw a textured pass with a tint from a constant buffer, the multiply and add keyword variants, a split combined sampler, blend off against `SrcAlpha`/`OneMinusSrcAlpha`, and cull Back against Off for a back-facing quad. Pixels are predicted in closed form from the texels, tint and offset, independent of the shader, and compared with the 8-bit bound from `worldrunner_render_gpu::tolerance_test_support` (0.625 steps). The alpha-blend test adds 0.5 step because lavapipe's fixed-point blend measured 0.8 steps from the prediction; the Radeon stayed within the shared bound.

## Not done

- **Engine values.** `BindingSource::Engine` names a binding the caller fills with a resource it already owns (below). There is no value provider for scalar and matrix engine values: a native game sets the matrices, times and environment resources its shaders read as ordinary properties. Computing object, camera and light matrices, time and shadow or environment values from a scene is not done.
- **Releasing programs.** A material can be released; a registered program lives as long as the runtime or a material that uses it.
- **Per-variant state.** A program has one render state for all its variants.
- **Struct arrays and specialization.** A constant layout holds scalar, vector and matrix members. Struct arrays and WGSL override constants are not represented.
- **Unity sampler state.** Every sampler binding gets one fixed sampler: linear filtering, clamped to the edge. The packed sampler state in the serialized parameters is not decoded, so wrap, filter and anisotropy settings are ignored.
- **Multisampling, stencil and multiple render targets for the plain and material draws.** `draw`, `prepare_draw`, material draws and `admit_draws` are single-sample with one colour attachment and an optional depth attachment, and refuse other signatures by name. Stencil state and a stencil depth format are refused everywhere. Only draws over lent resources record into several attachments or a multisampled pass, and `render-scene` takes those only for dynamic quads.
- **Frame planning and composition beyond one pass.** The runtime still owns no pass order and no attachments. `render-scene` takes admitted draws into its stock opaque pass for one single-sample colour attachment and `Depth32Float`. It refuses scenes with temporal reconstruction, screen-space passes or multisampling by name. Camera render textures, sampling a draw's target in a later pass, compute between passes, transparent ordering, the depth-fitted capture continuation and `render-plan` descriptions of program work are not done. The standalone target that `create_render_target` makes remains the caller's to create and release.
- **Calling shader preparation.** `unity_serialized::spirv_prep::prepare_stages` is in the Foundation layer, which this crate may depend on, but the runtime does not call it. Callers supply the prepared words and `PreparedModule::bindings` of each stage, prepared together with the variant's other stages. `unity-import player-shader-layouts` does this for every Vulkan variant of a player.
- **Multisampled textures, storage images and compute masks.** Dimension codes 5 and 11 are multisampled images, which are named errors because the adapter declares no multisampled player texture. A storage image is a named error. A compute stage's bit in the stage mask has not been seen, so compute ignores the mask.
- **Player struct shapes beyond the measured one.** A struct that starts anywhere but byte 0, a buffer with a struct and members, a buffer with several structs, and editor-style structs are `ConstantError::UnsupportedStruct`, because the player has none: all 5,605 buffers with structs are one struct array at byte 0 and nothing else. Whether `index` is a byte offset, a position or an ordinal is therefore open. Structs inside structs and arrays inside structs are not seen (the SPIR-V struct types hold none) and are refused as unsupported values. Which property the engine fills the struct members from (the per-draw instance data) is not decided here: a caller supplies them under the members' own names.
- **Player member kinds and matrix shapes.** Only float (0) and int (1) member kinds and matrices of four columns are written. Every one of the 201,382 members of the Old World player's bound buffers is one of those (float scalars, vectors and 4x4 matrices, with or without an array, and int scalars), and so are the 10,931 members of its structs, which add int4. So a bool, unsigned, narrower matrix or int vector at the top level is a named error that the player checked never reaches.
- **Prepared numbering across stages.** The stages of a variant must come from `spirv_prep::prepare_stages`, which gives every stage one binding table. Modules prepared one at a time by `prepare` number the same Unity binding differently when they split different samplers, and `ProgramLayout::build` then reports `LayoutError::Conflict`: in the Old World player that was 1,350 of 10,768 two-stage variants, and with the stages prepared together none of them is, and all 10,768 share one numbering. `prepare_stages` fails with `PrepareError::BindingClass` when stages declare one Unity binding as different classes of resource; no variant in that player does. The table lists every binding any stage declares, so a stage's table can hold entries its module does not declare: a parameter that names such a binding for the stage is reported in `ProgramLayout::unused`, not as `LayoutError::UnboundSlot`.

## Tests

The suites that build Unity records (`program` and `variant`) run only with the `unity` feature:

```sh
cargo test -p worldrunner-render-program --features unity -- --test-threads=1
```

Without the feature, `cargo test -p worldrunner-render-program` runs the unit tests, `tests/native.rs`, `tests/imports.rs`, `tests/targets.rs`, `tests/lent/` and `tests/material/`, builds `examples/native_material.rs` and runs the guide's code blocks as doctests. `cargo xtask test` enables every feature.

`tests/program/player_structs.rs` packs struct arrays at the byte level from hand-built player descriptions, including every named error. `tests/program/player_struct_block.rs` builds a uniform block of an array of structs with naga from WGSL, predicts its layout by hand, reads the `Offset` and `ArrayStride` decorations naga wrote back from the SPIR-V, compares the packed bytes with them and runs a compute kernel on the GPU that reads each element.

The draw, dispatch and layout tests in `tests/program` run once with editor-style parameters and once with a player variant's, from equivalent hand-built descriptions, and must agree.

`tests/program/packet.rs` covers preparation and recording into a caller's pass, and `arena.rs` covers admission, authority, lifetime and budget refusals. The frame tests in `render-scene` enable the feature as a dev-dependency and run with `cargo test -p worldrunner-render-scene --lib -- program_frame --test-threads=1`.

The draw and dispatch tests need a Vulkan adapter and use the one wgpu selects. They obtain the device through `render-gpu` (`headless_instance`, `request_device`), build a `SubmissionAuthority` and submit every frame through it. The first test to create a runtime prints the adapter's name to stderr; run with `--nocapture` to see it. Set `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json` to run them on lavapipe.
