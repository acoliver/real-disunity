# Format-neutral shader program runtime

`worldrunner-render-program` builds layouts and pipelines for shader programs, binds their resources and records draws and dispatches with wgpu.
It sits in the Graphics layer.
Its root modules describe programs in engine terms and name no shader file format.
A default-off `unity` module converts Unity player programs into those descriptions, and the runtime executes the result.

## Neutral model

A caller states each stage's code, the resources and constants the stage declares, and the fixed-function state of a draw.
Nothing below mentions a source format.

- Stage module
  - Types: `module::StageModule`, `StageSource::{SpirV, Wgsl}`, `StageKind`
  - What they say: Code, entry point and stage.
    SPIR-V keeps naga's Y flip, which turns its clip space into the one wgpu draws with.
    WGSL is used as written.
- Declarations
  - Types: `module::StageDeclarations`
  - What they say: The `ResourceLayout` and the `BoundConstantLayout`s of one stage.
- Resource layout
  - Types: `binding::{BindingAddress, ResourceBinding, ResourceKind, BindingSource, PropertyId}`
  - What they say: Set, binding, visibility and kind: uniform or storage buffer (with access and a minimum size), texture (dimension, sample type, multisampled) and sampler (filtering, non-filtering or comparison).
- Constant layout
  - Types: `constants::{ConstantLayout, Member, ScalarKind, ArrayLayout, MatrixLayout}`, `pack_constants`
  - What they say: Named members with kind, rows, columns, array count and stride, byte offset and matrix line stride and order.
    A signed integer member is a scalar or a vector of four components, and an array of either.
- Render state
  - Types: `state::{RenderState, RasterState, DepthState, ColorState, StencilState}`
  - What they say: Cull, front face, depth test, write and bias, one entry per colour target (blend and write mask) and stencil.
- Variants
  - Types: `variant::{FeatureId, FeatureSet, Variants, select_variant, UnknownFeatures}`
  - What they say: A program per exact feature set.
- Property values
  - Types: `properties::{PropertySet, PropertyValue}`, `runtime::{Resource, BoundResource, BufferHandle, TextureHandle, TargetHandle}`
  - What they say: Floats, integers, unsigned integers, booleans, vectors, integer vectors, matrices, arrays and resource handles, looked up by name in layers where the first layer with a name wins.

Execution consumes only these values.
`layout::ProgramLayout::build` checks each stage's declarations against the resources its entry point uses, `constants::pack_constants` fills a buffer, `ProgramRuntime::graphics_program` and `compute_program` build the wgpu layouts, and `draw`, `prepare_draw`, `admit_draws` and `dispatch` record the work.

- **Layouts.** A resource the entry point uses must be declared at its address, as the kind the shader has and visible to the stage that uses it.
  A binding declared twice, a declaration of another kind, a declaration the stage cannot see and stages that declare one binding differently are named `LayoutError`s.
  A declaration the entry point does not use is listed in `ProgramLayout::unused` and gets no slot.
  A slot is as large as the larger of the shader's need and the declared minimum size, and its visibility is the declared one.
- **Constants.** Offsets, array strides and matrix line strides come from the layout, so no packing rule of a shader language is built in.
  A vector writes its leading columns.
  `PropertyValue::Matrix` holds element `(row, column)` at index `column * 4 + row`, and the layout's storage order places the logical values.
  An unsigned member takes `Uint` or a non-negative `Int`, and a boolean member takes `Bool` or an `Int` of 0 or 1.
  A signed integer member takes `Int` (when it has one component), `IntVector`, which writes its leading `cols` components, or an `IntVectorArray`, one element per array element.
  An unsigned or boolean member of an array takes one value, which is element 0's.
  A member that is missing from the layers stays zero and is listed in `PackedConstants::missing`.
  A member outside the buffer, a value of another kind, an array longer than its member and a member shape the packer does not write are named `ConstantError`s.
- **Variants.** `Variants` matches the enabled features, restricted to the ones its programs differ by, against an exact set.
  `UnknownFeatures::Fail` names an enabled feature that no program differs by, and `Ignore` leaves it out.
- **State.** A draw through `draw`, `prepare_draw` or a material writes one colour target.
  Stencil, several colour targets and an unwritten colour target are representable and refused by name (`ProgramError::UnsupportedState`) when such a draw is prepared, and alpha to coverage is refused as `ProgramError::AlphaToCoverageNeedsSamples`, because these draws have one sample.
  A draw over lent resources (below) checks the state against its signature with `RenderState::check_targets(colors, samples)`, which takes one `Some` colour state per attachment, with any write mask, and admits alpha to coverage for a signature of more than one sample.
  A state without depth tests nothing and writes no depth.
  A depth bias is drawn and is part of the pipeline's key.
  Every pipeline is also checked against its fragment stage: a colour target with a write mask that is not empty needs an output at its location, or the draw fails with `ProgramError::TargetWithoutFragmentOutput`, because wgpu builds such a pipeline and leaves the attachment undefined.
  An output with no target is allowed, as wgpu discards it.
  The offscreen replay checks its draws against the same rule after giving a colour attachment its fragment stage has no output for an empty write mask, which leaves that attachment as it was.
- **Draws.** `ProgramRuntime::draw` records non-indexed or indexed triangles with a vertex layout the caller gives, into a colour target with an optional depth target.
  `read_target` records a copy of the pixels.
  Render and compute pipelines and shader modules are cached by content, and the pipeline key includes the stage sources, the layout, the identity of each lent layout, the whole state and the target signature.
- **Compute.** `ProgramRuntime::dispatch` records a kernel; `read_buffer` records a copy of a storage buffer.
  A kernel may also read and write storage images (see below).
- **Samplers.** Every sampler binding gets one fixed sampler: linear filtering, clamped to the edge.
  A sampler declared non-filtering gets the runtime's nearest-texel one.
  A comparison sampler is a named `UnsupportedSampler` error at layout creation unless its binding is sourced from an engine semantic and the owner imports it.

`tests/native.rs` drives this interface with WGSL stages and no Unity record, and runs without the `unity` feature.

## SPIR-V typed buffer images

Before naga parsing, `StageSource::SpirV` lowers read-only typed buffer fetches to
ordinary storage buffers at the original descriptor set and binding.
Declare the
slot as `ResourceKind::Storage { access: StorageAccess::ReadOnly, min_size }` and
bind a plain buffer containing tightly packed elements.
No texture view is needed.
The exact supported formats are `R32f/i/ui`, `Rg32f/i/ui` and `Rgba32f/i/ui`, with
element strides of 4, 8 and 16 bytes respectively.
Missing fetched components are
filled as Vulkan specifies: `(x, 0, 0, 1)` or `(x, y, 0, 1)`, using integer zero
and one for integer formats.
In-range fetches preserve the element bits; callers
must provide the same valid element indices as the source shader requires.

`ModuleError::TexelBuffer { set, binding, what }` names unsupported operations:
writes and storage buffer images, size queries, sparse fetches, descriptor arrays,
arrayed images, image function parameters, `OpCopyObject`, other non-fetch uses,
and fetch operands beyond an absent or empty image-operands mask.
Formats needing
conversion, including Unknown, 8/16-bit, normalized, packed and 64-bit formats,
are refused rather than approximated.
The pass changes neither source hashes nor
stage hashes.
Modules without a Buffer image borrow their original words unless
they declare unused `SampledBuffer` or `ImageBuffer` capabilities, which naga
rejects.
In that case, only those capability instructions are removed from an
owned copy; every other word stays unchanged.
Modules with Buffer images still
undergo the operation and format checks above before any capability is removed.

The pass uses `rspirv` (MIT/Apache-2.0) for operand-aware parsing and emission.
The owned HLSL and committed DXC fixtures in `tests/fixtures/texel_buffer/` need
no compiler in CI.
Regenerate them with
`python3 crates/render-program/tests/fixtures/texel_buffer/regenerate.py /path/to/dxc`;
the script prints each exact command.
CPU tests cover lowering and reflection.
`texel_buffer::gpu_typed_buffer_fetches_match_storage_buffers` compares
all nine element shapes with WGSL storage-buffer dispatches on the selected GPU.
`texel_buffer::gpu_missing_components_match_vulkan_defaults` checks all four
returned channels for the six scalar/two-component formats.
Its fixtures expose
the raw fetched vec4 with a documented post-DXC edit, because HLSL otherwise
reconstructs or discards the missing channels.

## SPIR-V non-finite constants

Compilers write NaN and infinity as `OpConstant` bit patterns, alone and inside
`OpConstantComposite`; a vertex stage that discards an instance writes a position of
`vec4(NaN)` this way. naga turns every constant into a constant expression and its
validator refuses a non-finite float literal in any expression, so `StageProgram::new`
used to fail with `ConstExpression { source: Literal(NaN) }`.
SPIR-V passthrough is not
an option because the same stages must also reach Metal through naga's MSL writer.

After the typed buffer pass and before naga parses the words, `StageSource::SpirV`
removes each non-finite 32-bit `OpConstant` and each `OpConstantComposite` that
contains one.
Every function that used a removed constant builds it at the top of its
entry block: a local `u32` variable initialised with the bit pattern, an `OpLoad` and an
`OpBitcast` to the float, and for a composite an `OpCompositeConstruct` of its parts.
Finite parts keep their original constants.
A local variable initialised with a removed
constant is stored to after the value exists.
The bits are unchanged: quiet NaN
payloads, signs and both infinities arrive at the GPU shader compiler as written.

naga 30.0.1's SPIR-V front end appends a function's expressions without evaluating
them, and its constant evaluator reports a bitcast as not implemented, so a bitcast
of a constant would probably not be folded today.
The rewrite does not depend on that:
the pipeline-constant pass evaluates every function expression whose operands are
constants, and a load of a local variable is never one, so the value stays a run-time value and is
never turned back into the refused literal.

A module that is rewritten loses its source-line markers and its debug-info records:
every `OpLine` and `OpNoLine`, every `OpExtInst` of the `NonSemantic.Shader.DebugInfo.100`
set, and the `OpName` of those records' results.
Debug and line attribution is therefore
not preserved for rewritten modules. naga reads none of these records, and the rewrite
cannot keep them: rspirv moves an `OpLine` that precedes an `OpLabel` out of its
function, and it refuses an `OpExtInst` outside a function.
A marker among an entry
block's variables would also split the run of leading variables whose initialisers the
rewrite moves.
The import, the `SPV_KHR_non_semantic_info` extension and every other
instruction stay.
Records of other `NonSemantic` sets are not touched; naga refuses
those sets anyway.

Modules without a non-finite 32-bit `OpConstant` borrow their words and are returned
byte for byte, line markers and debug records included, and so are the modules that
are declined below.
Limits that leave the module to naga, which refuses it as before:

- Specialization constants are never rewritten.
  `OpSpecConstant` with a non-finite
  default stays an override with a refused literal, because a pipeline may replace its
  value and nothing computed in a function would honour that.
- A non-finite constant that a global variable initialiser or a specialization constant
  instruction uses leaves the whole module unchanged.
- A module in which an instruction that stays still reads the result of a removed
  debug-info record is left unchanged, so no reference is left without its definition.
- 64-bit and 16-bit constants are not touched.
- Hardware and driver behaviour of NaN is the device's: a driver that compiles with
  fast-math may treat NaN and infinity as undefined.
  The tests run on lavapipe only;
  naga's MSL writer translates the rewritten modules, which no test runs on Metal.

`src/nonfinite/tests.rs` covers admission, byte-identical pass-through, the refused
cases and MSL translation on the CPU.
`src/nonfinite/tests/debug_records.rs` covers line
markers and debug-info records, and `src/nonfinite/tests/guards.rs` covers nested
composites shared by functions and used by an `OpPhi`, and specialization constants.
`tests/nonfinite.rs` runs synthetic modules on the selected GPU: a NaN position discards
its primitives while the others draw, and scalars, mixed composites, local initialisers
and constants shared by two functions keep their exact bits through a storage buffer.
The modules are WGSL compiled to SPIR-V with
finite sentinel constants whose words are replaced by the non-finite patterns
(`tests/support/sentinel.rs`), because no naga module can hold such a literal.

## The native API

A Rust game registers programs and makes materials without building stages, layouts and bindings by hand.
The calls are on `ProgramRuntime` and take no Unity type. [Drawing with your own shader programs](../../docs/render-program-native.md) is the guide, with a complete example that `cargo run -p worldrunner-render-program --example native_material` runs and that CI compiles.

- `register_program(ProgramDesc) -> ProgramHandle`: Parses and validates each variant's stages (`StageDesc::wgsl` or `StageDesc::spirv`), checks declarations, builds layouts, checks the state is drawable and derives the property schema.
  A failed registration publishes no handle.
- `create_material(ProgramHandle, PropertySet) -> MaterialHandle`: A material with initial properties and no feature enabled.
- `set_property(material, name, PropertyValue)`: Typed values and `Resource` handles for buffers and textures.
  An unknown property is `UnknownProperty` and a value of another kind is `WrongPropertyKind`, both naming the property.
  A refused call leaves the material as it was.
- `set_features(material, &[&str])`: Replaces the enabled features and checks that a variant has exactly that set.
  An unknown feature is `VariantError::UnknownFeature` and an unmatched set is `NoMatch`.
  A refused call keeps the old features.
- `prepare_material_draw(frame, &MaterialDraw) -> MaterialPacket`: Selects the variant, packs the constants into buffers the packet and the frame share, and takes the resources the properties name.
  The packet is a snapshot: later edits and `release_material` do not change it.
- `record_material(frame, target, &MaterialPacket)` and `draw_material(frame, target, &MaterialDraw)`: Record into a pass over a render target through the same path as `draw`.
- `admit_material_draws(frame, &ProgramArena, TargetSignature, &[MaterialPacket]) -> AdmittedPrograms`: The frame-join path of `admit_draws`, with each packet's instance range.

