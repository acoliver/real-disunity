# Drawing with your own shader programs

`worldrunner-render-program` runs shader programs that a Rust game describes itself. You register a program, make a material from it, set the material's properties and features by name, and draw it into a frame you own. The calls need no shader-file format and no Unity type, and they work with the crate's default features.

This page covers the native calls. The crate also has a default-off `unity` feature that converts Unity player programs into the same descriptions; the [crate README](../crates/render-program/README.md) describes that adapter.

## A complete program

This program draws a triangle that covers a 4 by 4 offscreen target, using a WGSL shader whose fragment colour comes from a material property. It reads the target back and checks one pixel. The same code is `crates/render-program/examples/native_material.rs`; run it with `cargo run -p worldrunner-render-program --example native_material`.

```rust
# use std::error::Error;
# use wgpu::TextureFormat;
# use worldrunner_render_gpu::configuration::AllocationBudget;
# use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
# use worldrunner_render_gpu::submission::SubmissionAuthority;
use wgpu::ShaderStages;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, PropertyId, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::constants::{
    BoundConstantLayout, ConstantLayout, Member, ScalarKind,
};
use worldrunner_render_program::module::{StageDeclarations, StageKind};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::registry::{ProgramDesc, StageDesc};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{
    Geometry, MaterialDraw, MaterialHandle, ProgramRuntime, VertexInput,
};
use worldrunner_render_program::state::RenderState;

const SHADER: &str = "
struct Tint { color: vec4<f32> }
@group(0) @binding(0) var<uniform> tint: Tint;
@vertex fn vs(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return tint.color; }";

/// A uniform buffer at group 0 binding 0 that holds the `color` property as a `vec4<f32>`.
fn fragment_declarations() -> StageDeclarations {
    let at = BindingAddress { set: 0, binding: 0 };
    StageDeclarations {
        resources: ResourceLayout::new(vec![ResourceBinding {
            at,
            source: BindingSource::Property(PropertyId::new("tint")),
            visibility: ShaderStages::FRAGMENT,
            kind: ResourceKind::Uniform { min_size: 16 },
        }]),
        constants: vec![BoundConstantLayout {
            at,
            layout: ConstantLayout {
                name: "Tint".to_owned(),
                byte_size: 16,
                members: vec![Member {
                    source: BindingSource::Property(PropertyId::new("color")),
                    kind: ScalarKind::F32,
                    rows: 1,
                    cols: 4,
                    array: None,
                    offset: 0,
                    matrix: None,
                }],
            },
        }],
    }
}

/// One counter-clockwise triangle that covers all of clip space.
fn triangle(runtime: &mut ProgramRuntime) -> Result<VertexInput, ProgramError> {
    let corners: [[f32; 3]; 3] = [[-1.0, -1.0, 0.5], [3.0, -1.0, 0.5], [-1.0, 3.0, 0.5]];
    let bytes: Vec<u8> = corners.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
    Ok(VertexInput {
        buffer: runtime.create_buffer("triangle", &bytes)?,
        stride: 12,
        step: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0 => Float32x3].to_vec(),
    })
}

/// Registers the shader and makes a material of it with the colour `color`.
fn tint_material(
    runtime: &mut ProgramRuntime,
    color: [f32; 4],
) -> Result<MaterialHandle, ProgramError> {
    let program = runtime.register_program(ProgramDesc::single(
        "tint",
        RenderState::opaque_depth(),
        StageDesc::wgsl(SHADER, "vs", StageKind::Vertex, StageDeclarations::default()),
        StageDesc::wgsl(SHADER, "fs", StageKind::Fragment, fragment_declarations()),
    ))?;
    let mut initial = PropertySet::new();
    initial.set("color", PropertyValue::Vector([1.0, 1.0, 1.0, 1.0]));
    let material = runtime.create_material(program, initial)?;
    runtime.set_property(material, "color", PropertyValue::Vector(color))?;
    Ok(material)
}

# fn main() -> Result<(), Box<dyn Error>> {
# let instance = headless_instance();
# let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
# let (device, queue, _) =
#     pollster::block_on(request_device(&adapter, &DeviceRequirements::default()))?;
# let authority = SubmissionAuthority::new(queue);
# let mut runtime = ProgramRuntime::new(device.clone(), authority.clone(), AllocationBudget(1 << 24));
let color = [0.25, 0.5, 0.75, 1.0];
let material = tint_material(&mut runtime, color)?;
let vertices = triangle(&mut runtime)?;
let target = runtime.create_render_target(
    "offscreen",
    [4, 4],
    TextureFormat::Rgba8Unorm,
    Some(TextureFormat::Depth32Float),
)?;
let draw = MaterialDraw {
    material,
    vertices: vec![vertices],
    geometry: Geometry::Vertices { first: 0, count: 3 },
    instances: 0..1,
    overrides: PropertySet::new(),
};

let mut frame = authority.begin(&device)?;
runtime.clear_target(&mut frame, target, [0.0; 4], 1.0)?;
runtime.draw_material(&mut frame, target, &draw)?;
let pending = runtime.read_target(&mut frame, target)?;
let submission = frame.finish().submit()?;
let image = pending.resolve(submission)?;

assert_eq!(image.pixel(2, 2), [64, 128, 191, 255]);
# Ok(())
# }
```

