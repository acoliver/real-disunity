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
use worldrunner_render_program::runtime::{Geometry, MaterialDraw, ProgramRuntime, VertexInput};
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

# fn main() -> Result<(), Box<dyn Error>> {
# let instance = headless_instance();
# let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
# let (device, queue, _) =
#     pollster::block_on(request_device(&adapter, &DeviceRequirements::default()))?;
# let authority = SubmissionAuthority::new(queue);
# let mut runtime = ProgramRuntime::new(device.clone(), authority.clone(), AllocationBudget(1 << 24));
let program = runtime.register_program(ProgramDesc::single(
    "tint",
    RenderState::opaque_depth(),
    StageDesc::wgsl(SHADER, "vs", StageKind::Vertex, StageDeclarations::default()),
    StageDesc::wgsl(SHADER, "fs", StageKind::Fragment, fragment_declarations()),
))?;
let mut initial = PropertySet::new();
initial.set("color", PropertyValue::Vector([1.0, 1.0, 1.0, 1.0]));
let material = runtime.create_material(program, initial)?;
let color = [0.25, 0.5, 0.75, 1.0];
runtime.set_property(material, "color", PropertyValue::Vector(color))?;

// One counter-clockwise triangle that covers all of clip space.
let corners: [[f32; 3]; 3] = [[-1.0, -1.0, 0.5], [3.0, -1.0, 0.5], [-1.0, 3.0, 0.5]];
let bytes: Vec<u8> = corners.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
let vertices = VertexInput {
    buffer: runtime.create_buffer("triangle", &bytes)?,
    stride: 12,
    step: wgpu::VertexStepMode::Vertex,
    attributes: wgpu::vertex_attr_array![0 => Float32x3].to_vec(),
};
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

The render state must be one a draw supports. Stencil, depth bias, alpha to coverage and anything other than one written colour target are refused by name at registration.

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

## Clip space and depth

A WGSL position is used as written. Clip Y is up, so positive Y lands in the top rows of the target, and depth runs from 0 at the near plane to 1 at the far plane, as wgpu defines them. A counter-clockwise triangle in clip space is a front face under `RenderState::opaque_depth`. SPIR-V stages keep the convention of the SPIR-V reader the crate uses, which flips the position's Y: the same vertices land in the bottom rows. Pick one source format per program and test an asymmetric scene when you switch.

## Current limits

- A draw writes one single-sample colour target with an optional depth target whose format has no stencil aspect. Multisampled targets, several colour targets and stencil formats are refused by name.
- Properties set values and resources. The runtime does not yet fill object, camera or light values for you, so a game sets the matrices and times its shaders read.
- A constant layout holds scalars, vectors and matrices, and arrays of them. Struct arrays and WGSL override constants are not represented.
- Comparison samplers are refused by name when a program is registered, and every sampler is the fixed linear, clamped one.

Issues [#449](https://github.com/acoliver/real-disunity/issues/449) and [#450](https://github.com/acoliver/real-disunity/issues/450) track the remaining work.
