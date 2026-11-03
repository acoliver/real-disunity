//! A Unity pass built by hand and the steps that draw it: choose variants, build the stages,
//! pack constants, bind a texture and draw a textured quad.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_program::constants::pack_constant_buffer;
use worldrunner_render_program::module::{ResourceBinding, StageKind, StageModule, StageProgram};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::runtime::{
    BoundResource, DrawRequest, Geometry, GraphicsProgram, ProgramRuntime, TargetImage, VertexInput,
};
use worldrunner_render_program::state::RenderState;
use worldrunner_render_program::variant::select_sub_program;
use worldrunner_unity_serialized::shader_program::{
    BindChannels, Binding, ConstantBuffer, Program, ProgramParameters, SamplerParam, SubProgram,
    TextureParam, ValueParam,
};

use crate::common::{read_target, run, same, spirv};
use crate::fixtures::textured;

pub const TINT: i32 = 0;
pub const OFFSET: i32 = 1;
pub const SCALE: i32 = 2;
pub const MAIN_TEX: i32 = 3;
const MATERIAL: i32 = 10;
const DRAW: i32 = 11;

/// Keyword indices of the two fragment variants.
pub const MULTIPLY: u16 = 0;
pub const ADD: u16 = 1;

const FLOAT: i8 = 0;
const TEX2D: i8 = 2;

/// How the pass binds its texture and sampler.
#[derive(Clone, Copy)]
pub enum Samplers {
    /// A separate texture and sampler, each with its own Unity binding.
    Separate,
    /// A combined image sampler that the importer split; the sampler took binding 2 and the
    /// draw constants moved from Unity's binding 2 to 3.
    Split,
}

pub struct Pass {
    pub names: Vec<(String, i32)>,
    pub vertex: Program,
    pub fragment: Program,
    pub blobs: Vec<Vec<u32>>,
    pub table: Vec<ResourceBinding>,
}

fn vector(name_index: i32, offset: i32) -> ValueParam {
    ValueParam {
        name_index,
        index: offset,
        array_size: 1,
        kind: FLOAT,
        dimension: 4,
    }
}

fn buffer(name_index: i32, vectors: Vec<ValueParam>, size: i32) -> ConstantBuffer {
    ConstantBuffer {
        name_index,
        matrices: vec![],
        vectors,
        structs: vec![],
        size,
        partial: false,
    }
}

fn cb_binding(name_index: i32, index: i32) -> Binding {
    Binding {
        name_index,
        index,
        array_size: 1,
    }
}

fn parameters(draw_binding: Option<i32>, samplers: Samplers) -> ProgramParameters {
    let mut parameters = ProgramParameters {
        constant_buffers: vec![
            buffer(MATERIAL, vec![vector(TINT, 0), vector(OFFSET, 16)], 32),
            buffer(DRAW, vec![vector(SCALE, 0)], 16),
        ],
        constant_buffer_bindings: vec![cb_binding(MATERIAL, 0)],
        ..ProgramParameters::default()
    };
    if let Some(binding) = draw_binding {
        parameters
            .constant_buffer_bindings
            .push(cb_binding(DRAW, binding));
    } else {
        parameters.textures.push(TextureParam {
            name_index: MAIN_TEX,
            index: 1,
            sampler_index: 2,
            multisampled: false,
            dimension: TEX2D,
        });
        if matches!(samplers, Samplers::Separate) {
            parameters.samplers.push(SamplerParam {
                sampler: 0,
                bind_point: 2,
            });
        }
    }
    parameters
}

fn sub_program(blob_index: u32, keywords: &[u16], parameters: ProgramParameters) -> SubProgram {
    SubProgram {
        blob_index,
        channels: BindChannels {
            channels: vec![],
            source_map: 0,
        },
        keyword_indices: keywords.to_vec(),
        hardware_tier: 0,
        gpu_program_type: 0,
        parameters,
        requirements: 0,
    }
}