The runtime never owns the queue and never submits. `authority.begin` opens the frame, every call records into it, and `frame.finish().submit()` is the one submission. The pixel check holds because 255 times 0.25, 0.5 and 0.75 round to 64, 128 and 191.

## Registering a program

`ProgramRuntime::register_program(ProgramDesc)` parses and validates every stage with naga, checks each stage's declarations against the resources its entry point uses, builds the bind group layouts and returns a `ProgramHandle`. Nothing is registered when a step fails, and the error names what was wrong. A registered program does not change.

A `ProgramDesc` has a label, a `RenderState` and a list of `VariantDesc`. Each variant has a feature set, a vertex stage and a fragment stage. `ProgramDesc::single` builds a program with one variant for the empty feature set.

A `StageDesc` holds a `StageModule` and its `StageDeclarations`. `StageDesc::wgsl` takes WGSL text and `StageDesc::spirv` takes SPIR-V words. Both take an entry point name, the stage kind and the declarations.

| Declaration | Type | What it says |
| --- | --- | --- |
| Resources | `ResourceLayout` of `ResourceBinding` | The group and binding, the stages that see it, and its kind: uniform or storage buffer, texture, or sampler. |
| Constants | `BoundConstantLayout` of `ConstantLayout` and `Member` | A uniform buffer the runtime fills from properties: each member's name, scalar kind, rows, columns, array count, byte offset and matrix layout. |

Declare every resource the entry point uses, as the kind the shader has and visible to the stage that uses it. Offsets and strides come from your declaration, so state the ones your shader's layout has. A declaration the entry point does not use is allowed and gets no slot.

A binding falls into one of three groups:

- A binding with a constant layout is a buffer the runtime packs from the properties named by the layout's members, once for each draw.
- A buffer or texture binding without a constant layout takes the property named by its `BindingSource::Property` and expects a resource handle.
- A sampler binding gets one fixed sampler: linear filtering, clamped to the edge.

The render state must be one a draw supports. Stencil, alpha to coverage and anything other than one written colour target are refused by name at registration.

## Materials and properties

`create_material(program, properties)` returns a `MaterialHandle`. `set_property(material, name, value)` changes one property, and `release_material` ends the handle.

The names a program accepts are the names of its constant members and of its buffer and texture bindings. A value must have the kind its declaration needs:

| Declaration | Accepted `PropertyValue` |
| --- | --- |
| `F32` member, one row | `Float` for one column, `Vector` for up to four, `VectorArray` for an array member |
| `F32` matrix member | `Matrix`, `MatrixArray` for an array member |
| `U32` member | `Uint`, or `Int` of zero or more |
| `I32` member | `Int` |
| `Bool32` member | `Bool`, or `Int` of 0 or 1 |
| Buffer binding | `Resource(Resource::Buffer(handle))` |
| Texture binding | `Resource(Resource::Texture(handle))` |

A matrix holds element (row, column) at index `column * 4 + row`, and the member's layout places the values in the buffer. A property the shader reads but nothing sets stays zero, and `MaterialPacket::missing` lists it. A buffer or texture property that has no value is an error when the draw is prepared.

`set_property` and `create_material` refuse bad input by name and change nothing:

- A property the program does not have is `ProgramError::UnknownProperty`.
- A value of another kind is `ProgramError::WrongPropertyKind`, which names the property, the kind it takes and the kind it was given.
- A resource handle that was released or belongs to another runtime is `ProgramError::UnknownHandle`.

After a refused call the material draws exactly what it drew before.

## Features and variants

A feature is a name. Each variant of a program lists the exact set of features it is built for, and the program's features are the ones its variants use. `set_features(material, &["DOUBLE"])` replaces the material's enabled features and selects the variant whose set equals them.

```rust,no_run
# use std::collections::BTreeSet;
# use worldrunner_render_program::registry::{ProgramDesc, StageDesc, VariantDesc};
# use worldrunner_render_program::runtime::ProgramRuntime;
# use worldrunner_render_program::state::RenderState;
# use worldrunner_render_program::variant::FeatureId;
# fn enable(
#     runtime: &mut ProgramRuntime,
#     vertex: StageDesc,
#     plain: StageDesc,
#     doubled: StageDesc,
# ) -> Result<(), Box<dyn std::error::Error>> {
let program = runtime.register_program(ProgramDesc {
    label: "tint".to_owned(),
    state: RenderState::opaque_depth(),
    variants: vec![
        VariantDesc {
            features: BTreeSet::new(),
            vertex: vertex.clone(),
            fragment: plain,
        },
        VariantDesc {
            features: BTreeSet::from([FeatureId::new("DOUBLE")]),
            vertex,
            fragment: doubled,
        },
    ],
})?;
let material = runtime.create_material(program, Default::default())?;
runtime.set_features(material, &["DOUBLE"])?;
# Ok(())
# }
```