- **Layers.** A draw's `overrides` come first and the material's properties second.
  A name found in the first layer wins.
- **Schema.** A program's property names and the kind of each come from its constant members and from the buffer and texture bindings that name a property.
  Each value is checked by packing it into a probe of its member, so the packer stays the one authority on what a member takes.
  Two declarations that give one name different kinds make registration fail with `PropertyConflict`.
- **Constants.** Every constant layout of the selected variant is packed with `pack_constants`, from the layers, into a buffer of its own for each prepared draw.
  Both stages may declare the same layout at one address and must declare it identically.
  A member with no value is zero and is listed by `MaterialPacket::missing`.
- **Retention and budget.** The buffers are admitted against the runtime's budget like any other allocation, and so under the open arena.
  They are not in the handle table.
  The packet and the frame that prepared it share their reservations, and the frame keeps them counted until it is cancelled, dropped or completed, so the retention rule below covers them with no release call.
- **Instances.** `MaterialDraw::instances` is the range of instances drawn.
  An instance-rate vertex buffer must hold as many strides as the range's end, and a backwards range is `InstanceRange`.
  The plain `DrawPacket` and `DrawRequest` still draw one instance.
- **Storage images.** A shader's storage texture reflects as `wgpu::BindingType::StorageTexture` with the access its declaration allows (`ReadOnly`, `WriteOnly`, or `ReadWrite` when it both loads and stores; an image declared for both that no entry point of the module loads from is declared for stores alone when the module is parsed and reflects `WriteOnly`, since DXC declares every `RWTexture2D` both ways and Metal binds some formats, such as `Rg32Uint`, for writes only; an image passed to a function counts as loaded), its format and the view dimension (1D, 2D, 2D array or 3D; cube images and atomic access are named `UnsupportedResource` errors).
  A declaration is `ResourceKind::StorageImage { access, format, dimension }` and must equal what the shader says, as every other kind does.
  A texture bound to one is checked before the dispatch records: its format is the shader's (`StorageImageError::Format`), it allows storage binding (`Usage`), has one sample per texel (`Multisampled`) and has the shape the view takes (`Dimension`: a 2D view needs a 2D texture of one layer, or one layer of a 2D array lent through a view stated as `D2`, an array view a 2D texture, a 3D view a 3D texture).
  The binding is the texture's first mip level only, and a lent `ImportedTexture` is checked the same way and must state the slot's dimension; a lent view of several mip levels is wgpu's to refuse.
  The device must also bind the format with the access, and `ProgramRuntime::supports_storage_image` asks it by trying a layout entry and a one-texel texture, once for each pair, so a program whose image the device cannot bind is refused when it is built (`StorageImage` with `Unsupported`).
  `create_storage_texture(label, [width, height, layers or depth], dimension, format)` makes a zeroed texture that kernels bind and draws sample, and `check_storage_images` runs the checks without recording.
  A subresource that a pass writes as a storage image cannot also be bound in that pass as another resource, because wgpu rejects the usage conflict at the end of the pass; the runtime refuses it before recording as `AliasedTexture`, naming both slots. wgpu tracks usage per mip and layer, so the runtime judges the overlap by subresource: views of one texture whose ranges (`ImportedTexture::range`: mips, layers and aspect) do not overlap may be bound in one dispatch while one of them is written, as a single-pass downsampler binds its source mip and the mips it writes, while overlapping ranges, one subresource sampled and written, and one subresource written through two slots stay `AliasedTexture`.
  A runtime texture's slots, and a lent view that states no range, cover the whole texture.
  Order inside a frame is the encoder's: wgpu inserts the barrier between a dispatch's compute pass and the later render pass, copy or readback that uses the texture, so the writes are visible to all of them in the same submission.
  `tests/storage.rs` writes a pattern from a kernel into an `Rgba8Unorm` texture, draws it into a target and reads both back, checks array, 3D, read-only and read-write images with a second kernel and a readback, and pins each refusal.
  `tests/lent/layer_view.rs` pins a one-layer `D2` view of a four-layer array taken by a 2D storage image, and the whole array refused; `tests/lent/subresources.rs` pins distinct mips of one texture in one dispatch (accepted, recorded on a no-op device, and reported written mip by mip) and the overlapping, read-and-written, written-twice and rangeless cases refused.
- **Handles.** A handle carries the number of the runtime that made it, so another runtime refuses it as `UnknownHandle`.
  A material keeps its program alive.

`tests/material/` drives the API without the `unity` feature: a full-target triangle of colour `[0.25, 0.5, 0.75, 1]` with codes predicted as `round(255 * c)` under the shared quantization bound, a feature-selected variant that doubles red, property edits against a packet prepared earlier, an asymmetric scene with depth that pins the WGSL clip convention, instance ranges, texture and buffer properties, SPIR-V stages, registration refusals, and packets in a caller's frame and arena.

## The Unity adapter

`render_program::unity` exists with the `unity` feature, which is off by default and enables the `worldrunner-unity-serialized` dependency.
It is the only code in the crate that names that crate.
The neutral types mean the same thing with the feature on or off.
Everything the adapter converts is data the importer has already prepared.
The crate does not read bundles or translate shaders.

- Prepared SPIR-V words, entry point and binding table (`unity_serialized::spirv_prep::ResourceBinding`, re-exported as `unity::module::ResourceBinding`)
  - Adapter module: `unity::module::{StageModule, StageProgram}`
  - Converted to: A neutral `StageModule` and, when a layout is built, `StageDeclarations` with each parameter placed at its prepared set and binding.
    A split combined sampler becomes a texture and a sampler binding.
    Every stage of a variant has the same table, from `spirv_prep::prepare_stages`.