/// The pass with a vertex program, and a fragment program of a multiply and an add variant.
pub fn pass(samplers: Samplers) -> Pass {
    let (draw_binding, table) = match samplers {
        Samplers::Separate => (3, (0..4).map(|b| same(0, b)).collect()),
        Samplers::Split => (
            2,
            vec![
                same(0, 0),
                ResourceBinding {
                    set: 0,
                    unity_binding: 1,
                    binding: 1,
                    sampler_binding: Some(2),
                },
                ResourceBinding {
                    set: 0,
                    unity_binding: 2,
                    binding: 3,
                    sampler_binding: None,
                },
            ],
        ),
    };
    let names = [
        ("_Tint", TINT),
        ("_Offset", OFFSET),
        ("_Scale", SCALE),
        ("_MainTex", MAIN_TEX),
        ("Material", MATERIAL),
        ("Draw", DRAW),
    ]
    .map(|(name, index)| (name.to_owned(), index))
    .to_vec();
    let vertex_parameters = parameters(Some(draw_binding), samplers);
    let fragment_parameters = parameters(None, samplers);
    Pass {
        names,
        vertex: Program {
            sub_programs: vec![sub_program(0, &[], vertex_parameters)],
            ..Program::default()
        },
        fragment: Program {
            sub_programs: vec![
                sub_program(1, &[MULTIPLY], fragment_parameters.clone()),
                sub_program(2, &[ADD], fragment_parameters),
            ],
            keyword_state_mask: vec![MULTIPLY, ADD],
            ..Program::default()
        },
        blobs: vec![
            spirv(&textured("texel * material.tint")),
            spirv(&textured("texel * material.tint")),
            spirv(&textured("texel + vec4<f32>(material.offset.xyz, 0.0)")),
        ],
        table,
    }
}

fn stage(
    pass: &Pass,
    program: &Program,
    enabled: &BTreeSet<u16>,
    entry: (&str, StageKind),
) -> StageProgram {
    let chosen = select_sub_program(program, enabled).unwrap();
    let module = StageModule {
        words: pass.blobs[chosen.blob_index as usize].clone(),
        entry_point: entry.0.to_owned(),
        kind: entry.1,
        bindings: pass.table.clone(),
    };
    StageProgram::new(module, chosen.parameters.clone()).unwrap()
}

/// What to draw: the keywords and properties, the render state and the quad's triangles.
pub struct Scene<'a> {
    pub pass: &'a Pass,
    pub enabled: &'a [u16],
    pub material: &'a PropertySet,
    pub globals: &'a PropertySet,
    pub state: &'a RenderState,
    /// What the target holds before the draw.
    pub clear: [f64; 4],
    /// Indices into the quad's four corners: top left, top right, bottom right, bottom left.
    pub indices: &'a [u16],
}

pub struct Rendered {
    pub image: TargetImage,
    pub missing: Vec<String>,
}

/// The four texels of the 2x2 texture, as RGBA bytes in row-major order.
pub const TEXELS: [[u8; 4]; 4] = [
    [200, 100, 50, 255],
    [10, 20, 30, 255],
    [255, 255, 255, 128],
    [0, 128, 64, 255],
];

/// Material properties: a tint, an offset whose w is the quad's clip depth, and the globals'
/// scale, which maps the quad onto the whole target.
pub fn properties() -> (PropertySet, PropertySet) {
    let mut material = PropertySet::new();
    material.set("_Tint", PropertyValue::Vector(TINT_VALUE));
    material.set("_Offset", PropertyValue::Vector(OFFSET_VALUE));
    let mut globals = PropertySet::new();
    globals.set("_Scale", PropertyValue::Vector([1.0, 1.0, 0.0, 0.0]));
    (material, globals)
}

pub const TINT_VALUE: [f32; 4] = [0.9, 0.5, 0.3, 0.6];
pub const OFFSET_VALUE: [f32; 4] = [0.1, 0.2, 0.3, 0.5];