A new material has no feature enabled. `set_features` fails with `ProgramError::Variant` and keeps the old features when a name is not one the program's variants use (`VariantError::UnknownFeature`, naming it) or when no variant has exactly that set (`VariantError::NoMatch`). Registration refuses two variants with one feature set.

## Drawing

A `MaterialDraw` names the material, the vertex buffers and how the vertex stage reads them, the `Geometry`, an instance range and a set of per-draw overrides. Overrides win over the material's properties for that draw only, and they are checked like `set_property`.

- `draw_material(frame, target, &draw)` records the draw into a pass over a render target, which keeps what it holds. Clear the target first with `clear_target` when it should start empty.
- `prepare_material_draw(frame, &draw)` resolves the draw now and returns a `MaterialPacket`. It selects the variant, packs the constants into buffers of their own and takes the resources the properties name. Changing or releasing the material afterwards does not change the packet. `record_material(frame, target, &packet)` records a packet, as often as you like.
- `admit_material_draws(frame, arena, signature, &packets)` hands packets to a renderer that owns the pass. The renderer reserves an arena once, and the packets are admitted into a frame against it.

```rust,no_run
# use worldrunner_render_program::runtime::{MaterialDraw, ProgramArena, ProgramRuntime, TargetSignature};
# fn join(
#     runtime: &mut ProgramRuntime,
#     frame: &wgpu::CommandEncoder,
#     arena: &ProgramArena,
#     draw: &MaterialDraw,
# ) -> Result<(), Box<dyn std::error::Error>> {
let packet = runtime.prepare_material_draw(frame, draw)?;
let signature = TargetSignature::new(
    &[wgpu::TextureFormat::Rgba8Unorm],
    Some(wgpu::TextureFormat::Depth32Float),
    1,
)?;
let admitted = runtime.admit_material_draws(frame, arena, signature, &[packet])?;
// Give `admitted` to the renderer; it records the draws into its own pass with
// `admitted.record(&mut pass)`.
# let _ = admitted;
# Ok(())
# }
```

The instance range is the instances drawn, so `1..3` draws instances 1 and 2. An instance-rate vertex buffer must hold as many strides as the range's end, and a backwards range is `ProgramError::InstanceRange`.

### Lifetimes and budget

Every buffer, texture and render target is counted against the budget you gave `ProgramRuntime::new`. The constants a prepared packet packs are counted too. The frame that recorded or prepared a packet keeps those bytes counted until the frame is cancelled, dropped or finished on the GPU, even when you drop the packet, release the material or drop the runtime first. A cancelled frame gives them back at once. Admission keeps the arena in use until the frame completes, and `ProgramArena::retire` fails with a busy lease error until then. The [crate README](../crates/render-program/README.md) lists the rules for each call.

A frame from another submission authority is refused with `ProgramError::Lease`, and an encoder that is not a frame is `ProgramError::Transaction`.

## Sets the pass owner binds

A renderer that opens a pass often binds a group of its own once, such as a frame uniform, and wants every program drawn in the pass to read it. A program can lend such a set instead of declaring it. The runtime then uses the renderer's `wgpu::BindGroupLayout` as it is, builds no group for the set and does not bind it when a draw records.

`ExternalSet` carries the set number, the layout and `bindings`, a `ResourceLayout` that says what the shader reads from the set. `graphics_program_in` checks the shader against `bindings` and builds the program's pipeline layout with the lent layout at that set:

```rust,no_run
# use worldrunner_render_program::binding::{
#     BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
# };
# use worldrunner_render_program::module::StageProgram;
# use worldrunner_render_program::runtime::{ExternalSet, ProgramRuntime};
# fn lend(
#     runtime: &ProgramRuntime,
#     device: &wgpu::Device,
#     vertex: &StageProgram,
#     fragment: &StageProgram,
# ) -> Result<(), Box<dyn std::error::Error>> {
// The layout the renderer builds its per-pass group against.
let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: Some("frame"),
    entries: &[wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }],
});
let frame = ExternalSet {
    set: 0,
    layout,
    bindings: ResourceLayout::new(vec![ResourceBinding {
        at: BindingAddress { set: 0, binding: 0 },
        source: BindingSource::Engine(EngineSemantic::named("scene.frame")),
        visibility: wgpu::ShaderStages::FRAGMENT,
        kind: ResourceKind::Uniform { min_size: 16 },
    }]),
};
let program = runtime.graphics_program_in(vertex, fragment, &[frame])?;
assert_eq!(program.external_sets(), [0]);
# Ok(())
# }
```

