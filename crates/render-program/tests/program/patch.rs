//! A program that fills a rectangle of clip space with a color from its constant buffer, at a
//! depth the vertices carry: the smallest pass that shows where a draw landed and what it
//! passed the depth test against.

use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::unity::constants::pack_constant_buffer;
use worldrunner_render_program::unity::module::{ModuleBinding, StageKind, StageProgram};
use worldrunner_render_program::unity::runtime::{
    BoundResource, GraphicsProgram, ProgramRuntime, VertexInput,
};
use worldrunner_render_program::unity::state::RenderState;
use worldrunner_unity_serialized::shader_program::{
    Binding, ConstantBuffer, ProgramParameters, ValueParam,
};

use crate::common::stage;

const WGSL: &str = "
struct Patch { color: vec4<f32> }
@group(0) @binding(0) var<uniform> fill: Patch;
@vertex fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4<f32> { return fill.color; }";

const COLOR_NAME: i32 = 0;
const BUFFER_NAME: i32 = 1;
const FLOAT: i8 = 0;

/// The fragment stage's parameters: one constant buffer at binding 0 holding `_PatchColor`.
fn parameters() -> ProgramParameters {
    ProgramParameters {
        constant_buffers: vec![ConstantBuffer {
            name_index: BUFFER_NAME,
            matrices: vec![],
            vectors: vec![ValueParam {
                name_index: COLOR_NAME,
                index: 0,
                array_size: 1,
                kind: FLOAT,
                dimension: 4,
            }],
            structs: vec![],
            size: 16,
            partial: false,
        }],
        constant_buffer_bindings: vec![Binding {
            name_index: BUFFER_NAME,
            index: 0,
            array_size: 1,
        }],
        ..ProgramParameters::default()
    }
}

fn table() -> Vec<ModuleBinding> {
    vec![ModuleBinding {
        set: 0,
        unity_binding: 0,
        binding: 0,
        sampler_binding: None,
    }]
}

pub struct Patch {
    pub program: GraphicsProgram,
    fragment: StageProgram,
}

/// The patch program built on `runtime`'s device.
pub fn patch(runtime: &ProgramRuntime) -> Patch {
    let vertex = stage(
        WGSL,
        ("vs_main", StageKind::Vertex),
        table(),
        ProgramParameters::default(),
    );
    let fragment = stage(
        WGSL,
        ("fs_main", StageKind::Fragment),
        table(),
        parameters(),
    );
    Patch {
        program: runtime.graphics_program(&vertex, &fragment).unwrap(),
        fragment,
    }
}

/// Opaque, both faces, `ZTest LEqual`, depth written, no blending.
pub fn state() -> RenderState {
    RenderState {
        cull: None,
        depth_compare: wgpu::CompareFunction::LessEqual,
        depth_write: true,
        color_write: wgpu::ColorWrites::ALL,
        blend: None,
    }
}

impl Patch {
    /// The constant buffer filled from `_PatchColor`, at the binding the parameters name.
    pub fn color(&self, runtime: &mut ProgramRuntime, rgba: [f32; 4]) -> Vec<BoundResource> {
        let mut properties = PropertySet::new();
        properties.set("_PatchColor", PropertyValue::Vector(rgba));
        let names = [("_PatchColor".to_owned(), COLOR_NAME)];
        let target = self.fragment.constant_buffer_targets().unwrap().remove(0);
        let packed = pack_constant_buffer(target.buffer, &names, &[&properties], 16).unwrap();
        assert!(packed.missing.is_empty());
        let buffer = runtime.create_buffer("patch color", &packed.bytes).unwrap();
        vec![BoundResource::buffer(target.set, target.binding, buffer)]
    }
}

/// Two triangles covering `[x0, y0]..[x1, y1]` in clip space at clip depth `z`. Positive clip Y
/// is the bottom of the image.
pub fn rectangle(runtime: &mut ProgramRuntime, [x0, y0, x1, y1]: [f32; 4], z: f32) -> VertexInput {
    let corners = [
        [x0, y0, z],
        [x1, y0, z],
        [x1, y1, z],
        [x0, y0, z],
        [x1, y1, z],
        [x0, y1, z],
    ];
    let bytes: Vec<u8> = corners
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    VertexInput {
        buffer: runtime.create_buffer("patch", &bytes).unwrap(),
        stride: 12,
        step: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0 => Float32x3].to_vec(),
    }
}