fn quad(runtime: &mut ProgramRuntime) -> VertexInput {
    // Position then uv; the top left corner is clip (-1, -1) in Vulkan's Y-down clip space.
    let corners: [[f32; 4]; 4] = [
        [-1.0, -1.0, 0.0, 0.0],
        [1.0, -1.0, 1.0, 0.0],
        [1.0, 1.0, 1.0, 1.0],
        [-1.0, 1.0, 0.0, 1.0],
    ];
    let bytes: Vec<u8> = corners
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    VertexInput {
        buffer: runtime.create_buffer("quad", &bytes).unwrap(),
        stride: 16,
        step: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2].to_vec(),
    }
}

/// Packs each constant buffer the stages declare once, from the material then the globals,
/// and uploads it at its prepared binding. Also returns the properties that were missing.
fn constant_bindings(
    runtime: &mut ProgramRuntime,
    scene: &Scene<'_>,
    program: &GraphicsProgram,
    stages: [&StageProgram; 2],
) -> (Vec<BoundResource>, Vec<String>) {
    let mut packed: BTreeMap<(u32, u32), Vec<u8>> = BTreeMap::new();
    let mut missing = Vec::new();
    for stage in stages {
        for target in stage.constant_buffer_targets().unwrap() {
            let min = program
                .layout()
                .slot(target.set, target.binding)
                .unwrap()
                .min_size;
            let result = pack_constant_buffer(
                target.buffer,
                &scene.pass.names,
                &[scene.material, scene.globals],
                usize::try_from(min).unwrap(),
            )
            .unwrap();
            missing.extend(result.missing);
            packed
                .entry((target.set, target.binding))
                .or_insert(result.bytes);
        }
    }
    let bindings = packed
        .iter()
        .map(|((set, binding), bytes)| {
            BoundResource::buffer(
                *set,
                *binding,
                runtime.create_buffer("constants", bytes).unwrap(),
            )
        })
        .collect();
    (bindings, missing)
}

/// Draws the quad with the textured pass into a cleared 2x2 target and reads it back.
pub fn render(runtime: &mut ProgramRuntime, scene: &Scene<'_>) -> Rendered {
    let enabled: BTreeSet<u16> = scene.enabled.iter().copied().collect();
    let vertex = stage(
        scene.pass,
        &scene.pass.vertex,
        &enabled,
        ("vs_main", StageKind::Vertex),
    );
    let fragment = stage(
        scene.pass,
        &scene.pass.fragment,
        &enabled,
        ("fs_main", StageKind::Fragment),
    );
    let program = runtime.graphics_program(&vertex, &fragment).unwrap();

    let (mut bindings, missing) = constant_bindings(runtime, scene, &program, [&vertex, &fragment]);
    let texels: Vec<u8> = TEXELS.iter().flatten().copied().collect();
    let texture = runtime
        .create_texture_2d("main", [2, 2], wgpu::TextureFormat::Rgba8Unorm, &texels)
        .unwrap();
    for target in fragment.texture_targets().unwrap() {
        assert_eq!(target.name_index, MAIN_TEX);
        bindings.push(BoundResource::texture(target.set, target.binding, texture));
    }

    let target = runtime
        .create_render_target("quad", [2, 2], wgpu::TextureFormat::Rgba8Unorm, None)
        .unwrap();
    run(runtime, |runtime, encoder| {
        runtime.clear_target(encoder, target, scene.clear, 1.0)
    })
    .unwrap();
    let input = quad(runtime);
    let index_bytes: Vec<u8> = scene.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
    let index_buffer = runtime.create_buffer("indices", &index_bytes).unwrap();
    run(runtime, |runtime, encoder| {
        runtime.draw(
            encoder,
            &DrawRequest {
                program: &program,
                state: scene.state,
                front_face: wgpu::FrontFace::Cw,
                target,
                vertex_inputs: &[input],
                bindings: &bindings,
                geometry: Geometry::Indexed {
                    buffer: index_buffer,
                    format: wgpu::IndexFormat::Uint16,
                    first: 0,
                    count: u32::try_from(scene.indices.len()).unwrap(),
                    base_vertex: 0,
                },
            },
        )
    })
    .unwrap();
    Rendered {
        image: read_target(runtime, target),
        missing,
    }
}

/// Both triangles of the quad, wound clockwise as displayed.
pub const QUAD: [u16; 6] = [0, 1, 2, 0, 2, 3];