- **What is checked.** Each resource a stage uses at a lent set must be listed in `bindings` at its address with the same class: the buffer kind and access, the texture dimension, multisampling and sample type, or the sampler class. The filterable flag of a float texture is ignored, because WGSL cannot say it: the renderer's layout can declare an unfilterable float texture that the shader reads with `textureLoad`. A used resource that is not listed is a `LayoutError::UndeclaredResource` that names the shader's variable, and one listed as another class is a `LayoutError::Mismatch`. A listed binding the shader does not use is allowed, so one declaration serves every program that reads part of the set. A stage's own declarations at a lent set are ignored.
- **What wgpu checks.** Visibility, minimum buffer sizes and the rest of the match between the shader and the layout are wgpu's to check when it creates the pipeline. The failure comes back from the draw's preparation as `ProgramError::Gpu`.
- **Pipelines.** The pipeline cache key includes the identity of each lent layout. Programs that lend one layout, cloned from one `wgpu::BindGroupLayout`, share the group the renderer binds. Two layouts with equal entries are still two layouts, and their programs do not share a pipeline, because a group built against one does not fit the other.
- **Recording.** `PreparedProgramDraw::external_sets()` lists the sets the pass owner must bind before the draw records, and `GraphicsProgram::external_sets()` lists them for the program. The pass binds each once, and every draw of the pass leaves it bound.
- **Conflicts.** A draw may not give resources to a lent set. A `BoundResource` at a lent set, an import of an engine semantic that a lent binding names and a set lent twice are each `ProgramError::ExternalSetConflict` naming the set.