- `ProgramParameters` or `VariantParameters` (`unity::module::StageParameters`)
  - Adapter module: `unity::layout::ProgramLayout`
  - Converted to: Resource declarations, each completed from the module's own binding when Unity's class matches it.
    A class that does not match is Unity's `Mismatch`.
    Loose vectors and matrices, buffer and constant buffer arrays and unknown texture dimensions are named `UnsupportedParameter` errors.
    See [Two forms of parameters](#two-forms-of-parameters).
- `ConstantBuffer` of either form and the pass's name table
  - Adapter module: `unity::constants::{pack_constant_buffer, pack_variant_buffer, pack_layout}`
  - Converted to: A `ConstantLayout` with 16-byte registers for vectors, array elements and matrix lines, packed by `pack_constants`.
    A matrix of dimension `n` writes `n` registers from the property's floats in the order given.
    A player buffer's struct array becomes members whose elements are the struct's size apart.
    See [Player structs](#player-structs).
    A scalar member takes the number of a value of the other number kind, because Unity holds one number under a name: an integer up to 2^24 in size in a float member, and a float that is a whole number in range in a signed, boolean (0 or 1) or unsigned member.
    The adapter converts these before the neutral packer, which refuses a value of another kind.
    Anything else is `WrongKind`.
- `StageProgram`
  - Adapter module: `unity::targets`
  - Converted to: The prepared set and binding that each constant buffer and texture of the stage fills: a `BufferLayout` and a `ParamName`.
- `PlayerSubProgram` and keyword indices
  - Adapter module: `unity::variant`
  - Converted to: `select_variant` chooses the variant whose keyword set equals the enabled keywords the stage declares, through the neutral `select_variant`.
    A keyword the stage does not declare is ignored.
    `declared_player_keywords` declares the keywords a pass's variants use, not the program's state mask.
    The enabled keywords are completed with the lowest-indexed keyword of each required group (two or more keywords that never occur together and of which every variant has exactly one) they have no member of.
    A set that has a variant already has a member of each such group, so completing it changes nothing.
    Any other missing set is `NoMatch`.
- `ShaderState` and property layers
  - Adapter module: `unity::state::resolve_state`
  - Converted to: A resolved target-zero state, which `RenderState::neutral(front_face)` turns into a neutral `RenderState`.
    State this slice does not draw is a named `StateError`.
- `DrawRequest`, `DrawPacket`, `DispatchRequest` with Unity's types
  - Adapter module: `unity::runtime::ProgramRuntime`
  - Converted to: A wrapper that dereferences to the neutral runtime and converts stages, programs and states before calling it.
    Resources, frames, readbacks, arenas and retention are the neutral runtime's own.

Unity's clip-space and matrix conventions, its target-zero blend behaviour and its policy on unknown keywords are measured compatibility behaviour and stay as they were.
The existing suites route through the adapter by import path only.

### Two forms of parameters

An editor-style sub program describes one stage with a name index for every property and a Unity binding for every resource, in no descriptor set.
A player variant describes the whole variant: constant buffers with named members (kind, rows, columns, matrix flag, array size, byte offset) and a typed binding list whose descriptor slots carry the binding, the set and the stages that use the resource.
Both reduce to the same internal lists, so layouts, constant buffers and targets have one implementation:

- `unity::source::Source` (private) turns either form into the resources a stage declares, its constant buffers and its textures.
  Layouts (`unity/declare.rs`, `unity/layout.rs`) and targets (`unity/targets.rs`) read only those lists.
- `unity::buffers::BufferLayout` is a constant buffer's members in one form: a `ParamName` (an index into the pass's names, or the name itself), a `Kind`, rows, columns, matrix flag, array size and byte offset.
  `unity::constants::pack_layout` converts it to a neutral `ConstantLayout` and packs it.
  `pack_constant_buffer` (editor-style) and `pack_variant_buffer` (player) build the `BufferLayout` and call it.
- The binding table is `unity_serialized::spirv_prep::ResourceBinding`.
  A player binding names a set and a binding in Unity's module, before the sampler split, and `entry` finds the table entry with that set and Unity binding.
  Editor-style bindings name no set, so the Unity binding must be unique across sets (`LayoutError::AmbiguousBinding` otherwise).
- A texture binding declares the image at the entry's prepared binding.
  When the entry's combined image sampler was split, the split sampler is declared too.
  A texture's sampler slot and a sampler binding declare a sampler at the entry's split sampler binding, or else at its own prepared binding.
  This is why a texture that names its own slot as its sampler, a texture with no sampler slot and a combined image sampler that was split all lay out alike.

This crate runs SPIR-V only.
On macOS the engine uses the Linux build's data, because the macOS build of the same Steam build ships no shader with a Vulkan platform (the Linux and Windows builds ship 171), and loading the macOS build's shaders fails with `PlayerError::NoSpirvPrograms` ("no SPIR-V programs; Metal programs unsupported").

### Player bindings

- Texture, dimension 4, 6, 8, 10
  - Declared as: 2D, 3D, cube, 2D array texture
  - Notes: Measured against the SPIR-V of the Old World player.
    Not multisampled
- Texture, dimension 5 or 11
  - Declared as: `LayoutError::MultisampledTexture`
  - Notes: Measured as a multisampled 2D and a multisampled 2D array image (6 and 3 bindings in the Old World player).
    This renderer has no multisampled textures
- Texture, any other dimension
  - Declared as: `LayoutError::UnsupportedParameter`
- Constant buffer
  - Declared as: Uniform buffer
  - Notes: Its layout is the `ConstantBuffer` of the same name; `LayoutError::UndescribedBuffer` if there is none
- Buffer block
  - Declared as: Storage buffer, read-only or not as the module says
- Storage image
  - Declared as: `LayoutError::UnsupportedParameter`
  - Notes: One instance in the player
- Sampler
  - Declared as: Filtering or comparison sampler as the module says
  - Notes: The packed state is not decoded

The stage mask is checked against the module's stage.
Bit 0 is the vertex stage and bit 1 the fragment stage:

- A binding that names other stages only is neither bound nor reported unused, and an unsupported binding among them does not fail the stage.
- A resource the module uses that the parameters bind for other stages only is `LayoutError::StageMismatch`.
- A slot's visibility is the stages in its mask plus the module's own stage.
- A binding that names the stage but that the entry point does not use is reported in `ProgramLayout::unused` and gets no slot.
- A compute stage has no bit in the mask, because none has been seen, so the mask is not consulted for it.

### Player structs

A player variant describes a constant buffer's structs beside its members.
`unity::constants::pack_layout` packs the one shape that was measured and refuses every other.
`unity-import player-shader-structs` repeats the measurement; it places each struct with `unity::buffers::BufferLayout::struct_placement`, the code that packs, and compares the result with the bound uniform block's SPIR-V type tree (`Offset`, `ArrayStride`, `MatrixStride` and nested struct types).

Measured on the Old World player (Unity 6000.3.15f1), counting each bound constant buffer of each variant, as `player-shader-layouts` does:

- Where does a struct instance start?
  - Answer: The struct's `index` equals the `Offset` of the block member that holds it.
    That member is the block's only member, at `Offset` 0, so `index` is 0
  - Buffers or members it covers: 5,605 of 5,605 buffers
- How does `index` relate to the buffer's member list?
  - Answer: It does not index it.
    Every one of these buffers has no members of its own, so a position in the member list, an ordinal among structs and a byte offset all give 0 and cannot be told apart
  - Buffers or members it covers: 5,605 of 5,605 buffers have zero members and one struct
- Are a struct member's offsets from the struct's start or from the buffer's start?
  - Answer: From the start of an element.
    The elements are 2 or 256 in number and share one member list, and all 10,931 members end within the struct's size, which an offset from the buffer's start could not do for element 1.
    Each offset is an `Offset` of the SPIR-V struct type
  - Buffers or members it covers: 10,931 of 10,931 members
- What is the stride of a struct array?
  - Answer: The struct's `size`.
    It is the block array's `ArrayStride` and the SPIR-V struct type's span, and the array size is the block array's length.
    The buffer's size is the array size times the struct size and the block's span
  - Buffers or members it covers: 5,605 of 5,605 buffers
- Can struct members collide with the buffer's own members?
  - Answer: Not observable: no bound buffer has both. 0 of 5,605 has any member of its own
  - Buffers or members it covers: 0 of 5,605 buffers
- In what order are the members listed?
  - Answer: Not the type's order in 544 buffers.
    The offsets are the same set, so members are found by offset, not by position
  - Buffers or members it covers: 5,061 of 5,605 buffers in the type's order

The rule explains all 5,605 buffers and all 10,931 struct members: 3,808 float4 members, 4,124 float 4x4 matrices and 2,999 int4 members.
A matrix is not a SPIR-V matrix type there: it is an array of four float4 at stride 16, the same four rows of 16 bytes that the packer writes for a top-level matrix.
The int4 is a `uint4` in the module; the packer writes the 32 bits of an `i32`.

Names.
Of 10 distinct struct member names and 3 distinct struct names, none is a material property name, none is another buffer's top-level member name and none is a struct name. 9 of the 10 member names are another buffer's top-level member name, or a material property name, followed by one and the same suffix.
So a struct member is named by its own name, not by the struct's name plus the member's name; nothing in the player says which property name the engine itself fills these members from.

What the packer does with this:

- The buffer must have one struct, no members of its own, and that struct's `index` must be 0.
  The struct then starts at byte 0.
- Element `e` of the struct array is at `e * size`.
  A member is at its offset within the element.
  A struct that is not an array (array size 0 or 1) is one element; that case was not seen in the player.
- A property named by the member's own name supplies it.
  A vector, matrix or integer-vector array property gives one value per element, so element `e` takes item `e`.
  A single value is element 0's.
  Elements the array does not reach, and members no layer names, stay zero; a missing member is listed once in `PackedConstants::missing` however many elements the struct array has.
- A member inside a struct is one element's value, so it is not an array itself.
  Float vectors of one to four components, float matrices of four columns and integer scalars are written as at the top level.
  Integer vectors of four components (`PropertyValue::IntVector`, `IntVectorArray`) are written inside structs only, where they were measured; at the top level they stay `UnsupportedValue`.
  The adapter gives the neutral packer one member for each struct member, with the struct's size as the array stride, so the neutral packer knows nothing of structs.
- Named errors: `UnsupportedStruct` for an editor-style struct, a struct in a buffer that has members or another struct, and a struct whose `index` is not 0; `NegativeSize` for a negative array size or struct size; `MemberOutsideStruct` for a member that does not fit within the struct's size; `OutOfBounds` for elements that run past the buffer; `ArrayTooLong`, `WrongKind` and `UnsupportedValue` as for top-level members.

## The boundary check

`cargo xtask policy` includes a check of this crate's source, in `crates/xtask/src/policy/boundary.rs`.
Outside `src/unity/`, no path, import, re-export, `extern crate`, attribute or macro input may name `worldrunner_unity_serialized`, however it is renamed or grouped.
No module may name the `unity` module, so a root module can neither expose an adapter type nor re-export the adapter, and `lib.rs` declares the module once behind `#[cfg(feature = "unity")]`.
The manifest may not rename the dependency.

The check reads the workspace manifests too.
A workspace crate's normal or build dependency on `worldrunner-render-program` may enable the `unity` feature only if the crate is `worldrunner-play` or `worldrunner-unity-import`.
Enabling it means listing `unity` in `features`, listing a feature that implies `unity`, or leaving `default-features` on while `unity` is in this crate's default set.
The rule reads the manifest as `cargo metadata` resolves it, so it holds for workspace-inherited dependencies, renamed ones and target-specific tables, which count as normal dependencies.
A dev-dependency may enable `unity` in any crate: cargo applies its features to that crate's own tests and examples only, which is how `render-scene` runs its program-frame tests.
The rule keeps that dev-only use from reaching a production build through feature unification.
`crates/xtask/src/policy/boundary/tests.rs` holds the negative fixtures for each form.

`cargo check -p worldrunner-render-program --no-default-features` builds the neutral runtime, and `cargo tree -p worldrunner-render-program --no-default-features -e normal,build` lists no `worldrunner-unity-serialized`.

## Driving it through the submission authority

The runtime shares the caller's `wgpu::Device` and `worldrunner_render_gpu::submission::SubmissionAuthority`.
It has no queue of its own and never submits.
`draw`, `dispatch`, `clear_target`, `write_buffer`, `read_buffer` and `read_target` record into an encoder, and each refuses an encoder that did not come from the authority's current frame (`ProgramError::Transaction`).
The caller decides what a frame holds and submits it once:

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

- **Construction.** `ProgramRuntime::new(device, authority, budget)`.
  The authority's queue is used only to upload initial texture contents through `render-gpu`'s `TextureUpload`, the use `SubmissionAuthority::queue` documents.
  Buffers are created with `render-gpu`'s `upload_buffer`.
- **Per-draw constants.** Create a constant buffer with `create_buffer`, or record `write_buffer`, which stages the bytes in a copy-source buffer and copies them into the destination inside the frame, so the write has a place in the frame's order.
- **Block-image transfer.** `ProgramRuntime::transfer_block_image(encoder, &BlockImageTransfer { source, destination })` records, into the frame `encoder` belongs to, the move of block-compressed data that a game's own compute or raster shaders encoded into an uncompressed storage image, one complete block in each texel, into a block-compressed texture array it samples.
  `source` is a `BlockSource { texture, mip, layer, origin, extent }` of `Rg32Uint` (8-byte blocks: BC1, BC4) or `Rgba32Uint` (16-byte blocks: BC5, and BC2, BC3, BC6H and BC7 by size) texels, and `destination` is a `BlockDestination { texture, mip, layer, extent }` whose `extent` is the logical texels the blocks describe. wgpu cannot copy between formats, so the bytes go texture to buffer to texture, unconverted, through one staging buffer the runtime owns.
  The rows of the buffer are padded to `COPY_BYTES_PER_ROW_ALIGNMENT` and `rows_per_image` is the number of block rows.
  Blocks are addressed by geometry, so a mip two texels across takes one block and a mip six texels across takes two blocks a side, as wgpu's copies into the physical size of a compressed mip allow; the extent must fit the logical mip, and `ceil(extent / block size)` must equal the source region's extent.
  Everything is checked before anything records, and each refusal is a named `ProgramError::BlockTransfer(BlockTransferError)`: `SourceFormat`, `DestinationNotCompressed`, `DeviceLacksFormat` (the feature the format needs, `TEXTURE_COMPRESSION_BC` for BC), `BlockSizeMismatch` (the source texel size is not the destination block size), `Usage` (`COPY_SRC` on the source, `COPY_DST` on the destination), `NotFlat` (a texture that is not single-sample 2D), `LayerOutOfRange`, `MipOutOfRange`, `EmptyExtent`, `ExtentPastMip`, `BlockCountMismatch` and `SourceRegionPastMip`.
  The staging buffer is made by the first transfer and again only when a later transfer needs more room, so a steady state allocates nothing (`RuntimeStats::transfer_buffers_built`).
  It is admitted against the budget like every runtime buffer (the `block transfer staging` holding), an over-budget growth is `OverBudget` with nothing recorded, and the frame retains its reservation with those of the two textures.
  Transfers recorded in one frame, or in later frames, run in order, each writing the staging buffer before it reads it, so they can share it.
  `create_empty_texture(label, TextureShape { size, layers, mips, format, storage, view })` makes the array textures (a zeroed 2D array with mips, which also takes a compressed format) and `gpu_texture(handle)` gives the wgpu texture for work the runtime does not record, such as reading a compressed layer back.
  `tests/block_transfer` writes blocks from a kernel into `Rg32Uint` and `Rgba32Uint` images, transfers them into layer 2 of `Bc1RgbaUnormSrgb`, `Bc4RUnorm` and `Bc5RgUnorm` arrays, a 2 by 2 mip and a 6 by 6 mip, compares the destination blocks byte for byte after a texture-to-buffer readback, samples the layer to check the colours the blocks encode, and pins each refusal and the reuse of the staging buffer.
- **Prepared dispatches and transfers.** `prepare_dispatch(&DispatchRequest)`, `prepare_imported_dispatch(&ImportedDispatch { program, bindings, imports, groups, owners })`, `prepare_block_transfer(&BlockImageTransfer)` and `prepare_lent_block_transfer(&BlockSource, &LentBlockDestination { texture, mip, layer, extent }, owners)` check everything `dispatch` and `transfer_block_image` check, build the pipeline and groups or plan the copies and reserve the staging buffer, and return a `PreparedDispatch` or `PreparedBlockTransfer` whose `retain(encoder)` makes a frame hold what it uses, whose `record(encoder)` only records (as prepared commands, a dispatch's compute pass takes the frame's label and a transfer's copies go inside a debug group of that name) and whose `writes()` names the subresources it writes (a transfer its destination mip and layer; a dispatch the base mip of every layer of each runtime storage image it binds, and the range of each lent one).
  A prepared dispatch is also compute work alone: its `PreparedCommands::compute()` gives a `PreparedCompute` whose `record_in_pass(pass, label)` records it into the compute pass a frame shares among the compute work staged one after another, inside a pass debug group of `label`, setting every group of its program; a transfer has no compute form and ends that pass.
  A prepared dispatch records inside the same validation-error guard as `dispatch`, in a pass of its own or in a shared one, so an encoder another pass holds is `ProgramError::Gpu` in either form.
  `dispatch` and `transfer_block_image` are that preparation, retention and recording in one call.
  A dispatch whose workgroup count exceeds the device's `max_compute_workgroups_per_dimension` is refused as `ProgramError::Workgroups` before anything records.
  An imported dispatch takes lent resources at engine-semantic bindings, as an imported draw does, and holds `owners`; a lent destination is a block-compressed texture another owner holds and counts, so only the source and the staging buffer are the runtime's.
  A prepared transfer keeps the staging buffer it was planned with, so a later larger transfer that grows the runtime's buffer does not change it.
  Both types implement `worldrunner_render_post::PreparedCommands`, so a frame feature stages them with `PrepareContext::record_prepared`, and the frame records them in staging order after every feature prepared; the frame refuses one whose `writes()` differ from the views it was staged as writing, and a refusal to retain or record it reaches the frame's caller as `FrameError::PreparedRefused`, whose `refusal.downcast_ref::<ProgramError>()` is the runtime's error.
  `tests/block_transfer/prepared.rs` compares prepared and recorded forms byte for byte, fills a layer of a lent texture, dispatches over a lent texture, pins the refusals before anything is counted, the written subresources each form reports, the typed retention refusal and the guarded recording; `tests/block_transfer/command_buffers.rs` pins that a prepared dispatch recorded on its own costs what a compute pass of its own costs and that 4097 recorded into one pass with `record_in_pass` submit on Metal, where a pass each would exceed the queue's 4096 command buffers; `tests/block_transfer/shared_pass.rs` reads back a chain of prepared dispatches in one pass, each reading the storage image and buffer the one before wrote, and pins that a dispatch has a compute form and a transfer none.
  A lent destination's `mip` is any level of its chain, checked as for a runtime destination: `tests/block_transfer/mips.rs` fills each mip of a three-mip BC4 texture with its own blocks (the last, two texels a side, as one rounded-up block), checks that `writes()` names the mip, and pins `MipOutOfRange`, `ExtentPastMip` and `BlockCountMismatch` for the lent form with nothing counted.
- **Readback.** `read_buffer`, `read_target` and `read_texture` record the copy into a staging buffer and return a `PendingBufferRead` or `PendingTargetRead`.
  `resolve` takes the `SubmissionIndex` that `submit` returned, waits for that submission with a 30 s bound, maps the staging buffer and returns the bytes.
  `read_lent_texture(encoder, &texture, TextureLevel { layer, mip })` copies one level of one layer (one slice of a 3D level) of a texture another owner holds; a block-compressed level comes back as rows of whole blocks, any other copyable format as rows of texels.
  `TextureLevel::readback_bytes` gives the staging it admits; a level outside the texture, a format without a block copy size (`Depth24Plus`, multi-planar formats) or a texture without `COPY_SRC` is `ProgramError::Readback`, naming the cause, before anything records.
  `PendingTargetRead::begin` starts the map without waiting through render-post's `AsyncReadback`, and `MappingTargetRead::try_finish` returns the image once the host has polled the device after the GPU completed the frame, `None` before; the blocking `resolve` and the non-blocking map share the copy, the staging and the unpadding.
- **Budget.** Buffers, textures, render targets and their depth attachments, readback staging buffers and the staging of a recorded write are preflighted with `AllocationSet::validate` and added to the bytes already live.
  Over the budget is `ProgramError::OverBudget`, and nothing is created.
  Pipelines, shader modules, bind groups and the sampler are not counted, because an `AllocationSet` describes buffer, texture and readback storage only.
  `live_bytes` reports the total.
- **Release.** `release_buffer`, `release_texture` and `release_target` fail with `ProgramError::Busy` while a frame is being recorded.
  They keep the bytes in the budget until the work submitted so far has completed, as `render-gpu`'s `Configuration::replace` does for retired generations; the next allocation, or `collect`, frees them.
  A pending readback holds its staging bytes until it is resolved or dropped and the frame that recorded the copy has completed or been cancelled.
- **Errors from the authority.** `SubmitError` values keep their names: `ProgramError::DeviceLost`, `Busy`, `Lease` and `Transaction`.
  Every creation and recording call checks for device loss first.

## Joining a renderer frame

A draw can also be recorded into a render pass that another crate opened.
`worldrunner-render-scene` uses this to draw player programs inside its stock opaque pass, after its own opaque geometry.
Both share the pass's color target, depth buffer and the frame's one submission.
This crate prepares the draws and owns their lifetime.
`render-scene` owns the pass.

```rust
// Once: the renderer reserves the arena, so its total counts the bytes.
let reservation = renderer.reserve(&context, ProgramRuntime::arena_allocations(ARENA))?;
let mut runtime = ProgramRuntime::new(device.clone(), authority.clone(), AllocationBudget(ARENA));
let mut arena = runtime.open_arena(reservation)?;

// Each frame: admit the draws, hand them to the scene, encode, submit once.
let mut frame = authority.begin(&device)?;
let admitted = runtime.admit_draws(&frame, &arena, signature, &packets)?;
scene_gpu.admit_programs(admitted);
renderer.encoding().encode_scene(
    &device,
    &mut frame,
    (&mut scene_gpu, &pipelines),
    worldrunner_render_scene::compositor::ColorDepth { color: &color, depth: &depth },
    uniform,
)?;
frame.finish().submit()?;
```

- **Preparation and recording.** `prepare_draw` takes a `DrawPacket`, which is a `DrawRequest` without its target, and a `TargetSignature`.
  It builds the pipeline for the signature's formats and sample count, the bind groups and the buffer references, and returns a `PreparedProgramDraw`.
  `PreparedProgramDraw::record` only sets state and draws, so it cannot fail inside a pass.
  `draw` is `prepare_draw` followed by `record` into a pass over the target.
- **Target signature.** A `TargetSignature` is one to eight color formats (none for a depth-only pass, which `TargetSignature::depth_only(format, samples)` makes), an optional depth format and a sample count.
  `TargetSignature::new` refuses what the runtime does not draw into, by name: more than one sample (`UnsupportedTarget::Multisampled`), other than one color attachment (`ColorAttachments`), a depth format with a stencil aspect (`Stencil`) and a depth attachment without depth (`NotDepth`).
  The pipeline cache key includes the signature.
- **Arena.** `ProgramRuntime::arena_allocations(bytes)` is the storage the renderer reserves through its configuration, so the renderer's budget refuses an arena that does not fit.
  `open_arena` refuses a reservation smaller than the runtime's budget (`ArenaTooSmall`), so everything the runtime admits lies within storage the renderer counted once.
  It also refuses a runtime that already holds allocations made before any arena (`ArenaAfterAllocations`), because the reservation could not cover them and would be given back while they are live; open the first arena before allocating.
  A `ProgramArena` belongs to the runtime that opened it (`ForeignArena`).
- **Admission.** `admit_draws` takes the frame's encoder, the arena, the signature and the packets.
  It prepares every packet and then makes the frame retain the arena through `Publications::retain_lease`, which checks the frame's authority (`LeaseError::ForeignAuthority`) and that the arena is still live (`LeaseError::Expired`).
  It returns an `AdmittedPrograms`, the only way to obtain one, or nothing at all.
- **Lifetime.** The frame keeps the arena's `FrameOwner` leased until the GPU has completed the submission.
  `ProgramArena::retire` fails with `Busy` while an unsubmitted or in-flight frame holds it and with `Expired` afterwards.
  Cancelling or dropping the prepared frame releases the lease and submits nothing.
  The prepared draws hold clones of the wgpu objects they use, which wgpu keeps alive while a recorded command needs them.
  The runtime refuses `release_buffer`, `release_texture` and `release_target` with `Busy` while a frame is being recorded or is prepared and unsubmitted, so no submission that could use the resource is still to come.
  It keeps a released resource's bytes in its budget until the newest submission made at the release has completed.
  Nothing unsubmitted can use the resource at that moment and the release excludes new frames while it records the serial, so that serial is at or after the last submission that used the resource. **One retention rule.** The reservations of the resources a call uses are held by a `FrameOwner` that the frame retains when the call records into it, and every recording entry point follows that rule through the same mechanism.
  `draw` holds the buffers and textures it binds, its vertex and index buffers and the target it draws into.
  `clear_target` holds the target.
  `dispatch` holds the buffers and textures it binds.
  `read_buffer` holds the buffer and the readback staging buffer.
  `read_target` holds the target and the staging buffer.
  `write_buffer` holds the buffer and the staging buffer of the write.
  `transfer_block_image` holds both textures and the transfer staging buffer.
  Prepared draws follow the rule when a consumer retains them on the frame (`AdmittedPrograms::retain`, `PreparedProgramDraw::retain`).
  The holds matter when the runtime is dropped with a frame still unsubmitted: the frame keeps the bytes counted, in the renderer's arena reservation, until its outcome, so work recorded into a frame cannot outlive the accounting of what it touches.
  A frame that is cancelled or dropped gives the bytes back at once and a submitted one when the GPU has completed it.
  A pending readback and a dropped readback hold the same: the staging bytes stay counted until the frame completes, not until the caller resolves or drops the read.
  Dropping the draw detaches the owner (`FrameOwner::detach`): a frame that retained it can still be submitted and keeps the bytes counted until it is cancelled, dropped or completed on the GPU, whatever happened to the draw and the runtime.
  A frame that never retained the draw holds nothing, so cancelling a frame returns the bytes at once.
  A consumer that records a prepared draw must retain it on that frame.
- **Checks at the consumer.** `AdmittedPrograms::check_target` compares the draws' signature with the pass's (`TargetMismatch`), and `retain` repeats the lease check for the frame that finally encodes them.

The only consumer so far is `render-scene`'s opaque pass, see [its README](../render-scene/README.md#player-program-draws).
The first slice covers one single-sample color attachment with `Depth32Float` and nothing else.

## Lending a renderer's resources

A renderer that already owns buffers, textures and samplers can have a program draw with them in place.
`prepare_imported_draw(&ImportedDraw, TargetSignature)` returns a `PreparedProgramDraw` like `prepare_draw` does, over:

- **Imports by semantic.** A binding declared with `BindingSource::Engine(EngineSemantic::named(..))` is filled by an `Import` of a buffer, a texture with its view or a sampler.
  Two declarations of one semantic at different addresses, or a stage pair that disagrees on it, are a `LayoutError::Conflict`.
  A binding with no import is `MissingImport`, an import no binding takes is `UnusedImport` and one semantic imported twice is `DuplicateImport`, each naming the semantic.
  The runtime checks an imported buffer against the shader's need, an imported texture's format against the binding's sample type and that the binding is a single-sample 2D texture, a resource of the wrong kind as `WrongResource`, and takes the sampler as given, so a sampler binding can be filtered, clamped or anisotropic as its owner made it.
  Bindings declared by property name are still filled from `BoundResource`s of the runtime's own handles in the same draw.
- **Borrowed geometry.** `ImportedVertexInput`, `ImportedIndices` and `ImportedGeometry` carry the wgpu buffers themselves.
  Ranges are checked against the buffers' sizes by the rules of `range.rs`, and an instance-rate buffer must hold as many strides as the end of `ImportedDraw::instances`, so an overrun is `VertexRangePastBuffer` before anything is recorded.
- **Topology.** `ImportedDraw::topology` is the `wgpu::PrimitiveTopology` the geometry is drawn in, so a draw can keep an original shader written for a strip.
  A strip read through `ImportedGeometry::Indexed` gets the indices' format as the pipeline's `strip_index_format` (wgpu refuses an indexed strip draw without it, and the format's largest value restarts the strip); lists and strips read in order get none.
  The topology and that format are part of the pipeline key, so a strip over `Uint16` and one over `Uint32` indices are two pipelines while lists over either format share one.
  A depth bias on a point or line topology is `DepthBiasWithoutTriangles` before a pipeline is built, as wgpu applies a bias to triangles only.
  Prepared pipelines (`prepare_pipeline`), plain and material draws draw triangle lists.
  `tests/lent/topology.rs` draws four vertices as a strip in order and through indices of both formats over the whole target, the same four as a list over the closed-form pixels of their first triangle, and counts the pipelines each builds.
- **Attachments.** `TargetSignature::attachments(&colors, depth, samples)` describes one to `MAX_COLOR_ATTACHMENTS` colour formats and a sample count of 1, 2, 4, 8 or 16.
  Combined depth/stencil formats are supported, preserving renderer-owned stencil unless a prepared pipeline's state names stencil (see **Stencil** below).
  The state gives each attachment a blend and a write mask.
  An empty mask leaves an attachment as it was, which is how a material that writes location zero shares a pass with attachments that belong to other materials.
  `TargetSignature::new` still describes the one single-sample attachment of the plain draw.
- **External sets.** `ProgramRuntime::graphics_program_in(vertex, fragment, &[ExternalSet])` lends whole bind group sets that the owner of the pass binds once.
  `ExternalSet { set, layout, bindings }` carries the renderer's `wgpu::BindGroupLayout`, which goes into the pipeline layout as it is, and a `ResourceLayout` of what the shader reads from the set.
  Each resource a stage uses at a lent set must be listed in `bindings` at its address with the same class (`ResourceKind::same_class`: buffer kind and access, texture dimension, multisampling and sample type, sampler class, ignoring the filterable flag).
  A resource that is not listed is `LayoutError::UndeclaredResource` naming the shader's variable, and one listed as another class is `LayoutError::Mismatch`.
  Listed bindings the shader does not use are allowed, and a stage's own declarations at a lent set are ignored. wgpu checks the rest, such as visibility, when it creates the pipeline, and its refusal is `ProgramError::Gpu`.
  A prepared draw builds no bind group for a lent set and `record` does not bind it.
  `PreparedProgramDraw::external_sets()` and `GraphicsProgram::external_sets()` list the sets the pass owner must have bound before recording.
  The pipeline cache key includes each lent layout's identity, so programs with equal stages and different lent layouts never share a pipeline, while clones of one layout do.
  A `BoundResource` at a lent set, an import of an engine semantic that a lent binding names, and a set lent twice are `ProgramError::ExternalSetConflict { set }`. [The guide](../../docs/render-program-native.md#sets-the-pass-owner-binds) has a compiled example, and `tests/lent/external.rs` draws two programs over one pass-bound group.
- **Stats.** `ProgramRuntime::stats()` returns `RuntimeStats { modules_built, pipelines_built, pipeline_keys_computed, bind_groups_built, sets_prepared, draws_prepared, transfer_buffers_built }`.
  Each count only rises and is incremented where the object is created: a shader module in the module cache, a pipeline in the render and compute pipeline caches, a bind group where a program's set is bound, and a draw where a prepared draw is made (`prepare_draw`, materials, admissions and `prepare_imported_draw`).
  `transfer_buffers_built` counts the staging buffers of block-image transfers.
  `pipeline_keys_computed` counts render pipeline content keys: a `GraphicsProgram` (and its clones) holds the key of each request made for it, found again by state, target signature, vertex layouts and primitive, so a draw prepared again in a later frame computes none (`render-scene`'s `water::tests::key_cost_tests` counts them over frames of water bodies).
  A cache hit or a call refused before it creates the object counts nothing.
  `sets_prepared` counts `prepare_set` calls; each also counts its bind group, and a draw that binds a prepared set counts neither.
  A cost test takes a snapshot before a frame and compares it with one after; `tests/lent/external.rs` predicts the exact counts of a sequence of draws.
- **Prepared sets.** `ProgramRuntime::prepare_set(program, set, &ImportTable, owners)` builds the bind group of one set once, from an `ImportTable` (engine semantic to `ImportedResource`, lenient: entries the program does not use are ignored, and a used binding with no entry is `MissingImport` naming the semantic, set and binding).
  `ImportedDraw::sets` takes `&[&PreparedSet]`, and a draw given a set builds no group for it.
  Programs with equal slot lists at a set share one `wgpu::BindGroupLayout` (`GraphicsProgram::set_layout`), so a `PreparedSet` fits all of them; a program with other slots, a set built by another runtime or a lent set refuses it (`SetLayoutMismatch`, `ExternalSetConflict`), and a set given twice or alongside imports or bound resources for it is `PreparedSetConflict`.
  The draw holds the group and the set's owners, and frames that retain the draw hold them in turn. [The guide](../../docs/render-program-native.md#sets-built-once-for-many-draws) has compiled examples, and `tests/lent/sets.rs` draws two programs through one set.
- **Texture dimensions.** `ImportedTexture` carries the `dimension` of its view and the `range` it covers.
  `Import::texture` assumes the default view (`D2` for one layer, `D2Array` for several), `Import::texture_array` takes a 2D array view of any layer count and `Import::texture_view` any dimension, each covering the whole texture (`whole_range`); `Import::texture_range` states a narrower `TextureRange`, such as one mip of one layer, which the aliasing check and a dispatch's `writes()` then use.
  A binding accepts an import whose view dimension and sample count match the shader's and whose format suits the sample type, so `D2Array`, `Uint` and depth views can be imported; any mismatch is `WrongResource`.
- **Registered variants.** `ProgramRuntime::variant_program(handle, &FeatureSet)` returns the `GraphicsProgram` of the variant exactly those features select, for `prepare_imported_draw` and `prepare_set`.
- **Prepared pipelines.** `ProgramRuntime::prepare_pipeline(&PipelineDraw, signature)` prepares what a draw has before its geometry: the program, state, a `VertexLayoutSpec` (stride, step and attributes of each vertex buffer), prepared sets, the groups for bound resources and imports, and the owners.
  `GeometryDraw::new(&layout, &buffers, &ImportedGeometry, instances)` is plain data: it checks the extents (`VertexBufferCount`, `InstanceRange`, `VertexRangePastBuffer`, `IndexRangePastBuffer` and the rest of the draw checks) and records the layout's content identity.
  `PreparedPipeline::bind(pass)` sets the pipeline and the groups, `draw(pass, &geometry)` sets the vertex and index buffers and draws, and `retain(frame)` is called once per frame.
  C indexed geometries over two vertex buffers and one prepared set record `2 + 4 * C` commands, against `6 * C` for a `PreparedProgramDraw` each.
  `draw` refuses a geometry made for another layout (`GeometryLayoutMismatch`) before recording a command.
  A draw that is recorded once and forgotten, as a scene object walk does for each object, skips the `GeometryDraw`: `PreparedPipeline::draw_borrowed(pass, &layout, &[&Buffer], BorrowedGeometry, instances)` makes the same checks in the same order (`VertexBufferCount`, `InstanceRange`, the extents, then `GeometryLayoutMismatch`), records the same commands and allocates nothing, because the buffers and the index buffer are borrowed for the call (`BorrowedGeometry` is `ImportedGeometry` with a borrowed index buffer, and `From<&ImportedGeometry>` converts).
  `GeometryDraw::new` and `draw_borrowed` share one check, so they cannot disagree.
  `tests/lent/borrowed.rs` compares the two paths command by command and error by error.
  `PreparedProgramDraw::record` is `bind` then `draw`, and `bind`, `draw` and `record` take any `PassCommands`, which `wgpu::RenderPass` implements.
  `tests/lent/batch.rs` counts the commands, draws four geometries from one instance buffer and checks that steady-state frames add nothing to `RuntimeStats`. [The guide](../../docs/render-program-native.md#many-geometries-through-one-pipeline) has a compiled example.
- **Stencil.** A `RenderState` given to `prepare_pipeline` or `prepare_imported_draw` may carry `StencilState { ops, reference }`, for a pass whose depth attachment has a stencil aspect (`TargetSignature::has_stencil`).
  `ops` (compare, operations, masks, per face) become the pipeline's stencil state and are part of its cache key, so states that differ only in stencil are distinct pipelines.
  The `reference` is the pass's: the owner calls `set_stencil_reference` once for the pass, as it binds the lent sets, and the runtime records no reference command, which keeps a recording at `2 + 4 * C` commands.
  A stencil state for a signature without a stencil aspect is `UnsupportedState { what: "stencil without a stencil attachment" }`, and the plain and material draws (`check_drawable`) still refuse stencil.
  `tests/lent/stencil.rs` marks the columns a draw covers with a reference and consumes the marks with a second draw, and reads colour and stencil back against closed forms.
- **Depth-only and vertex-only.** `TargetSignature::depth_only(format, samples)` describes a pass with a depth attachment and no colour attachment (a depth format without stencil, 1, 2, 4, 8 or 16 samples, each refused by name).
  `ProgramRuntime::vertex_only_program(&StageProgram)` builds a `GraphicsProgram` with no fragment stage (`fragment()` is `None`), which only a depth-only pass draws: a colour signature is `NoFragmentForColors`.
  A state for a depth-only pass has no colour states, and one that has some is `ColorStatesOnDepthOnly`.
  `prepare_imported_draw` and `prepare_pipeline` take these, so a shadow caster is a `PreparedPipeline` over many geometries.
  `DepthState::bias` is drawn instead of refused and is part of the pipeline key.
  `tests/lent/depth_only.rs` reads the depth back and predicts it: constant depth, the `Less` test between two programs, the bias on a primitive of constant depth (`constant` times `2^-16` in `Depth16Unorm`) and on one of constant slope, and the prepared pipeline. [The guide](../../docs/render-program-native.md#depth-only-passes-and-vertex-only-programs) has compiled examples.
- **Alpha to coverage.** `RasterState::alpha_to_coverage` is honoured by `prepare_imported_draw` and `prepare_pipeline` when the signature has more than one sample: the pipeline's multisample count is the signature's, its mask is all ones and its alpha-to-coverage flag is the state's, which is part of the pipeline key.
  At one sample the state is `ProgramError::AlphaToCoverageNeedsSamples { samples: 1 }`.
  `tests/lent/coverage.rs` renders a white fragment of alpha 0, 0.25, 0.5, 0.75 and 1 into a 4x `Rgba16Float` target, resolves it and reads back: every pixel covers 0, 1, 2, 3 and 4 samples on lavapipe and on the Radeon, and the state off covers 4 for every alpha. [The guide](../../docs/render-program-native.md#alpha-to-coverage) has a compiled example.
- **Writing a set once.** `ProgramRuntime::set_layout(&ResourceLayout)` builds the `wgpu::BindGroupLayout` of the set a `ResourceLayout` declares: one entry for each declaration, with its visibility and `ResourceKind::binding_type`, a buffer's `min_size` as the minimum binding size.
  `external_set(ResourceLayout)` returns the `ExternalSet` for `graphics_program_in`, with the set number the declarations name, so the engine declares the set once and uses it for its groups and for the programs.
  A declaration with no bindings, with several sets or with an address twice is a `LayoutError`.
  Every call builds a new layout object.
  `tests/lent/set_layout.rs` builds a group of six bindings (uniform, storage, unfilterable texture with a non-filtering sampler, depth texture with a comparison sampler) against the built layout, shows the layout refuses a short buffer and a plain sampler in the comparison slot, draws through it and reads the channels back bitwise. [The guide](../../docs/render-program-native.md#writing-the-set-once) has a compiled example.
- **Comparison samplers.** A binding of `Sampler(Comparison)` whose source is `BindingSource::Engine` takes an imported sampler made with `SamplerDescriptor::compare`, with a depth texture declared `sample: Depth`, in `prepare_imported_draw` and in sets from `prepare_set`.
  A comparison binding the runtime would have to fill itself is still `UnsupportedSampler`.
  A plain sampler lent to the slot is wgpu's to refuse (`ProgramError::Gpu`).
  `tests/lent/comparison.rs` compares the reference 0.5 with `Depth16Unorm` codes 16384, 32767, 32768 and 49151 and reads back `[0, 0, 1, 1]` for `LessEqual` and `[1, 1, 0, 0]` for `Greater`, exact on both adapters. [The guide](../../docs/render-program-native.md#comparison-samplers) has a compiled example.
- **Unfilterable float textures.** A declaration may say what WGSL cannot: `ResourceKind::Texture` with `Float { filterable: false }` narrows the shader's float texture (`ResourceKind::narrows`), and `Sampler(NonFiltering)` narrows its `sampler`.
  The slot takes the declared type, an unfilterable format such as `R32Float` can be imported to it, and a binding declared filterable still refuses such a format as `WrongResource` on a device that cannot filter it: an import cannot change a layout.
  Filterability is the device's (`TextureFormat::sample_type` with the device's features), so on a device with `FLOAT32_FILTERABLE` an `R32Float` import suits a filterable binding and samples through a linear sampler, as wgpu binds it.
  A non-filtering sampler binding with no import gets the runtime's nearest-texel sampler.
  A program that samples an unfilterable texture with a filtering sampler is `LayoutError::FilteringSampler`, found from the sampling calls in the module, for lent sets too.
  `tests/lent/unfilterable.rs` samples an `R32Float` texture through the runtime's sampler and through a lent one into an `R32Float` target and compares every texel bitwise with the `f32` uploaded; on wgpu's no-op device it binds an `R32Float` import to a filterable binding with `FLOAT32_FILTERABLE` and refuses it by name without, and on a device that offers the feature it samples halfway between texels through a lent linear sampler and reads the linear blend. [The guide](../../docs/render-program-native.md#float-textures-that-cannot-be-filtered) has a compiled example.
- **Buffer ranges.** `Import::buffer_range(semantic, buffer, offset, size)` binds `size` bytes from `offset` of a buffer, in an `ImportedDraw` and in an `ImportTable`.
  The offset must be a multiple of the device's `min_uniform_buffer_offset_alignment` or `min_storage_buffer_offset_alignment` for the binding's kind (`BufferRangeMisaligned`), the size at least the shader's need and the declared `min_size` (`BufferTooSmall`, which an empty range is too), and the range must lie within the buffer (`BufferRangePastBuffer`).
  `tests/lent/ranges.rs` draws slices of one uniform buffer through an import and through a prepared set, reads a storage range from its offset and counts its elements by `arrayLength`, and pins each refusal; alignment is 256 for both on lavapipe and on the Radeon. [The guide](../../docs/render-program-native.md#buffer-ranges) has a compiled example.
- **Groups for external sets.** `ProgramRuntime::prepare_external_set(&ExternalSet, &ImportTable)` builds the group of a set the pass owner binds, from imports by engine semantic, so an owner that keeps its resources elsewhere (a game whose resources the renderer owns) never builds a bind group itself.
  Every declared binding must name an engine semantic (`MissingBinding` otherwise) with an entry in the table (`MissingImport` naming the semantic, set and binding); entries the set does not declare are ignored.
  Each import is checked as a draw's imports are (kind, sample type, view dimension, sample count, buffer size: `WrongResource`, `BufferTooSmall`).
  The returned `ExternalGroup` binds itself with `bind(pass)` at its `set()`.
  `Import`, `ImportedResource`, `ImportedTexture` and `ImportTable` compare equal when they lend the same wgpu objects the same way (wgpu compares handles by identity), so an owner can key a cached group on the imports it was made from.
  A depth texture, or the depth aspect of a depth-stencil texture, suits a binding declared `Float { filterable: false }`, as wgpu allows.
  `tests/lent/set_layout.rs` draws through a group the runtime built from imports and reads the channels back bitwise, and checks the refusals and the depth case.
- **External holds.** `ProgramRuntime::hold_external(label, bytes)` counts the bytes of a resource its owner creates and keeps itself in `live_bytes` and against the runtime's budget until the returned `ExternalHold` drops, and refuses by name (`OverBudget`) with nothing counted when they do not fit.
  An owner calls it before it creates the resource, so one budget bounds what the owner holds and what the runtime allocates.
  The bytes count as given (render-gpu `AllocationSet::external`): the owner has checked its resource against the device, so no buffer rule applies, and a texture of three one-byte texels or one larger than `max_buffer_size` is held like any other; zero bytes are refused.
  `tests/program/budget.rs` fills the budget from outside and shows the runtime's own allocation refused, then admitted after the hold drops, and holds byte counts no buffer could have.
- **Lent sets for vertex-only programs.** `ProgramRuntime::vertex_only_program_in(vertex, &[ExternalSet])` lends sets to a program with no fragment stage, under the rules of `graphics_program_in` (`ExternalSetConflict` for a set lent twice, `UndeclaredResource` and `Mismatch` for a resource the declaration misses).
  The cascades of a shadow caster then share one prepared pipeline, and the pass binds each cascade's light frame at set 0 before it records.
  `tests/lent/depth_only.rs` switches the lent group between two passes and between two draws of one pass and reads the depth back exactly.
- **Parse once.** `StageProgram::sibling(entry_point, kind)` makes another entry point of a stage's source from the same parse and validation (`shares_parse_with` tells), with its own resources, no declarations and no overrides.
  `register_program` uses it whenever the stages of a program, or of its variants, have equal sources.
  The runtime compiles one `ShaderModule` for each source, whatever its entry points, stage kinds and overrides, so a program whose stages share a source adds one to `RuntimeStats::modules_built`; `tests/native.rs` counts them and checks that registration shares the parse. [The guide](../../docs/render-program-native.md#one-parse-for-a-whole-source) has a compiled example.
- **Override constants.** `StageProgram::with_overrides(&[(name, value)])` sets the WGSL `override` constants of a stage, and `StageDesc::with_overrides` does the same for a registered program.
  A constant is named as wgpu names it (its identifier, or its `@id` as a decimal number), a value is checked against the constant's type, and `OverrideError::Unknown`, `Duplicate` and `Value` (`ProgramError::Override` at registration) name the constant that is refused.
  The values are part of the stage's `content_key` and so of the pipeline's cache key, and they reach `PipelineCompilationOptions::constants` of the vertex, fragment and compute stage they were set on.
  Stages of one source that differ only in their overrides share one compiled `ShaderModule`.
  `tests/lent/overrides.rs` draws the same source with different values, counts the pipelines and modules, and sets a compute constant. [The guide](../../docs/render-program-native.md#override-constants) has compiled examples.
- **Retention.** The runtime counts none of the lent resources: their owner has its own accounting or none, and a second count would charge them twice.
  `ImportedDraw::owners` are `Keepalive` values, `Arc<dyn Any + Send + Sync>`, that the draw holds for as long as the draw or a frame that called `PreparedProgramDraw::retain` lives, through the `FrameOwner` that holds a runtime's own reservations.
  A cancelled frame gives them back at once and a submitted one when the GPU has completed it.
  A lender whose resources must outlast a frame that is still being recorded or is in flight passes the object that owns them as an owner.

## Offscreen work through the checked operations

The offscreen module is Unity's: it resolves the player's passes and variants from `unity_serialized` records, reads Unity's `GraphicsFormat` codes and maps Unity's vertex source channels.
It lives in the adapter as `unity::offscreen` (with `unity::shader` for `ProgramShader`), behind the `unity` feature.
`OffscreenOperations::into_pending` hands the operations and their exports to the viewer as a format-neutral `offscreen::PendingOffscreen`.
The viewer admits them on its frame's recording and `render-scene` encodes the resulting `offscreen::AdmittedOffscreen`.
Neither consumer needs the adapter feature in production.
`ids`, `layers` and `sampler` (mesh, texture and shader ids, property layers and sampler states) are neutral too.

A build makes the texture view of a range, version and view dimension of an allocation once and gives every later draw that binds it a clone (`OffscreenOperations::view_counts` counts the views made and the views reused).
The cache belongs to the build's tracker and goes with it, and its key holds the allocation's generation, so a view never outlives the allocation or survives a replacement of its storage; a write of a texture takes a new version, so draws after the write get a view of their own.
`OffscreenOperations::planned_draws` is the number of draws the build held planned, and `OffscreenFrame::planned_draw_bound` is the most a record plans, which a host uses to build a frame in parts of a bounded number of draws.
`tests/program/offscreen/views.rs` pins these.

Unity player offscreen work is a camera rendering into a render texture, `Graphics.Blit` with a material pass and `Graphics.CopyTexture`.
This crate turns a frame's records of it into `worldrunner_render_post::contracts::TypedOperation`s that run the player's own programs.
It has no recorder of its own: the operations go to the one executor, which checks hazards, budgets, cancellation and timing, and the frame's timing and statistics list them.
A second scheduler writing into the encoder before `encode_scene` is excluded by the one-renderer rule.

### Path of a frame

```text
host adapter       maps the world export to an OffscreenFrame (`play`'s `offscreen` module; this crate does not read world-simulation)
unity::offscreen::OffscreenState::build(runtime, arena, frame)       the one entry point: resolution, packing, versions
   -> OffscreenOperations { operations, later_reads, .. }    typed operations in call order, before the scene
OffscreenOperations::admit(encoder)                ResourceRegistry::compile_operations_with_reads
   -> AdmittedOffscreen
SceneGpu::admit_offscreen(admitted)                next to admit_programs
FrameEncoding::encode_scene(..)                    encodes the operations with the frame's pass timer, then the stock scene
frame.finish().submit()                            the frame's one submission
```

`render-scene`'s `FrameEncoding::encode_scene` already calls `FrameRenderer::encode_frame` after it has encoded other preparation work into the same encoder (terrain geometry, sky, terrain pages).
The offscreen operations are encoded in the same place, before the stock operations.
The pass timer lives in the post chain (`TimingState`), so the contract gets one entry point that encodes a compiled list with the frame's timer (`TimingState::encode_checked`).
The passes then appear in `FrameTiming::passes` as `PassId::Operation`.

### Mapping to the contract

- All uniform data of the frame
  - Operation: One `TypedWork::Upload` into a budgeted constants buffer, first in the list
  - Resource uses: Writes the buffer range; every draw reads its slice as `BufferUseKind::Uniform`
- `CameraRender`
  - Operation: `TypedWork::Draws { pass, draws }`, one `CheckedDraw` per sub-mesh
  - Resource uses: The pass clears or loads the render texture and its depth as the camera says (`AttachmentLoad`, `StoreOp::Store`); textures the materials sample are `ResourceUse::read(.., Sampled)` in `pass.uses`; vertex and index buffers are `BufferUse::read(.., Vertex/Index)`
- `Blit` with a material
  - Operation: `TypedWork::Fullscreen` with the material's pass
  - Resource uses: The target is an attachment loaded with its prior contents, the source is read as `Sampled`
- `Blit` without a material
  - Operation: `TypedWork::CopyTexture` when the sizes and formats agree, else `TypedWork::Fullscreen` of a built-in sampling program
  - Resource uses: Copy or sampled read, and write
- `CopyTexture`
  - Operation: `TypedWork::CopyTexture` with element and mip in the `TextureRange` of each view
  - Resource uses: Copy read of the source range, copy write of the destination range
- `GenerateMips`
  - Operation: One `TypedWork::Fullscreen` of the built-in sampling program for each layer of each level from 1
  - Resource uses: Sampled read of the layer's level above, attachment write of its level, every layer of a level at one new version
- Scene draws that sample a result
  - Operation: Not an operation: declared with `ResourceRegistry::compile_operations_with_reads`
  - Resource uses: Sampled reads of the ranges the host names in `OffscreenFrame::later_reads`

Versions are per frame.
Every render texture, array and source texture is imported with initial version 0 (a new wgpu texture is zero-filled, so version 0 is truthful), and each write takes the next version of its mip and layer.
Each frame builds a new `ResourceRegistry`, so the registry never accumulates retired textures.
A runtime-owned texture is imported with `import_leased` and the arena's lease, so a retired arena fails the compile and the encode by name, and the encode retains the lease on the frame.

### Contract extensions in render-post

- `CheckedDraw::rebind`: A draw is a pipeline, bind groups and geometry.
  `CheckedDraw::with_geometry` creates the pipeline every time.
  Splat cameras draw hundreds of sub-meshes with a few programs, so the pipeline is created once per program and state and the other draws rebind it, with the layouts and vertex ABI checked against it
- `TypedWork::Fullscreen` vertex streams: The player's blit vertex stages read position and texture coordinates from vertex buffers.
  `Fullscreen` drew three vertices with no vertex buffer, which a pipeline whose vertex stage has inputs cannot use
- `ResourceRegistry::compile_operations_with_reads`: A scene draw that samples a render texture is recorded by the stock pass, which is not an operation.
  The host declares its reads, and the same version rules that check operations check them: a read before any write, a read of a stale version and a read of a range a later operation writes are errors
- `TimingState::encode_checked`: The frame's pass timer is inside the post chain.
  A host outside render-post had no way to give `CompiledOperations::encode` the frame's timer

Each has its own test in `crates/render-post`; see the [render-post README](../render-post/README.md).

### Resolution

`unity::shader::ProgramShader` is the decoded `unity_serialized::shader::Shader` and the decompressed chunks of its Vulkan platform.
`ProgramShader::resolve(selector, keywords)` walks the first sub-shader's passes, picks the pass, picks the variant, decodes its record and parameters, prepares its stages together with `spirv_prep::prepare_stages` and builds a `GraphicsProgram`.
The walk is the one `unity-import`'s `player_variants` does (this crate does not depend on `unity-import`).

- **Pass selector.** `PassSelector::Index(n)` is the n-th pass of the sub-shader, for `Graphics.Blit`.
  `PassSelector::Tag { key, values }` is the first pass whose tag `key` equals the first of `values` that any pass has, for camera draws.
  Keys and values are compared without regard to ASCII case (a player build spells the value of its shadow caster passes `SHADOWCASTER` where a pipeline asks for `ShadowCaster`); the pass's own tags are searched after the render state's.
  The crate names no tag key and no value; the caller gives both.
  `PassSelector::TagWithUntagged { key, values, untagged }` is `Tag` with a pass that has no tag `key` counted as having `untagged`; a pass that has the tag keeps its own value, and the first of `values` that any pass has still decides.
  No match is `ResolveError::NoPass`, never another pass.
  `OffscreenOperations` reports what the camera draws did: `camera_draws()` lists each camera render with the draws it recorded and the ones left out for want of a pass, `unmatched_passes()` counts the left-out draws by the tag values the shader's passes have against the ones selected, and `partial_mip_reads_by_pass()` counts the reads that bound only the levels written with the base level by texture property, levels and drawing pass.
- **Keywords.** The material's keywords and the enabled global keywords are mapped to the shader's keyword indices by name.
  `unity::variant::select_variant` then matches them exactly among the variants of the Vulkan platform.
  A keyword the shader does not list cannot change a variant and is ignored.
  The keywords a pass's variants differ by are the ones those variants use (`declared_player_keywords`); the vertex program's keyword state mask is not consulted, because in the Unity 6 player it names stereo keywords that no variant uses, and declaring it would select the variant with no keywords for every draw of such a pass.
  A pass whose variants all take one keyword of an exclusive group, such as the terrain passes' `_MAIN_LIGHT_SHADOWS` and `_MAIN_LIGHT_SHADOWS_CASCADE`, has no variant for a draw that enables neither; that draw takes the group's first (lowest-indexed) keyword, which is the keyword of the last variant in each such list of 128 or more variants in the Old World player.
  No match is `ResolveError::NoVariant`.
  `tests/program/variant_state_mask.rs` covers a mask that names keywords the variants do not use and `tests/program/variant_required_group.rs` covers the required groups.
- **Cache.** A prepared variant is cached by the content of its record (the SPIR-V, the entry points and the parameters), so two shaders or passes with the same variant share one program.
  Render pipelines are cached by program, state, target signature, vertex layout and bind group layouts.

### Property layers and samplers

A `PropertyLayer` holds numeric values and texture inputs by name.
A draw reads four layers, highest first: the caller's per-draw values, the renderer's property block, the material and the globals.
The renderer's property block is two layers: the block of the draw's slot (`Renderer::slot_blocks`, by slot index) above the block of every slot (`Renderer::block`), so a slot's block wins for a name both set and a slot without one reads `block` alone.
For a camera draw the camera's values (view and projection, which the caller names however its shaders do) sit in the per-draw layer, below the renderer's own per-draw values.
This crate names no Unity built-in.
A constant buffer member or texture no layer has keeps the packer's behaviour (zero) and is listed in `OffscreenOperations::missing`; a texture no layer has is `OffscreenError::MissingTexture` for a blit, and for a camera draw the draw is left out, counted in `camera_draws()` and by texture property in `missing_texture_draws()`, while the camera's other draws are kept.
A camera draw whose mesh lacks a vertex channel the pass reads is handled the same way and counted by channel and shader input in `missing_channel_draws()`; a blit with such a mesh still fails with `MissingChannel`.
A draw of a camera render, a draw list or a procedural draw whose pass samples a level of a texture that same pass draws into (a color or depth attachment at an overlapping mip and layer) is left out with `OffscreenError::FeedbackLoop`, counted in the `feedback_loop` field of `camera_draws()` and `list_draws()` and by texture in `feedback_loop_draws()`; the other draws and operations of the frame still run.
Without that refusal the contract's hazard check would refuse the whole frame at `admit`.
The check stays in place for any pass the planner does not cover, such as a blit whose source is its own target.

A `PropertyLayer` also holds structured buffers: `buffers` maps a property name to the `BufferHandle` of a runtime buffer (`ProgramRuntime::create_buffer`), the handle a compute dispatch binds, so one upload serves a dispatch and a draw.
A read-only storage slot of a graphics program (Unity's `StructuredBuffer<T>`, set with `Material.SetBuffer`) is named by the HLSL name its stage's parameters give it, the same name a kernel's buffer is bound under, and the draw binds the whole buffer read only to the stages the slot declares.
The layers are asked in the order textures are: per-draw values, the camera's or list's values, the renderer's property block, the material, the material's fallback and the globals, and the first that names the buffer wins.
A draw whose buffer no layer names is left out and counted by property in `missing_buffer_draws()` (a blit with such a material fails with `MissingBuffer`).
A writable storage slot, a buffer shorter than the shader reads, and a stage the device binds fewer storage buffers to than the shader declares (a vertex stage with none, on some adapters) are refused by name as `UnsupportedBuffer`.
`ListDraw::instances` is the number of instances a list item draws, as `DrawMeshInstancedIndirect` does with the instance count of its arguments; 0 draws nothing and plans nothing, and camera renders draw one.

A `TextureInput` may carry a `SamplerState` (wrap along U, V and W, a filter mode and an anisotropy).
`SamplerState::new(u, v, filter)` wraps W by clamping to the edge and reads without anisotropy.
The anisotropy is the wgpu `anisotropy_clamp` and is applied only to a trilinear state, because wgpu refuses an anisotropy above 1 unless the texel filters and the mip filter are all linear; point and bilinear states read with 1, and a value outside 1 to 16 is clamped into that range.
There is no mip bias, since wgpu samplers have none.
`SamplerCache` creates one `CheckedSampler` per state.
The state of a texture input overrides the state of the texture, and a texture with neither keeps the fixed sampler the crate always used, so existing tests do not change.
A split combined sampler takes the state of the texture it was split from.

### Render textures

A `RenderTexture` has a caller id, a size, a `GraphicsFormat` code, depth bits, a mip count, an sRGB flag and a generation.
`OffscreenState` creates it through the runtime, so it counts against the runtime's budget and the renderer's arena, and creates it again when the generation or any of the other fields change.
The old texture is released when the runtime is not recording, and a release that finds a frame being recorded is retried at the next build.
The `GraphicsFormat` codes are the engine's own managed `GraphicsFormat` values; the table has 4 (`Rgba8UnormSrgb`), 5 (`R8Unorm`), 8 (`Rgba8Unorm`), 45 (`R16Float`), 48 (`Rgba16Float`) and 52 (`Rgba32Float`), and any other is `OffscreenError::UnsupportedFormat`.
Depth is 16 bits (`Depth16Unorm`), 24 or 32; any other count is a named error.
A render texture with color and 24 depth bits has a `Depth24PlusStencil8` depth buffer, and one with 32 bits has `Depth32FloatStencil8` on a device with that feature and `Depth32Float`, without stencil, on one without.
So a draw into a render texture with color and 24 or 32 depth bits can test and write 8 bits of stencil where the device allows it, and a 16-bit one has none.
A depth-only render texture is the depth alone (`Depth16Unorm`, `Depth24Plus`, `Depth32Float`) so that it can be sampled and copied, and has no stencil.

**Stencil on camera draws.** A pass's `Stencil` state is resolved per draw into the draw's state: the comparison, pass, fail and depth-fail operations (the serialized state carries each operation as a number from 0 to 7, which this engine maps to `Keep`, `Zero`, `Replace`, `IncrementClamp`, `DecrementClamp`, `Invert`, `IncrementWrap`, `DecrementWrap` in that order), the read and write masks and the reference, each of which may name a material property (`Ref [_StencilRef]`).
A face takes its own value of a setting unless it holds the unset one (keep, `Disabled` or `Always`), and then the value set for both faces.
A state that keeps the buffer and always passes is no stencil.
The pipeline takes the operations and masks and is keyed without the reference; each draw sets its own reference on the pass (`CheckedDraw::with_stencil_reference`), so draws of one pipeline with different `Ref` values each write their own.
The stencil is kept between passes and cleared to 0 with the depth.
A draw whose pass has no stencil aspect (no depth buffer, 16 bits, a depth-only render texture, or 32 bits without the device feature) ignores its stencil state: the test passes and nothing is written, because there is no stencil buffer to test or write.
A value outside the numbers the engine maps (0 to 7 for an operation, 0 to 8 for a comparison) or past 255 for a mask or the reference is a named `StateError::UnknownValue`.
A `Clear` record's stencil value is not drawn; only the depth clear's reset to 0.

A source texture takes the render table's codes and 106 (`Bc6hRgbUfloat`), the unsigned-float form of BC6H, which the player stores some of its maps in.
A BC6H source is uploaded as its blocks (`create_texture_2d` takes whole 4x4 blocks of 16 bytes for a block-compressed format) and the draw samples decoded floats, so the device needs `TEXTURE_COMPRESSION_BC`; without it the upload is the named `ProgramError`.
A render texture may not be 106.
A plain 2D source uploads its base level only.
An array, a cube and a volume upload every level they state in `SourceTexture::mips`: `SourceDimension::Cube` is a cube of six square faces in the order +X, -X, +Y, -Y, +Z, -Z, each followed by the rest of its mip chain (`create_texture_cube(label, edge, mips, format, bytes)`), which a draw samples through a cube view, and `SourceDimension::D3(depth)` is a 3D texture of `depth` slices whose `bytes` are its mip levels from the base level down, each holding its slices one after the other and as wide, as high and as deep as the base level shifted down, at least one texel (`create_texture_3d(label, [width, height, depth], mips, format, bytes)`), which a draw samples through a 3D view.
A block-compressed format is not uploaded as a volume.
A DXT1 or DXT5 array (codes 96, 97, 100 and 101) takes whole 4x4 blocks for each level, needs `TEXTURE_COMPRESSION_BC` as a 2D source does, and must have a base level of whole blocks.
A shader slot declared as a plain 2D texture over an array of several layers is the named `OffscreenError::FlatSlotOverArray`; a slot over an array of one layer has a 2D view and samples it.
A slot whose declared view dimension the stored texture cannot give otherwise (a 2D slot over a cube, cube array or volume, a cube slot over a texture that is not six square layers, a 3D slot over anything but a volume, and a volume read through any other view) is the named `OffscreenError::SlotDimensionMismatch`, which gives the property and both dimensions.
A frame names the texture an unset, undeclared slot reads per view dimension in `OffscreenFrame::undeclared_textures`: `d2`, `cube` and `d3`.

A render texture lives in the state until `OffscreenState::retain_render_textures` is given a live set that omits it.
A host whose script destroys render textures calls it with the ones that still exist before each build, and the storage of the others goes back to the budget (when the GPU has finished with it).
Without that call the state keeps every render texture any frame ever named.

GraphicsFormat 0 (`None`) with depth bits is a depth-only render texture: its storage is one depth texture with the spec's layers and mips, and that depth is the texture that `OffscreenState::texture_handle`, copies, readbacks and sampled reads use.
A camera renders into it in a `RasterPass` with no color attachment (`colors` is empty) and the pipelines have no color target; render-post's checks accept such a pass, so the contract needed no extension.
`TargetSignature::depth_only` is the crate-internal signature of that pass, and `TargetSignature::color` is `None` for it.
`TargetSignature::new` still refuses anything but one color attachment.
A blit with a material needs a color attachment to draw over and refuses a depth-only target with `OffscreenError::NoColorAttachment`.
`CopyTexture` between two depth-only textures of the same format is a texture copy, and a blit with a material samples the depth as a non-filterable float through a point sampler (the rule for textures the device cannot filter).
A readback copies into a buffer, which `Depth24Plus` does not allow, so `read_back` of it is `OffscreenError::UnreadableFormat`; `Depth32Float` reads back.
GraphicsFormat 0 with no depth bits is `OffscreenError::UnsupportedFormat { code: 0 }`.

`OffscreenFrame::taps` lists indices into `records`; right after planning each such record the state copies the record's target (`Record::target`) into staging buffers, colour (`TextureAspect::All`) and depth (`DepthOnly`, from the depth allocation's own format), at mip 0 and layer 0, with the version the cell has at that point of the frame.
The depth of a tap is the depth the record rendered with (`Record::depth_target`): the depth attachment a clear, a draw list or a procedural draw binds, which may be a render texture of its own, and a camera's target's own depth buffer. A blit, a copy, a mip generation and a clear or draw that binds no depth have none and refuse it by name.
`OffscreenOperations::take_taps` gives one `TapRead` for each tap, whose `colour` and `depth` are a `PendingImage` or the reason there is none: a record that draws nothing (`Globals`), a target without colour or without a depth buffer, a depth format whose depth aspect cannot be copied (`Depth24Plus`, `Depth24PlusStencil8`) and an index past the last record.
A refusal is data, not an `OffscreenError`, so a bad tap never drops the frame's draws.
The copies sit in the frame's own submission, so the frame ends with the same image with or without taps.
`PendingImage::resolve` waits for the submission and reads a tap without panicking.
`read_back` and the taps share one copy recorder, `readback_operation`.

A pass declares its textures as filterable floats.
A texture whose format the device cannot filter, such as `Rgba32Float` without `FLOAT32_FILTERABLE`, is bound as a non-filterable float and read through a point sampler that keeps the texture's wrap modes (clamp to the edge when it has no sampler state), so such a read returns the nearest texel instead of failing the contract's sample type check.
Depth textures are read this way too.
A slot that declares its texture non-filterable (`Float { filterable: false }`) is read the same way whatever the format.
The sampler decides the pipeline's validity as much as the texture does, since wgpu refuses a filtering sampler paired with a non-filterable float texture: a sampler that a texture parameter pairs with is non-filtering when that texture is, a sampler shared by several textures is non-filtering when any of them is, a sampler no parameter pairs with (an inline sampler of a Unity shader) is non-filtering when any texture of the draw is, and a slot the program declares `NonFiltering` always is.
The player asks for `FLOAT32_FILTERABLE` where the adapter has it; with the feature the device filters 32-bit floats and the declared filtering applies.

### Storage images

A host that runs compute kernels over render textures marks the textures a kernel writes before it builds: `OffscreenState::bind_as_storage(ids)`.
A marked texture is created with `STORAGE_BINDING` usage when the device binds its color format as a write-only storage image (`texture_storage(id)` shows the format and whether it did), and a texture that already existed without it is created again, empty, by the first build after it was marked.
Marks are kept, so later builds keep the texture.
A marked texture whose format the device cannot bind is created without storage, as an unmarked one would be, and the binding is the host's to refuse by name.
A depth-only render texture is never a storage image.

A `RenderTextureSpec` has a `dimension` that states the shape and its layers: `D2` (one layer), `D2Array(layers)` (a 2D texture of that many layers, which may be one), `D3(depth)` (a 3D texture that deep with one mip and no depth buffer, `VolumeShape` otherwise), `Cube` (six square layers), `CubeArray(cubes)` (six layers per cube, square) and `Unknown(code)`, which the state refuses as `UnknownDimension` and makes nothing.
A cube that is not square is refused as `CubeShape`, and a depth or layer count past the device's `max_texture_dimension_3d` or `max_texture_array_layers` as `DepthLimit`.
The texture is made with those layers the first time a frame names it, so a kernel's first dispatch finds the shape the host stated.
A kernel reads and writes a 3D texture, and a record of the frame that names one is refused as `VolumeInGraph`, since the graph tracks 2D cells (an uploaded `SourceDimension::D3` source of any depth is a single cell whose levels are read together, and a draw samples it through a 3D view; the resource registry gives a 3D view of a volume whole); arrays, cubes and cube arrays are 2D cells per layer and are not refused.
`OffscreenState::texture_view_dimension(id)` gives the view dimension a draw or a kernel samples the texture through (`D2`, `D2Array`, `D3`, `Cube` or `CubeArray`, so a one-layer array has an array view), `storage_view_dimension(id)` the view a storage image binds through (the same, except that a cube and a cube array bind as a `D2Array` of their faces) and `texture_dimension(id)` the dimension the state made it with.

### Camera draws

Renderers are selected by the camera's culling mask against the renderer's layer and drawn in the order of their material's render queue; equal queues keep input order.
There is no front-to-back sorting.
One `CheckedDraw` is built per sub-mesh from the resolved program, the packed constants and the bindings.
Vertex buffers follow the variant's vertex input table: each (source channel, target) pair takes the mesh channel `source` and feeds shader location `target`.
A source channel the mesh lacks is `OffscreenError::MissingChannel`.
Culling is the pass's, with clockwise front faces as the other draws here; mirrored transforms are not detected.

### Blits

A blit with a material binds its source under the main texture property name `OffscreenFrame::main_texture` (default `_MainTex`, the name the public blit API uses).
Every draw, a blit included, fills texel sizes for the textures its layers bind: a constant buffer member named `<texture>_TexelSize` holds `1/w, 1/h, w, h` of the first level of the texture bound under `<texture>`, below every layer, so a layer that sets the member wins (`DRAW_LAYERS` lists it last as `size of the bound texture`).
Old World's terrain PVT normal pass declares `_GlobalTerrainLastHeightmap_TexelSize`, which the game never sets; without it the pass wrote 0 to red and green of every texel of its normal.
The vertex stage reads a quad of two triangles in clip space.
`OffscreenFrame::blit_values` is a layer above the source and the material for every blit with a material: the host puts there what the blit call itself sets, such as the matrices a pass places its quad with.
A member the pass reads that no layer supplies packs as zero, so a pass that places the quad by a matrix nothing supplies collapses it to a point and draws nothing.
A blit without a material is a texture copy when the size and format agree.
Otherwise the source's first level is drawn over the target's first level by a built-in program: one triangle with no mesh, texture coordinates 0 to 1 across the target (scale one, offset zero), sampled at level 0 with the source's own filter and wrap (linear and clamped when it has none).
Sampling decodes sRGB and block formats and the target encodes, so sRGB to sRGB keeps its values within one level.
A source that holds depth, is not a floating-point color format, or is the target itself is `OffscreenError::BlitNeedsMaterial`.

### Copies

`CopyTexture` checks the source and destination before the executor does: the mip and the element exist, the sizes at those mips are equal and the formats are equal up to sRGB.
Anything else is `OffscreenError::CopyMismatch` (size or format) or `OffscreenError::NoSuchSubresource`.

### Generating mips

The engine writes the base level of a render texture and no other, so a texture with a mip chain that a draw samples holds nothing in its lower levels, and the draw binds only the levels the frame wrote (`partial_mip_reads()` counts those reads).
`Record::GenerateMips { texture }` fills the chain, and is a typed operation in call order like a blit or a copy.

- **What it averages.** Level `n` of each layer is drawn from level `n - 1` of the same layer, for `n` from 1 to the texture's mips, and every layer of a 2D array and every face of a cube or cube array is generated on its own from its own level 0.
  The draw is the built-in program of a blit between sizes (one triangle, no mesh) read through a bilinear sampler that clamps at the edge.
  A destination texel's centre lies on the corner shared by the 2x2 source texels it covers, so each destination texel is the average of those four, with equal weights: a 2x2 box.
  A side that halves from an odd one has no such corner and reads the texel its centre falls between, which is not an exact box. sRGB formats are read as linear values and written encoded, so the average is taken in linear light.
  Level 0 is read and never written, and only the levels below the base change.
- **What it refuses.** A 3D render texture is `OffscreenError::MipsOfVolume`.
  A texture with no color storage (a depth-only render texture) and a color format the device does not filter as floating-point values, such as `R32Float` and `Rgba32Float` on a device without `FLOAT32_FILTERABLE`, are `OffscreenError::MipsOfFormat`, which names the format.
  A texture that is not a render texture of the frame is `NotRenderTexture`.
  Refusals do not depend on the texture's mip count, so a refused texture is refused even when it has one level; a texture of one level that is accepted has no pass.
- **Versions.** Each level takes one new version of the texture's storage, shared by all of its layers, in the order the passes run.
  A later sampled read therefore finds every level written at a version of its own and binds the whole chain even when the frame wrote no base level (a texture drawn in an earlier frame), and `partial_mip_reads()` is 0.
  Level 0 keeps the versions its own writes gave it, per layer when copies wrote the layers one by one.
- **Frame.** The passes are operations in call order inside the frame's one submission.
  A caller that changes level 0 after the record must record it again to refresh the chain.

### Vertex source channels in the Old World player

`unity-import player-shader-channels <player data directory>` counts, through the engine's decoders, how many variants read each source channel of their vertex input table (`BindChannels`: pairs of a source channel and a shader input).
Over the 171 shaders with a Vulkan platform and their 10,773 variants (none unreadable):

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

Channels 0 to 4 matter: 0, 1 and 4 are read by most variants, 2 and 3 by a third.
Channel 5 is read by 159 variants and every channel from 6 on by at most 7.
The shader inputs fed are 13 to 24; inputs 13 to 17 take 10,002, 8,647, 7,913, 4,037 and 3,328 variants and the rest at most 13.
Tables hold 0 to 6 pairs (771 variants none, 3,876 three, 3,315 five) and 2 variants hold 12.
No variant reads a source channel or feeds an input twice, and in all 10,773 the source mask is exactly the bits of the pairs' source channels, so the mask adds nothing a caller needs.
The numbers are the codes the table stores; this crate maps them to mesh channels only through `MeshChannel::source`, so a host adapter decides what each code is.
`tests/program/offscreen/camera.rs` draws with codes 0 and 4 and refuses a missing channel by name; the other codes take the same path.

## Loop continuing blocks

naga 30's Metal writer prints a loop's `continuing` block at the top of the next iteration, before the body, and prints a body expression it has not named yet inline.
A body `Load` that `continuing` uses is then read again at its use, after any store `continuing` made to the variable first.
DXC writes that shape whenever a continue block computes one phi's next value from another phi; terrain block compression packed every BC index three bits high on Metal because of it.
The naga IR and the WGSL and SPIR-V output are correct, and naga's trunk fixes its Metal writer (gfx-rs/wgpu#4558), but no release has the fix yet.

So `StageProgram::new` runs a pass (`src/continuing.rs`) after validation, for SPIR-V and WGSL alike.
Each body value that `continuing` or `break_if` uses and whose value depends on when it is evaluated (a load, an image read, a derivative, a statement's result) is stored into a local named `continuing_snapshot` right after the body computes it.
`continuing` starts by loading those locals and computing again, from the loads, the body's arithmetic and addresses that lead from them to its uses.
Nothing writes the snapshots in `continuing`, so every backend reads the body's values however it prints them.
The module is validated again when the pass changed it.
A value no local can hold is `ModuleError::ContinuingSnapshot`, naming the function and the loop.
Delete the pass when the engine moves to a naga release with the hoist fix.

`tests/loops.rs` runs two SPIR-V fixtures of that shape (`tests/fixtures/loop_continuing/`, assembled with `spirv-as --target-env vulkan1.1` from the `.spvasm` next to them) and one WGSL kernel.
It checks the IR, the Metal text naga writes with wgpu-hal's options, and the words each kernel writes on the GPU.

## Measured conventions

The tests in `tests/program/conventions.rs` draw into an 8x8 target through the adapter and compare bytes.
They pin what a Unity Vulkan module needs, which holds for any SPIR-V stage:

- The module's position output is used unflipped.
  Positive clip Y lands in the bottom rows of the image.
- The depth range that survives clipping is 0 to w, as in Vulkan; the nearer of two triangles wins under `LessEqual`, and z of -0.5 or 1.5 is clipped.
- `wgpu::FrontFace::Cw` marks a triangle that is clockwise in the image, with row 0 at the top, as a front face.
  Cull Back drops the others and Cull Front drops the front faces.
  The winding is `RasterState::front_face` in the neutral state and a field of the adapter's draw request, so a caller states it.

`tests/program/pass.rs` and `drawn_state.rs` draw a textured pass with a tint from a constant buffer, the multiply and add keyword variants, a split combined sampler, blend off against `SrcAlpha`/`OneMinusSrcAlpha`, and cull Back against Off for a back-facing quad.
Pixels are predicted in closed form from the texels, tint and offset, independent of the shader, and compared with the 8-bit bound from `worldrunner_render_gpu::tolerance_test_support` (0.625 steps).
The alpha-blend test adds 0.5 step because lavapipe's fixed-point blend measured 0.8 steps from the prediction; the Radeon stayed within the shared bound.

## Not done

- **Engine values.** `BindingSource::Engine` names a binding the caller fills with a resource it already owns (below).
  There is no value provider for scalar and matrix engine values: a native game sets the matrices, times and environment resources its shaders read as ordinary properties.
  Computing object, camera and light matrices, time and shadow or environment values from a scene is not done.
- **Releasing programs.** A material can be released; a registered program lives as long as the runtime or a material that uses it.
- **Per-variant state.** A program has one render state for all its variants.
- **Struct arrays and specialization.** A constant layout holds scalar, vector and matrix members.
  Struct arrays are not represented.
  WGSL override constants are not properties: a stage sets them with `with_overrides`.
- **Unity sampler state.** Every sampler binding gets one fixed sampler: linear filtering, clamped to the edge.
  The packed sampler state in the serialized parameters is not decoded, so wrap, filter and anisotropy settings are ignored.
- **Multisampling, stencil and multiple render targets for the plain and material draws.** `draw`, `prepare_draw`, material draws and `admit_draws` are single-sample with one colour attachment and an optional depth attachment, and refuse other signatures by name.
  Authored stencil state is refused by them; only the prepared pipelines of a renderer's pass take it.
  Renderer-owned combined depth/stencil attachments are accepted through `TargetSignature::attachments` and left alone unless a prepared pipeline's state names stencil.
  Only draws over lent resources record into several attachments or a multisampled pass, and `render-scene` takes those only for dynamic quads.
- **Common parameters of a program.** A player program stores the constant buffer members that all its variants share once, in `Program::common_parameters`, and a variant's own entry can list the same buffer with fewer members (the Old World terrain splat programs list `UnityPerDraw` with none).
  `ProgramShader` adds the common members to the variant's buffer of the same name (`VariantParameters::with_common`), by the pass's name table; a member the variant lists keeps its own layout.
  Struct arrays in the common parameters are not added, and none has been measured.
- **Frame planning and composition beyond one pass.** The runtime still owns no pass order and no attachments.
  `render-scene` takes admitted draws into its stock opaque pass for one single-sample colour attachment and `Depth32Float`.
  It refuses scenes with temporal reconstruction, screen-space passes or multisampling by name.
  Camera render textures, sampling a draw's target in a later pass, compute between passes, transparent ordering, the depth-fitted capture continuation and `render-plan` descriptions of program work are not done.
  The standalone target that `create_render_target` makes remains the caller's to create and release.
- **Offscreen work.**
  - Camera renders are drawn in render queue order without front-to-back or back-to-front sorting, and a draw's culling is the pass's.
    Mirrored transforms do not flip the winding.
    Multisampled, cube and 3D render textures are not covered.
    A camera or a blit draws into mip 0 of element 0; other mips and elements are reached by `CopyTexture`, which the array test does.
  - The GraphicsFormat table has 4, 5, 8, 45, 48, 49 and 52, format 0 with depth bits as a depth-only target, and the depth sizes 16, 24 and 32; any other code is `OffscreenError::UnsupportedFormat`.
    Formats Old World asks for beyond these are not known from the unity-import report.
  - A blit without a material reads and writes the first level of the first layer only.
  - `later_reads` names the ranges a scene draw samples; the host must name them, because a scene draw that samples a render texture is not an operation.
  - The host adapter that maps the world export onto `OffscreenFrame`, builds the `ProgramShader` of each shader and supplies the built-in per-draw and per-camera values is the `offscreen` module of `play`, see [its README](../play/README.md#offscreen-work-from-a-player).
    The built-ins it leaves missing show up in `OffscreenOperations::missing`.
  - Property blocks, per-instance arrays and instancing of a renderer's many sub-meshes are built one draw each; draws are not batched.
- **Calling shader preparation.** `unity_serialized::spirv_prep::prepare_stages` is in the Foundation layer, which this crate may depend on, but the runtime does not call it.
  Callers supply the prepared words and `PreparedModule::bindings` of each stage, prepared together with the variant's other stages.
  `unity-import player-shader-layouts` does this for every Vulkan variant of a player.
- **Multisampled textures and compute masks.** Dimension codes 5 and 11 are multisampled images, which are named errors because the adapter declares no multisampled player texture.
  A compute stage's bit in the stage mask has not been seen, so compute ignores the mask.
- **Player struct shapes beyond the measured one.** A struct that starts anywhere but byte 0, a buffer with a struct and members, a buffer with several structs, and editor-style structs are `ConstantError::UnsupportedStruct`, because the player has none: all 5,605 buffers with structs are one struct array at byte 0 and nothing else.
  Whether `index` is a byte offset, a position or an ordinal is therefore open.
  Structs inside structs and arrays inside structs are not seen (the SPIR-V struct types hold none) and are refused as unsupported values.
  Which property the engine fills the struct members from (the per-draw instance data) is not decided here: a caller supplies them under the members' own names.
- **Player member kinds and matrix shapes.** Only float (0) and int (1) member kinds and matrices of four columns are written.
  Every one of the 201,382 members of the Old World player's bound buffers is one of those (float scalars, vectors and 4x4 matrices, with or without an array, and int scalars), and so are the 10,931 members of its structs, which add int4.
  So a bool, unsigned, narrower matrix or int vector at the top level is a named error that the player checked never reaches.
- **Prepared numbering across stages.** The stages of a variant must come from `spirv_prep::prepare_stages`, which gives every stage one binding table.
  Modules prepared one at a time by `prepare` number the same Unity binding differently when they split different samplers, and `ProgramLayout::build` then reports `LayoutError::Conflict`: in the Old World player that was 1,350 of 10,768 two-stage variants, and with the stages prepared together none of them is, and all 10,768 share one numbering.
  `prepare_stages` fails with `PrepareError::BindingClass` when stages declare one Unity binding as different classes of resource; no variant in that player does.
  The table lists every binding any stage declares, so a stage's table can hold entries its module does not declare: a parameter that names such a binding for the stage is reported in `ProgramLayout::unused`, not as `LayoutError::UnboundSlot`.

## Tests

The suites that build Unity records run only with the `unity` feature, and the `program` suite also needs `test-support` (which implies `unity`) for the synthetic shaders of the offscreen tests:

```sh
cargo test -p worldrunner-render-program --features test-support -- --test-threads=1
```

Without the feature, `cargo test -p worldrunner-render-program` runs the unit tests, `tests/native.rs`, `tests/imports.rs`, `tests/targets.rs`, `tests/lent/` and `tests/material/`, builds `examples/native_material.rs` and runs the guide's code blocks as doctests.
`cargo xtask test` enables every feature.

`tests/program/player_structs.rs` packs struct arrays at the byte level from hand-built player descriptions, including every named error.
`tests/program/player_struct_block.rs` builds a uniform block of an array of structs with naga from WGSL, predicts its layout by hand, reads the `Offset` and `ArrayStride` decorations naga wrote back from the SPIR-V, compares the packed bytes with them and runs a compute kernel on the GPU that reads each element.

The draw, dispatch and layout tests in `tests/program` run once with editor-style parameters and once with a player variant's, from equivalent hand-built descriptions, and must agree.

`tests/program/packet.rs` covers preparation and recording into a caller's pass, and `arena.rs` covers admission, authority, lifetime and budget refusals.
The frame tests in `render-scene` enable the feature as a dev-dependency and run with `cargo test -p worldrunner-render-scene --lib -- program_frame --test-threads=1`.

The draw and dispatch tests need a Vulkan adapter and use the one wgpu selects.
They obtain the device through `render-gpu` (`headless_instance`, `request_device`), build a `SubmissionAuthority` and submit every frame through it.
The first test to create a runtime prints the adapter's name to stderr; run with `--nocapture` to see it.
Set `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json` to run them on lavapipe.

### Offscreen tests

`tests/program/offscreen/` uses tiny player-shaped shaders built with naga from WGSL (`src/unity/synthetic.rs`, behind the `test-support` feature, which implies `unity` and which `render-scene` also enables for its own tests).
Each has Unity-shaped `VariantParameters` and a vertex input table; the camera shader has two passes tagged with LightMode values and two keyword variants.
Predicted pixels come from the placements, clear colors, tints and inversions, never from the output:

- `resolve.rs`: pass by index and by tag list, exact keyword match completed with the default keyword of each required group (`variant_required_group.rs`), missing pass and missing variant named, variants cached by content.
- `camera.rs`: two renderers filtered by layer over a clear color, render queue order, depth, a camera that does not clear, missing vertex channel and missing texture named, missing property listed.
- `layers.rs` and `sampler.rs`: the four layers in priority order and the camera below the renderer; repeat, clamp and mirror against the fixed sampler at predicted texels; a texture input's state over the texture's.
- `blit_values.rs`: a blit pass that places its quad by built-in matrices covers the target when the frame's blit values supply them, draws nothing when no layer does, and keeps them against a global of the same name.
- `blit.rs` and `copy.rs`: a material pass by index, the texel size vector, the main texture name, a plain copy, element and mip copies, and the named errors for sizes, formats, subresources, unsupported formats and depths.
- `mips.rs`: a base level written by a camera, then read by a draw with and without the chain generated (`partial_mip_reads()` 1 and 0, with the pass-by-pass listing); a chain written level by level; `GenerateMips` read back level by level against an exact 2x2 box average of a synthetic pattern for a 2D texture, for each layer of an array and each face of a cube; generated levels of a texture whose base an earlier frame wrote; and the named refusals of a 3D texture, a depth-only texture, 32-bit float formats and an uploaded texture.
- `formats.rs`: cameras and blits into `R16Float` and `Rgba32Float` render textures read back at predicted half-float and float values, and a 2x2 float texture blitted up to 8x8 through the blit pass's filtering sampler: nearest texels when the device cannot filter the format, bilinear blends for `Rgba16Float`.
- `unfilterable.rs`: a 2x2 `Rgba32Float` render texture read by a blit whose sampler no texture parameter pairs with: the pipeline builds and the 8x8 result is the nearest texel on a device without `FLOAT32_FILTERABLE`, and the linear blend computed in the test on one that has it.
- `undeclared.rs`, `undeclared_cube.rs` and `undeclared_volume.rs`: a texture property no layer sets and the shader does not declare reads the frame's grey default for the slot's dimension (`OffscreenFrame::undeclared_textures`, one entry for 2D, one for cube and one for 3D) and is counted by property; a declared property, a slot of a dimension without a default and a frame without the default stay `MissingTexture`; the cube default is read along its own faces and the 3D default is read from the texture the frame names (`_3DNoiseTexture` in Old World).
- `procedural.rs`: `DrawProcedural` draws plan their textures and stencil state as camera and list draws do: an unset undeclared texture reads the frame's grey default and is counted, and a `Replace` draw marks the stencil that a later `Equal` draw tests.
- `depth_only.rs`: a camera into depth-only textures of 32 bits (depth read back as exact floats), and of 24 bits (depth sampled by a blit, and carried by a `CopyTexture`), the pass with no color attachment, and the named errors for a blit into a depth-only target, a `Depth24Plus` readback and format 0 without depth.
- `lifetime.rs`: contents kept between frames, re-creation on a new generation or size with the old storage released, the budget, and a cancelled frame that moves no counter and leaves the arena free.
- `render-scene`'s `draw/tests/program_frame/offscreen.rs`: a camera into a render texture, a blit with a material pass, a copy into an array element and a scene draw sampling that element, with a readback after each stage, one submission and the passes at the head of the frame's timing.
  Its device requests timestamp queries and the test fails if there are none.

```sh
cargo test -p worldrunner-render-program --features test-support --test program -- offscreen --test-threads=1
cargo test -p worldrunner-render-scene --lib -- program_frame --test-threads=1
```