The draw side is `prepare_imported_draw`, described in the [crate README](../crates/render-program/README.md#lending-a-renderers-resources). A program with no lent set behaves as `graphics_program` always has: `graphics_program_in` with an empty slice builds the same program and shares its pipelines.

### Counting what the runtime builds

`ProgramRuntime::stats()` returns a `RuntimeStats` with the number of shader modules, pipelines, bind groups and draws the runtime has built since it was made. Each count only rises, so a cost test takes a snapshot, runs a frame and compares:

```rust,no_run
# use worldrunner_render_program::runtime::{ImportedDraw, ProgramRuntime, TargetSignature};
# fn steady_state(
#     runtime: &mut ProgramRuntime,
#     draw: &ImportedDraw<'_>,
#     signature: TargetSignature,
# ) -> Result<(), Box<dyn std::error::Error>> {
let before = runtime.stats();
let _prepared = runtime.prepare_imported_draw(draw, signature)?;
let after = runtime.stats();
// A cache hit builds no module or pipeline, and a draw over lent sets builds a group only for
// the sets the program owns.
assert_eq!(after.modules_built, before.modules_built);
assert_eq!(after.pipelines_built, before.pipelines_built);
assert_eq!(after.draws_prepared, before.draws_prepared + 1);
# Ok(())
# }
```

A call that is refused before it creates an object does not count it, and a cache hit counts nothing. `sets_prepared` counts the sets `prepare_set` builds, described next; each also adds its group to `bind_groups_built`, and a draw that binds a prepared set adds neither.

## Sets built once for many draws

A material's own resources are the same for every draw that uses the material, so the runtime should build their bind group once. `prepare_set` does: it fills the bindings of one set of a program from an `ImportTable`, a map from engine semantic to imported resource, and returns a `PreparedSet` that any number of draws bind through `ImportedDraw::sets`. A draw given a prepared set for a set builds no group for it.

```rust,no_run
# use worldrunner_render_program::binding::EngineSemantic;
# use worldrunner_render_program::runtime::{
#     GraphicsProgram, Import, ImportTable, ImportedDraw, ProgramRuntime, TargetSignature,
# };
# fn share(
#     runtime: &mut ProgramRuntime,
#     program: &GraphicsProgram,
#     block: wgpu::Buffer,
#     palette: (wgpu::Texture, wgpu::TextureView),
#     draw: ImportedDraw<'_>,
#     signature: TargetSignature,
# ) -> Result<(), Box<dyn std::error::Error>> {
// Everything any variant of the material reads. Entries a program does not use are ignored.
let table = ImportTable::new()
    .with(Import::buffer(EngineSemantic::named("terrain.block"), block))
    .with(Import::texture_array(
        EngineSemantic::named("terrain.palette"),
        palette.0,
        palette.1,
    ));
let set = runtime.prepare_set(program, 1, &table, Vec::new())?;

let before = runtime.stats();
let first = runtime.prepare_imported_draw(&ImportedDraw { sets: &[&set], ..draw.clone() }, signature)?;
let second = runtime.prepare_imported_draw(&ImportedDraw { sets: &[&set], ..draw }, signature)?;
// Neither draw built a bind group.
assert_eq!(runtime.stats().bind_groups_built, before.bind_groups_built);
# let _ = (first, second);
# Ok(())
# }
```

- **What the table must hold.** A binding the program uses and the table lacks is `ProgramError::MissingImport`, which names the semantic, the set and the binding. Entries for bindings the program does not use are ignored, so one table serves every variant of a material. A sampler binding that names no engine semantic gets the runtime's sampler. A buffer or texture binding that does not come from an engine semantic has nothing to fill it and is `MissingBinding`.
- **Sharing.** The runtime keeps one `wgpu::BindGroupLayout` for each distinct list of slots at a set, so programs whose slots at a set are equal share a layout object (`GraphicsProgram::set_layout` returns it) and one `PreparedSet` fits all of them. A draw whose program has other slots at the set refuses the set with `SetLayoutMismatch`, as it does a set another runtime built. A set given twice, or given alongside an import or a bound resource for one of its bindings, is `PreparedSetConflict`. A prepared set for a set the pass owner binds is `ExternalSetConflict`, and `prepare_set` for a set the program has no bindings at is `UnknownSet`.
- **Lifetimes.** The set's bind group and the `owners` given to `prepare_set` are held by every draw that binds the set, and by every frame that retains such a draw, until the frame is cancelled, dropped or completed on the GPU. The `PreparedSet` itself can be dropped once the draws are made.

### Texture views of any dimension

An imported texture carries the dimension of its view, because a view cannot be asked. `Import::texture` assumes the texture's default view, which is `D2` for one layer and `D2Array` for several. `Import::texture_array` takes the view as a 2D array whatever the layer count, which is how a texture of one layer or a view of some of the layers is read by `texture_2d_array`, and `Import::texture_view` takes any dimension, such as one layer of an array as `D2`. A binding accepts an import when the view's dimension and the texture's sample count match the shader's declaration and the format suits its sample type, so array, `Uint` and depth views can be imported. A mismatch is `ProgramError::WrongResource` naming what the binding expects and what was imported.

### Programs from registered variants

A registered program chooses its stages by feature set. Draws over lent resources take a program rather than a material, and `variant_program` returns the program that the variant for exactly those features built at registration, so resolving it builds nothing:

```rust,no_run
# use worldrunner_render_program::runtime::{GraphicsProgram, ProgramHandle, ProgramRuntime};
# use worldrunner_render_program::variant::{FeatureId, FeatureSet};
# fn resolve(
#     runtime: &ProgramRuntime,
#     handle: ProgramHandle,
# ) -> Result<GraphicsProgram, Box<dyn std::error::Error>> {
let features = FeatureSet::from([FeatureId::new("LAYERED")]);
Ok(runtime.variant_program(handle, &features)?)
# }
```

A feature no variant differs by is `VariantError::UnknownFeature`, and a set no variant has is `VariantError::NoMatch`.

## Many geometries through one pipeline

A draw over lent resources binds its pipeline and groups every time it records. A renderer with hundreds of meshes that share a material wants to bind once and draw each mesh. Split the draw in two: a `PreparedPipeline` holds the program, state, vertex layout, prepared sets and owners, and a `GeometryDraw` holds one mesh's buffers and ranges and nothing else.

```rust,no_run
# use worldrunner_render_program::runtime::{
#     GeometryDraw, GraphicsProgram, ImportedGeometry, PipelineDraw, PreparedSet, ProgramRuntime,
#     TargetSignature, VertexBufferSpec, VertexLayoutSpec,
# };
# use worldrunner_render_program::state::RenderState;
# fn batch(
#     runtime: &mut ProgramRuntime,
#     device: &wgpu::Device,
#     frame: &mut wgpu::CommandEncoder,
#     program: &GraphicsProgram,
#     material: &PreparedSet,
#     signature: TargetSignature,
#     chunk: (&wgpu::Buffer, &wgpu::Buffer, &ImportedGeometry),
#     pass: &mut wgpu::RenderPass<'_>,
# ) -> Result<(), Box<dyn std::error::Error>> {
// Once per material and pass context: the layout of the vertex buffers, then the pipeline.
let layout = VertexLayoutSpec::new(vec![
    VertexBufferSpec {
        stride: 56,
        step: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0 => Float32x3].to_vec(),
    },
    VertexBufferSpec {
        stride: 80,
        step: wgpu::VertexStepMode::Instance,
        attributes: wgpu::vertex_attr_array![3 => Float32x4].to_vec(),
    },
]);
let state = RenderState::opaque_depth();
let pipeline = runtime.prepare_pipeline(
    &PipelineDraw {
        program,
        state: &state,
        layout: &layout,
        bindings: &[],
        imports: &[],
        sets: &[material],
        owners: &[],
    },
    signature,
)?;

// When a mesh is uploaded: pure data, with the extent checks made here.
let (vertices, instances, geometry) = chunk;
let chunk = GeometryDraw::new(&layout, &[vertices.clone(), instances.clone()], geometry, 7..8)?;

// Each frame: retain once, bind once, draw each mesh.
pipeline.retain(frame)?;
pipeline.bind(pass);
pipeline.draw(pass, &chunk)?;
# let _ = device;
# Ok(())
# }
```

- **Commands.** `bind` records `set_pipeline` and one `set_bind_group` for each group the pipeline holds. `draw` records one `set_vertex_buffer` for each vertex buffer, `set_index_buffer` for indexed geometry and the draw call. C indexed geometries over two vertex buffers and one prepared set are `2 + 4 * C` commands, where a `PreparedProgramDraw` for each geometry records `6 * C`. The sets the pass owner binds are not recorded by either call.
- **Instances.** The instance range selects records of an instance-rate buffer that many geometries share: range `k..k + 1` draws record `k`. `GeometryDraw::new` checks that the buffer holds `k + 1` strides, so a range past the buffer is refused when the geometry is made, with the buffer's slot named.
- **Layouts.** A geometry is made for a layout and a pipeline is prepared for one, and `draw` refuses a geometry of another layout with `ProgramError::GeometryLayoutMismatch` before it records a command. Layouts compare by content, so two layouts built from equal buffers are one layout. A geometry given the wrong number of buffers for its layout is `VertexBufferCount`.
- **Retention.** A frame that calls `PreparedPipeline::retain` once holds the pipeline's owners and the owners of its prepared sets until it is cancelled, dropped or completed on the GPU, however many geometries it draws. Put whatever keeps the geometry's buffers alive in the pipeline's `owners`; wgpu itself keeps a buffer that a recorded command uses.
- **Cost.** Neither `bind` nor `draw` reaches the runtime, so a frame that binds and draws prepared pipelines and geometries adds nothing to `RuntimeStats`. `prepare_pipeline` is not a draw and adds none to `draws_prepared`.
- **Passes.** `bind` and `draw` take any `PassCommands`, which `wgpu::RenderPass` implements. `PreparedProgramDraw::record` is `bind` then `draw` for one geometry, so existing callers are unchanged.

## One parse for a whole source

A vertex stage and a fragment stage are usually two entry points of one WGSL text. `StageProgram::new` parses and validates the text for the entry point it is given, so building both with it parses twice. `sibling` makes another entry point of the same source from the first stage's parse:

```rust
# use std::sync::Arc;
# use worldrunner_render_program::module::{
#     StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
# };
# fn main() -> Result<(), Box<dyn std::error::Error>> {
const WGSL: &str = "
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32(index), 0.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }";
let vertex = StageProgram::new(
    StageModule {
        source: StageSource::Wgsl(Arc::from(WGSL)),
        entry_point: "vs".to_owned(),
        kind: StageKind::Vertex,
    },
    StageDeclarations::default(),
)?;
let fragment = vertex.sibling("fs", StageKind::Fragment)?;
assert!(fragment.shares_parse_with(&vertex));
# Ok(())
# }
```

A sibling has no declarations and no overrides, so give it its own with `with_declarations` and `with_overrides`. It reads the resources of its own entry point, and an entry point the source lacks for that stage is `ModuleError::EntryPointMissing`.

- **Registration.** `register_program` makes the stages of a program this way when their sources are equal, across the variants of one program too, so a program written as one WGSL text is parsed once. A `StageDesc` needs no change.
- **Shader modules.** The runtime compiles one `ShaderModule` for each source, and every entry point of the source, with any overrides, uses it. A program whose two stages share a source adds one module to `RuntimeStats::modules_built` and `cached_modules()`, and a source that two separate parses produced still compiles once, because the key is the text and not the parse. `StageProgram::shares_parse_with` tells whether two stages came from one parse.

## Depth-only passes and vertex-only programs

A shadow map is a pass with a depth attachment and no colour attachment, drawn by a program that needs no fragment stage. `TargetSignature::depth_only(format, samples)` describes the pass, `ProgramRuntime::vertex_only_program` builds a program from a vertex stage alone, and a state with no colour targets and a depth bias fits them:

```rust,no_run
# use worldrunner_render_program::module::StageProgram;
# use worldrunner_render_program::runtime::{
#     GeometryDraw, ImportedGeometry, PipelineDraw, ProgramRuntime, TargetSignature, VertexBufferSpec,
#     VertexLayoutSpec,
# };
# use worldrunner_render_program::state::{DepthState, RasterState, RenderState};
# fn caster(
#     runtime: &mut ProgramRuntime,
#     vertex: &StageProgram,
#     positions: wgpu::Buffer,
#     frame: &mut wgpu::CommandEncoder,
#     pass: &mut wgpu::RenderPass<'_>,
# ) -> Result<(), Box<dyn std::error::Error>> {
let program = runtime.vertex_only_program(vertex)?;
let signature = TargetSignature::depth_only(wgpu::TextureFormat::Depth32Float, 1)?;
let state = RenderState {
    raster: RasterState {
        cull: Some(wgpu::Face::Back),
        front_face: wgpu::FrontFace::Ccw,
        alpha_to_coverage: false,
    },
    depth: Some(DepthState {
        test: wgpu::CompareFunction::Less,
        write: true,
        bias: wgpu::DepthBiasState { constant: 2, slope_scale: 1.75, clamp: 0.0 },
    }),
    colors: Vec::new(),
    stencil: None,
};
let layout = VertexLayoutSpec::new(vec![VertexBufferSpec {
    stride: 12,
    step: wgpu::VertexStepMode::Vertex,
    attributes: wgpu::vertex_attr_array![0 => Float32x3].to_vec(),
}]);
let pipeline = runtime.prepare_pipeline(
    &PipelineDraw {
        program: &program,
        state: &state,
        layout: &layout,
        bindings: &[],
        imports: &[],
        sets: &[],
        owners: &[],
    },
    signature,
)?;
let mesh = GeometryDraw::new(&layout, &[positions], &ImportedGeometry::Vertices { first: 0, count: 3 }, 0..1)?;
pipeline.retain(frame)?;
pipeline.bind(pass);
pipeline.draw(pass, &mesh)?;
# Ok(())
# }
```

A signature refuses what a depth pass cannot be, by name, without a device:

```rust
# use worldrunner_render_program::error::{ProgramError, UnsupportedTarget};
# use worldrunner_render_program::runtime::TargetSignature;
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let signature = TargetSignature::depth_only(wgpu::TextureFormat::Depth32Float, 4)?;
assert_eq!((signature.color_count(), signature.samples()), (0, 4));
assert!(matches!(
    TargetSignature::depth_only(wgpu::TextureFormat::Depth24PlusStencil8, 1),
    Err(ProgramError::UnsupportedTarget(UnsupportedTarget::Stencil { .. })),
));
# Ok(())
# }
```

- **Colour.** A depth-only signature has no colour attachment, so its state has no colour states (`colors: Vec::new()`). A state that describes one is `ProgramError::ColorStatesOnDepthOnly`, and a vertex-only program prepared for a pass that has a colour attachment is `NoFragmentForColors`. A program with a fragment stage may also draw into a depth-only pass, for a fragment stage that discards.
- **Where.** `prepare_imported_draw` and `prepare_pipeline` take a depth-only signature. `prepare_draw` and materials draw into one colour target, as before. `GraphicsProgram::fragment()` is `None` for a vertex-only program.
- **Depth bias.** `DepthState::bias` is part of the pipeline: states that differ in it are different pipelines. The offset added to a fragment's depth is `slope_scale * m + constant * r`, where `m` is the largest slope of the primitive's depth along x or y in texels and `r` the smallest difference the format resolves. For a unorm depth format `r` is `2^-n` for `n` bits, so `Depth16Unorm` moves a primitive of constant depth by `constant / 65536`. A float depth format scales `r` with the primitive's depth, and the exact rule is the device's. A `clamp` other than 0 caps the offset. `tests/lent/depth_only.rs` predicts the written depth in `Depth16Unorm` for a primitive of constant depth and for one of constant slope, and the lavapipe and Radeon renders agree with it to the rounding of the code; other formats are not asserted.

## Override constants

A WGSL `override` is a constant the pipeline sets when it is created, so one source serves every value of a switch, a convention or a radius without a second copy of the text. `StageProgram::with_overrides` returns the same stage with values set, and it checks them against the module:

```rust
# use std::sync::Arc;
# use worldrunner_render_program::module::{
#     OverrideError, StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
# };
# fn main() -> Result<(), Box<dyn std::error::Error>> {
const WGSL: &str = "
override gain: f32 = 0.25;
override mirrored: bool = false;
@fragment fn fs() -> @location(0) vec4<f32> {
    return vec4<f32>(gain, select(0.0, 1.0, mirrored), 0.0, 1.0);
}";
let stage = StageProgram::new(
    StageModule {
        source: StageSource::Wgsl(Arc::from(WGSL)),
        entry_point: "fs".to_owned(),
        kind: StageKind::Fragment,
    },
    StageDeclarations::default(),
)?;

// A bool takes 0 or 1, an integer type an integer in range, a float type a finite number.
let mirrored = stage.with_overrides(&[("gain", 0.75), ("mirrored", 1.0)])?;
assert_ne!(stage.content_key(), mirrored.content_key());

// A constant the module does not declare, and a value its type does not hold, are named.
assert_eq!(
    stage.with_overrides(&[("gian", 0.5)]).unwrap_err(),
    OverrideError::Unknown { name: "gian".to_owned() },
);
assert!(matches!(
    stage.with_overrides(&[("mirrored", 0.5)]),
    Err(OverrideError::Value { name, .. }) if name == "mirrored",
));
# Ok(())
# }
```

A program built from the specialized stages draws with the values, and a program registered from a `StageDesc` does the same: `StageDesc::with_overrides` records the values and registration applies them, so a name the module does not declare fails the registration as `ProgramError::Override`.

```rust,no_run
# use worldrunner_render_program::module::{StageDeclarations, StageKind};
# use worldrunner_render_program::registry::{ProgramDesc, StageDesc};
# use worldrunner_render_program::runtime::ProgramRuntime;
# use worldrunner_render_program::state::RenderState;
# fn register(
#     runtime: &mut ProgramRuntime,
#     source: &str,
# ) -> Result<(), Box<dyn std::error::Error>> {
let vertex = StageDesc::wgsl(source, "vs", StageKind::Vertex, StageDeclarations::default());
let fragment = StageDesc::wgsl(source, "fs", StageKind::Fragment, StageDeclarations::default())
    .with_overrides(&[("gain", 0.75)]);
let handle = runtime.register_program(ProgramDesc::single(
    "tuned",
    RenderState::opaque_depth(),
    vertex,
    fragment,
))?;
# let _ = handle;
# Ok(())
# }
```

- **Names.** A constant is named as wgpu names it: by its identifier, or by its `@id` written as a decimal number when it has one. Giving a name twice is `OverrideError::Duplicate`, and the order of the values does not matter. A call replaces the values a stage had, so `with_overrides(&[])` returns the stage as it was parsed.
- **Cost.** The parsed module is shared and so is the compiled `ShaderModule`: overrides apply when a pipeline is created, so two stages of one source that differ only in their overrides add one pipeline and no module to `RuntimeStats`. The values are part of the stage's `content_key`, and so of the pipeline's cache key, and equal values share a pipeline.
- **Stages.** Vertex, fragment and compute stages each take their own values, as the stages have their own entry points. A constant with no default that the entry point uses, and that no call set, is the GPU's to refuse when the pipeline is created, as `ProgramError::Gpu`.

## Alpha to coverage

In a pass of more than one sample, alpha to coverage lets the fragment's alpha decide how many of a pixel's samples the fragment covers, which gives a cut-out edge such as foliage anti-aliasing without blending or sorting. It is a state: `RasterState::alpha_to_coverage`. The pipeline takes its sample count from the signature and sets alpha to coverage from the state, so the same program draws with and without it as two pipelines.

```rust,no_run
# use worldrunner_render_program::error::ProgramError;
# use worldrunner_render_program::runtime::{ImportedDraw, ProgramRuntime, TargetSignature};
# use worldrunner_render_program::state::RenderState;
# fn cut_out(
#     runtime: &mut ProgramRuntime,
#     draw: &ImportedDraw<'_>,
# ) -> Result<(), Box<dyn std::error::Error>> {
// `draw.state` is `RenderState { raster: RasterState { alpha_to_coverage: true, .. }, .. }`.
let four = TargetSignature::attachments(&[wgpu::TextureFormat::Rgba16Float], None, 4)?;
let prepared = runtime.prepare_imported_draw(draw, four)?;

// A single-sample pass has no samples to drop, so the same state is refused by name.
let one = TargetSignature::attachments(&[wgpu::TextureFormat::Rgba16Float], None, 1)?;
assert_eq!(
    runtime.prepare_imported_draw(draw, one).err(),
    Some(ProgramError::AlphaToCoverageNeedsSamples { samples: 1 }),
);
# let _ = prepared;
# Ok(())
# }
```

- **Where.** `prepare_imported_draw` and `prepare_pipeline` take it with any signature of 2, 4, 8 or 16 samples. `RenderState::check_targets(colors, samples)` checks a state against a signature. `prepare_draw`, materials and registered programs draw into one single-sample target, and `RenderState::check_drawable` refuses it for them with `ProgramError::AlphaToCoverageNeedsSamples { samples: 1 }`.
- **How many samples.** The coverage is the device's to compute from the alpha of colour attachment 0. `tests/lent/coverage.rs` renders a white fragment of alpha 0, 0.25, 0.5, 0.75 and 1 at 4x into transparent black, resolves into `Rgba16Float` and reads back. Every pixel covers 0, 1, 2, 3 and 4 samples on lavapipe and on the Radeon. Those alphas are exact multiples of a quarter, where the count does not depend on how a device rounds or spreads its samples; an alpha between two multiples is the device's to round, and the test asserts nothing about it.
- **Mask.** The pipeline's sample mask is all ones.

## Clip space and depth

A WGSL position is used as written. Clip Y is up, so positive Y lands in the top rows of the target, and depth runs from 0 at the near plane to 1 at the far plane, as wgpu defines them. A counter-clockwise triangle in clip space is a front face under `RenderState::opaque_depth`. SPIR-V stages keep the convention of the SPIR-V reader the crate uses, which flips the position's Y: the same vertices land in the bottom rows. Pick one source format per program and test an asymmetric scene when you switch.

## Current limits

- A draw writes one single-sample colour target with an optional depth target whose format has no stencil aspect. Multisampled targets, several colour targets and stencil formats are refused by name.
- Properties set values and resources. The runtime does not yet fill object, camera or light values for you, so a game sets the matrices and times its shaders read.
- A constant layout holds scalars, vectors and matrices, and arrays of them. Struct arrays are not represented.
- Comparison samplers are refused by name when a program is registered, and every sampler is the fixed linear, clamped one.

Issues [#449](https://github.com/acoliver/real-disunity/issues/449) and [#450](https://github.com/acoliver/real-disunity/issues/450) track the remaining work.
